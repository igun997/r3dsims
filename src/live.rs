//! `r3dsims live`: runs a war diorama in real time and delivers the picture and the control surface.
//!
//! Delivery is deliberately plain so another application can own the platform connector:
//!   * webhooks in:   `POST /webhook` (gift, like, comment ... JSON), `/hooks`, `/overlay`, `/control`, `/command`
//!   * picture out:   `GET /stream.mjpg` (multipart JPEG), `GET /view` (page for a browser source),
//!                    `GET /frame.jpg`, and optionally `--out TARGET` through ffmpeg (rtmp://, srt://,
//!                    udp://, a file, or `-` for MPEG-TS on stdout)
//!   * introspection: `GET /state`, `/hooks`, `/overlay`, `/health`

use std::io::Read;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail};
use serde_json::{Value, json};
use tiny_http::{Header, Method, Request, Response, Server, StatusCode};

use crate::gpu::{Gpu, SceneRenderer};
use crate::history::History;
use crate::overlay::{Compositor, Drawable};
use crate::output::{Output, join_target};
use crate::record::upscale_nearest;
use crate::tiktok::{TikTok, TikTokOpts};
use crate::show::{Show, load_show};
use crate::war::{Event, Hooks};
use crate::world::World;

pub struct LiveOpts {
    /// the show file: `war.json` (a game) or `studio.json` (any scene, no game)
    pub show: PathBuf,
    pub listen: String,
    pub token: Option<String>,
    pub out_w: u32,
    pub out_h: u32,
    pub pixel: u32,
    /// clay style (smooth hand-made figures) instead of voxel cubes
    pub clay: bool,
    /// append results of every round to this JSON-lines file (None = no history)
    pub history: Option<PathBuf>,
    pub fps: u32,
    pub mjpeg: bool,
    pub quality: u8,
    /// ffmpeg output target (`rtmp://...`, `srt://...`, a file, `-`)
    pub out: Option<String>,
    pub bitrate_k: u32,
    pub emoji_dir: Option<PathBuf>,
    /// stop after this many simulated seconds (tests, CI)
    pub seconds: Option<f32>,
    /// pace to wall-clock; false renders as fast as possible
    pub realtime: bool,
    pub log_events: bool,
    /// TikTok LIVE account to follow from the start (the dashboard can change it while running)
    pub tiktok: TikTokOpts,
    /// accept connections from other machines without a token (the dashboard can start a broadcast, so this is refused by default)
    pub allow_no_token: bool,
}

pub enum Msg {
    Events(Vec<Event>),
    Publish(Hooks),
    Overlay(Value),
    Control(String),
    Command(String),
    Output(Value),
    Vars(Value),
}

pub struct Req {
    pub msg: Msg,
    pub reply: Sender<Value>,
}

// ---------------------------------------------------------------- frame hub

struct HubInner {
    seq: u64,
    w: usize,
    h: usize,
    rgb: Option<Arc<Vec<u8>>>,
    jpeg: Option<(u64, Arc<Vec<u8>>)>,
}

pub struct FrameHub {
    inner: Mutex<HubInner>,
    cv: Condvar,
    quality: u8,
}

impl FrameHub {
    fn new(quality: u8) -> FrameHub {
        FrameHub { inner: Mutex::new(HubInner { seq: 0, w: 0, h: 0, rgb: None, jpeg: None }), cv: Condvar::new(), quality }
    }

    fn publish(&self, rgb: Arc<Vec<u8>>, w: usize, h: usize) {
        let mut g = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        g.seq += 1;
        g.w = w;
        g.h = h;
        g.rgb = Some(rgb);
        g.jpeg = None;
        self.cv.notify_all();
    }

    /// The JPEG of the first frame newer than `after`, encoded once per frame however many viewers.
    fn next_jpeg(&self, after: u64, timeout: Duration) -> Option<(u64, Arc<Vec<u8>>)> {
        let deadline = Instant::now() + timeout;
        let mut g = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        while g.seq <= after || g.rgb.is_none() {
            let left = deadline.checked_duration_since(Instant::now())?;
            g = self.cv.wait_timeout(g, left).unwrap_or_else(|e| e.into_inner()).0;
        }
        self.jpeg_locked(g)
    }

    fn latest_jpeg(&self) -> Option<(u64, Arc<Vec<u8>>)> {
        let g = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        if g.rgb.is_none() {
            return None;
        }
        self.jpeg_locked(g)
    }

    fn jpeg_locked(&self, g: std::sync::MutexGuard<'_, HubInner>) -> Option<(u64, Arc<Vec<u8>>)> {
        if let Some((s, j)) = &g.jpeg {
            if *s == g.seq {
                return Some((*s, j.clone()));
            }
        }
        let (seq, w, h, rgb) = (g.seq, g.w, g.h, g.rgb.clone()?);
        drop(g);
        let mut out = Vec::with_capacity(w * h / 6);
        jpeg_encoder::Encoder::new(&mut out, self.quality).encode(&rgb, w as u16, h as u16, jpeg_encoder::ColorType::Rgb).ok()?;
        let out = Arc::new(out);
        let mut g = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        if g.seq == seq {
            g.jpeg = Some((seq, out.clone()));
        }
        Some((seq, out))
    }
}

struct MjpegReader {
    sh: Arc<Shared>,
    last: u64,
    buf: Vec<u8>,
    pos: usize,
}

impl Read for MjpegReader {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        if self.pos >= self.buf.len() {
            match self.sh.hub.next_jpeg(self.last, Duration::from_secs(15)) {
                Some((seq, jpg)) => {
                    self.last = seq;
                    self.buf.clear();
                    self.buf.extend_from_slice(format!("--frame\r\nContent-Type: image/jpeg\r\nContent-Length: {}\r\n\r\n", jpg.len()).as_bytes());
                    self.buf.extend_from_slice(&jpg);
                    self.buf.extend_from_slice(b"\r\n");
                    self.pos = 0;
                }
                None => return Ok(0),
            }
        }
        let n = out.len().min(self.buf.len() - self.pos);
        out[..n].copy_from_slice(&self.buf[self.pos..self.pos + n]);
        self.pos += n;
        Ok(n)
    }
}

// ---------------------------------------------------------------- http

struct Shared {
    hub: FrameHub,
    state: Mutex<Value>,
    hooks: Mutex<Value>,
    overlay: Mutex<Value>,
    token: Option<String>,
    history: Option<Arc<History>>,
    tiktok: TikTok,
    output: Mutex<Value>,
    tx: Mutex<Sender<Req>>,
}

fn header(k: &str, v: &str) -> Header {
    Header::from_bytes(k.as_bytes(), v.as_bytes()).expect("static header")
}

fn json_response(code: u16, v: &Value) -> Response<std::io::Cursor<Vec<u8>>> {
    Response::from_string(v.to_string()).with_status_code(StatusCode(code)).with_header(header("Content-Type", "application/json")).with_header(header("Access-Control-Allow-Origin", "*"))
}

fn text_response(code: u16, ct: &str, body: String) -> Response<std::io::Cursor<Vec<u8>>> {
    Response::from_string(body).with_status_code(StatusCode(code)).with_header(header("Content-Type", ct))
}

const INDEX: &str = "<!doctype html><meta charset=utf-8><title>r3dsims live</title><body style=\"font:14px/1.5 monospace;background:#111;color:#ddd;padding:2em\"><h3>r3dsims live</h3>\
<p><a href=/view style=color:#8cf>/view</a> picture &middot; <a href=/stream.mjpg style=color:#8cf>/stream.mjpg</a> &middot; <a href=/frame.jpg style=color:#8cf>/frame.jpg</a> &middot; \
<a href=/state style=color:#8cf>/state</a> &middot; <a href=/hooks style=color:#8cf>/hooks</a> &middot; <a href=/overlay style=color:#8cf>/overlay</a></p>\
<pre>POST /webhook   {\"type\":\"gift\",\"user\":\"Budi\",\"gift\":\"Rose\",\"count\":3}   (object, array or {\"events\":[..]})\n\
POST /hooks     {\"triggers\":[..],\"macros\":{..},\"actions\":{..},\"bots\":[..],\"remove\":[..],\"replace\":false}\n\
POST /overlay   {\"add\":[{\"type\":\"emoji\",\"emoji\":\":rose:\"}],\"remove\":[\"id\"],\"patch\":{\"id\":{..}},\"clear\":false}\n\
POST /control   {\"op\":\"start|next_round|reset|stop|pause|resume\"}\n\
POST /command   any story/camera step, e.g. {\"op\":\"orbit\",\"dyaw\":90,\"secs\":2}\n\
auth: Authorization: Bearer TOKEN | X-Token: TOKEN | ?token=TOKEN (when --token is set)</pre>";

const VIEW: &str = "<!doctype html><meta charset=utf-8><title>r3dsims</title><style>html,body{margin:0;height:100%;background:#000}img{width:100%;height:100%;object-fit:contain;image-rendering:pixelated}</style><img src=/stream.mjpg>";

/// The dashboard: the picture and, beside it, the TikTok connection, the video output, the match, a test bench and the other streams.
const DASHBOARD: &str = include_str!("dashboard.html");

fn authorised(req: &Request, sh: &Shared, query: &str) -> bool {
    let Some(tok) = sh.token.as_deref() else { return true };
    for h in req.headers() {
        let name = h.field.as_str().as_str().to_ascii_lowercase();
        let val = h.value.as_str();
        if (name == "authorization" && val.strip_prefix("Bearer ").is_some_and(|t| t == tok)) || (name == "x-token" && val == tok) {
            return true;
        }
    }
    query.split('&').any(|kv| kv.split_once('=').is_some_and(|(k, v)| k == "token" && v == tok))
}

fn ask(sh: &Shared, msg: Msg) -> Value {
    let (reply, rx) = mpsc::channel();
    let sent = sh.tx.lock().map(|tx| tx.send(Req { msg, reply })).is_ok_and(|r| r.is_ok());
    if !sent {
        return json!({"ok": false, "error": "stream is shutting down"});
    }
    rx.recv_timeout(Duration::from_secs(5)).unwrap_or_else(|_| json!({"ok": false, "error": "no answer from the simulation within 5s"}))
}

fn parse_events(body: &str) -> Result<Vec<Event>> {
    let v: Value = serde_json::from_str(body).context("body is not JSON")?;
    let list = match v {
        Value::Array(a) => a,
        Value::Object(mut o) if o.contains_key("events") => match o.remove("events") {
            Some(Value::Array(a)) => a,
            _ => bail!("\"events\" must be an array"),
        },
        other => vec![other],
    };
    if list.is_empty() {
        bail!("no events in body");
    }
    list.iter().enumerate().map(|(i, e)| Event::from_json(e).with_context(|| format!("event #{i}"))).collect()
}

fn handle(mut req: Request, sh: Arc<Shared>) {
    let url = req.url().to_string();
    let (path, query) = url.split_once('?').unwrap_or((url.as_str(), ""));
    let method = req.method().clone();
    let resp = match (&method, path) {
        (Method::Get, "/") => text_response(200, "text/html; charset=utf-8", INDEX.into()),
        (Method::Get, "/view") if query.split('&').any(|q| q == "panel" || q == "test" || q.starts_with("panel=") || q.starts_with("test=")) => text_response(200, "text/html; charset=utf-8", DASHBOARD.into()),
        (Method::Get, "/view") => text_response(200, "text/html; charset=utf-8", VIEW.into()),
        (Method::Get, "/dashboard" | "/test" | "/panel") => text_response(200, "text/html; charset=utf-8", DASHBOARD.into()),
        (Method::Get, "/tiktok") => json_response(200, &sh.tiktok.snapshot()),
        (Method::Get, "/output") => json_response(200, &sh.output.lock().map(|v| v.clone()).unwrap_or(Value::Null)),
        (Method::Get, "/history") => match &sh.history {
            Some(h) => json_response(200, &h.summary()),
            None => json_response(200, &json!({"enabled": false, "sessions": [], "totals": {}})),
        },
        (Method::Get, "/health") => text_response(200, "text/plain", "ok".into()),
        (Method::Get, "/state") => json_response(200, &sh.state.lock().map(|v| v.clone()).unwrap_or(Value::Null)),
        (Method::Get, "/hooks") => json_response(200, &sh.hooks.lock().map(|v| v.clone()).unwrap_or(Value::Null)),
        (Method::Get, "/overlay") => json_response(200, &sh.overlay.lock().map(|v| v.clone()).unwrap_or(Value::Null)),
        (Method::Get, "/frame.jpg") => match sh.hub.latest_jpeg() {
            Some((_, j)) => Response::from_data((*j).clone()).with_header(header("Content-Type", "image/jpeg")).with_header(header("Cache-Control", "no-store")),
            None => text_response(503, "text/plain", "no frame yet".into()),
        },
        (Method::Get, "/stream.mjpg") => {
            let reader = MjpegReader { sh: sh.clone(), last: 0, buf: vec![], pos: 0 };
            let r = Response::new(
                StatusCode(200),
                vec![header("Content-Type", "multipart/x-mixed-replace; boundary=frame"), header("Cache-Control", "no-store"), header("Access-Control-Allow-Origin", "*")],
                reader,
                None,
                None,
            );
            let _ = req.respond(r);
            return;
        }
        (Method::Options, _) => text_response(204, "text/plain", String::new())
            .with_header(header("Access-Control-Allow-Origin", "*"))
            .with_header(header("Access-Control-Allow-Headers", "authorization, content-type, x-token"))
            .with_header(header("Access-Control-Allow-Methods", "GET, POST, OPTIONS")),
        (Method::Post, "/webhook" | "/event" | "/events" | "/hooks" | "/overlay" | "/control" | "/command" | "/tiktok" | "/output" | "/vars") => {
            if !authorised(&req, &sh, query) {
                let _ = req.respond(json_response(401, &json!({"ok": false, "error": "missing or wrong token"})));
                return;
            }
            let mut body = String::new();
            if req.as_reader().take(1 << 20).read_to_string(&mut body).is_err() {
                let _ = req.respond(json_response(400, &json!({"ok": false, "error": "body is not valid UTF-8 text"})));
                return;
            }
            let (code, v) = route_post(&sh, path, &body);
            json_response(code, &v)
        }
        _ => json_response(404, &json!({"ok": false, "error": format!("no route {} {}", method, path)})),
    };
    let _ = req.respond(resp);
}

fn route_post(sh: &Shared, path: &str, body: &str) -> (u16, Value) {
    let bad = |e: anyhow::Error| (400, json!({"ok": false, "error": format!("{e:#}")}));
    if path == "/tiktok" {
        let v: Value = match serde_json::from_str(body) {
            Ok(v) => v,
            Err(e) => return bad(anyhow!("tiktok JSON: {e}")),
        };
        return match v.get("op").and_then(Value::as_str).unwrap_or("connect") {
            "connect" => match sh.tiktok.connect(v.get("username").and_then(Value::as_str).unwrap_or("")) {
                Ok(()) => (200, json!({"ok": true})),
                Err(e) => (400, json!({"ok": false, "error": e})),
            },
            "disconnect" => {
                sh.tiktok.disconnect();
                (200, json!({"ok": true}))
            }
            other => (400, json!({"ok": false, "error": format!("unknown tiktok op '{other}' (connect, disconnect)")})),
        };
    }
    let msg = match path {
        "/webhook" | "/event" | "/events" => match parse_events(body) {
            Ok(e) => Msg::Events(e),
            Err(e) => return bad(e),
        },
        "/hooks" => match serde_json::from_str::<Hooks>(body) {
            Ok(h) => Msg::Publish(h),
            Err(e) => return bad(anyhow!("hooks JSON: {e}")),
        },
        "/overlay" => match serde_json::from_str::<Value>(body) {
            Ok(v) => Msg::Overlay(v),
            Err(e) => return bad(anyhow!("overlay JSON: {e}")),
        },
        "/control" => {
            let op = serde_json::from_str::<Value>(body).ok().and_then(|v| v.get("op").and_then(Value::as_str).map(str::to_string)).unwrap_or_else(|| body.trim().trim_matches('"').to_string());
            Msg::Control(op)
        }
        "/vars" => match serde_json::from_str::<Value>(body) {
            Ok(v) => Msg::Vars(v),
            Err(e) => return bad(anyhow!("vars JSON: {e}")),
        },
        "/output" => match serde_json::from_str::<Value>(body) {
            Ok(v) => Msg::Output(v),
            Err(e) => return bad(anyhow!("output JSON: {e}")),
        },
        _ => Msg::Command(body.to_string()),
    };
    let v = ask(sh, msg);
    let ok = v.get("ok").and_then(Value::as_bool).unwrap_or(true);
    (if ok { 200 } else { 400 }, v)
}

// ---------------------------------------------------------------- the stream

pub struct Live {
    opts: LiveOpts,
    world: World,
    show: Box<dyn Show>,
    shared: Arc<Shared>,
    rx: Receiver<Req>,
    addr: SocketAddr,
    base_dir: PathBuf,
    history: Option<Arc<History>>,
    output: Output,
}

impl Live {
    /// Loads the show and its scene, starts the HTTP server. No GPU yet.
    pub fn start(opts: LiveOpts) -> Result<Live> {
        let (mut world, show) = load_show(&opts.show)?;
        world.clay = opts.clay;
        if opts.out_h > opts.out_w {
            // a vertical picture (TikTok): the scene camera was authored for a wide one
            let t = &world.terrain;
            let (yaw, pitch, zoom) = portrait_camera(t.w as f32 * t.cell, t.d as f32 * t.cell, opts.out_w as f32 / opts.out_h as f32);
            let mut cam = world.rig.cam;
            (cam.yaw, cam.pitch, cam.zoom) = (yaw, pitch, zoom);
            world.rig.cam = cam;
            world.rig.home = cam;
        }
        // a history of wins and losses only makes sense for a show that has camps
        let history = opts.history.clone().and_then(|p| show.camps().map(|pair| Arc::new(History::open(p, &show.title(), pair))));
        let base_dir = opts.show.parent().map(|p| p.to_path_buf()).unwrap_or_default();
        if opts.token.as_deref().is_none_or(str::is_empty) && !opts.allow_no_token && !listen_is_loopback(&opts.listen) {
            bail!("--listen {} reaches other machines but there is no --token: the dashboard can start a broadcast and change the stream. Add --token SECRET (or --allow-no-token to accept the risk).", opts.listen);
        }
        let output = Output::new(opts.out_w, opts.out_h, opts.fps.max(1));
        let server = Server::http(&opts.listen).map_err(|e| anyhow!("listen on {}: {e}", opts.listen))?;
        let addr = server.server_addr().to_ip().ok_or_else(|| anyhow!("server has no IP address"))?;
        let (tx, rx) = mpsc::channel();
        // the TikTok connector hands its events to the simulation like a webhook does, without waiting for an answer
        let sink_tx = Arc::new(Mutex::new(tx.clone()));
        let sink: crate::tiktok::Sink = Arc::new(move |events| {
            let (reply, _answer) = mpsc::channel();
            if let Ok(tx) = sink_tx.lock() {
                let _ = tx.send(Req { msg: Msg::Events(events), reply });
            }
        });
        let tiktok = TikTok::start(opts.tiktok.clone(), sink);
        let shared = Arc::new(Shared {
            hub: FrameHub::new(opts.quality.clamp(10, 100)),
            state: Mutex::new(show.state_json()),
            hooks: Mutex::new(show.hooks_json()),
            overlay: Mutex::new(json!([])),
            token: opts.token.clone().filter(|t| !t.is_empty()),
            history: history.clone(),
            tiktok,
            output: Mutex::new(json!({"state": "off"})),
            tx: Mutex::new(tx),
        });
        let sh = shared.clone();
        std::thread::spawn(move || {
            for req in server.incoming_requests() {
                let sh = sh.clone();
                std::thread::spawn(move || handle(req, sh));
            }
        });
        Ok(Live { opts, world, show, shared, rx, addr, base_dir, history, output })
    }

    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// `{"op":"start","server":"rtmp://...","key":"...","bitrate":3500}` (or `"url"`), `{"op":"stop"}`, `{"op":"status"}`.
    /// From the dashboard only network targets are accepted: a file path would let a remote client overwrite files.
    fn output_cmd(&mut self, v: &Value) -> Value {
        let op = v.get("op").and_then(Value::as_str).unwrap_or("start");
        match op {
            "start" => {
                let target = match v.get("url").and_then(Value::as_str) {
                    Some(u) if !u.trim().is_empty() => u.trim().to_string(),
                    _ => join_target(v.get("server").and_then(Value::as_str).unwrap_or(""), v.get("key").and_then(Value::as_str).unwrap_or("")),
                };
                let lower = target.to_ascii_lowercase();
                if !["rtmp://", "rtmps://", "srt://", "udp://", "tcp://", "rtp://"].iter().any(|s| lower.starts_with(s)) {
                    return json!({"ok": false, "error": "the target must start with rtmp://, rtmps://, srt://, udp://, tcp:// or rtp:// (files and stdout are set with --out at start)"});
                }
                let bitrate = v.get("bitrate").and_then(Value::as_u64).map_or(self.opts.bitrate_k, |b| b.min(u64::from(u32::MAX)) as u32);
                if let Err(e) = self.output.start(&target, bitrate) {
                    return json!({"ok": false, "error": format!("{e:#}")});
                }
            }
            "stop" => self.output.stop(),
            "status" => {}
            other => return json!({"ok": false, "error": format!("unknown output op '{other}' (start, stop, status)")}),
        }
        let st = self.output.status();
        *self.shared.output.lock().unwrap_or_else(|e| e.into_inner()) = st.clone();
        json!({"ok": true, "output": st})
    }

    fn handle_msg(&mut self, msg: Msg) -> Value {
        match msg {
            Msg::Events(list) => {
                let mut results = vec![];
                for ev in list {
                    let r = self.show.handle_event(&mut self.world, ev.clone());
                    if self.opts.log_events {
                        eprintln!("[event] {} {} {} x{} -> {}", ev.kind, ev.user, ev.gift, ev.count, r);
                    }
                    results.push(r);
                }
                json!({"ok": true, "results": results})
            }
            Msg::Publish(h) => match self.show.publish(&self.world, h) {
                Ok(v) => {
                    *self.shared.hooks.lock().unwrap_or_else(|e| e.into_inner()) = self.show.hooks_json();
                    let mut v = v;
                    v["ok"] = json!(true);
                    v
                }
                Err(e) => json!({"ok": false, "error": format!("{e:#}")}),
            },
            Msg::Overlay(v) => match self.show.overlay_op(&v) {
                Ok(mut r) => {
                    r["ok"] = json!(true);
                    r
                }
                Err(e) => json!({"ok": false, "error": format!("{e:#}")}),
            },
            Msg::Control(op) => match self.show.control(&mut self.world, &op) {
                Ok(mut r) => {
                    r["ok"] = json!(true);
                    r
                }
                Err(e) => json!({"ok": false, "error": format!("{e:#}")}),
            },
            Msg::Command(line) => match self.world.run_json(&line) {
                Ok(()) => json!({"ok": true}),
                Err(e) => json!({"ok": false, "error": format!("{e:#}")}),
            },
            Msg::Output(v) => self.output_cmd(&v),
            Msg::Vars(v) => match self.show.set_vars(&v) {
                Ok(mut r) => {
                    r["ok"] = json!(true);
                    r
                }
                Err(e) => json!({"ok": false, "error": format!("{e:#}")}),
            },
        }
    }

    /// Runs until `seconds` (if set) or forever. Returns the final state.
    pub fn run(mut self) -> Result<Value> {
        let (ow, oh) = (self.opts.out_w as usize, self.opts.out_h as usize);
        let pixel = self.opts.pixel.max(1) as usize;
        let (rw, rh) = ((ow / pixel).max(16), (oh / pixel).max(16));
        let gpu = Gpu::headless()?;
        eprintln!("gpu: {}", gpu.adapter);
        let mut renderer = SceneRenderer::new(&gpu.device, &gpu.queue, rw as u32, rh as u32);
        let mut comp = Compositor::new(self.base_dir.clone(), self.opts.emoji_dir.clone());
        if let Some(t) = self.opts.out.clone() {
            self.output.start(&t, self.opts.bitrate_k)?;
        }
        let fps = self.opts.fps.max(1);
        let base_dt = 1.0 / fps as f32;
        let frame_len = Duration::from_secs_f32(base_dt);
        eprintln!(
            "live: {}x{} (render {}x{}) @ {} fps, listening on http://{}  [picture: /view, dashboard: /dashboard, webhook: POST /webhook]",
            ow, oh, rw, rh, fps, self.addr
        );
        let started = Instant::now();
        let mut next = started;
        let mut last = started;
        let mut sim = 0.0f32;
        let mut state_t = 0.0f32;
        // rolling stats over ~2 s of wall clock
        let (mut n_frames, mut ms_render, mut ms_upscale, mut ms_compose, mut ms_total) = (0u32, 0.0f32, 0.0f32, 0.0f32, 0.0f32);
        let mut stats_since = Instant::now();
        let mut perf = json!({"fps": fps, "ms": {}});
        let mut slow_warned = Instant::now() - Duration::from_secs(60);
        let mut heartbeat = Instant::now();
        loop {
            let frame_start = Instant::now();
            // Paced live: the world follows the wall clock, so a slow machine drops frames instead of
            // playing in slow motion. Unpaced runs (tests) step a fixed 1/fps.
            let dt = if self.opts.realtime { frame_start.duration_since(last).as_secs_f32().clamp(base_dt, 0.1) } else { base_dt };
            last = frame_start;
            while let Ok(req) = self.rx.try_recv() {
                let v = self.handle_msg(req.msg);
                let _ = req.reply.send(v);
            }
            self.output.poll();
            self.show.update(dt, &mut self.world);
            for ev in self.show.take_log() {
                if let Some(h) = &self.history {
                    h.append(ev);
                }
            }
            if self.history.is_some() && heartbeat.elapsed() >= Duration::from_secs(30) {
                heartbeat = Instant::now();
                if let Some(h) = &self.history {
                    h.append(json!({"ev": "hb"}));
                }
            }
            self.world.update(dt);
            sim += dt;

            let t0 = Instant::now();
            let low = renderer.render_rgb(&self.world)?;
            let t1 = Instant::now();
            let mut frame = upscale_nearest(&low, rw, rh, ow, oh);
            let t2 = Instant::now();
            let vars = self.show.vars();
            let hud_items = self.show.hud(ow, oh);
            let mut items: Vec<Drawable> = hud_items.iter().map(|(e, age, left)| Drawable { el: e, age: *age, left: *left }).collect();
            items.extend(self.show.overlay().drawables());
            items.sort_by_key(|d| d.el.z);
            comp.draw(&mut frame, ow, oh, &items, &vars);
            drop(items);
            let t3 = Instant::now();
            let frame = Arc::new(frame);
            self.output.push(&frame);
            if self.opts.mjpeg {
                self.shared.hub.publish(frame, ow, oh);
            }

            n_frames += 1;
            ms_render += (t1 - t0).as_secs_f32() * 1e3;
            ms_upscale += (t2 - t1).as_secs_f32() * 1e3;
            ms_compose += (t3 - t2).as_secs_f32() * 1e3;
            ms_total += (Instant::now() - frame_start).as_secs_f32() * 1e3;
            if stats_since.elapsed() >= Duration::from_secs(2) {
                let n = n_frames.max(1) as f32;
                let actual = n_frames as f32 / stats_since.elapsed().as_secs_f32();
                let r = |x: f32| (x / n * 10.0).round() / 10.0;
                perf = json!({"fps": (actual * 10.0).round() / 10.0, "target_fps": fps, "ms": {"render": r(ms_render), "upscale": r(ms_upscale), "overlay": r(ms_compose), "frame": r(ms_total)}});
                if self.opts.realtime && actual < fps as f32 * 0.9 && slow_warned.elapsed() > Duration::from_secs(30) {
                    eprintln!("[warn] rendering at {actual:.1} fps, below the {fps} fps target (frame {} ms, of which GPU render {} ms). Try a bigger --pixel, a smaller --size or a lower --fps.", r(ms_total), r(ms_render));
                    slow_warned = Instant::now();
                }
                (n_frames, ms_render, ms_upscale, ms_compose, ms_total) = (0, 0.0, 0.0, 0.0, 0.0);
                stats_since = Instant::now();
            }

            state_t += dt;
            if state_t >= 0.25 {
                state_t = 0.0;
                let mut st = self.show.state_json();
                st["time"] = json!((sim * 10.0).round() / 10.0);
                st["perf"] = perf.clone();
                st["tiktok"] = serde_json::to_value(self.shared.tiktok.status()).unwrap_or(Value::Null);
                let out = self.output.status();
                *self.shared.output.lock().unwrap_or_else(|e| e.into_inner()) = out.clone();
                st["output"] = out;
                *self.shared.state.lock().unwrap_or_else(|e| e.into_inner()) = st;
                *self.shared.overlay.lock().unwrap_or_else(|e| e.into_inner()) = serde_json::to_value(self.show.overlay().list()).unwrap_or(Value::Null);
            }

            if self.opts.seconds.is_some_and(|s| sim >= s) {
                break;
            }
            if self.opts.realtime {
                next += frame_len;
                let now = Instant::now();
                if next > now {
                    std::thread::sleep(next - now);
                } else if now - next > Duration::from_millis(500) {
                    next = now;
                }
            }
        }
        self.output.finish();
        let mut st = self.show.state_json();
        st["time"] = json!(sim);
        Ok(st)
    }
}

/// `host:port`, `[::1]:port`, `localhost:port`: does the listen address stay on this machine?
fn listen_is_loopback(listen: &str) -> bool {
    let host = listen.rsplit_once(':').map_or(listen, |(h, _)| h).trim_matches(['[', ']']);
    host.eq_ignore_ascii_case("localhost") || host.parse::<std::net::IpAddr>().is_ok_and(|ip| ip.is_loopback())
}

/// Camera for a vertical picture: turn it so the long side of the arena runs up the screen and make the whole arena fit
/// between the HUD at the top and the gift menu at the bottom (about half of the height is free). Returns
/// `(yaw, pitch, zoom)`; `zoom` is the number of world units across the shorter side, the width.
pub fn portrait_camera(width: f32, depth: f32, aspect: f32) -> (f32, f32, f32) {
    let pitch = 52.0f32;
    let (yaw, long, short) = if width >= depth { (90.0, width, depth) } else { (0.0, depth, width) };
    // vertical extent on the screen of the long side seen from `pitch`, plus a little for the height of things
    let on_screen = long * pitch.to_radians().sin() + 4.0;
    let zoom = (aspect * on_screen / 0.5).max(short * 1.12);
    (yaw, pitch, zoom)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_vertical_picture_turns_the_arena_and_leaves_room_for_the_hud() {
        // the templates' 52 x 30 tile arena on a 9:16 screen
        let (yaw, pitch, zoom) = portrait_camera(52.0, 30.0, 720.0 / 1280.0);
        assert_eq!(yaw, 90.0, "the long side (x) runs up the screen");
        let across = zoom; // world units across the width
        let up = zoom / (720.0 / 1280.0); // world units up the height
        assert!(across >= 30.0 * 1.1, "the short side fits across: {across}");
        assert!(52.0 * pitch.to_radians().sin() <= up * 0.55, "the long side fits in about half of the height: {up}");
        // a map that is taller than wide needs no turn
        assert_eq!(portrait_camera(30.0, 52.0, 0.5625).0, 0.0);
    }
}
