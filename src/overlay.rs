//! Overlay elements for live streams and renders: text (with inline emoji), emoji, SVG, PNG, rectangles
//! and bars, composited on the final frame at output resolution. Elements are plain JSON, so an agent
//! adds, patches and removes them over the CLI / webhook while the stream runs.
//!
//! Units: positions `at` are fractions of the frame (0..1); every length (`size`, `w`, `h`, `pad`,
//! `offset`) is a fraction of the SHORTER frame side, so a layout survives a change of format.
//! Text uses the built-in pixel font (A-Z, 0-9, common punctuation, case-insensitive); emoji and SVG
//! are rasterised with resvg (SVG `<text>` needs a system font).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result, anyhow, bail};
use resvg::{tiny_skia, usvg};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{emoji, font};

fn d_black() -> String {
    "#000000".into()
}
fn d_half() -> f32 {
    0.5
}
pub type Vars = HashMap<String, String>;

fn yes() -> bool {
    true
}
fn one() -> f32 {
    1.0
}
fn d_at() -> [f32; 2] {
    [0.5, 0.5]
}
fn d_anchor() -> String {
    "center".into()
}
fn d_anim_secs() -> f32 {
    0.35
}
fn d_text_size() -> f32 {
    0.04
}
fn d_white() -> String {
    "#ffffff".into()
}
fn d_pad() -> f32 {
    0.008
}
fn d_align() -> String {
    "center".into()
}
fn d_emoji_size() -> f32 {
    0.08
}
fn d_bar_color() -> String {
    "#4caf50".into()
}
fn d_bar_bg() -> String {
    "#000000aa".into()
}
fn is_zero2(v: &[f32; 2]) -> bool {
    v[0] == 0.0 && v[1] == 0.0
}
fn is_one(v: &f32) -> bool {
    *v == 1.0
}
fn is_zero(v: &i32) -> bool {
    *v == 0
}

// ---------------------------------------------------------------- schema

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Kind {
    /// Pixel-font text. `{var}` is replaced by live values (`{hp.A}`, `{round}`, `{user}`...).
    /// `:rose:` short names and real emoji are drawn inline.
    Text {
        text: String,
        #[serde(default = "d_text_size")]
        size: f32,
        #[serde(default = "d_white")]
        color: String,
        #[serde(default = "yes")]
        shadow: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        outline: Option<String>,
        /// background panel colour, `#rrggbb` or `#rrggbbaa`
        #[serde(default, skip_serializing_if = "Option::is_none")]
        bg: Option<String>,
        #[serde(default = "d_pad")]
        pad: f32,
        /// left | center | right (multi-line text)
        #[serde(default = "d_align")]
        align: String,
        /// wrap at this fraction of the frame width
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max_width: Option<f32>,
    },
    /// One emoji, e.g. `"emoji": "🌹"` or `"emoji": ":rose:"`. `size` = height.
    Emoji {
        emoji: String,
        #[serde(default = "d_emoji_size")]
        size: f32,
    },
    /// SVG from a file (`src`, relative to the project) or inline markup (`svg`). `w`/`h` = box size.
    Svg {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        src: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        svg: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        w: Option<f32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        h: Option<f32>,
    },
    /// PNG (or SVG by extension) from a file.
    Image {
        src: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        w: Option<f32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        h: Option<f32>,
    },
    Rect {
        #[serde(default)]
        w: f32,
        #[serde(default)]
        h: f32,
        color: String,
        #[serde(default)]
        radius: f32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        border: Option<String>,
        #[serde(default)]
        border_w: f32,
        /// cover the whole frame (tints, flashes); `w` and `h` are ignored
        #[serde(default)]
        full: bool,
    },
    /// Darkened (or tinted) edges over the whole frame; `strength` 0..1.
    Vignette {
        #[serde(default = "d_black")]
        color: String,
        #[serde(default = "d_half")]
        strength: f32,
    },
    /// Progress bar. `value` 0..1, or `bind` a live variable (`"hp_frac.A"`).
    Bar {
        w: f32,
        h: f32,
        #[serde(default)]
        value: f32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        bind: Option<String>,
        #[serde(default = "d_bar_color")]
        color: String,
        #[serde(default = "d_bar_bg")]
        bg: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        border: Option<String>,
        /// fill grows from the right edge (the right-hand side's health bar)
        #[serde(default)]
        from_right: bool,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Element {
    /// Same id replaces the element, `overlay remove` takes it away. Generated when empty.
    #[serde(default)]
    pub id: String,
    #[serde(flatten)]
    pub kind: Kind,
    /// Anchor point as a fraction of the frame.
    #[serde(default = "d_at")]
    pub at: [f32; 2],
    /// Which point of the element sits on `at`: center, top-left, top, top-right, left, right, bottom-left, bottom, bottom-right.
    #[serde(default = "d_anchor")]
    pub anchor: String,
    #[serde(default, skip_serializing_if = "is_zero2")]
    pub offset: [f32; 2],
    #[serde(default = "one", skip_serializing_if = "is_one")]
    pub opacity: f32,
    /// Seconds on screen, then it fades out and is removed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ttl: Option<f32>,
    /// pop | fade | slide_up | slide_down | slide_left | slide_right | none
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anim_in: Option<String>,
    /// fade (default) | pop | none
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anim_out: Option<String>,
    #[serde(default = "d_anim_secs")]
    pub anim_secs: f32,
    /// Higher draws on top.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub z: i32,
    /// Timeline window on the overlay clock (seconds since the stream or render started).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub until: Option<f32>,
    #[serde(default = "yes")]
    pub visible: bool,
}

impl Element {
    pub fn new(kind: Kind) -> Element {
        Element {
            id: String::new(),
            kind,
            at: d_at(),
            anchor: d_anchor(),
            offset: [0.0, 0.0],
            opacity: 1.0,
            ttl: None,
            anim_in: None,
            anim_out: None,
            anim_secs: d_anim_secs(),
            z: 0,
            from: None,
            until: None,
            visible: true,
        }
    }

    pub fn text(text: &str, size: f32) -> Element {
        Element::new(Kind::Text {
            text: text.into(),
            size,
            color: d_white(),
            shadow: true,
            outline: None,
            bg: None,
            pad: d_pad(),
            align: d_align(),
            max_width: None,
        })
    }

    pub fn at(mut self, x: f32, y: f32, anchor: &str) -> Element {
        self.at = [x, y];
        self.anchor = anchor.into();
        self
    }

    pub fn id(mut self, id: &str) -> Element {
        self.id = id.into();
        self
    }
}

/// Accepts one element, an array, or `{"elements": [...]}`.
pub fn parse_elements(json: &str) -> Result<Vec<Element>> {
    let v: Value = serde_json::from_str(json).context("overlay JSON")?;
    let list = match v {
        Value::Array(a) => a,
        Value::Object(mut o) if o.contains_key("elements") => match o.remove("elements") {
            Some(Value::Array(a)) => a,
            _ => bail!("\"elements\" must be an array"),
        },
        other => vec![other],
    };
    list.into_iter().enumerate().map(|(i, e)| serde_json::from_value(e).with_context(|| format!("overlay element #{i}"))).collect()
}

// ---------------------------------------------------------------- live set

struct Item {
    el: Element,
    born: f32,
    removing: Option<f32>,
}

pub struct Drawable<'a> {
    pub el: &'a Element,
    pub age: f32,
    /// seconds until it is gone (fade-out window), when it has an end
    pub left: Option<f32>,
}

#[derive(Default)]
pub struct Overlay {
    items: Vec<Item>,
    pub time: f32,
    next: u64,
}

impl Overlay {
    pub fn new() -> Overlay {
        Overlay::default()
    }

    pub fn add(&mut self, mut el: Element) -> String {
        if el.id.is_empty() {
            self.next += 1;
            el.id = format!("ov{}", self.next);
        }
        let id = el.id.clone();
        self.items.retain(|i| i.el.id != id);
        let born = el.from.unwrap_or(self.time);
        self.items.push(Item { el, born, removing: None });
        id
    }

    /// Starts the fade-out of `id`; false when there is no such element.
    pub fn remove(&mut self, id: &str) -> bool {
        let t = self.time;
        match self.items.iter_mut().find(|i| i.el.id == id) {
            Some(i) => {
                if i.removing.is_none() {
                    i.removing = Some(t);
                }
                true
            }
            None => false,
        }
    }

    pub fn clear(&mut self) {
        self.items.clear();
    }

    /// Merges the fields of `patch` into the element `id` (`{"text": "NEW"}`, `{"value": 0.4}`).
    pub fn patch(&mut self, id: &str, patch: &Value) -> Result<()> {
        let it = self.items.iter_mut().find(|i| i.el.id == id).ok_or_else(|| anyhow!("no overlay element '{id}'"))?;
        let mut v = serde_json::to_value(&it.el)?;
        let (Some(dst), Some(src)) = (v.as_object_mut(), patch.as_object()) else { bail!("patch must be a JSON object") };
        for (k, val) in src {
            dst.insert(k.clone(), val.clone());
        }
        it.el = serde_json::from_value(v).context("patched element is invalid")?;
        it.el.id = id.to_string();
        Ok(())
    }

    pub fn tick(&mut self, dt: f32) {
        self.time += dt;
        let t = self.time;
        self.items.retain(|i| {
            if let Some(t0) = i.removing {
                return t - t0 < i.el.anim_secs.max(0.01);
            }
            if let Some(ttl) = i.el.ttl {
                if t >= i.born + ttl {
                    return false;
                }
            }
            i.el.until.is_none_or(|u| t < u)
        });
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn list(&self) -> Vec<&Element> {
        self.items.iter().map(|i| &i.el).collect()
    }

    pub fn drawables(&self) -> Vec<Drawable<'_>> {
        let t = self.time;
        let mut v: Vec<Drawable> = self
            .items
            .iter()
            .filter(|i| i.el.visible && t >= i.born)
            .map(|i| {
                let mut left: Option<f32> = None;
                let mut upd = |x: f32| left = Some(left.map_or(x, |l| l.min(x)));
                if let Some(t0) = i.removing {
                    upd(i.el.anim_secs - (t - t0));
                }
                if let Some(ttl) = i.el.ttl {
                    upd(i.born + ttl - t);
                }
                if let Some(u) = i.el.until {
                    upd(u - t);
                }
                Drawable { el: &i.el, age: t - i.born, left }
            })
            .collect();
        v.sort_by_key(|d| d.el.z);
        v
    }
}

// ---------------------------------------------------------------- sprites

/// Premultiplied RGBA.
#[derive(Clone)]
pub struct Sprite {
    pub w: usize,
    pub h: usize,
    pub px: Vec<u8>,
}

impl Sprite {
    fn new(w: usize, h: usize) -> Sprite {
        let (w, h) = (w.max(1), h.max(1));
        Sprite { w, h, px: vec![0; w * h * 4] }
    }

    /// Source-over of a straight-alpha colour.
    fn over(&mut self, x: i32, y: i32, c: [u8; 4]) {
        if x < 0 || y < 0 || x as usize >= self.w || y as usize >= self.h || c[3] == 0 {
            return;
        }
        let i = (y as usize * self.w + x as usize) * 4;
        let a = c[3] as u32;
        let inv = 255 - a;
        for k in 0..3 {
            self.px[i + k] = ((c[k] as u32 * a + self.px[i + k] as u32 * inv) / 255) as u8;
        }
        self.px[i + 3] = (a + self.px[i + 3] as u32 * inv / 255) as u8;
    }

    fn fill(&mut self, x: i32, y: i32, w: i32, h: i32, c: [u8; 4]) {
        for yy in y..y + h {
            for xx in x..x + w {
                self.over(xx, yy, c);
            }
        }
    }

    /// Source-over of a premultiplied sprite.
    fn over_sprite(&mut self, o: &Sprite, x: i32, y: i32) {
        for sy in 0..o.h {
            let dy = y + sy as i32;
            if dy < 0 || dy as usize >= self.h {
                continue;
            }
            for sx in 0..o.w {
                let dx = x + sx as i32;
                if dx < 0 || dx as usize >= self.w {
                    continue;
                }
                let s = &o.px[(sy * o.w + sx) * 4..][..4];
                if s[3] == 0 {
                    continue;
                }
                let i = (dy as usize * self.w + dx as usize) * 4;
                let inv = 255 - s[3] as u32;
                for k in 0..4 {
                    self.px[i + k] = (s[k] as u32 + self.px[i + k] as u32 * inv / 255).min(255) as u8;
                }
            }
        }
    }

    /// Rounded rectangle with anti-aliased edge. `inset` shrinks it, for borders.
    fn round_rect(&mut self, x: f32, y: f32, w: f32, h: f32, radius: f32, c: [u8; 4]) {
        let r = radius.min(w / 2.0).min(h / 2.0).max(0.0);
        let (cx, cy) = (x + w / 2.0, y + h / 2.0);
        let (hw, hh) = (w / 2.0, h / 2.0);
        let (x0, x1) = ((x.floor() as i32).max(0), ((x + w).ceil() as i32).min(self.w as i32));
        let (y0, y1) = ((y.floor() as i32).max(0), ((y + h).ceil() as i32).min(self.h as i32));
        for yy in y0..y1 {
            for xx in x0..x1 {
                let (px, py) = (xx as f32 + 0.5 - cx, yy as f32 + 0.5 - cy);
                let (qx, qy) = (px.abs() - (hw - r), py.abs() - (hh - r));
                let d = qx.max(0.0).hypot(qy.max(0.0)) + qx.max(qy).min(0.0) - r;
                let cov = (0.5 - d).clamp(0.0, 1.0);
                if cov > 0.0 {
                    self.over(xx, yy, [c[0], c[1], c[2], (c[3] as f32 * cov) as u8]);
                }
            }
        }
    }
}

/// `#rgb`, `#rrggbb`, `#rrggbbaa`.
pub fn parse_color(s: &str) -> [u8; 4] {
    let h = s.trim().trim_start_matches('#');
    let v = |i: usize, n: usize| u8::from_str_radix(&h[i..i + n], 16).ok();
    let dup = |x: u8| x * 17;
    if !h.is_ascii() {
        return [255, 0, 255, 255];
    }
    match h.len() {
        3 => match (v(0, 1), v(1, 1), v(2, 1)) {
            (Some(r), Some(g), Some(b)) => [dup(r), dup(g), dup(b), 255],
            _ => [255, 0, 255, 255],
        },
        6 | 8 => match (v(0, 2), v(2, 2), v(4, 2)) {
            (Some(r), Some(g), Some(b)) => [r, g, b, if h.len() == 8 { v(6, 2).unwrap_or(255) } else { 255 }],
            _ => [255, 0, 255, 255],
        },
        _ => [255, 0, 255, 255],
    }
}

/// `{key}` -> vars[key]; unknown keys are left as written.
pub fn interpolate(s: &str, vars: &Vars) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('{') {
        out.push_str(&rest[..i]);
        match rest[i..].find('}') {
            Some(j) => {
                let key = &rest[i + 1..i + j];
                match vars.get(key) {
                    Some(v) => out.push_str(v),
                    None => out.push_str(&rest[i..i + j + 1]),
                }
                rest = &rest[i + j + 1..];
            }
            None => {
                out.push_str(&rest[i..]);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

enum Run {
    Glyph(char),
    Emoji(String),
}

fn runs_of(s: &str) -> Vec<Run> {
    emoji::split(s)
        .into_iter()
        .flat_map(|p| match p {
            emoji::Piece::Text(t) => t.chars().map(Run::Glyph).collect::<Vec<_>>(),
            emoji::Piece::Emoji(e) => vec![Run::Emoji(e)],
        })
        .collect()
}

fn run_w(r: &Run, s: i32) -> i32 {
    match r {
        Run::Glyph(c) => font::advance_px(*c) * s,
        Run::Emoji(_) => 8 * s,
    }
}

fn line_w(line: &[Run], s: i32) -> i32 {
    (line.iter().map(|r| run_w(r, s)).sum::<i32>() - s).max(0)
}

/// Lays text into lines, wrapping words at `max_px` when set.
fn layout(text: &str, s: i32, max_px: Option<i32>) -> Vec<Vec<Run>> {
    let mut lines = vec![];
    for para in text.split('\n') {
        let Some(max) = max_px else {
            lines.push(runs_of(para));
            continue;
        };
        let mut cur: Vec<Run> = vec![];
        for word in para.split_whitespace() {
            let w = runs_of(word);
            let ww: i32 = w.iter().map(|r| run_w(r, s)).sum();
            let cw: i32 = cur.iter().map(|r| run_w(r, s)).sum();
            if !cur.is_empty() && cw + 4 * s + ww > max {
                lines.push(std::mem::take(&mut cur));
            }
            if !cur.is_empty() {
                cur.push(Run::Glyph(' '));
            }
            cur.extend(w);
        }
        lines.push(cur);
    }
    lines
}

fn fit(w: Option<f32>, h: Option<f32>, aspect: f32, short: f32, default_h: f32) -> (u32, u32) {
    let (w, h) = match (w, h) {
        (Some(w), Some(h)) => (w * short, h * short),
        (Some(w), None) => (w * short, w * short / aspect),
        (None, Some(h)) => (h * short * aspect, h * short),
        (None, None) => (default_h * short * aspect, default_h * short),
    };
    ((w.round() as u32).clamp(1, 4096), (h.round() as u32).clamp(1, 4096))
}

fn bilinear(src: &Sprite, tw: usize, th: usize) -> Sprite {
    let mut out = Sprite::new(tw, th);
    for y in 0..th {
        let fy = ((y as f32 + 0.5) * src.h as f32 / th as f32 - 0.5).clamp(0.0, src.h as f32 - 1.0);
        let (y0, y1, ty) = (fy.floor() as usize, (fy.floor() as usize + 1).min(src.h - 1), fy.fract());
        for x in 0..tw {
            let fx = ((x as f32 + 0.5) * src.w as f32 / tw as f32 - 0.5).clamp(0.0, src.w as f32 - 1.0);
            let (x0, x1, tx) = (fx.floor() as usize, (fx.floor() as usize + 1).min(src.w - 1), fx.fract());
            for k in 0..4 {
                let p = |xx: usize, yy: usize| src.px[(yy * src.w + xx) * 4 + k] as f32;
                let top = p(x0, y0) * (1.0 - tx) + p(x1, y0) * tx;
                let bot = p(x0, y1) * (1.0 - tx) + p(x1, y1) * tx;
                out.px[(y * tw + x) * 4 + k] = (top * (1.0 - ty) + bot * ty).round() as u8;
            }
        }
    }
    out
}

fn load_png(path: &Path) -> Result<Sprite> {
    let f = std::fs::File::open(path).with_context(|| format!("open {}", path.display()))?;
    let mut dec = png::Decoder::new(std::io::BufReader::new(f));
    dec.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut rd = dec.read_info()?;
    let mut buf = vec![0; rd.output_buffer_size()];
    let info = rd.next_frame(&mut buf)?;
    let (w, h) = (info.width as usize, info.height as usize);
    let mut sp = Sprite::new(w, h);
    for i in 0..w * h {
        let (r, g, b, a) = match info.color_type {
            png::ColorType::Rgba => (buf[i * 4], buf[i * 4 + 1], buf[i * 4 + 2], buf[i * 4 + 3]),
            png::ColorType::Rgb => (buf[i * 3], buf[i * 3 + 1], buf[i * 3 + 2], 255),
            png::ColorType::GrayscaleAlpha => (buf[i * 2], buf[i * 2], buf[i * 2], buf[i * 2 + 1]),
            png::ColorType::Grayscale => (buf[i], buf[i], buf[i], 255),
            png::ColorType::Indexed => bail!("indexed png was not expanded"),
        };
        let a32 = a as u32;
        sp.px[i * 4] = (r as u32 * a32 / 255) as u8;
        sp.px[i * 4 + 1] = (g as u32 * a32 / 255) as u8;
        sp.px[i * 4 + 2] = (b as u32 * a32 / 255) as u8;
        sp.px[i * 4 + 3] = a;
    }
    Ok(sp)
}

// ---------------------------------------------------------------- compositor

pub struct Compositor {
    base_dir: PathBuf,
    emoji_dir: Option<PathBuf>,
    fonts: Option<Arc<usvg::fontdb::Database>>,
    cache: HashMap<String, Arc<Sprite>>,
}

impl Compositor {
    pub fn new(base_dir: PathBuf, emoji_dir: Option<PathBuf>) -> Compositor {
        Compositor { base_dir, emoji_dir, fonts: None, cache: HashMap::new() }
    }

    pub fn clear_cache(&mut self) {
        self.cache.clear();
    }

    fn fonts(&mut self) -> Arc<usvg::fontdb::Database> {
        self.fonts
            .get_or_insert_with(|| {
                let mut db = usvg::fontdb::Database::new();
                db.load_system_fonts();
                Arc::new(db)
            })
            .clone()
    }

    fn path(&self, p: &str) -> PathBuf {
        let p = Path::new(p);
        if p.is_absolute() { p.to_path_buf() } else { self.base_dir.join(p) }
    }

    /// Rasterises SVG data into a `tw` x `th` premultiplied sprite.
    pub fn svg_sprite(&mut self, data: &[u8], tw: u32, th: u32) -> Result<Sprite> {
        let mut opt = usvg::Options::default();
        opt.resources_dir = Some(self.base_dir.clone());
        if data.windows(5).any(|w| w == b"<text") {
            opt.fontdb = self.fonts();
        }
        let tree = usvg::Tree::from_data(data, &opt).map_err(|e| anyhow!("svg: {e}"))?;
        let size = tree.size();
        let mut pm = tiny_skia::Pixmap::new(tw, th).ok_or_else(|| anyhow!("svg target {tw}x{th} is empty"))?;
        let t = tiny_skia::Transform::from_scale(tw as f32 / size.width(), th as f32 / size.height());
        resvg::render(&tree, t, &mut pm.as_mut());
        Ok(Sprite { w: tw as usize, h: th as usize, px: pm.data().to_vec() })
    }

    fn svg_size(data: &[u8]) -> Option<(f32, f32)> {
        let tree = usvg::Tree::from_data(data, &usvg::Options::default()).ok()?;
        let s = tree.size();
        Some((s.width(), s.height()))
    }

    /// One emoji cluster at `px` x `px`: built-in set, then `--emoji-dir`, then a placeholder.
    pub fn emoji_sprite(&mut self, cluster: &str, px: u32) -> Arc<Sprite> {
        let key = format!("emoji|{cluster}|{px}");
        if let Some(s) = self.cache.get(&key) {
            return s.clone();
        }
        let ek = emoji::key_of(cluster);
        let sp = self.try_emoji(&ek, px).unwrap_or_else(|| placeholder(px as usize));
        let sp = Arc::new(sp);
        self.cache.insert(key, sp.clone());
        sp
    }

    fn try_emoji(&mut self, ek: &str, px: u32) -> Option<Sprite> {
        if let Some(svg) = emoji::by_key(ek) {
            return self.svg_sprite(svg.as_bytes(), px, px).ok();
        }
        let dir = self.emoji_dir.clone()?;
        for cand in [ek.to_string(), ek.replace('-', "_")] {
            let svg = dir.join(format!("{cand}.svg"));
            if let Ok(data) = std::fs::read(&svg) {
                let (w, h) = Self::svg_size(&data).unwrap_or((1.0, 1.0));
                let (tw, th) = if w >= h { (px, ((px as f32) * h / w).round().max(1.0) as u32) } else { (((px as f32) * w / h).round().max(1.0) as u32, px) };
                let s = self.svg_sprite(&data, tw, th).ok()?;
                let mut sq = Sprite::new(px as usize, px as usize);
                sq.over_sprite(&s, (px as i32 - tw as i32) / 2, (px as i32 - th as i32) / 2);
                return Some(sq);
            }
            let png = dir.join(format!("{cand}.png"));
            if png.exists() {
                let s = load_png(&png).ok()?;
                return Some(bilinear(&s, px as usize, px as usize));
            }
        }
        None
    }

    fn text_sprite(&mut self, k: &TextArgs, short: f32, frame_w: f32) -> Sprite {
        let s = ((k.size * short / 5.0).round() as i32).max(1);
        let pad = (k.pad * short).round() as i32;
        let max_px = k.max_width.map(|m| (m * frame_w) as i32 - 2 * pad);
        let lines = layout(&k.text, s, max_px);
        let line_h = 8 * s;
        let widths: Vec<i32> = lines.iter().map(|l| line_w(l, s)).collect();
        let tw = widths.iter().copied().max().unwrap_or(0);
        let th = lines.len() as i32 * line_h;
        let (w, h) = (tw + 2 * pad + s, th + 2 * pad + s);
        let mut sp = Sprite::new(w as usize, h as usize);
        if let Some(bg) = &k.bg {
            sp.round_rect(0.0, 0.0, w as f32, h as f32, (pad as f32).max(2.0 * s as f32), parse_color(bg));
        }
        let color = parse_color(&k.color);
        let outline = k.outline.as_deref().map(parse_color);
        for (li, line) in lines.iter().enumerate() {
            let lw = widths[li];
            let mut x = pad
                + match k.align.as_str() {
                    "left" => 0,
                    "right" => tw - lw,
                    _ => (tw - lw) / 2,
                };
            let y = pad + li as i32 * line_h;
            for run in line {
                match run {
                    Run::Glyph(c) => {
                        if let Some(rows) = font::glyph_rows(*c) {
                            let gy = y + (line_h - 5 * s) / 2;
                            let mut draw = |dx: i32, dy: i32, col: [u8; 4]| {
                                for (ry, row) in rows.iter().enumerate() {
                                    for (rx, ch) in row.chars().enumerate() {
                                        if ch == '#' {
                                            sp.fill(x + dx + rx as i32 * s, gy + dy + ry as i32 * s, s, s, col);
                                        }
                                    }
                                }
                            };
                            if k.shadow {
                                draw(s, s, [0, 0, 0, 150]);
                            }
                            if let Some(o) = outline {
                                for (ox, oy) in [(-1, 0), (1, 0), (0, -1), (0, 1), (-1, -1), (1, 1), (-1, 1), (1, -1)] {
                                    draw(ox * s, oy * s, o);
                                }
                            }
                            draw(0, 0, color);
                        }
                    }
                    Run::Emoji(e) => {
                        let es = self.emoji_sprite(e, (7 * s) as u32);
                        sp.over_sprite(&es, x, y + s / 2);
                    }
                }
                x += run_w(run, s);
            }
        }
        sp
    }

    fn build(&mut self, el: &Element, vars: &Vars, fw: usize, fh: usize) -> Result<Arc<Sprite>> {
        let short = fw.min(fh) as f32;
        let (key, kind) = match &el.kind {
            Kind::Text { text, .. } => {
                let mut k = el.kind.clone();
                if let Kind::Text { text: t, color, bg, outline, .. } = &mut k {
                    *t = interpolate(text, vars);
                    *color = interpolate(color, vars);
                    if let Some(b) = bg {
                        *b = interpolate(b, vars);
                    }
                    if let Some(o) = outline {
                        *o = interpolate(o, vars);
                    }
                }
                (serde_json::to_string(&k)?, k)
            }
            Kind::Bar { bind, value, .. } => {
                let mut k = el.kind.clone();
                let v = bind.as_ref().and_then(|b| vars.get(b)).and_then(|s| s.parse::<f32>().ok()).unwrap_or(*value);
                if let Kind::Bar { value, .. } = &mut k {
                    *value = (v.clamp(0.0, 1.0) * 256.0).round() / 256.0;
                }
                (serde_json::to_string(&k)?, k)
            }
            other => (serde_json::to_string(other)?, other.clone()),
        };
        let key = format!("{key}|{fw}x{fh}");
        if let Some(s) = self.cache.get(&key) {
            return Ok(s.clone());
        }
        if self.cache.len() > 400 {
            self.cache.clear();
        }
        let sp = match kind {
            Kind::Text { text, size, color, shadow, outline, bg, pad, align, max_width } => {
                self.text_sprite(&TextArgs { text, size, color, shadow, outline, bg, pad, align, max_width }, short, fw as f32)
            }
            Kind::Emoji { emoji: e, size } => {
                let px = ((size * short).round() as u32).clamp(1, 2048);
                let pieces = emoji::split(&e);
                let cluster = match pieces.first() {
                    Some(emoji::Piece::Emoji(c)) => c.clone(),
                    _ => bail!("'{e}' is not an emoji (use a character like 🌹 or a short name like :rose:)"),
                };
                let a = self.emoji_sprite(&cluster, px);
                (*a).clone()
            }
            Kind::Svg { src, svg, w, h } => {
                let data = match (src, svg) {
                    (_, Some(s)) => s.into_bytes(),
                    (Some(p), None) => std::fs::read(self.path(&p)).with_context(|| format!("read svg {p}"))?,
                    (None, None) => bail!("svg element needs `src` or `svg`"),
                };
                let (iw, ih) = Self::svg_size(&data).ok_or_else(|| anyhow!("cannot parse svg"))?;
                let (tw, th) = fit(w, h, iw / ih, short, 0.1);
                self.svg_sprite(&data, tw, th)?
            }
            Kind::Image { src, w, h } => {
                let path = self.path(&src);
                if path.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("svg")) {
                    let data = std::fs::read(&path).with_context(|| format!("read {src}"))?;
                    let (iw, ih) = Self::svg_size(&data).ok_or_else(|| anyhow!("cannot parse svg"))?;
                    let (tw, th) = fit(w, h, iw / ih, short, 0.1);
                    self.svg_sprite(&data, tw, th)?
                } else {
                    let img = load_png(&path)?;
                    let (tw, th) = fit(w, h, img.w as f32 / img.h as f32, short, 0.1);
                    bilinear(&img, tw as usize, th as usize)
                }
            }
            Kind::Rect { w, h, color, radius, border, border_w, full } => {
                let (pw, ph) = if full { (fw, fh) } else { (((w * short).round() as usize).max(1), ((h * short).round() as usize).max(1)) };
                let mut sp = Sprite::new(pw, ph);
                let r = radius * short;
                let bw = border_w * short;
                if full {
                    let c = parse_color(&color);
                    for px in sp.px.chunks_exact_mut(4) {
                        let a = c[3] as u32;
                        px[0] = (c[0] as u32 * a / 255) as u8;
                        px[1] = (c[1] as u32 * a / 255) as u8;
                        px[2] = (c[2] as u32 * a / 255) as u8;
                        px[3] = c[3];
                    }
                } else if let (Some(b), true) = (&border, bw > 0.0) {
                    sp.round_rect(0.0, 0.0, pw as f32, ph as f32, r, parse_color(b));
                    sp.round_rect(bw, bw, pw as f32 - 2.0 * bw, ph as f32 - 2.0 * bw, (r - bw).max(0.0), parse_color(&color));
                } else {
                    sp.round_rect(0.0, 0.0, pw as f32, ph as f32, r, parse_color(&color));
                }
                sp
            }
            Kind::Vignette { color, strength } => {
                let mut sp = Sprite::new(fw, fh);
                let c = parse_color(&color);
                let (cx, cy) = (fw as f32 / 2.0, fh as f32 / 2.0);
                for y in 0..fh {
                    let dy = (y as f32 + 0.5 - cy) / cy;
                    for x in 0..fw {
                        let dx = (x as f32 + 0.5 - cx) / cx;
                        let r = (dx * dx + dy * dy).sqrt();
                        let t = ((r - 0.55) / 0.85).clamp(0.0, 1.0);
                        let a = (t * t * strength.clamp(0.0, 1.0) * c[3] as f32) as u32;
                        let i = (y * fw + x) * 4;
                        sp.px[i] = (c[0] as u32 * a / 255) as u8;
                        sp.px[i + 1] = (c[1] as u32 * a / 255) as u8;
                        sp.px[i + 2] = (c[2] as u32 * a / 255) as u8;
                        sp.px[i + 3] = a as u8;
                    }
                }
                sp
            }
            Kind::Bar { w, h, value, color, bg, border, from_right, .. } => {
                let (pw, ph) = (((w * short).round() as usize).max(2), ((h * short).round() as usize).max(2));
                let mut sp = Sprite::new(pw, ph);
                let r = ph as f32 / 2.0;
                let bw = if border.is_some() { (ph as f32 * 0.14).max(1.0) } else { 0.0 };
                if let Some(b) = &border {
                    sp.round_rect(0.0, 0.0, pw as f32, ph as f32, r, parse_color(b));
                }
                sp.round_rect(bw, bw, pw as f32 - 2.0 * bw, ph as f32 - 2.0 * bw, (r - bw).max(0.0), parse_color(&bg));
                let inner_w = pw as f32 - 2.0 * bw;
                let fw_ = inner_w * value;
                if fw_ >= 1.0 {
                    let x = if from_right { bw + inner_w - fw_ } else { bw };
                    sp.round_rect(x, bw, fw_, ph as f32 - 2.0 * bw, (r - bw).max(0.0), parse_color(&color));
                }
                sp
            }
        };
        let sp = Arc::new(sp);
        self.cache.insert(key, sp.clone());
        Ok(sp)
    }

    /// Draws every drawable onto an RGB frame (`fw * fh * 3`).
    pub fn draw(&mut self, frame: &mut [u8], fw: usize, fh: usize, items: &[Drawable], vars: &Vars) {
        for d in items {
            let (alpha, scale, slide) = anim(d.el, d.age, d.left);
            if alpha <= 0.003 || scale <= 0.01 {
                continue;
            }
            let sp = match self.build(d.el, vars, fw, fh) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("[warn] overlay '{}': {e:#}", d.el.id);
                    continue;
                }
            };
            let short = fw.min(fh) as f32;
            let (dw, dh) = (sp.w as f32 * scale, sp.h as f32 * scale);
            let (ax, ay) = match d.el.anchor.as_str() {
                "top-left" => (0.0, 0.0),
                "top" | "top-center" => (0.5, 0.0),
                "top-right" => (1.0, 0.0),
                "left" => (0.0, 0.5),
                "right" => (1.0, 0.5),
                "bottom-left" => (0.0, 1.0),
                "bottom" | "bottom-center" => (0.5, 1.0),
                "bottom-right" => (1.0, 1.0),
                _ => (0.5, 0.5),
            };
            let px = (d.el.at[0] + slide[0]) * fw as f32 + d.el.offset[0] * short - ax * dw;
            let py = (d.el.at[1] + slide[1]) * fh as f32 + d.el.offset[1] * short - ay * dh;
            blit(frame, fw, fh, &sp, px.round() as i32, py.round() as i32, scale, alpha);
        }
    }
}

struct TextArgs {
    text: String,
    size: f32,
    color: String,
    shadow: bool,
    outline: Option<String>,
    bg: Option<String>,
    pad: f32,
    align: String,
    max_width: Option<f32>,
}

fn placeholder(px: usize) -> Sprite {
    let mut sp = Sprite::new(px, px);
    sp.round_rect(0.0, 0.0, px as f32, px as f32, px as f32 * 0.2, [120, 130, 140, 255]);
    let s = (px / 9).max(1) as i32;
    if let Some(rows) = font::glyph_rows('?') {
        let (ox, oy) = ((px as i32 - 3 * s) / 2, (px as i32 - 5 * s) / 2);
        for (ry, row) in rows.iter().enumerate() {
            for (rx, ch) in row.chars().enumerate() {
                if ch == '#' {
                    sp.fill(ox + rx as i32 * s, oy + ry as i32 * s, s, s, [255, 255, 255, 255]);
                }
            }
        }
    }
    sp
}

fn ease_out_back(u: f32) -> f32 {
    let (c1, c3) = (1.70158f32, 2.70158f32);
    1.0 + c3 * (u - 1.0).powi(3) + c1 * (u - 1.0).powi(2)
}

/// (opacity, scale, slide as a fraction of the frame)
fn anim(el: &Element, age: f32, left: Option<f32>) -> (f32, f32, [f32; 2]) {
    let secs = el.anim_secs.max(0.01);
    let mut alpha = el.opacity;
    let mut scale = 1.0;
    let mut slide = [0.0, 0.0];
    let u = (age / secs).clamp(0.0, 1.0);
    match el.anim_in.as_deref() {
        Some("pop") => {
            scale = ease_out_back(u);
            alpha *= (u * 2.0).min(1.0);
        }
        Some("fade") => alpha *= u,
        Some("slide_up") => {
            slide[1] = (1.0 - u) * 0.05;
            alpha *= u;
        }
        Some("slide_down") => {
            slide[1] = -(1.0 - u) * 0.05;
            alpha *= u;
        }
        Some("slide_left") => {
            slide[0] = (1.0 - u) * 0.05;
            alpha *= u;
        }
        Some("slide_right") => {
            slide[0] = -(1.0 - u) * 0.05;
            alpha *= u;
        }
        _ => {}
    }
    if let Some(l) = left {
        let v = (l / secs).clamp(0.0, 1.0);
        match el.anim_out.as_deref() {
            Some("none") => {
                if l <= 0.0 {
                    alpha = 0.0;
                }
            }
            Some("pop") => {
                scale *= 0.6 + 0.4 * v;
                alpha *= v;
            }
            _ => alpha *= v,
        }
    }
    (alpha, scale, slide)
}

/// Premultiplied sprite over an RGB frame, nearest-neighbour scaled by `scale`.
fn blit(frame: &mut [u8], fw: usize, fh: usize, sp: &Sprite, x: i32, y: i32, scale: f32, opacity: f32) {
    let dw = (sp.w as f32 * scale).round() as i32;
    let dh = (sp.h as f32 * scale).round() as i32;
    let op = (opacity.clamp(0.0, 1.0) * 256.0) as u32;
    // when scaled, grow about the sprite's own centre so the anchor stays put
    let (x, y) = if (scale - 1.0).abs() > f32::EPSILON { (x + (sp.w as i32 - dw) / 2, y + (sp.h as i32 - dh) / 2) } else { (x, y) };
    if (scale - 1.0).abs() <= f32::EPSILON {
        // 1:1 (everything but the pop animation): copy rows, no per-pixel mapping
        let (x0, x1) = ((-x).max(0) as usize, ((fw as i32 - x).max(0) as usize).min(sp.w));
        for sy in 0..sp.h {
            let fy = y + sy as i32;
            if fy < 0 || fy as usize >= fh || x0 >= x1 {
                continue;
            }
            let src = &sp.px[(sy * sp.w + x0) * 4..(sy * sp.w + x1) * 4];
            let base = (fy as usize * fw + (x + x0 as i32) as usize) * 3;
            let dst = &mut frame[base..base + (x1 - x0) * 3];
            for (s, d) in src.chunks_exact(4).zip(dst.chunks_exact_mut(3)) {
                if s[3] == 0 {
                    continue;
                }
                let a = s[3] as u32 * op / 256;
                let inv = 255 - a;
                d[0] = ((s[0] as u32 * op / 256) + d[0] as u32 * inv / 255).min(255) as u8;
                d[1] = ((s[1] as u32 * op / 256) + d[1] as u32 * inv / 255).min(255) as u8;
                d[2] = ((s[2] as u32 * op / 256) + d[2] as u32 * inv / 255).min(255) as u8;
            }
        }
        return;
    }
    for dy in 0..dh {
        let fy = y + dy;
        if fy < 0 || fy as usize >= fh {
            continue;
        }
        let sy = ((dy as f32 / scale) as usize).min(sp.h - 1);
        for dx in 0..dw {
            let fx = x + dx;
            if fx < 0 || fx as usize >= fw {
                continue;
            }
            let sx = ((dx as f32 / scale) as usize).min(sp.w - 1);
            let s = &sp.px[(sy * sp.w + sx) * 4..][..4];
            if s[3] == 0 {
                continue;
            }
            let a = s[3] as u32 * op / 256;
            let inv = 255 - a;
            let i = (fy as usize * fw + fx as usize) * 3;
            for k in 0..3 {
                frame[i + k] = ((s[k] as u32 * op / 256) + frame[i + k] as u32 * inv / 255).min(255) as u8;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn comp() -> Compositor {
        Compositor::new(PathBuf::from("."), None)
    }

    #[test]
    fn every_builtin_emoji_renders_visible_pixels() {
        let mut c = comp();
        for (key, name, svg) in emoji::SET {
            let sp = c.svg_sprite(svg.as_bytes(), 48, 48).unwrap_or_else(|e| panic!("emoji {name} ({key}): {e}"));
            let covered = sp.px.chunks_exact(4).filter(|p| p[3] > 8).count();
            assert!(covered > 48 * 48 / 10, "emoji {name} ({key}) is nearly empty: {covered} px");
        }
    }

    #[test]
    fn parses_elements_in_any_shape() {
        let one = parse_elements(r#"{"type":"text","text":"HI","at":[0.5,0.1]}"#).unwrap();
        assert_eq!(one.len(), 1);
        let many = parse_elements(r#"[{"type":"emoji","emoji":"🌹","size":0.1,"ttl":2},{"type":"bar","w":0.3,"h":0.02,"bind":"hp_frac.A"}]"#).unwrap();
        assert_eq!(many.len(), 2);
        let wrapped = parse_elements(r##"{"elements":[{"type":"rect","w":0.1,"h":0.1,"color":"#ff0000"}]}"##).unwrap();
        assert_eq!(wrapped.len(), 1);
        assert!(parse_elements(r#"{"type":"nope"}"#).is_err());
    }

    #[test]
    fn interpolates_known_variables_only() {
        let mut v = Vars::new();
        v.insert("user".into(), "Budi".into());
        assert_eq!(interpolate("{user} sent {gift} {", &v), "Budi sent {gift} {");
    }

    #[test]
    fn ttl_expires_and_patch_changes_content() {
        let mut o = Overlay::new();
        let id = o.add(Element::text("A", 0.04));
        let mut e = Element::text("B", 0.04);
        e.ttl = Some(1.0);
        e.anim_secs = 0.2;
        o.add(e);
        assert_eq!(o.len(), 2);
        o.patch(&id, &serde_json::json!({"text": "NEW"})).unwrap();
        match &o.list()[0].kind {
            Kind::Text { text, .. } => assert_eq!(text, "NEW"),
            _ => panic!(),
        }
        o.tick(0.9);
        assert_eq!(o.len(), 2);
        o.tick(0.2);
        assert_eq!(o.len(), 1);
        assert!(o.remove(&id));
        o.tick(0.5);
        assert!(o.is_empty());
    }

    #[test]
    fn draws_text_emoji_svg_and_bar_into_a_frame() {
        let mut c = comp();
        let (w, h) = (360usize, 640usize);
        let mut frame = vec![20u8; w * h * 3];
        let mut o = Overlay::new();
        o.add(Element::text("BUDI SENT :rose: X3", 0.04).at(0.5, 0.1, "center").id("t"));
        o.add(Element::new(Kind::Emoji { emoji: "🔥".into(), size: 0.1 }).at(0.2, 0.5, "center"));
        o.add(Element::new(Kind::Svg { src: None, svg: Some(r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10"><circle cx="5" cy="5" r="5" fill="#00ff00"/></svg>"##.into()), w: Some(0.1), h: None }).at(0.8, 0.5, "center"));
        o.add(Element::new(Kind::Bar { w: 0.4, h: 0.03, value: 0.5, bind: Some("hp_frac.A".into()), color: "#ff0000".into(), bg: "#000000".into(), border: None, from_right: false }).at(0.5, 0.9, "center"));
        let mut vars = Vars::new();
        vars.insert("hp_frac.A".into(), "0.75".into());
        c.draw(&mut frame, w, h, &o.drawables(), &vars);
        let changed = frame.chunks_exact(3).filter(|p| p != &[20, 20, 20]).count();
        assert!(changed > 800, "overlay drew only {changed} px");
        // emoji area contains orange-ish fire pixels, svg area contains green
        let px = |x: usize, y: usize| &frame[(y * w + x) * 3..][..3];
        let (ex, ey) = ((0.2 * w as f32) as usize, (0.5 * h as f32) as usize);
        assert!(px(ex, ey)[0] > 150 && px(ex, ey)[2] < 120, "fire emoji centre is {:?}", px(ex, ey));
        let (sx, sy) = ((0.8 * w as f32) as usize, (0.5 * h as f32) as usize);
        assert!(px(sx, sy)[1] > 200 && px(sx, sy)[0] < 60, "svg centre is {:?}", px(sx, sy));
        // the bar is 75% full: red at 60% of its width, dark at 90%
        let bw = (0.4 * w as f32) as usize;
        let left = w / 2 - bw / 2;
        let by = (0.9 * h as f32) as usize;
        assert!(px(left + bw * 6 / 10, by)[0] > 200, "bar fill missing");
        assert!(px(left + bw * 9 / 10, by)[0] < 60, "bar should be empty past 75%");
    }
}
