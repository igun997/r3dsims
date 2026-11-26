use std::io::Read;
use std::path::PathBuf;
use std::time::Instant;

use anyhow::{Context, Result, bail};
use clap::{Args, Parser, Subcommand};
use r3dsims::client::Client;
use r3dsims::gpu::{Gpu, SceneRenderer};
use r3dsims::live::{Live, LiveOpts};
use r3dsims::overlay::{Compositor, Overlay, Vars, parse_elements};
use r3dsims::record::{RecordOpts, Recorder, preset_size, upscale_nearest};
use r3dsims::story::Step;
use r3dsims::war::{Event, War};
use r3dsims::world::World;
use r3dsims::{demo, emoji, harbor, house, perang, templates};
use serde_json::{Value, json};

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
        /// kampung (island village), rumah (two-storey dollhouse), pelabuhan (complex harbour) perang (two kingdoms at war, for `live`) or demo (citizens vs government street demonstration, for `live`)
        #[arg(default_value = "kampung")]
        name: String,
        #[arg(short, long)]
        out: Option<PathBuf>,
    },
    /// Load a scene (or a war.json) and run it headless once: durations, counts, warnings, a self-played war.
    Validate { scene: PathBuf },
    /// Render the scene to mp4/mov/webm/gif/png or a PNG sequence (headless, deterministic).
    Render(RenderArgs),
    /// Run a war diorama live: webhooks in, picture out (MJPEG over HTTP and/or ffmpeg to rtmp/srt/file).
    Live(LiveArgs),
    /// Send a viewer event (gift, like, comment...) to a running `live`, like the connector app does.
    Send(SendArgs),
    /// Publish, list or remove webhook functions (triggers, actions, macros, bots) in a running `live`.
    Hook {
        #[command(subcommand)]
        cmd: HookCmd,
    },
    /// Add, patch, remove or list on-screen overlay elements (text, emoji, SVG, PNG, bars) in a running `live`.
    Overlay {
        #[command(subcommand)]
        cmd: OverlayCmd,
    },
    /// Match control: start, next_round, reset, stop, pause, resume.
    Control {
        op: String,
        #[command(flatten)]
        conn: Conn,
    },
    /// Switch the biome of a running `live` (a key of `biomes`, or random).
    Biome {
        name: String,
        #[command(flatten)]
        conn: Conn,
    },
    /// Show the state of a running `live` (round, castle HP, winner, macros, top supporters).
    State {
        #[command(flatten)]
        conn: Conn,
    },
    /// Run a story/camera step on a running `live`, e.g. '{"op":"orbit","dyaw":90,"secs":2}'.
    Step {
        json: String,
        #[command(flatten)]
        conn: Conn,
    },
}

#[derive(Args, Clone)]
struct Conn {
    /// address of the running `r3dsims live`
    #[arg(long, env = "R3D_SERVER", default_value = "http://127.0.0.1:8787", global = false)]
    server: String,
    /// shared secret, when `live` was started with --token
    #[arg(long, env = "R3D_TOKEN")]
    token: Option<String>,
}

impl Conn {
    fn client(&self) -> Result<Client> {
        Client::new(&self.server, self.token.clone())
    }
}

#[derive(Subcommand)]
enum HookCmd {
    /// Publish triggers/actions/macros/bots from a JSON file, `-` (stdin) or a JSON string.
    Publish {
        source: String,
        #[command(flatten)]
        conn: Conn,
    },
    /// Show what is published now.
    List {
        #[command(flatten)]
        conn: Conn,
    },
    /// Remove triggers, actions, macros or bots by name.
    Remove {
        names: Vec<String>,
        #[command(flatten)]
        conn: Conn,
    },
}

#[derive(Subcommand)]
enum OverlayCmd {
    /// Add elements: a JSON file, `-` (stdin) or a JSON string (one element, an array, or {"add":[..]}).
    Add {
        source: String,
        #[command(flatten)]
        conn: Conn,
    },
    /// Change fields of an element: `overlay patch ID '{"text":"NEW"}'`.
    Patch {
        id: String,
        json: String,
        #[command(flatten)]
        conn: Conn,
    },
    Remove {
        ids: Vec<String>,
        #[command(flatten)]
        conn: Conn,
    },
    Clear {
        #[command(flatten)]
        conn: Conn,
    },
    /// Elements on screen right now.
    List {
        #[command(flatten)]
        conn: Conn,
    },
    /// The built-in emoji (key, short name). Other emoji: put `<key>.svg` in --emoji-dir.
    Emoji,
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
    /// voxel (cubes) or clay (smooth hand-made figures)
    #[arg(long, default_value = "voxel")]
    style: String,
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
    /// Overlay elements (text, emoji, SVG, PNG, bars) drawn on every frame; `from`/`until` are seconds
    #[arg(long)]
    overlay: Option<PathBuf>,
    /// Directory with <key>.svg / <key>.png for emoji outside the built-in set
    #[arg(long)]
    emoji_dir: Option<PathBuf>,
}

#[derive(Args)]
struct LiveArgs {
    /// war.json (see `r3dsims template perang`)
    war: PathBuf,
    /// address for webhooks and the picture
    #[arg(long, default_value = "127.0.0.1:8787")]
    listen: String,
    /// require this secret on every POST (Authorization: Bearer, X-Token, or ?token=)
    #[arg(long, env = "R3D_TOKEN")]
    token: Option<String>,
    /// live-wide (1280x720), live (720x1280), or any `render` preset name
    #[arg(long, default_value = "live-wide")]
    preset: String,
    #[arg(long)]
    size: Option<String>,
    /// final pixels per rendered pixel: 1 = fine detail (default), 2 = chunky pixel art, cheaper on the GPU
    #[arg(long, default_value_t = 1)]
    pixel: u32,
    /// clay (smooth hand-made figures, default) or voxel (cubes)
    #[arg(long, default_value = "clay")]
    style: String,
    #[arg(long, default_value_t = 30)]
    fps: u32,
    /// also push the picture to ffmpeg: rtmp://..., srt://..., udp://..., a file, or - for MPEG-TS on stdout
    #[arg(long)]
    out: Option<String>,
    /// video bitrate for --out, kbit/s
    #[arg(long, default_value_t = 3500)]
    bitrate: u32,
    /// serve no MJPEG (only --out)
    #[arg(long)]
    no_mjpeg: bool,
    /// JPEG quality of the MJPEG stream
    #[arg(long, default_value_t = 80)]
    quality: u8,
    /// Directory with <key>.svg / <key>.png for emoji outside the built-in set
    #[arg(long)]
    emoji_dir: Option<PathBuf>,
    /// stop after this many seconds (tests)
    #[arg(long)]
    seconds: Option<f32>,
    /// do not pace to the wall clock (tests)
    #[arg(long)]
    no_realtime: bool,
    /// print every webhook event and which triggers it fired
    #[arg(long)]
    log_events: bool,
}

#[derive(Args)]
struct SendArgs {
    /// gift | like | comment | follow | share | join | subscribe | custom
    kind: Option<String>,
    /// gift name (for gift/custom) or the comment text (for comment) or the like count (for like)
    what: Option<String>,
    #[arg(long, default_value = "viewer")]
    user: String,
    #[arg(long)]
    count: Option<u32>,
    #[arg(long)]
    diamonds: Option<u32>,
    /// side the viewer supports: A or B
    #[arg(long)]
    side: Option<String>,
    #[arg(long)]
    id: Option<String>,
    /// send this raw JSON event (object, array or {"events":[..]}) instead
    #[arg(long)]
    json: Option<String>,
    #[command(flatten)]
    conn: Conn,
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

fn clock(t: f32) -> String {
    format!("{}:{:02}", t as u32 / 60, t as u32 % 60)
}

fn parse_style(s: &str) -> Result<bool> {
    match s {
        "clay" => Ok(true),
        "voxel" => Ok(false),
        other => bail!("--style must be clay or voxel (got '{other}')"),
    }
}

fn cmd_render(a: RenderArgs) -> Result<()> {
    let out_size = final_size(&a.size)?;
    let (rw, rh) = render_res(out_size, a.size.pixel)?;
    let mut world = World::load(&a.scene)?;
    world.clay = parse_style(&a.size.style)?;
    if let Some(name) = &a.play {
        world.story = None;
        world.run(Step::PlayScene { name: name.clone() });
    }
    let timed = match &a.commands {
        Some(p) => load_timed(p)?,
        None => vec![],
    };
    let mut overlay = Overlay::new();
    let mut comp = Compositor::new(a.scene.parent().map(|p| p.to_path_buf()).unwrap_or_default(), a.emoji_dir.clone());
    let has_overlay = a.overlay.is_some();
    if let Some(p) = &a.overlay {
        let txt = std::fs::read_to_string(p).with_context(|| format!("read {}", p.display()))?;
        for e in parse_elements(&txt)? {
            overlay.add(e);
        }
    }
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
        overlay.tick(dt);
        sim += dt;
    }
    let frames = if a.at.is_some() { 1 } else { (a.seconds * a.fps as f32).round().max(1.0) as u32 };
    // with an overlay the frames are composited at output size here, so the recorder gets them as they are
    let (src_w, src_h) = if has_overlay { (out_size.0 as usize, out_size.1 as usize) } else { (rw, rh) };
    let mut rec = Recorder::start(RecordOpts {
        out: a.out.clone(),
        fps: a.fps,
        out_w: out_size.0,
        out_h: out_size.1,
        src_w,
        src_h,
        audio: a.audio.clone(),
        crf: a.crf,
    })?;
    for f in 0..frames {
        if f > 0 || a.at.is_none() {
            run_due(&mut world, &timed, &mut next);
            world.update(dt);
            overlay.tick(dt);
        }
        let rgb = r.render_rgb(&world)?;
        if has_overlay {
            let mut big = upscale_nearest(&rgb, rw, rh, out_size.0 as usize, out_size.1 as usize);
            let mut vars = Vars::new();
            vars.insert("time".into(), clock(world.time));
            vars.insert("t".into(), format!("{:.1}", world.time));
            comp.draw(&mut big, out_size.0 as usize, out_size.1 as usize, &overlay.drawables(), &vars);
            rec.push(&big)?;
        } else {
            rec.push(&rgb)?;
        }
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

fn is_war_file(path: &PathBuf) -> bool {
    std::fs::read_to_string(path).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok()).is_some_and(|v| v.get("sides").is_some() && v.get("scene").is_some())
}

fn cmd_validate(scene: PathBuf) -> Result<()> {
    if is_war_file(&scene) {
        return validate_war(scene);
    }
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

/// Loads the war, checks every reference, then plays a match by itself: every trigger is fed a
/// synthetic event now and then, and the match is followed until it has a winner.
fn validate_war(path: PathBuf) -> Result<()> {
    let (mut world, mut war) = War::load(&path)?;
    let def = war.def.clone();
    println!("war '{}': mode {}, castle HP {}", def.title, def.match_.mode, def.castle_hp);
    for (k, s) in &def.sides {
        println!("  side {k}: {} ({}), castle '{}', default unit '{}'", s.name, s.color, s.castle, s.unit);
    }
    println!(
        "  {} units, {} actions, {} triggers, {} macros, {} bots, {} overlay elements, hud {}",
        def.units.len(),
        def.actions.len(),
        def.triggers.len(),
        def.macros.len(),
        def.bots.len(),
        def.overlay.len(),
        if def.hud { "on" } else { "off" }
    );
    for t in &def.triggers {
        println!("  trigger {:<14} on {:?} -> side {}", t.name, t.on.list(), t.side);
    }
    war.control(&mut world, "start")?;
    let dt = 1.0 / 30.0;
    let mut t = 0.0f32;
    let mut next_ev = 2.0f32;
    let mut i = 0usize;
    let mut fired = 0;
    let mut rounds_seen = 0u64;
    let mut last_round = 1u64;
    let limit = 1800.0;
    loop {
        if t >= next_ev && !def.triggers.is_empty() {
            let trig = &def.triggers[i % def.triggers.len()];
            i += 1;
            let pat = trig.on.list().first().copied().unwrap_or("gift:x").to_string();
            let (kind, arg) = pat.split_once(':').map(|(k, a)| (k.to_string(), a.to_string())).unwrap_or((pat.clone(), String::new()));
            let ev = Event {
                kind: kind.to_ascii_lowercase(),
                user: format!("tester{i}"),
                gift: if kind.eq_ignore_ascii_case("comment") { String::new() } else { arg.clone() },
                count: trig.min_count.max(2),
                diamonds: trig.min_diamonds,
                text: arg.trim_start_matches('~').to_string(),
                side: None,
                id: None,
            };
            let r = war.handle_event(&mut world, ev);
            if r["matched"].as_array().is_some_and(|m| m.iter().any(|x| x.as_str().is_some_and(|s| !s.contains('('))))  {
                fired += 1;
            }
            next_ev += 3.5;
        }
        war.update(dt, &mut world);
        world.update(dt);
        t += dt;
        let st = war.state_json();
        let round = st["round"].as_u64().unwrap_or(1);
        if round != last_round {
            rounds_seen += 1;
            last_round = round;
            println!("  round {} started at {:.0}s (last round winner: {})", round, t, st["last_round_winner"]);
        }
        if def.match_.mode == "endless" && rounds_seen >= 2 && st["phase"] == "round_end" {
            war.control(&mut world, "stop")?;
        }
        if st["phase"] == "match_end" || t > limit {
            println!("  match ended after {:.0}s: winner {} (round wins {} / {})", t, st["match_winner"], st["sides"][&war.side_keys()[0]]["round_wins"], st["sides"][&war.side_keys()[1]]["round_wins"]);
            if t > limit {
                bail!("war did not finish within {limit}s of simulated time");
            }
            break;
        }
    }
    println!("  {fired} synthetic events fired triggers; no problems found");
    Ok(())
}

fn read_source(s: &str) -> Result<String> {
    if s == "-" {
        let mut buf = String::new();
        std::io::stdin().read_to_string(&mut buf)?;
        return Ok(buf);
    }
    let p = std::path::Path::new(s);
    if p.is_file() {
        return std::fs::read_to_string(p).with_context(|| format!("read {s}"));
    }
    Ok(s.to_string())
}

fn show(client: &Client, method: &str, path: &str, body: Option<&str>) -> Result<()> {
    let (code, text) = client.request(method, path, body)?;
    let pretty = serde_json::from_str::<Value>(&text).ok().and_then(|v| serde_json::to_string_pretty(&v).ok()).unwrap_or(text);
    println!("{pretty}");
    if !(200..300).contains(&code) {
        bail!("server answered {code}");
    }
    Ok(())
}

fn cmd_send(a: SendArgs) -> Result<()> {
    let client = a.conn.client()?;
    let body = match &a.json {
        Some(j) => read_source(j)?,
        None => {
            let kind = a.kind.clone().context("give an event kind (gift, like, comment...) or --json")?;
            let mut ev = json!({"type": kind, "user": a.user});
            let what = a.what.clone();
            match kind.as_str() {
                "gift" | "custom" => ev["gift"] = json!(what.context("which gift? e.g. `send gift rose`")?),
                "comment" => ev["text"] = json!(what.context("what comment?")?),
                "like" => {
                    if let Some(n) = what.and_then(|w| w.parse::<u32>().ok()) {
                        ev["count"] = json!(n);
                    }
                }
                _ => {}
            }
            if let Some(c) = a.count {
                ev["count"] = json!(c);
            }
            if let Some(d) = a.diamonds {
                ev["diamonds"] = json!(d);
            }
            if let Some(s) = &a.side {
                ev["side"] = json!(s);
            }
            if let Some(i) = &a.id {
                ev["id"] = json!(i);
            }
            ev.to_string()
        }
    };
    show(&client, "POST", "/webhook", Some(&body))
}

fn cmd_live(a: LiveArgs) -> Result<()> {
    let (out_w, out_h) = match (&a.size, a.preset.as_str()) {
        (Some(s), _) => {
            let (w, h) = s.split_once('x').context("--size must look like 1280x720")?;
            (w.parse()?, h.parse()?)
        }
        (None, "live-wide") => (1280, 720),
        (None, "live") => (720, 1280),
        (None, p) => preset_size(p).with_context(|| format!("unknown preset '{p}'"))?,
    };
    let live = Live::start(LiveOpts {
        war: a.war,
        listen: a.listen,
        token: a.token,
        out_w,
        out_h,
        pixel: a.pixel,
        clay: parse_style(&a.style)?,
        fps: a.fps,
        mjpeg: !a.no_mjpeg,
        quality: a.quality,
        out: a.out,
        bitrate_k: a.bitrate,
        emoji_dir: a.emoji_dir,
        seconds: a.seconds,
        realtime: !a.no_realtime,
        log_events: a.log_events,
    })?;
    let st = live.run()?;
    eprintln!("final state: {}", st);
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
                "demo" => {
                    demo::write(&out)?;
                    println!("wrote {0}/war.json, {0}/scene.json and {0}/assets/*.json", out.display());
                    println!("next: r3dsims validate {0}/war.json && r3dsims live {0}/war.json   (then open http://127.0.0.1:8787/view)", out.display());
                    return Ok(());
                }
                "perang" => {
                    perang::write(&out)?;
                    println!("wrote {0}/war.json, {0}/scene.json and {0}/assets/*.json", out.display());
                    println!("next: r3dsims validate {0}/war.json && r3dsims live {0}/war.json   (then open http://127.0.0.1:8787/view)", out.display());
                    return Ok(());
                }
                other => bail!("unknown template '{other}' (available: kampung, rumah, pelabuhan, perang, demo)"),
            }
            println!("wrote {}/scene.json and {}/assets/*.json", out.display(), out.display());
            println!("next: r3dsims validate {0}/scene.json && r3dsims render {0}/scene.json -o {1}.mp4", out.display(), name);
            Ok(())
        }
        Cmd::Validate { scene } => cmd_validate(scene),
        Cmd::Render(a) => cmd_render(a),
        Cmd::Live(a) => cmd_live(a),
        Cmd::Send(a) => cmd_send(a),
        Cmd::Hook { cmd } => match cmd {
            HookCmd::Publish { source, conn } => show(&conn.client()?, "POST", "/hooks", Some(&read_source(&source)?)),
            HookCmd::List { conn } => show(&conn.client()?, "GET", "/hooks", None),
            HookCmd::Remove { names, conn } => show(&conn.client()?, "POST", "/hooks", Some(&json!({"remove": names}).to_string())),
        },
        Cmd::Overlay { cmd } => match cmd {
            OverlayCmd::Add { source, conn } => show(&conn.client()?, "POST", "/overlay", Some(&read_source(&source)?)),
            OverlayCmd::Patch { id, json: patch, conn } => {
                let p: Value = serde_json::from_str(&read_source(&patch)?).context("patch is not JSON")?;
                show(&conn.client()?, "POST", "/overlay", Some(&json!({"patch": {id: p}}).to_string()))
            }
            OverlayCmd::Remove { ids, conn } => show(&conn.client()?, "POST", "/overlay", Some(&json!({"remove": ids}).to_string())),
            OverlayCmd::Clear { conn } => show(&conn.client()?, "POST", "/overlay", Some(&json!({"clear": true}).to_string())),
            OverlayCmd::List { conn } => show(&conn.client()?, "GET", "/overlay", None),
            OverlayCmd::Emoji => {
                for (key, name, _) in emoji::SET {
                    let ch: String = key.split('-').filter_map(|h| u32::from_str_radix(h, 16).ok()).filter_map(char::from_u32).collect();
                    println!("{key:<8} :{name}:   {ch}");
                }
                Ok(())
            }
        },
        Cmd::Control { op, conn } => show(&conn.client()?, "POST", "/control", Some(&json!({"op": op}).to_string())),
        Cmd::State { conn } => show(&conn.client()?, "GET", "/state", None),
        Cmd::Biome { name, conn } => show(&conn.client()?, "POST", "/control", Some(&json!({"op": format!("biome:{name}")}).to_string())),
        Cmd::Step { json: j, conn } => show(&conn.client()?, "POST", "/command", Some(&read_source(&j)?)),
    }
}
