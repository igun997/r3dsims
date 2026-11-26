//! Modular voxel assets.
//!
//! An asset is JSON (or built from Rust, see `templates`): a palette, a tree of rigid
//! *parts* (each a bag of voxels with a pivot) and any number of named animation clips.
//! Voxel coordinates are integers in the model's rest pose: feet at y=0, x/z centered on 0,
//! the model faces +Z. Nothing here is GLB: a villager is a few KB of text.

use std::collections::{BTreeMap, HashMap};

use anyhow::{Context, Result, anyhow, bail};
use glam::{Mat4, Vec3};
use serde::{Deserialize, Serialize};

pub type Rgb = [u8; 3];

/// Parses `#rgb` / `#rrggbb`, optionally `:material` (wood, brick, tile, stone, grass, dirt,
/// fabric, metal, plaster, sand) and a glow suffix: `!` = always glowing (unlit), `*` = glows only
/// at night. Examples: `#b9824f:wood`, `#ffd27a*`, `#ffe08a:metal!`.
/// Returns (color, flags) where `flags & 3` is glow (0 none / 1 always / 2 night only) and
/// `flags >> 2` is the material id (see `texture`).
pub fn parse_hex(s: &str) -> Option<(Rgb, u8)> {
    let s = s.trim();
    let mut glow = 0u8;
    let mut strip = |t: &str| -> String {
        let mut t = t.trim();
        if let Some(r) = t.strip_suffix('!') {
            t = r;
            glow = 1;
        } else if let Some(r) = t.strip_suffix('*') {
            t = r;
            glow = 2;
        }
        t.to_string()
    };
    let (hex, mat) = match s.split_once(':') {
        Some((h, m)) => (strip(h), strip(m)),
        None => (strip(s), String::new()),
    };
    let mat = crate::texture::from_name(&mat)?;
    let h = hex.trim_start_matches('#');
    if !h.is_ascii() {
        return None;
    }
    let byte = |i: usize, n: usize| u8::from_str_radix(&h[i..i + n], 16).ok();
    let flags = glow | (mat << 2);
    match h.len() {
        6 => Some(([byte(0, 2)?, byte(2, 2)?, byte(4, 2)?], flags)),
        3 => Some(([byte(0, 1)? * 17, byte(1, 1)? * 17, byte(2, 1)? * 17], flags)),
        _ => None,
    }
}

fn d_voxel() -> f32 {
    0.1
}
fn yes() -> bool {
    true
}
fn d_ease() -> String {
    "smooth".into()
}

// ---------------------------------------------------------------- file schema

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AssetDef {
    pub name: String,
    /// World size of one voxel (default 0.1; terrain tile = 1.0).
    #[serde(default = "d_voxel")]
    pub voxel: f32,
    /// key -> `#rrggbb`. Append `!` (always glow) or `*` (night glow). Keys are overridable per entity.
    pub palette: BTreeMap<String, String>,
    pub parts: Vec<PartDef>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub anims: BTreeMap<String, ClipDef>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PartDef {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    /// Joint position in rest-pose voxel coordinates; animation rotates around it.
    #[serde(default)]
    pub pivot: [f32; 3],
    pub shape: Vec<Op>,
}

/// Shape operations, applied in order. `c` is a palette key or a literal `#rrggbb`.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Op {
    Box { at: [i32; 3], size: [i32; 3], c: String },
    Sphere { at: [f32; 3], r: f32, c: String },
    Line { from: [i32; 3], to: [i32; 3], c: String },
    /// ASCII art. `layers` bottom -> top (y); each layer is rows back -> front (z); chars are x.
    /// Each char is a palette key; `.` or space is empty.
    Grid { at: [i32; 3], layers: Vec<Vec<String>> },
    Clear { at: [i32; 3], size: [i32; 3] },
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ClipDef {
    pub duration: f32,
    #[serde(default = "yes", rename = "loop")]
    pub looping: bool,
    /// "smooth" (ease between keys) or "linear".
    #[serde(default = "d_ease")]
    pub ease: String,
    pub tracks: BTreeMap<String, TrackDef>,
}

/// Keyframes `[time, [x, y, z]]`. `rot` = euler degrees around the part pivot, `pos` = voxel offset.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct TrackDef {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rot: Vec<(f32, [f32; 3])>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pos: Vec<(f32, [f32; 3])>,
}

// ---------------------------------------------------------------- baked model

#[derive(Clone, Copy, Debug)]
pub struct Voxel {
    pub p: [i16; 3],
    pub c: u8,
    /// exposed-face bitmask: +x -x +y -y +z -z
    pub f: u8,
    /// Ambient occlusion per face, 2 bits per corner (0 = most occluded .. 3 = open), corner order
    /// matching the quad corners the renderer emits.
    pub ao: [u8; 6],
    /// Wall cut-away flags (buildings): bits 0..4 = exterior sides +x -x +z -z, bit 4 = interior
    /// wall (always lowered), bit 7 = cuttable. Zero for ordinary models.
    pub cut: u8,
}

#[derive(Clone, Debug)]
pub struct Part {
    pub name: String,
    pub parent: Option<usize>,
    pub pivot: Vec3,
    pub voxels: Vec<Voxel>,
    /// Voxels at or above this y (part-local rest coordinates) are hidden when their `cut` flags
    /// say the wall is lowered. `i16::MAX` = nothing is ever cut.
    pub cut_y: i16,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Pose {
    pub rot: Vec3,
    pub pos: Vec3,
}

impl Pose {
    pub fn lerp(a: Pose, b: Pose, u: f32) -> Pose {
        Pose { rot: a.rot.lerp(b.rot, u), pos: a.pos.lerp(b.pos, u) }
    }
}

#[derive(Clone, Debug)]
pub struct Track {
    pub part: usize,
    pub rot: Vec<(f32, Vec3)>,
    pub pos: Vec<(f32, Vec3)>,
}

#[derive(Clone, Debug)]
pub struct Clip {
    pub duration: f32,
    pub looping: bool,
    pub smooth: bool,
    pub tracks: Vec<Track>,
}

#[derive(Clone, Debug)]
pub struct Model {
    pub name: String,
    pub voxel: f32,
    pub colors: Vec<Rgb>,
    /// 0 none, 1 always glow, 2 night glow (per palette entry)
    pub glow: Vec<u8>,
    pub color_names: HashMap<String, u8>,
    pub parts: Vec<Part>,
    pub part_index: HashMap<String, usize>,
    pub clips: HashMap<String, Clip>,
    /// Height in world units (rest pose), used for name tags and speech bubbles.
    pub height: f32,
}

fn color_idx(
    c: &str,
    colors: &mut Vec<Rgb>,
    glow: &mut Vec<u8>,
    names: &mut HashMap<String, u8>,
) -> Result<u8> {
    if let Some(&i) = names.get(c) {
        return Ok(i);
    }
    if c.starts_with('#') {
        let (rgb, g) = parse_hex(c).ok_or_else(|| anyhow!("bad color '{c}'"))?;
        if colors.len() >= 255 {
            bail!("palette larger than 255 colors");
        }
        let i = colors.len() as u8;
        colors.push(rgb);
        glow.push(g);
        names.insert(c.to_string(), i);
        return Ok(i);
    }
    bail!("unknown palette key '{c}'")
}

const DIRS: [[i32; 3]; 6] = [[1, 0, 0], [-1, 0, 0], [0, 1, 0], [0, -1, 0], [0, 0, 1], [0, 0, -1]];

impl Model {
    pub fn bake(def: &AssetDef) -> Result<Model> {
        let mut colors = vec![];
        let mut glow = vec![];
        let mut names = HashMap::new();
        for (k, v) in &def.palette {
            let (c, g) = parse_hex(v).with_context(|| format!("palette '{k}': bad color '{v}'"))?;
            if colors.len() >= 255 {
                bail!("palette larger than 255 colors");
            }
            names.insert(k.clone(), colors.len() as u8);
            colors.push(c);
            glow.push(g);
        }

        // parents first
        let n = def.parts.len();
        let mut order: Vec<usize> = vec![];
        let mut placed = vec![false; n];
        let by_name: HashMap<&str, usize> =
            def.parts.iter().enumerate().map(|(i, p)| (p.name.as_str(), i)).collect();
        while order.len() < n {
            let before = order.len();
            for (i, p) in def.parts.iter().enumerate() {
                if placed[i] {
                    continue;
                }
                let ok = match &p.parent {
                    None => true,
                    Some(pn) => match by_name.get(pn.as_str()) {
                        Some(&pi) => placed[pi],
                        None => bail!("part '{}': unknown parent '{pn}'", p.name),
                    },
                };
                if ok {
                    placed[i] = true;
                    order.push(i);
                }
            }
            if order.len() == before {
                bail!("part hierarchy has a cycle");
            }
        }
        let new_index: HashMap<&str, usize> =
            order.iter().enumerate().map(|(ni, &oi)| (def.parts[oi].name.as_str(), ni)).collect();

        let mut parts = vec![];
        let mut max_y = 0i32;
        for &oi in &order {
            let pd = &def.parts[oi];
            let mut grid: HashMap<[i32; 3], u8> = HashMap::new();
            for op in &pd.shape {
                apply_op(op, &mut grid, &mut colors, &mut glow, &mut names)
                    .with_context(|| format!("part '{}'", pd.name))?;
            }
            let mut voxels: Vec<Voxel> = grid
                .iter()
                .filter_map(|(p, &c)| {
                    let mut f = 0u8;
                    let mut ao = [3u8 | 3 << 2 | 3 << 4 | 3 << 6; 6];
                    for (k, d) in DIRS.iter().enumerate() {
                        if !grid.contains_key(&[p[0] + d[0], p[1] + d[1], p[2] + d[2]]) {
                            f |= 1 << k;
                            ao[k] = face_ao(&grid, *p, k);
                        }
                    }
                    (f != 0).then_some(Voxel { p: [p[0] as i16, p[1] as i16, p[2] as i16], c, f, ao, cut: 0 })
                })
                .collect();
            voxels.sort_by_key(|v| (v.p[1], v.p[2], v.p[0]));
            max_y = max_y.max(grid.keys().map(|p| p[1] + 1).max().unwrap_or(0));
            parts.push(Part {
                name: pd.name.clone(),
                parent: pd.parent.as_deref().map(|s| new_index[s]),
                pivot: Vec3::from(pd.pivot),
                voxels,
                cut_y: i16::MAX,
            });
        }
        let part_index: HashMap<String, usize> =
            parts.iter().enumerate().map(|(i, p)| (p.name.clone(), i)).collect();

        let mut clips = HashMap::new();
        for (name, cd) in &def.anims {
            clips.insert(
                name.clone(),
                bake_clip(cd, &part_index).with_context(|| format!("anim '{name}'"))?,
            );
        }
        Ok(Model {
            name: def.name.clone(),
            voxel: def.voxel,
            colors,
            glow,
            color_names: names,
            parts,
            part_index,
            clips,
            height: max_y as f32 * def.voxel,
        })
    }

    pub fn voxel_count(&self) -> usize {
        self.parts.iter().map(|p| p.voxels.len()).sum()
    }
}

/// Corner AO levels of face `k` of the voxel at `p`: for each of the four corners (in the quad's
/// corner order) 0..=3 from the two side neighbours and the diagonal one.
fn face_ao(grid: &HashMap<[i32; 3], u8>, p: [i32; 3], k: usize) -> u8 {
    const CORNERS: [(i32, i32); 4] = [(-1, -1), (1, -1), (1, 1), (-1, 1)];
    let a = k / 2;
    let s = if k % 2 == 0 { 1 } else { -1 };
    let (t1, t2) = ((a + 1) % 3, (a + 2) % 3);
    let mut out = 0u8;
    for (ci, (sx, sy)) in CORNERS.iter().enumerate() {
        let mut base = p;
        base[a] += s;
        let occ = |d1: i32, d2: i32| {
            let mut q = base;
            q[t1] += d1;
            q[t2] += d2;
            grid.contains_key(&q) as i32
        };
        let (s1, s2, c) = (occ(*sx, 0), occ(0, *sy), occ(*sx, *sy));
        let lvl = if s1 == 1 && s2 == 1 { 0 } else { 3 - (s1 + s2 + c) };
        out |= (lvl as u8) << (ci * 2);
    }
    out
}

fn apply_op(
    op: &Op,
    grid: &mut HashMap<[i32; 3], u8>,
    colors: &mut Vec<Rgb>,
    glow: &mut Vec<u8>,
    names: &mut HashMap<String, u8>,
) -> Result<()> {
    match op {
        Op::Box { at, size, c } => {
            let ci = color_idx(c, colors, glow, names)?;
            for x in at[0]..at[0] + size[0] {
                for y in at[1]..at[1] + size[1] {
                    for z in at[2]..at[2] + size[2] {
                        grid.insert([x, y, z], ci);
                    }
                }
            }
        }
        Op::Sphere { at, r, c } => {
            let ci = color_idx(c, colors, glow, names)?;
            let (lo, hi) = (|v: f32| (v - r).floor() as i32, |v: f32| (v + r).ceil() as i32);
            for x in lo(at[0])..=hi(at[0]) {
                for y in lo(at[1])..=hi(at[1]) {
                    for z in lo(at[2])..=hi(at[2]) {
                        let d = Vec3::new(x as f32 + 0.5, y as f32 + 0.5, z as f32 + 0.5) - Vec3::from(*at);
                        if d.length_squared() <= r * r {
                            grid.insert([x, y, z], ci);
                        }
                    }
                }
            }
        }
        Op::Line { from, to, c } => {
            let ci = color_idx(c, colors, glow, names)?;
            let a = Vec3::new(from[0] as f32, from[1] as f32, from[2] as f32);
            let b = Vec3::new(to[0] as f32, to[1] as f32, to[2] as f32);
            let steps = (b - a).abs().max_element().ceil().max(1.0) as i32;
            for i in 0..=steps {
                let p = a.lerp(b, i as f32 / steps as f32).round();
                grid.insert([p.x as i32, p.y as i32, p.z as i32], ci);
            }
        }
        Op::Grid { at, layers } => {
            for (y, layer) in layers.iter().enumerate() {
                for (z, row) in layer.iter().enumerate() {
                    for (x, ch) in row.chars().enumerate() {
                        if ch == '.' || ch == ' ' {
                            continue;
                        }
                        let ci = color_idx(&ch.to_string(), colors, glow, names)?;
                        grid.insert([at[0] + x as i32, at[1] + y as i32, at[2] + z as i32], ci);
                    }
                }
            }
        }
        Op::Clear { at, size } => {
            for x in at[0]..at[0] + size[0] {
                for y in at[1]..at[1] + size[1] {
                    for z in at[2]..at[2] + size[2] {
                        grid.remove(&[x, y, z]);
                    }
                }
            }
        }
    }
    Ok(())
}

pub fn bake_clip(cd: &ClipDef, part_index: &HashMap<String, usize>) -> Result<Clip> {
    if !(cd.duration > 0.0) {
        bail!("duration must be > 0");
    }
    let mut tracks = vec![];
    for (pname, td) in &cd.tracks {
        let part = *part_index.get(pname).ok_or_else(|| anyhow!("unknown part '{pname}'"))?;
        let conv = |k: &Vec<(f32, [f32; 3])>| {
            let mut v: Vec<(f32, Vec3)> = k.iter().map(|(t, a)| (*t, Vec3::from(*a))).collect();
            v.sort_by(|a, b| a.0.total_cmp(&b.0));
            v
        };
        tracks.push(Track { part, rot: conv(&td.rot), pos: conv(&td.pos) });
    }
    Ok(Clip { duration: cd.duration, looping: cd.looping, smooth: cd.ease != "linear", tracks })
}

fn sample_keys(k: &[(f32, Vec3)], t: f32, dur: f32, looping: bool, smooth: bool) -> Vec3 {
    let n = k.len();
    if n == 0 {
        return Vec3::ZERO;
    }
    if n == 1 {
        return k[0].1;
    }
    let t = if looping { t.rem_euclid(dur) } else { t.clamp(0.0, dur) };
    let seg = (0..n - 1).find(|&j| t >= k[j].0 && t < k[j + 1].0);
    let (a, b, ta, tb) = match seg {
        Some(j) => (k[j].1, k[j + 1].1, k[j].0, k[j + 1].0),
        None if t >= k[n - 1].0 => {
            if looping {
                (k[n - 1].1, k[0].1, k[n - 1].0, k[0].0 + dur)
            } else {
                return k[n - 1].1;
            }
        }
        None => {
            if looping {
                (k[n - 1].1, k[0].1, k[n - 1].0 - dur, k[0].0)
            } else {
                return k[0].1;
            }
        }
    };
    let mut u = if tb > ta { ((t - ta) / (tb - ta)).clamp(0.0, 1.0) } else { 1.0 };
    if smooth {
        u = u * u * (3.0 - 2.0 * u);
    }
    a.lerp(b, u)
}

impl Clip {
    /// Writes the pose of every animated part into `out` (other parts untouched).
    pub fn sample(&self, t: f32, out: &mut [Pose]) {
        for tr in &self.tracks {
            if let Some(slot) = out.get_mut(tr.part) {
                slot.rot = sample_keys(&tr.rot, t, self.duration, self.looping, self.smooth);
                slot.pos = sample_keys(&tr.pos, t, self.duration, self.looping, self.smooth);
            }
        }
    }
}

/// World matrices of every part. `root` must already contain translation, yaw and voxel scale.
pub fn part_matrices(model: &Model, pose: &[Pose], root: Mat4, out: &mut Vec<Mat4>) {
    out.clear();
    for (i, part) in model.parts.iter().enumerate() {
        let (base, off) = match part.parent {
            Some(p) => (out[p], part.pivot - model.parts[p].pivot),
            None => (root, part.pivot),
        };
        let p = pose.get(i).copied().unwrap_or_default();
        let r = p.rot * (std::f32::consts::PI / 180.0);
        let m = Mat4::from_translation(off + p.pos) * Mat4::from_euler(glam::EulerRot::XYZ, r.x, r.y, r.z);
        out.push(base * m);
    }
}

/// Loads `.json` (AssetDef) or `.vox` (MagicaVoxel) from disk and bakes it.
pub fn load_model(path: &std::path::Path) -> Result<Model> {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    let def = if ext == "vox" {
        let bytes = std::fs::read(path).with_context(|| format!("read {}", path.display()))?;
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("vox");
        crate::vox::import(&bytes, stem)?
    } else {
        let txt = std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
        serde_json::from_str::<AssetDef>(&txt).with_context(|| format!("parse {}", path.display()))?
    };
    Model::bake(&def).with_context(|| format!("asset {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_and_glow() {
        assert_eq!(parse_hex("#fff"), Some(([255, 255, 255], 0)));
        assert_eq!(parse_hex("#102030!"), Some(([16, 32, 48], 1)));
        assert_eq!(parse_hex("#102030*"), Some(([16, 32, 48], 2)));
        assert_eq!(parse_hex("nope"), None);
    }

    #[test]
    fn looping_keys_wrap() {
        let k = [(0.0, Vec3::ZERO), (1.0, Vec3::new(10.0, 0.0, 0.0))];
        // duration 2: 0 -> 10 (t 0..1) -> 0 (t 1..2 wraps to first key)
        let a = sample_keys(&k, 1.5, 2.0, true, false);
        assert!((a.x - 5.0).abs() < 1e-4, "{a:?}");
        let b = sample_keys(&k, 2.5, 2.0, true, false);
        assert!((b.x - 5.0).abs() < 1e-4, "{b:?}");
    }

    #[test]
    fn bake_culls_interior_voxels() {
        let def = AssetDef {
            name: "cube".into(),
            voxel: 0.1,
            palette: [("a".to_string(), "#ff0000".to_string())].into(),
            parts: vec![PartDef {
                name: "b".into(),
                parent: None,
                pivot: [0.0; 3],
                shape: vec![Op::Box { at: [0, 0, 0], size: [3, 3, 3], c: "a".into() }],
            }],
            anims: Default::default(),
        };
        let m = Model::bake(&def).unwrap();
        assert_eq!(m.voxel_count(), 26); // 27 minus the hidden center
    }

    #[test]
    fn bad_parent_is_error() {
        let def = AssetDef {
            name: "x".into(),
            voxel: 0.1,
            palette: Default::default(),
            parts: vec![PartDef { name: "b".into(), parent: Some("zzz".into()), pivot: [0.0; 3], shape: vec![] }],
            anims: Default::default(),
        };
        assert!(Model::bake(&def).is_err());
    }
}
