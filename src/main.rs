use std::path::PathBuf;
use std::time::Instant;

use anyhow::{Context, Result, bail};
use clap::{Args, Parser, Subcommand};
use r3dsims::record::{RecordOpts, Recorder, preset_size};
use r3dsims::gpu::{Gpu, SceneRenderer};
use r3dsims::story::Step;
use r3dsims::{harbor, house, templates};
use r3dsims::world::World;

#[derive(Parser)]
#[command(name = "r3dsims", version, about = "Scriptable voxel-pixel 3D diorama sandbox")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Write a sample diorama (scene + modular assets as editable JSON).
    Template {
        /// template: "kampung" (island village), "rumah" (two-storey dollhouse) or "pelabuhan" (complex: harbour, lighthouse, 3-storey guesthouse)
        #[arg(default_value = "kampung")]
        name: String,
        #[arg(short, long)]
        out: Option<PathBuf>,
    },
    /// Load a scene, run its whole story headless once and report duration, counts and warnings.
    Validate { scene: PathBuf },
    /// Render the scene to mp4/mov/webm/gif/png or a PNG sequence (headless, deterministic).
    Render(RenderArgs),
}

#[derive(Args)]
struct SizeArgs {
    /// reel|story|tiktok (1080x1920), feed (1080x1350), square (1080x1080), landscape (1920x1080), preview
    #[arg(long, default_value = "reel")]
    preset: String,
    /// Override final size, e.g. 1080x1920
    #[arg(long)]
    size: Option<String>,
    /// Final pixels per rendered pixel (bigger = chunkier pixel art)
    #[arg(long, default_value_t = 3)]
    pixel: u32,
}

#[derive(Args)]
struct RenderArgs {
    scene: PathBuf,
    /// Output: .mp4 .mov .webm (needs ffmpeg), .gif, .png (still), or a directory (PNG sequence)
    #[arg(short, long)]
    out: PathBuf,
    #[command(flatten)]
    size: SizeArgs,
    #[arg(long, default_value_t = 30)]
    fps: u32,
    /// Video length in seconds
    #[arg(long, default_value_t = 20.0)]
    seconds: f32,
    /// Simulate this many seconds before the first recorded frame
    #[arg(long, default_value_t = 0.0)]
    start: f32,
    /// Render one still at this time (seconds) instead of a video
    #[arg(long)]
    at: Option<f32>,
    /// Audio track to mux into mp4/mov/webm
    #[arg(long)]
    audio: Option<PathBuf>,
    /// H.264 quality (lower = better, 18 is visually lossless)
    #[arg(long, default_value_t = 18)]
    crf: u32,
    /// JSON array of timed commands: [{"at": 3.0, "do": "camera", "op": "orbit", "dyaw": 90, "secs": 2}, ...]
    #[arg(long)]
    commands: Option<PathBuf>,
    /// Ignore the story and only play this named scene
    #[arg(long)]
    play: Option<String>,
}

fn final_size(a: &SizeArgs) -> Result<(u32, u32)> {
    if let Some(s) = &a.size {
        let (w, h) = s.split_once('x').context("--size must look like 1080x1920")?;
        return Ok((w.parse()?, h.parse()?));
    }
    preset_size(&a.preset).with_context(|| format!("unknown preset '{}'", a.preset))
}

fn render_res(out: (u32, u32), pixel: u32) -> Result<(usize, usize)> {
    if pixel == 0 {
        bail!("--pixel must be >= 1");
    }
    if out.0 % pixel != 0 || out.1 % pixel != 0 {
        eprintln!("[warn] {}x{} is not divisible by --pixel {pixel}; pixels will be slightly uneven", out.0, out.1);
    }
    Ok(((out.0 / pixel).max(16) as usize, (out.1 / pixel).max(16) as usize))
}

struct Timed {
    at: f32,
    json: String,
}

fn load_timed(path: &PathBuf) -> Result<Vec<Timed>> {
    let txt = std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let arr: Vec<serde_json::Value> = serde_json::from_str(&txt).context("commands file must be a JSON array")?;
    let mut out = vec![];
    for (i, mut v) in arr.into_iter().enumerate() {
        let obj = v.as_object_mut().with_context(|| format!("command #{i} is not an object"))?;
        let at = obj.remove("at").and_then(|a| a.as_f64()).with_context(|| format!("command #{i} needs a numeric \"at\""))? as f32;
        out.push(Timed { at, json: serde_json::to_string(&v)? });
    }
    out.sort_by(|a, b| a.at.total_cmp(&b.at));
    Ok(out)
}

fn run_due(world: &mut World, timed: &[Timed], next: &mut usize) {
    while *next < timed.len() && timed[*next].at <= world.time + 1e-4 {
        if let Err(e) = world.run_json(&timed[*next].json) {
            eprintln!("[warn] command at {:.2}s: {e:#}", timed[*next].at);
        }
        *next += 1;
    }
}

fn cmd_render(a: RenderArgs) -> Result<()> {
    let out_size = final_size(&a.size)?;
    let (rw, rh) = render_res(out_size, a.size.pixel)?;
    let mut world = World::load(&a.scene)?;
    if let Some(name) = &a.play {
        world.story = None;
        world.run(Step::PlayScene { name: name.clone() });
    }
    let timed = match &a.commands {
        Some(p) => load_timed(p)?,
        None => vec![],
    };
    let mut next = 0;
    let gpu = Gpu::headless()?;
    eprintln!("gpu: {}", gpu.adapter);
    let mut r = SceneRenderer::new(&gpu.device, &gpu.queue, rw as u32, rh as u32);
    let dt = 1.0 / a.fps as f32;

    let t0 = Instant::now();
    let warmup = a.at.unwrap_or(0.0) + a.start;
    let mut sim = 0.0;
    while sim + dt * 0.5 < warmup {
        run_due(&mut world, &timed, &mut next);
        world.update(dt);
        sim += dt;
    }
    let frames = if a.at.is_some() { 1 } else { (a.seconds * a.fps as f32).round().max(1.0) as u32 };
    let mut rec = Recorder::start(RecordOpts {
        out: a.out.clone(),
        fps: a.fps,
        out_w: out_size.0,
        out_h: out_size.1,
        src_w: rw,
        src_h: rh,
        audio: a.audio.clone(),
        crf: a.crf,
    })?;
    for f in 0..frames {
        if f > 0 || a.at.is_none() {
            run_due(&mut world, &timed, &mut next);
            world.update(dt);
        }
        let rgb = r.render_rgb(&world)?;
        rec.push(&rgb)?;
    }
    let n = rec.frames;
    rec.finish()?;
    println!(
        "{} -> {} ({}x{} from {}x{} render, {} frames @ {} fps) in {:.1}s",
        a.scene.display(),
        a.out.display(),
        out_size.0,
        out_size.1,
        rw,
        rh,
        n,
        a.fps,
        t0.elapsed().as_secs_f32()
    );
    Ok(())
}

fn cmd_validate(scene: PathBuf) -> Result<()> {
    let mut world = World::load(&scene)?;
    let voxels: usize = world.models.values().map(|m| m.voxel_count()).sum();
    println!("scene '{}': {}x{} tiles, {} terrain faces", world.name, world.terrain.w, world.terrain.d, world.terrain.quads.len());
    println!("assets: {} ({} surface voxels), entities: {}, scenes: {}", world.models.len(), voxels, world.entities.len(), world.stages.len());
    for m in world.models.values() {
        let mut clips: Vec<_> = m.clips.keys().cloned().collect();
        clips.sort();
        println!("  asset {:<10} {:>5} voxels  {} parts  clips: {}", m.name, m.voxel_count(), m.parts.len(), clips.join(", "));
    }
    if world.story.is_none() {
        println!("no story");
        return Ok(());
    }
    world.story_loop = false;
    let dt = 1.0 / 30.0;
    while world.story.is_some() && world.time < 900.0 {
        world.update(dt);
    }
    if world.story.is_some() {
        bail!("story did not finish within 900s of simulated time");
    }
    println!("story finishes after {:.1}s of simulated time", world.time);
    Ok(())
}

fn main() -> Result<()> {
    match Cli::parse().cmd {
        Cmd::Template { name, out } => {
            let out = out.unwrap_or_else(|| PathBuf::from(&name));
            match name.as_str() {
                "kampung" => templates::write_kampung(&out)?,
                "rumah" => house::write(&out)?,
                "pelabuhan" => harbor::write(&out)?,
                other => bail!("unknown template '{other}' (available: kampung, rumah, pelabuhan)"),
            }
            println!("wrote {}/scene.json and {}/assets/*.json", out.display(), out.display());
            println!("next: r3dsims validate {0}/scene.json && r3dsims render {0}/scene.json -o {1}.mp4", out.display(), name);
            Ok(())
        }
        Cmd::Validate { scene } => cmd_validate(scene),
        Cmd::Render(a) => cmd_render(a),
    }
}
