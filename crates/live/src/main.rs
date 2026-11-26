//! r3dlive: the live view of an r3dsims diorama, drawn with Rune (github.com/anomalyco/rune).
//!
//! The diorama is rendered on the GPU by r3dsims' wgpu renderer *on Rune's own device* and blitted
//! (nearest filtered, so pixels stay crisp) into the viewport slot of the UI, so there is no
//! readback while watching. Recording runs a second render at the output size only while REC is on.
//! Mouse: drag = orbit, right/middle drag = pan, wheel = zoom. Everything the buttons do is also a
//! JSON step you can paste into the command box: what an agent sends and what a person clicks are
//! the same thing.
//!
//!   r3dlive                        built-in "kampung" demo (island village)
//!   r3dlive --demo rumah           two-storey dollhouse
//!   r3dlive --demo pelabuhan       complex: harbour island with a three-storey guesthouse
//!   r3dlive --file scene.json      your own scene
//!   r3dlive --preset feed --out recordings
//!   r3dlive --snap ui.png --at 3   headless screenshot of the whole UI (Rune's own flag)

#![forbid(unsafe_code)]

use std::path::PathBuf;

use r3dsims::camera::CameraCmd;
use r3dsims::gpu::{Blit, SceneRenderer};
use r3dsims::record::{RecordOpts, Recorder, preset_size, upscale_nearest, write_png};
use r3dsims::story::Step;
use r3dsims::world::World;
use r3dsims::{harbor, house, templates};
use rune::tok::{self, radius, type_};
use rune::{
    AppSpec, Color, Compositor, Cx, Drag, El, Frame, Key, KeyEvent, LiveText, Phase, PointerButton, ScrollSpec, TextEdit, Tx,
    WheelEvent, col, el, row, text, text_input,
};

const PRESETS: [(&str, &str); 4] = [("Reel 9:16", "reel"), ("Feed 4:5", "feed"), ("Square", "square"), ("Wide 16:9", "landscape")];
const PIXEL: u32 = 3;
const FPS: u32 = 30;
const VIEWPORT: u32 = 1;

enum Source {
    Demo(String),
    File(PathBuf),
}

impl Source {
    fn load(&self) -> anyhow::Result<World> {
        match self {
            Source::Demo(name) if name == "rumah" => World::from_scene(house::scene_inline()?, PathBuf::new()),
            Source::Demo(name) if name == "pelabuhan" => World::from_scene(harbor::scene_inline()?, PathBuf::new()),
            Source::Demo(_) => World::from_scene(templates::kampung_scene_inline()?, PathBuf::new()),
            Source::File(p) => World::load(p),
        }
    }
}

struct Rec {
    rec: Recorder,
    path: PathBuf,
}

struct State {
    source: Source,
    world: World,
    out: (u32, u32),
    res: (u32, u32),
    preset: usize,
    paused: bool,
    last_t: f64,
    rec: Option<Rec>,
    rec_acc: f32,
    /// a fixed-step frame is due and the compositor must push it to the recorder
    rec_due: bool,
    shot_pending: bool,
    out_dir: PathBuf,
    cmd: TextEdit,
    status: String,
    drag_prev: (f32, f32),
    info: LiveText,
    rec_text: LiveText,
    info_at: f64,
    frames: u32,
    scenes: Vec<String>,
    actors: Vec<(String, String)>,
}

fn unix() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

fn sizes(preset: usize) -> ((u32, u32), (u32, u32)) {
    let out = preset_size(PRESETS[preset].1).unwrap_or((1080, 1920));
    (out, (out.0 / PIXEL, out.1 / PIXEL))
}

fn init() -> State {
    let extra = rune::extra_args();
    let flag = |name: &str| extra.iter().position(|a| a == name).and_then(|i| extra.get(i + 1)).cloned();
    let preset = flag("--preset").and_then(|p| PRESETS.iter().position(|(_, id)| *id == p)).unwrap_or(0);
    let source = match (flag("--file"), flag("--demo")) {
        (Some(p), _) => Source::File(p.into()),
        (None, Some(d)) => Source::Demo(d),
        _ => Source::Demo("kampung".into()),
    };
    let (world, status) = match source.load() {
        Ok(w) => (w, String::from("Ready")),
        Err(e) => {
            eprintln!("{e:#}");
            (Source::Demo("kampung".into()).load().expect("built-in demo scene"), format!("Could not load scene: {e:#}"))
        }
    };
    let scenes = world.stages.iter().map(|s| s.name.clone()).collect();
    let actors = world.entities.iter().filter_map(|e| e.label.as_ref().map(|l| (e.id.clone(), l.clone()))).collect();
    let (out, res) = sizes(preset);
    State {
        source,
        world,
        out,
        res,
        preset,
        paused: false,
        last_t: -1.0,
        rec: None,
        rec_acc: 0.0,
        rec_due: false,
        shot_pending: false,
        out_dir: flag("--out").map_or_else(|| PathBuf::from("recordings"), PathBuf::from),
        cmd: TextEdit::new(""),
        status,
        drag_prev: (0.0, 0.0),
        info: LiveText::new(""),
        rec_text: LiveText::new("REC 00:00"),
        info_at: 0.0,
        frames: 0,
        scenes,
        actors,
    }
}

fn poll(s: &mut State, t: f64, _tx: &mut Tx) -> bool {
    let real = if s.last_t < 0.0 { 0.0 } else { (t - s.last_t).clamp(0.0, 0.1) as f32 };
    s.last_t = t;
    if s.rec.is_some() {
        // record at a fixed step so the video is deterministic whatever the display rate is
        s.rec_acc += real;
        let step = 1.0 / FPS as f32;
        if s.rec_acc >= step {
            s.rec_acc -= step;
            s.world.update(step);
            s.rec_due = true;
        }
    } else if s.paused {
        s.world.update(0.0);
    } else {
        s.world.update(real);
    }
    s.frames += 1;
    if t - s.info_at >= 0.5 {
        let fps = s.frames as f64 / (t - s.info_at).max(0.001);
        s.info.set(format!("t {:.1}s   {:.0} fps   {}x{} px   GPU", s.world.time, fps, s.res.0, s.res.1));
        s.info_at = t;
        s.frames = 0;
    }
    false
}

// ---------------------------------------------------------------- GPU side

struct GpuSide {
    key: rune::compose::DeviceKey,
    view: SceneRenderer,
    blit: Blit,
    bind: wgpu::BindGroup,
    /// second renderer at the output size, only used while recording or taking a screenshot
    cap: Option<SceneRenderer>,
}

#[derive(Default)]
struct Scene3d {
    gpu: Option<GpuSide>,
}

impl Compositor<State> for Scene3d {
    fn render(&mut self, f: &mut Frame<'_, State>) {
        let key = f.device_key();
        let res = f.rt.state.res;
        if self.gpu.as_ref().is_none_or(|g| g.key != key) {
            let view = SceneRenderer::new(f.device, f.queue, res.0, res.1);
            let blit = Blit::new(f.device);
            let bind = blit.bind_group(view.view());
            self.gpu = Some(GpuSide { key, view, blit, bind, cap: None });
        }
        let g = self.gpu.as_mut().unwrap();
        if g.view.size() != res {
            g.view.resize(res.0, res.1);
            g.bind = g.blit.bind_group(g.view.view());
        }
        g.view.render(&f.rt.state.world, f.encoder);

        let st = &mut f.rt.state;
        let want_frame = st.rec_due && st.rec.is_some();
        if want_frame || st.shot_pending {
            let cap = g.cap.get_or_insert_with(|| SceneRenderer::new(f.device, f.queue, res.0, res.1));
            cap.resize(res.0, res.1);
            match cap.render_rgb(&st.world) {
                Ok(rgb) => {
                    if want_frame {
                        if let Some(rc) = &mut st.rec {
                            match rc.rec.push(&rgb) {
                                Ok(()) => {
                                    st.rec_text.set(format!("REC {:02}:{:02}", rc.rec.frames / FPS / 60, rc.rec.frames / FPS % 60));
                                }
                                Err(e) => {
                                    st.status = format!("Recording stopped: {e:#}");
                                    st.rec = None;
                                }
                            }
                        }
                    }
                    if st.shot_pending {
                        st.status = save_png(st, &rgb);
                    }
                }
                Err(e) => st.status = format!("Capture failed: {e:#}"),
            }
            st.rec_due = false;
            st.shot_pending = false;
        }

        f.ui.prepare(f.device, f.queue, &mut f.rt.text.atlas, &f.rt.instances, f.size);
        let clear = f.rt.clear_color();
        let gpu = self.gpu.as_mut().unwrap();
        f.ui.draw_with(f.encoder, f.target, clear, &mut |pass, site| {
            if site.mark == VIEWPORT {
                let r = site.instance.rect;
                gpu.blit.draw(pass, site.format, &gpu.bind, [r[0] - site.origin[0], r[1] - site.origin[1], r[2], r[3]]);
            }
        });
    }

    fn animating(&self) -> bool {
        true
    }

    fn release_gpu(&mut self) {
        self.gpu = None;
    }
}

fn save_png(s: &State, rgb: &[u8]) -> String {
    let path = s.out_dir.join(format!("r3d_{}.png", unix()));
    let (ow, oh) = s.out;
    let big = upscale_nearest(rgb, s.res.0 as usize, s.res.1 as usize, ow as usize, oh as usize);
    match std::fs::create_dir_all(&s.out_dir).map_err(anyhow::Error::from).and_then(|()| write_png(&path, &big, ow, oh)) {
        Ok(()) => format!("Saved {}", path.display()),
        Err(e) => format!("Screenshot failed: {e:#}"),
    }
}

// ---------------------------------------------------------------- actions

fn camera(s: &mut State, cmd: CameraCmd) {
    s.world.camera_cmd(&cmd);
}

fn run(s: &mut State, step: Step) {
    s.world.run(step);
}

fn restart(s: &mut State) {
    match s.source.load() {
        Ok(w) => {
            s.scenes = w.stages.iter().map(|x| x.name.clone()).collect();
            s.world = w;
            s.status = "Restarted".into();
        }
        Err(e) => s.status = format!("Reload failed: {e:#}"),
    }
}

fn toggle_record(s: &mut State) {
    if let Some(rc) = s.rec.take() {
        let frames = rc.rec.frames;
        s.status = match rc.rec.finish() {
            Ok(()) => format!("Saved {} ({frames} frames)", rc.path.display()),
            Err(e) => format!("Recording failed: {e:#}"),
        };
        return;
    }
    let path = s.out_dir.join(format!("r3d_{}.mp4", unix()));
    let opts = RecordOpts {
        out: path.clone(),
        fps: FPS,
        out_w: s.out.0,
        out_h: s.out.1,
        src_w: s.res.0 as usize,
        src_h: s.res.1 as usize,
        audio: None,
        crf: 18,
    };
    match Recorder::start(opts) {
        Ok(rec) => {
            s.rec_text.set("REC 00:00");
            s.status = format!("Recording to {}", path.display());
            s.rec_acc = 0.0;
            s.rec = Some(Rec { rec, path });
        }
        Err(e) => s.status = format!("{e:#}"),
    }
}

fn submit_command(s: &mut State, _: &mut Tx) {
    let line = s.cmd.text.trim().to_string();
    if line.is_empty() {
        return;
    }
    s.status = match s.world.run_json(&line) {
        Ok(()) => "Sent".into(),
        Err(e) => format!("{e:#}"),
    };
    s.cmd.clear();
}

fn on_key(s: &mut State, e: &KeyEvent, _: &mut Tx) -> bool {
    match &e.key {
        Key::Text(t) if t == " " => {
            s.paused = !s.paused;
            true
        }
        Key::Text(t) if t == "r" => {
            camera(s, CameraCmd::Reset);
            true
        }
        _ => false,
    }
}

fn on_drag(s: &mut State, d: &mut Drag, tx: &mut Tx) {
    if d.phase == Phase::Down {
        s.drag_prev = (0.0, 0.0);
    }
    let (ddx, ddy) = (d.dx - s.drag_prev.0, d.dy - s.drag_prev.1);
    s.drag_prev = (d.dx, d.dy);
    let rig = &mut s.world.rig;
    match tx.button() {
        PointerButton::Secondary | PointerButton::Middle => {
            let per = rig.cam.zoom / d.w.min(d.h).max(1.0);
            rig.manual_pan(-ddx * per, -ddy * per * 1.4);
        }
        _ => rig.manual_orbit(-ddx * 0.4, ddy * 0.3),
    }
    tx.live_only();
}

fn on_wheel(s: &mut State, e: WheelEvent, tx: &mut Tx) -> bool {
    s.world.rig.manual_zoom(1.0 + e.dy.clamp(-100.0, 100.0) * 0.004);
    tx.live_only();
    true
}

// ---------------------------------------------------------------- view

fn button(label: &str, key: String, active: bool, f: impl Fn(&mut State) + 'static) -> El {
    let t = tok::theme(true);
    let bg = if active { tok::accent(0, true) } else { t.control_off };
    let fg = if active { Color::WHITE } else { t.text };
    el().key(key)
        .role("button")
        .label(label)
        .focusable(true)
        .h(28.0)
        .px(10.0)
        .grow(1.0)
        .center()
        .radius(radius::SM)
        .fill(bg)
        .child(text(label).font(type_::LABEL).color(fg))
        .on_press(f)
}

fn heading(label: &str) -> El {
    text(label).font(type_::LABEL).color(tok::theme(true).text_secondary)
}

fn viewport(s: &State) -> El {
    let aspect = s.res.0 as f32 / s.res.1 as f32;
    let mut frame = el()
        .key("viewport")
        .aspect_fit(aspect)
        .effect(VIEWPORT)
        .pointer_buttons([PointerButton::Secondary, PointerButton::Middle])
        .on_drag(on_drag)
        .on_wheel(on_wheel);
    if s.rec.is_some() {
        let badge = row()
            .key("rec")
            .absolute(10.0, 10.0)
            .px(8.0)
            .h(22.0)
            .gap(6.0)
            .center_y()
            .radius(11.0)
            .fill(Color { r: 0.0, g: 0.0, b: 0.0, a: 0.6 })
            .child(el().size(8.0, 8.0).radius(4.0).fill(Color { r: 1.0, g: 0.2, b: 0.2, a: 1.0 }))
            .child(text("REC 00:00").font(type_::LABEL).color(Color::WHITE).live_text(&s.rec_text));
        frame = frame.child(badge);
    }
    el().grow(1.0).h_full().min_w(0.0).center().padding(16.0).fill(Color { r: 0.035, g: 0.04, b: 0.055, a: 1.0 }).child(frame)
}

fn building_controls(s: &State) -> Option<El> {
    let b = s.world.buildings.first()?;
    let id = b.id.clone();
    let floors = b.floors();
    let mut show = row().gap(6.0);
    {
        let id = id.clone();
        show = show.child(button("All floors", "floor-all".into(), b.show == floors, move |s: &mut State| {
            s.world.building_cmd(&id, Some(-1), None, None, 0.0);
        }));
    }
    for f in 0..floors {
        let id = id.clone();
        let label = b.names[f].clone();
        show = show.child(button(&label, format!("floor-{f}"), b.show == f + 1 && b.show != floors, move |s: &mut State| {
            s.world.building_cmd(&id, Some(f as i32), None, None, 0.0);
        }));
    }
    let exploded = b.gap > 0.01;
    let mode = |label: &'static str, key: &'static str, name: &'static str| {
        let id = id.clone();
        button(label, key.into(), false, move |s: &mut State| {
            s.world.building_cmd(&id, None, None, Some(name), 0.0);
        })
    };
    let eid = id.clone();
    let opts = row()
        .gap(6.0)
        .child(button("Explode", "explode".into(), exploded, move |s: &mut State| {
            let gap = if s.world.buildings[0].gap > 0.01 { 0.0 } else { 3.2 };
            s.world.building_cmd(&eid, None, Some(gap), None, 1.2);
        }))
        .child(mode("Walls auto", "cut-auto", "auto"))
        .child(mode("Walls low", "cut-low", "low"))
        .child(mode("Walls full", "cut-none", "none"));
    Some(col().gap(8.0).child(heading("Building")).child(show).child(opts))
}

fn panel(s: &State) -> El {
    let t = tok::theme(true);
    let recording = s.rec.is_some();

    let formats = PRESETS.iter().enumerate().fold(row().gap(6.0), |r, (i, (label, _))| {
        r.child(button(label, format!("preset-{i}"), s.preset == i, move |s: &mut State| {
            if s.rec.is_none() {
                s.preset = i;
                (s.out, s.res) = sizes(i);
            }
        }))
    });
    let play = row()
        .gap(6.0)
        .child(button(if s.paused { "Play" } else { "Pause" }, "pause".into(), s.paused, |s: &mut State| s.paused = !s.paused))
        .child(button("Restart", "restart".into(), false, restart));
    let record = row()
        .gap(6.0)
        .child(button(if recording { "Stop recording" } else { "Record mp4" }, "record".into(), recording, toggle_record))
        .child(button("Screenshot", "shot".into(), false, |s: &mut State| s.shot_pending = true));

    let orbit = |label: &str, key: &str, dyaw: f32, dpitch: f32| {
        button(label, key.into(), false, move |s: &mut State| camera(s, CameraCmd::Orbit { dyaw, dpitch, secs: 0.5, ease: None }))
    };
    let zoom = |label: &str, key: &str, factor: f32| {
        button(label, key.into(), false, move |s: &mut State| camera(s, CameraCmd::Zoom { factor, secs: 0.4, ease: None }))
    };
    let cam1 = row().gap(6.0).child(orbit("Left", "orbit-l", -45.0, 0.0)).child(orbit("Right", "orbit-r", 45.0, 0.0)).child(orbit("Up", "tilt-u", 0.0, 8.0)).child(orbit("Down", "tilt-d", 0.0, -8.0));
    let cam2 = row()
        .gap(6.0)
        .child(zoom("Zoom in", "zoom-in", 1.4))
        .child(zoom("Zoom out", "zoom-out", 1.0 / 1.4))
        .child(button("Ortho/3D", "proj".into(), false, |s: &mut State| {
            let c = &mut s.world.rig.cam;
            c.ortho = !c.ortho;
        }));
    let cam3 = row()
        .gap(6.0)
        .child(button("Spin", "spin".into(), s.world.rig.spin != 0.0, |s: &mut State| {
            let dps = if s.world.rig.spin != 0.0 { 0.0 } else { 12.0 };
            camera(s, CameraCmd::Spin { dps });
        }))
        .child(button("Reset", "cam-reset".into(), false, |s: &mut State| camera(s, CameraCmd::Reset)))
        .child(button("Free", "unfollow".into(), false, |s: &mut State| camera(s, CameraCmd::Unfollow)));
    let follow = s.actors.iter().fold(row().gap(6.0), |r, (id, label)| {
        let id = id.clone();
        r.child(button(label, format!("follow-{id}"), s.world.rig.follow.as_ref().is_some_and(|(f, _)| *f == id), move |s: &mut State| {
            camera(s, CameraCmd::Follow { actor: id.clone(), smooth: None })
        }))
    });

    let scenes = s.scenes.iter().fold(row().gap(6.0), |r, name| {
        let n = name.clone();
        r.child(button(name, format!("scene-{name}"), false, move |s: &mut State| run(s, Step::PlayScene { name: n.clone() })))
    });
    let sky = ["day", "dawn", "sunset", "night"].iter().fold(row().gap(6.0), |r, p| {
        let preset = p.to_string();
        r.child(button(p, format!("sky-{p}"), false, move |s: &mut State| run(s, Step::SetSky { preset: preset.clone(), secs: Some(2.0) })))
    });

    let field = text_input(&s.cmd, type_::BODY)
        .key("cmd")
        .label("JSON command")
        .single_line()
        .grow(1.0)
        .color(t.text)
        .placeholder("{\"do\":\"say\",\"actor\":\"ani\",\"text\":\"HAI\"}", t.text_secondary)
        .on_edit(|s: &mut State| &mut s.cmd)
        .on_submit(submit_command);
    let command = row().h(32.0).px(10.0).center_y().radius(radius::SM).fill(t.control_off).child(field);

    let mut body = col()
        .gap(8.0)
        .child(text("r3dsims live").font(type_::TITLE).color(t.text))
        .child(text(s.world.name.clone()).font(type_::LABEL).color(t.text_secondary))
        .child(text("").font(type_::LABEL).color(t.text_secondary).live_text(&s.info).h(16.0).w_full())
        .child(heading("Format"))
        .child(formats)
        .child(heading("Playback"))
        .child(play)
        .child(heading("Recording"))
        .child(record);
    if let Some(b) = building_controls(s) {
        body = body.child(b);
    }
    body = body
        .child(heading("Camera"))
        .child(cam1)
        .child(cam2)
        .child(cam3)
        .child(follow)
        .child(heading("Scenes"))
        .child(scenes)
        .child(heading("Sky"))
        .child(sky)
        .child(heading("Command (same JSON as story steps)"))
        .child(command)
        .child(text(s.status.clone()).font(type_::LABEL).color(t.text_secondary).w_full().max_lines(4))
        .child(text("Drag orbit · right-drag pan · wheel zoom · Space pause · R reset").font(type_::LABEL).color(t.text_secondary).w_full().max_lines(3));

    col().w(330.0).h_full().padding(14.0).fill(t.card).child(body.scroll(ScrollSpec { stick_bottom: false }).shrink().min_h(0.0))
}

fn view(s: &State, _: &mut Cx<State>) -> El {
    row().w_full().h_full().fill(tok::theme(true).bg).child(viewport(s)).child(panel(s))
}

fn main() {
    rune::run_with(
        AppSpec::<State> {
            poll: Some(poll),
            on_key: Some(on_key),
            min_size: Some((900.0, 620.0)),
            ..AppSpec::new("r3dsims live", (1240.0, 800.0), init, view)
        },
        Scene3d::default(),
    );
}
