//! Vector stage: a scene made of SVG layers that move, rendered on the CPU (resvg + tiny-skia). Crisp at any resolution,
//! no GPU needed. Authored as text, like a Lottie file: markup for the pictures, JSON for how they move.
//!
//! ```json
//! {
//!   "width": 720, "height": 1280, "background": "#9fd3f0",
//!   "layers": [
//!     {"id": "sky", "src": "svg/sky.svg", "x": 360, "y": 640, "w": 720, "h": 1280, "parallax": 0},
//!     {"id": "tree", "src": "svg/tree.svg", "x": 100, "y": 700, "anchor": [0.5, 1], "z": 5,
//!      "anim": [{"preset": "sway", "amp": 2.5, "secs": 4}]}
//!   ]
//! }
//! ```
//!
//! Positions are in design units (`width` x `height`, default 720 x 1280); the picture is scaled to the output size.
//! `x`/`y` is where the layer's `anchor` (fractions of its own picture, default the centre) sits.
//!
//! Animations are looping tracks laid over the layer's own values: for `x`, `y` and `rot` (degrees) the value is added, for
//! `scale` and `opacity` it multiplies. `preset`: `bob` (y), `sway` (rot), `pulse` (scale), `drift` (x), `blink` (opacity),
//! `flicker` (opacity), `spin` (rot, a full turn per `secs`); or `from`/`to`, or `keys` (`[[t, value], ...]`, t from 0 to 1).
//! `repeat`: `loop` (default), `pingpong`, `once`.
//!
//! A running show changes the stage with `{"do": "stage", "op": ...}` steps: `add`, `remove`, `set` (any property, numbers
//! tween when `secs` is given), `pop` (spring in), `camera`, `anim`.

use std::collections::{HashMap, HashSet};
use std::f32::consts::{PI, TAU};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result, anyhow, bail};
use resvg::{tiny_skia, usvg};
use serde::{Deserialize, Serialize};
use serde_json::Value;

fn d_w() -> f32 {
    720.0
}
fn d_h() -> f32 {
    1280.0
}
fn d_bg() -> String {
    "#000000".into()
}
fn one() -> f32 {
    1.0
}
fn yes() -> bool {
    true
}
fn d_anchor() -> [f32; 2] {
    [0.5, 0.5]
}
fn d_secs() -> f32 {
    2.0
}
fn d_repeat() -> String {
    "loop".into()
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct StageDef {
    #[serde(default)]
    pub name: String,
    #[serde(default = "d_w")]
    pub width: f32,
    #[serde(default = "d_h")]
    pub height: f32,
    #[serde(default = "d_bg")]
    pub background: String,
    pub layers: Vec<LayerDef>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct LayerDef {
    pub id: String,
    /// inline SVG markup
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub svg: Option<String>,
    /// SVG file, relative to the stage file
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub src: Option<String>,
    #[serde(default)]
    pub x: f32,
    #[serde(default)]
    pub y: f32,
    /// size in design units; one of them alone keeps the picture's proportions; none = the picture's own size
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub w: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub h: Option<f32>,
    #[serde(default = "d_anchor")]
    pub anchor: [f32; 2],
    #[serde(default = "one")]
    pub scale: f32,
    /// degrees, clockwise
    #[serde(default)]
    pub rot: f32,
    #[serde(default = "one")]
    pub opacity: f32,
    #[serde(default)]
    pub z: i32,
    #[serde(default)]
    pub flip: bool,
    #[serde(default = "yes")]
    pub visible: bool,
    /// normal | multiply | screen | overlay | darken | lighten | soft_light | plus
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blend: Option<String>,
    /// how much the camera moves this layer: 1 = with the world (default), 0 = fixed to the screen (sky), in between = depth
    #[serde(default = "one")]
    pub parallax: f32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub anim: Vec<Anim>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Anim {
    /// x | y | rot | scale | opacity (a preset picks it when absent)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prop: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preset: Option<String>,
    /// size of a preset's swing (pixels, degrees, or a fraction for scale and opacity)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub amp: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to: Option<f32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keys: Vec<(f32, f32)>,
    /// one cycle takes this long
    #[serde(default = "d_secs")]
    pub secs: f32,
    #[serde(default)]
    pub delay: f32,
    /// start this far (0..1) into the cycle, so neighbours do not move in step
    #[serde(default)]
    pub phase: f32,
    /// linear | sine | in | out | in_out | out_back
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ease: Option<String>,
    #[serde(default = "d_repeat")]
    pub repeat: String,
}

/// One `{"do": "stage", ...}` step.
#[derive(Deserialize, Clone, Debug)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum StageOp {
    Add {
        layer: Box<LayerDef>,
    },
    Remove {
        id: String,
    },
    /// numbers (`x y scale rot opacity`) tween over `secs`; `z visible svg src blend w h flip anchor parallax` change at once
    Set {
        id: String,
        props: serde_json::Map<String, Value>,
        #[serde(default)]
        secs: f32,
        #[serde(default)]
        ease: Option<String>,
    },
    /// spring a layer in: from `from` x its scale (default 0.4) and transparent, to its own scale, overshooting a little
    Pop {
        id: String,
        #[serde(default = "d_pop_secs")]
        secs: f32,
        #[serde(default)]
        from: Option<f32>,
    },
    Camera {
        #[serde(default)]
        x: Option<f32>,
        #[serde(default)]
        y: Option<f32>,
        #[serde(default)]
        zoom: Option<f32>,
        #[serde(default)]
        secs: f32,
        #[serde(default)]
        ease: Option<String>,
    },
    /// replace the animation tracks of a layer
    Anim {
        id: String,
        anim: Vec<Anim>,
    },
}

fn d_pop_secs() -> f32 {
    0.6
}

/// `{"do": "stage", "op": ...}` as a macro or story step.
#[derive(Deserialize, Clone, Debug)]
#[serde(tag = "do", rename_all = "snake_case")]
pub enum StageStep {
    Stage(StageOp),
}

// ---------------------------------------------------------------- easing and tracks

fn ease(name: Option<&str>, t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    match name.unwrap_or("sine") {
        "linear" => t,
        "in" => t * t,
        "out" => 1.0 - (1.0 - t) * (1.0 - t),
        "in_out" => t * t * (3.0 - 2.0 * t),
        "out_back" => {
            let (c1, c3) = (1.70158f32, 2.70158f32);
            1.0 + c3 * (t - 1.0).powi(3) + c1 * (t - 1.0).powi(2)
        }
        _ => 0.5 - 0.5 * (PI * t).cos(),
    }
}

const PROPS: [&str; 5] = ["x", "y", "rot", "scale", "opacity"];

fn prop_index(p: &str) -> Option<usize> {
    PROPS.iter().position(|x| *x == p)
}

fn multiplies(prop: usize) -> bool {
    prop >= 3
}

impl Anim {
    fn prop_name(&self) -> Result<&str> {
        let preset_prop = match self.preset.as_deref() {
            Some("bob") => Some("y"),
            Some("sway") | Some("spin") => Some("rot"),
            Some("pulse") => Some("scale"),
            Some("drift") => Some("x"),
            Some("blink") | Some("flicker") => Some("opacity"),
            Some("sine") | None => None,
            Some(other) => bail!("unknown animation preset '{other}' (bob, sway, pulse, drift, blink, flicker, spin, sine)"),
        };
        match (self.prop.as_deref(), preset_prop) {
            (Some(p), _) | (None, Some(p)) => {
                if prop_index(p).is_none() {
                    bail!("unknown animation property '{p}' (x, y, rot, scale, opacity)");
                }
                Ok(p)
            }
            (None, None) => bail!("an animation needs a `prop` or a `preset`"),
        }
    }

    fn check(&self) -> Result<()> {
        self.prop_name()?;
        if self.secs <= 0.0 {
            bail!("animation `secs` must be above 0");
        }
        if !matches!(self.repeat.as_str(), "loop" | "pingpong" | "once") {
            bail!("animation `repeat` is loop, pingpong or once");
        }
        if self.preset.is_none() && self.from.is_none() && self.to.is_none() && self.keys.is_empty() {
            bail!("an animation needs a `preset`, `from`/`to` or `keys`");
        }
        Ok(())
    }

    /// raw value at `t` seconds into the layer's life
    fn value(&self, t: f32) -> f32 {
        let prop = prop_index(self.prop_name().unwrap_or("x")).unwrap_or(0);
        let local = t - self.delay;
        if local < 0.0 {
            return if multiplies(prop) { 1.0 } else { 0.0 };
        }
        let cycles = local / self.secs + self.phase;
        let phase = cycles.fract();
        if let Some(p) = self.preset.as_deref() {
            let amp = self.amp.unwrap_or(match p {
                "bob" => 6.0,
                "sway" => 3.0,
                "pulse" => 0.04,
                "drift" => 10.0,
                "blink" => 0.5,
                "flicker" => 0.35,
                _ => 1.0,
            });
            let s = (TAU * phase).sin();
            return match p {
                "spin" => 360.0 * phase,
                "blink" => 1.0 - amp * (0.5 + 0.5 * s),
                "flicker" => 1.0 - amp * (0.5 + 0.5 * s * (TAU * (phase * 3.7 + 0.2)).sin()).abs(),
                _ if multiplies(prop) => 1.0 + amp * s,
                _ => amp * s,
            };
        }
        let u = match self.repeat.as_str() {
            "once" => (local / self.secs).min(1.0),
            "pingpong" => {
                let c = (cycles * 2.0).rem_euclid(2.0);
                if c <= 1.0 { c } else { 2.0 - c }
            }
            _ => phase,
        };
        let u = if self.repeat == "once" || self.repeat == "pingpong" { ease(self.ease.as_deref(), u) } else { u };
        if !self.keys.is_empty() {
            let k = &self.keys;
            if u <= k[0].0 {
                return k[0].1;
            }
            for w in k.windows(2) {
                if u <= w[1].0 {
                    let f = if w[1].0 > w[0].0 { (u - w[0].0) / (w[1].0 - w[0].0) } else { 1.0 };
                    return w[0].1 + (w[1].1 - w[0].1) * ease(self.ease.as_deref().or(Some("linear")), f);
                }
            }
            return k[k.len() - 1].1;
        }
        let neutral = if multiplies(prop) { 1.0 } else { 0.0 };
        let (a, b) = (self.from.unwrap_or(neutral), self.to.unwrap_or(neutral));
        let u = if self.repeat == "loop" { ease(self.ease.as_deref(), u) } else { u };
        a + (b - a) * u
    }
}

#[derive(Clone, Copy, Debug)]
struct Tween {
    from: f32,
    to: f32,
    t: f32,
    dur: f32,
    ease: Option<&'static str>,
}

fn ease_name(e: Option<&str>) -> Option<&'static str> {
    match e {
        Some("linear") => Some("linear"),
        Some("in") => Some("in"),
        Some("out") => Some("out"),
        Some("in_out") => Some("in_out"),
        Some("out_back") => Some("out_back"),
        _ => None,
    }
}

/// A number that can glide to a new value.
#[derive(Clone, Copy, Debug)]
struct Num {
    v: f32,
    tw: Option<Tween>,
}

impl Num {
    fn new(v: f32) -> Num {
        Num { v, tw: None }
    }
    fn set(&mut self, to: f32, secs: f32, e: Option<&str>) {
        if secs > 0.0 {
            self.tw = Some(Tween { from: self.v, to, t: 0.0, dur: secs, ease: ease_name(e) });
        } else {
            self.v = to;
            self.tw = None;
        }
    }
    fn step(&mut self, dt: f32) {
        if let Some(tw) = &mut self.tw {
            tw.t += dt;
            let u = (tw.t / tw.dur).min(1.0);
            self.v = tw.from + (tw.to - tw.from) * ease(tw.ease, u);
            if tw.t >= tw.dur {
                self.v = tw.to;
                self.tw = None;
            }
        }
    }
}

// ---------------------------------------------------------------- the stage

struct Picture {
    hash: u64,
    size: (f32, f32),
}

struct Layer {
    id: String,
    pic: Picture,
    w: f32,
    h: f32,
    num: [Num; 5], // x y rot scale opacity
    anchor: [f32; 2],
    z: i32,
    flip: bool,
    visible: bool,
    blend: tiny_skia::BlendMode,
    parallax: f32,
    anim: Vec<Anim>,
    born: f32,
    seq: u64,
}

fn blend_mode(name: Option<&str>) -> Result<tiny_skia::BlendMode> {
    use tiny_skia::BlendMode as B;
    Ok(match name.unwrap_or("normal") {
        "normal" => B::SourceOver,
        "multiply" => B::Multiply,
        "screen" => B::Screen,
        "overlay" => B::Overlay,
        "darken" => B::Darken,
        "lighten" => B::Lighten,
        "soft_light" => B::SoftLight,
        "plus" => B::Plus,
        other => bail!("unknown blend '{other}' (normal, multiply, screen, overlay, darken, lighten, soft_light, plus)"),
    })
}

fn hash_bytes(b: &[u8]) -> u64 {
    let mut h = 0xcbf29ce484222325u64;
    for &x in b {
        h ^= x as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

fn parse_color(s: &str) -> Result<tiny_skia::Color> {
    let h = s.trim().trim_start_matches('#');
    let v = |i: usize| u8::from_str_radix(h.get(i..i + 2).unwrap_or("zz"), 16).map_err(|_| anyhow!("'{s}' is not a #rrggbb colour"));
    if h.len() != 6 && h.len() != 8 {
        bail!("'{s}' is not a #rrggbb colour");
    }
    let a = if h.len() == 8 { v(6)? } else { 255 };
    Ok(tiny_skia::Color::from_rgba8(v(0)?, v(2)?, v(4)?, a))
}

pub struct Stage {
    def: StageDef,
    base_dir: PathBuf,
    layers: Vec<Layer>,
    cam: [Num; 3], // x y zoom
    time: f32,
    seq: u64,
    trees: HashMap<u64, Arc<usvg::Tree>>,
    sprites: HashMap<(u64, u32, u32), Arc<tiny_skia::Pixmap>>,
    canvas: Option<tiny_skia::Pixmap>,
}

impl Stage {
    pub fn load(path: &Path) -> Result<Stage> {
        let txt = std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
        let def: StageDef = serde_json::from_str(&txt).with_context(|| format!("parse {}", path.display()))?;
        Stage::new(def, path.parent().map(Path::to_path_buf).unwrap_or_default())
    }

    pub fn new(def: StageDef, base_dir: PathBuf) -> Result<Stage> {
        if def.width <= 0.0 || def.height <= 0.0 {
            bail!("stage width and height must be above 0");
        }
        parse_color(&def.background).context("stage background")?;
        let mut s = Stage {
            cam: [Num::new(def.width / 2.0), Num::new(def.height / 2.0), Num::new(1.0)],
            def: def.clone(),
            base_dir,
            layers: vec![],
            time: 0.0,
            seq: 0,
            trees: HashMap::new(),
            sprites: HashMap::new(),
            canvas: None,
        };
        let mut ids = HashSet::new();
        for l in &def.layers {
            if !ids.insert(l.id.clone()) {
                bail!("two layers are called '{}'", l.id);
            }
            s.add(l).with_context(|| format!("layer '{}'", l.id))?;
        }
        Ok(s)
    }

    /// Back to the file's state (a show's `reset`).
    pub fn reset(&mut self) -> Result<()> {
        let fresh = Stage::new(self.def.clone(), self.base_dir.clone())?;
        *self = fresh;
        Ok(())
    }

    pub fn size(&self) -> (f32, f32) {
        (self.def.width, self.def.height)
    }

    pub fn ids(&self) -> Vec<String> {
        self.layers.iter().map(|l| l.id.clone()).collect()
    }

    fn picture(&mut self, svg: &Option<String>, src: &Option<String>) -> Result<Picture> {
        let data = match (svg, src) {
            (Some(s), _) => s.clone().into_bytes(),
            (None, Some(p)) => std::fs::read(self.base_dir.join(p)).with_context(|| format!("read {p}"))?,
            (None, None) => bail!("a layer needs `svg` or `src`"),
        };
        let hash = hash_bytes(&data);
        if !self.trees.contains_key(&hash) {
            let tree = usvg::Tree::from_data(&data, &usvg::Options::default()).map_err(|e| anyhow!("svg: {e}"))?;
            if self.trees.len() > 200 {
                self.trees.clear();
            }
            self.trees.insert(hash, Arc::new(tree));
        }
        let sz = self.trees[&hash].size();
        Ok(Picture { hash, size: (sz.width(), sz.height()) })
    }

    fn fit(pic: &Picture, w: Option<f32>, h: Option<f32>) -> (f32, f32) {
        let aspect = pic.size.0 / pic.size.1.max(0.001);
        match (w, h) {
            (Some(w), Some(h)) => (w, h),
            (Some(w), None) => (w, w / aspect),
            (None, Some(h)) => (h * aspect, h),
            (None, None) => pic.size,
        }
    }

    pub fn add(&mut self, l: &LayerDef) -> Result<()> {
        if l.id.is_empty() {
            bail!("a layer needs an id");
        }
        for a in &l.anim {
            a.check()?;
        }
        let pic = self.picture(&l.svg, &l.src)?;
        let (w, h) = Stage::fit(&pic, l.w, l.h);
        let blend = blend_mode(l.blend.as_deref())?;
        self.layers.retain(|x| x.id != l.id);
        self.seq += 1;
        self.layers.push(Layer {
            id: l.id.clone(),
            pic,
            w,
            h,
            num: [Num::new(l.x), Num::new(l.y), Num::new(l.rot), Num::new(l.scale), Num::new(l.opacity)],
            anchor: l.anchor,
            z: l.z,
            flip: l.flip,
            visible: l.visible,
            blend,
            parallax: l.parallax,
            anim: l.anim.clone(),
            born: self.time,
            seq: self.seq,
        });
        Ok(())
    }

    pub fn update(&mut self, dt: f32) {
        self.time += dt;
        for n in self.cam.iter_mut() {
            n.step(dt);
        }
        for l in self.layers.iter_mut() {
            for n in l.num.iter_mut() {
                n.step(dt);
            }
        }
    }

    /// Applies one `stage` op (the part after `{"do": "stage"}`; a whole step with `do` is accepted too).
    pub fn apply(&mut self, step: &Value) -> Result<()> {
        let op: StageOp = match serde_json::from_value::<StageStep>(step.clone()) {
            Ok(StageStep::Stage(op)) => op,
            Err(_) => serde_json::from_value(step.clone()).map_err(|e| anyhow!("stage op: {e}"))?,
        };
        self.apply_op(op)
    }

    fn find(&mut self, id: &str) -> Result<&mut Layer> {
        self.layers.iter_mut().find(|l| l.id == id).ok_or_else(|| anyhow!("no layer '{id}'"))
    }

    pub fn apply_op(&mut self, op: StageOp) -> Result<()> {
        match op {
            StageOp::Add { layer } => self.add(&layer)?,
            StageOp::Remove { id } => {
                let before = self.layers.len();
                self.layers.retain(|l| l.id != id);
                if self.layers.len() == before {
                    bail!("no layer '{id}'");
                }
            }
            StageOp::Anim { id, anim } => {
                for a in &anim {
                    a.check()?;
                }
                self.find(&id)?.anim = anim;
            }
            StageOp::Camera { x, y, zoom, secs, ease } => {
                for (i, v) in [x, y, zoom].into_iter().enumerate() {
                    if let Some(v) = v {
                        self.cam[i].set(v, secs, ease.as_deref());
                    }
                }
            }
            StageOp::Pop { id, secs, from } => {
                let l = self.find(&id)?;
                let (s, o) = (l.num[3].tw.map_or(l.num[3].v, |t| t.to), l.num[4].tw.map_or(l.num[4].v, |t| t.to));
                l.num[3].v = s * from.unwrap_or(0.4);
                l.num[3].tw = None;
                l.num[4].v = 0.0;
                l.num[4].tw = None;
                l.num[3].set(s, secs.max(0.05), Some("out_back"));
                l.num[4].set(o, (secs * 0.5).max(0.05), Some("out"));
            }
            StageOp::Set { id, props, secs, ease } => {
                // look at everything first so a bad property changes nothing
                for (k, v) in &props {
                    match k.as_str() {
                        "x" | "y" | "rot" | "scale" | "opacity" | "z" | "w" | "h" | "parallax" => {
                            v.as_f64().with_context(|| format!("'{k}' must be a number"))?;
                        }
                        "visible" | "flip" => {
                            v.as_bool().with_context(|| format!("'{k}' must be true or false"))?;
                        }
                        "svg" | "src" | "blend" => {
                            v.as_str().with_context(|| format!("'{k}' must be a string"))?;
                        }
                        "anchor" => {
                            serde_json::from_value::<[f32; 2]>(v.clone()).map_err(|_| anyhow!("'anchor' must be [x, y]"))?;
                        }
                        other => bail!("unknown layer property '{other}'"),
                    }
                }
                let new_pic = match (props.get("svg").and_then(Value::as_str), props.get("src").and_then(Value::as_str)) {
                    (None, None) => None,
                    (s, p) => Some(self.picture(&s.map(str::to_string), &p.map(str::to_string))?),
                };
                let blend = props.get("blend").and_then(Value::as_str).map(|b| blend_mode(Some(b))).transpose()?;
                let l = self.find(&id)?;
                if let Some(pic) = new_pic {
                    let keep = (props.contains_key("w") || props.contains_key("h")).then_some(());
                    if keep.is_none() {
                        let (w, h) = Stage::fit(&pic, Some(l.w), None);
                        // a new picture keeps the layer's width and takes its own proportions
                        l.w = w;
                        l.h = h;
                    }
                    l.pic = pic;
                }
                if let Some(b) = blend {
                    l.blend = b;
                }
                for (k, v) in &props {
                    match k.as_str() {
                        "z" => l.z = v.as_f64().unwrap_or(0.0) as i32,
                        "w" => l.w = v.as_f64().unwrap_or(0.0) as f32,
                        "h" => l.h = v.as_f64().unwrap_or(0.0) as f32,
                        "parallax" => l.parallax = v.as_f64().unwrap_or(1.0) as f32,
                        "visible" => l.visible = v.as_bool().unwrap_or(true),
                        "flip" => l.flip = v.as_bool().unwrap_or(false),
                        "anchor" => l.anchor = serde_json::from_value(v.clone()).unwrap_or(l.anchor),
                        other => {
                            if let Some(i) = prop_index(other) {
                                l.num[i].set(v.as_f64().unwrap_or(0.0) as f32, secs, ease.as_deref());
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn sprite(&mut self, hash: u64, pw: u32, ph: u32) -> Option<Arc<tiny_skia::Pixmap>> {
        if let Some(s) = self.sprites.get(&(hash, pw, ph)) {
            return Some(s.clone());
        }
        let tree = self.trees.get(&hash)?.clone();
        let mut pm = tiny_skia::Pixmap::new(pw, ph)?;
        let sz = tree.size();
        resvg::render(&tree, tiny_skia::Transform::from_scale(pw as f32 / sz.width(), ph as f32 / sz.height()), &mut pm.as_mut());
        if self.sprites.len() > 300 {
            self.sprites.clear();
        }
        let a = Arc::new(pm);
        self.sprites.insert((hash, pw, ph), a.clone());
        Some(a)
    }

    /// One frame, `w` x `h` pixels, as packed RGB.
    pub fn render(&mut self, w: usize, h: usize) -> Result<Vec<u8>> {
        let (sw, sh) = self.size();
        let k = (w as f32 / sw).min(h as f32 / sh);
        let (padx, pady) = ((w as f32 - sw * k) / 2.0, (h as f32 - sh * k) / 2.0);
        let mut canvas = match self.canvas.take() {
            Some(c) if c.width() as usize == w && c.height() as usize == h => c,
            _ => tiny_skia::Pixmap::new(w as u32, h as u32).ok_or_else(|| anyhow!("frame {w}x{h} is empty"))?,
        };
        canvas.fill(parse_color(&self.def.background)?);
        let cam = [self.cam[0].v, self.cam[1].v, self.cam[2].v.max(0.05)];
        let mut order: Vec<usize> = (0..self.layers.len()).collect();
        order.sort_by_key(|&i| (self.layers[i].z, self.layers[i].seq));
        for i in order {
            let l = &self.layers[i];
            if !l.visible {
                continue;
            }
            let mut v = [l.num[0].v, l.num[1].v, l.num[2].v, l.num[3].v, l.num[4].v];
            let t = self.time - l.born;
            for a in &l.anim {
                if let Ok(p) = a.prop_name() {
                    let pi = prop_index(p).unwrap_or(0);
                    let val = a.value(t);
                    if multiplies(pi) {
                        v[pi] *= val;
                    } else {
                        v[pi] += val;
                    }
                }
            }
            let opacity = v[4].clamp(0.0, 1.0);
            if opacity < 0.003 || v[3].abs() < 0.001 {
                continue;
            }
            let zoom = 1.0 + (cam[2] - 1.0) * l.parallax;
            let (ox, oy) = ((cam[0] - sw / 2.0) * l.parallax, (cam[1] - sh / 2.0) * l.parallax);
            let cx = ((v[0] - ox - sw / 2.0) * zoom + sw / 2.0) * k + padx;
            let cy = ((v[1] - oy - sh / 2.0) * zoom + sh / 2.0) * k + pady;
            let s = v[3] * zoom * k;
            // A layer whose size changes (tween, animation, camera zoom) is drawn from a picture rasterised in steps of 1/8, so
            // the cache does not fill up while it grows. One that keeps its size is rasterised exactly: sharp, and copied
            // without filtering when it also keeps its place.
            let is_prop = |a: &Anim, names: &[&str]| a.prop_name().is_ok_and(|p| names.contains(&p));
            let scale_moves = l.num[3].tw.is_some() || self.cam[2].tw.is_some() || l.anim.iter().any(|a| is_prop(a, &["scale"]));
            let pos_moves = l.num[0].tw.is_some() || l.num[1].tw.is_some() || self.cam[0].tw.is_some() || self.cam[1].tw.is_some() || l.anim.iter().any(|a| is_prop(a, &["x", "y"]));
            let q = if scale_moves { ((s / k.max(0.001) * 8.0).round() / 8.0).max(0.125) * k } else { s.max(0.01) };
            let (lw, lh) = (l.w, l.h);
            let (pw, ph) = (((lw * q).round() as u32).clamp(1, 4096), ((lh * q).round() as u32).clamp(1, 4096));
            let (hash, anchor, flip, blend) = (l.pic.hash, l.anchor, l.flip, l.blend);
            let Some(sp) = self.sprite(hash, pw, ph) else { continue };
            let r = s * lw / pw as f32; // residual scale from the cached size to the wanted one
            let sx = if flip { -r } else { r };
            let (ax, ay) = (anchor[0] * pw as f32, anchor[1] * ph as f32);
            // an upright sprite at its natural size lands on whole pixels: copy it, no filtering (sky, island, still layers)
            let upright = !pos_moves && v[2].abs() < 0.01 && !flip && (r - 1.0).abs() < 0.01;
            let (tf, quality) = if upright {
                (tiny_skia::Transform::from_translate((cx - ax).round(), (cy - ay).round()), tiny_skia::FilterQuality::Nearest)
            } else {
                (tiny_skia::Transform::from_translate(cx, cy).pre_rotate(v[2]).pre_scale(sx, r).pre_translate(-ax, -ay), tiny_skia::FilterQuality::Bilinear)
            };
            let paint = tiny_skia::PixmapPaint { opacity, blend_mode: blend, quality };
            canvas.draw_pixmap(0, 0, tiny_skia::Pixmap::as_ref(&sp), &paint, tf, None);
        }
        let mut rgb = vec![0u8; w * h * 3];
        for (dst, src) in rgb.chunks_exact_mut(3).zip(canvas.data().chunks_exact(4)) {
            dst.copy_from_slice(&src[..3]);
        }
        self.canvas = Some(canvas);
        Ok(rgb)
    }
}

/// Checks a stage file without rendering: every layer's picture parses, every animation is well formed.
pub fn validate(def: &StageDef, base_dir: &Path) -> Result<()> {
    Stage::new(def.clone(), base_dir.to_path_buf()).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn rect(color: &str) -> String {
        format!(r#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20"><rect width="20" height="20" fill="{color}"/></svg>"#)
    }

    fn stage(layers: Value) -> Stage {
        let def: StageDef = serde_json::from_value(json!({"width": 100, "height": 100, "background": "#000000", "layers": layers})).unwrap();
        Stage::new(def, PathBuf::new()).unwrap()
    }

    fn px(rgb: &[u8], w: usize, x: usize, y: usize) -> [u8; 3] {
        let i = (y * w + x) * 3;
        [rgb[i], rgb[i + 1], rgb[i + 2]]
    }

    #[test]
    fn a_layer_is_drawn_where_its_anchor_says() {
        let mut s = stage(json!([{"id": "a", "svg": rect("#ff0000"), "x": 30, "y": 40}]));
        let f = s.render(100, 100).unwrap();
        assert_eq!(px(&f, 100, 30, 40), [255, 0, 0], "centre of the layer");
        assert_eq!(px(&f, 100, 21, 31), [255, 0, 0], "inside the corner");
        assert_eq!(px(&f, 100, 19, 30), [0, 0, 0], "outside it");
        // anchored by its top-left corner
        let mut s = stage(json!([{"id": "a", "svg": rect("#00ff00"), "x": 30, "y": 40, "anchor": [0, 0]}]));
        let f = s.render(100, 100).unwrap();
        assert_eq!((px(&f, 100, 31, 41), px(&f, 100, 29, 39)), ([0, 255, 0], [0, 0, 0]));
    }

    #[test]
    fn the_picture_is_scaled_to_the_output_size() {
        let mut s = stage(json!([{"id": "a", "svg": rect("#0000ff"), "x": 50, "y": 50}]));
        let f = s.render(200, 200).unwrap();
        assert_eq!(px(&f, 200, 100, 100), [0, 0, 255]);
        assert_eq!(px(&f, 200, 82, 100), [0, 0, 255], "20 design units are 40 pixels wide");
        assert_eq!(px(&f, 200, 78, 100), [0, 0, 0]);
    }

    #[test]
    fn higher_z_is_in_front_and_blend_multiply_darkens() {
        let mut s = stage(json!([
            {"id": "back", "svg": rect("#ff0000"), "x": 50, "y": 50, "z": 1},
            {"id": "front", "svg": rect("#00ff00"), "x": 50, "y": 50, "z": 2}
        ]));
        assert_eq!(px(&s.render(100, 100).unwrap(), 100, 50, 50), [0, 255, 0]);
        let mut s = stage(json!([
            {"id": "base", "svg": rect("#ffffff"), "x": 50, "y": 50},
            {"id": "tint", "svg": rect("#808080"), "x": 50, "y": 50, "z": 1, "blend": "multiply"}
        ]));
        let c = px(&s.render(100, 100).unwrap(), 100, 50, 50);
        assert!((120..=135).contains(&c[0]) && c[0] == c[1] && c[1] == c[2], "white x grey = grey: {c:?}");
    }

    #[test]
    fn animations_offset_and_multiply_the_layer() {
        let mut s = stage(json!([{"id": "a", "svg": rect("#ff0000"), "x": 50, "y": 50,
            "anim": [{"prop": "x", "from": 0, "to": 20, "secs": 2, "repeat": "once", "ease": "linear"}]}]));
        let at = |s: &mut Stage| {
            let f = s.render(100, 100).unwrap();
            (0..100).find(|&x| px(&f, 100, x, 50)[0] == 255).unwrap_or(999)
        };
        assert!((39..=41).contains(&at(&mut s)), "starts at 40 (left edge of a layer centred at 50)");
        s.update(1.0);
        assert!((49..=51).contains(&at(&mut s)), "half way: +10");
        s.update(5.0);
        assert!((59..=61).contains(&at(&mut s)), "`once` stops at the end: +20");
        // a swaying layer comes back to where it started after one cycle
        let a = Anim { prop: None, preset: Some("sway".into()), amp: Some(10.0), from: None, to: None, keys: vec![], secs: 4.0, delay: 0.0, phase: 0.0, ease: None, repeat: "loop".into() };
        assert!(a.value(0.0).abs() < 1e-3 && (a.value(1.0) - 10.0).abs() < 1e-3 && a.value(4.0).abs() < 1e-3);
        let pulse = Anim { preset: Some("pulse".into()), amp: Some(0.1), ..a.clone() };
        assert!((pulse.value(1.0) - 1.1).abs() < 1e-3, "pulse multiplies scale around 1");
        let blink = Anim { preset: Some("blink".into()), amp: Some(0.6), ..a.clone() };
        assert!((blink.value(0.0) - 0.7).abs() < 1e-3);
        let keys = Anim { preset: None, keys: vec![(0.0, 1.0), (0.5, 0.0), (1.0, 1.0)], prop: Some("opacity".into()), ..a };
        assert!((keys.value(2.0) - 0.0).abs() < 1e-3 && (keys.value(1.0) - 0.5).abs() < 1e-3);
    }

    #[test]
    fn set_tweens_numbers_and_swaps_the_picture() {
        let mut s = stage(json!([{"id": "a", "svg": rect("#ff0000"), "x": 20, "y": 50}]));
        s.apply(&json!({"do": "stage", "op": "set", "id": "a", "props": {"x": 80}, "secs": 1.0, "ease": "linear"})).unwrap();
        s.update(0.5);
        let f = s.render(100, 100).unwrap();
        assert_eq!(px(&f, 100, 50, 50), [255, 0, 0], "half way along: centred at 50");
        s.update(1.0);
        s.apply(&json!({"op": "set", "id": "a", "props": {"svg": rect("#0000ff"), "opacity": 0.0}})).unwrap();
        let f = s.render(100, 100).unwrap();
        assert_eq!(px(&f, 100, 80, 50), [0, 0, 0], "invisible now");
        s.apply(&json!({"op": "set", "id": "a", "props": {"opacity": 1.0}})).unwrap();
        assert_eq!(px(&s.render(100, 100).unwrap(), 100, 80, 50), [0, 0, 255], "the new picture");
        // a bad property or id changes nothing and says so
        assert!(s.apply(&json!({"op": "set", "id": "a", "props": {"x": 1, "colour": "red"}})).unwrap_err().to_string().contains("colour"));
        assert!(s.apply(&json!({"op": "set", "id": "nope", "props": {"x": 1}})).unwrap_err().to_string().contains("nope"));
        assert!(s.apply(&json!({"op": "set", "id": "a", "props": {"svg": "<not svg"}})).is_err());
    }

    #[test]
    fn add_remove_pop_and_camera() {
        let mut s = stage(json!([{"id": "a", "svg": rect("#ff0000"), "x": 50, "y": 50}]));
        s.apply(&json!({"do": "stage", "op": "add", "layer": {"id": "b", "svg": rect("#00ff00"), "x": 10, "y": 10}})).unwrap();
        assert_eq!(s.ids(), vec!["a", "b"]);
        s.apply(&json!({"do": "stage", "op": "remove", "id": "a"})).unwrap();
        assert_eq!(s.ids(), vec!["b"]);
        assert!(s.apply(&json!({"do": "stage", "op": "remove", "id": "a"})).is_err());
        // pop: starts small and transparent, ends at its own size
        s.apply(&json!({"op": "pop", "id": "b", "secs": 0.5})).unwrap();
        assert_eq!(px(&s.render(100, 100).unwrap(), 100, 10, 10), [0, 0, 0]);
        s.update(1.0);
        assert_eq!(px(&s.render(100, 100).unwrap(), 100, 10, 10), [0, 255, 0]);
        // camera zoom 2 about the centre pushes a layer at (10,10) out of view, and a fixed (parallax 0) layer stays
        s.apply(&json!({"op": "add", "layer": {"id": "hud", "svg": rect("#ffffff"), "x": 90, "y": 90, "parallax": 0}})).unwrap();
        s.apply(&json!({"op": "camera", "zoom": 2.0})).unwrap();
        s.update(0.0);
        let f = s.render(100, 100).unwrap();
        assert_eq!(px(&f, 100, 10, 10), [0, 0, 0]);
        assert_eq!(px(&f, 100, 90, 90), [255, 255, 255]);
    }

    #[test]
    fn bad_stages_are_refused_with_the_layer_named() {
        let bad = |layers: Value| {
            let def: StageDef = serde_json::from_value(json!({"layers": layers})).unwrap();
            Stage::new(def, PathBuf::new()).err().map(|e| format!("{e:#}")).unwrap_or_default()
        };
        assert!(bad(json!([{"id": "a", "svg": "<svg"}])).contains("layer 'a'"));
        assert!(bad(json!([{"id": "a", "svg": rect("#000")}, {"id": "a", "svg": rect("#000")}])).contains("two layers"));
        assert!(bad(json!([{"id": "a", "svg": rect("#000"), "anim": [{"preset": "wobble"}]}])).contains("wobble"));
        assert!(bad(json!([{"id": "a", "svg": rect("#000"), "anim": [{"prop": "x"}]}])).contains("preset"));
        assert!(bad(json!([{"id": "a", "svg": rect("#000"), "blend": "glow"}])).contains("glow"));
        assert!(bad(json!([{"id": "a"}])).contains("svg"));
    }
}
