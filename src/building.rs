//! Dollhouse buildings: floor plans as ASCII, built into a voxel model with one part per floor.
//! The camera sees into the rooms from above (no roof). Walls facing the camera are lowered
//! automatically ("cut-away"), floors above a chosen one can be hidden, and floors can be pulled
//! apart vertically (exploded view) with animated steps.
//!
//! Plan characters: `#` wall, `W` window, `D` door (opening), `.` wooden floor, `c` carpet,
//! `t` tiles, `<` `>` `^` `v` stairs rising toward -x, +x, -z, +z (a run of N tiles climbs one
//! storey; leave a hole `' '` above it on the next floor), `' '` nothing (outside / hole).

use std::collections::BTreeMap;
use std::sync::Arc;

use anyhow::{Result, bail};
use glam::{Mat4, Vec3};
use serde::{Deserialize, Serialize};

use crate::mesh::{Mesh, Placed, push_model};
use crate::model::{AssetDef, Model, Op, PartDef, Pose, Rgb};

fn d_voxel() -> f32 {
    0.25
}
fn d_tile() -> i32 {
    2
}
fn d_floor_height() -> i32 {
    12
}
fn d_slab() -> i32 {
    2
}
fn d_cut() -> i32 {
    3
}
fn d_auto() -> String {
    "auto".into()
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct BuildingDef {
    pub id: String,
    /// World [x, z] of the plan's top-left corner (use whole numbers so doors meet terrain tiles).
    pub pos: [f32; 2],
    /// World size of one building voxel.
    #[serde(default = "d_voxel")]
    pub voxel: f32,
    /// Voxels per plan character.
    #[serde(default = "d_tile")]
    pub tile: i32,
    /// Voxels from one floor's base to the next.
    #[serde(default = "d_floor_height")]
    pub floor_height: i32,
    /// Floor slab thickness in voxels.
    #[serde(default = "d_slab")]
    pub slab: i32,
    /// Height in voxels (above the slab) that cut-away walls are lowered to.
    #[serde(default = "d_cut")]
    pub cut_height: i32,
    /// `auto` (walls facing the camera are lowered), `none`, or `low` (all walls lowered).
    #[serde(default = "d_auto")]
    pub cutaway: String,
    /// Colour overrides: wall, trim, floor, carpet, tile, slab, glass, stair, stair_top.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub palette: BTreeMap<String, String>,
    pub floors: Vec<FloorDef>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct FloorDef {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    /// Rows back (z = 0) to front; characters left (x = 0) to right.
    pub plan: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Void,
    Wall,
    Window,
    Door,
    Floor,
    Stairs,
}

impl Kind {
    pub fn walkable(self) -> bool {
        matches!(self, Kind::Door | Kind::Floor | Kind::Stairs)
    }
    fn is_wall(self) -> bool {
        matches!(self, Kind::Wall | Kind::Window | Kind::Door)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Stair {
    /// Direction of ascent in plan cells.
    pub dir: (i32, i32),
    /// Local heights (world units above the floor base) at the low and high edge of this tile.
    pub lo: f32,
    pub hi: f32,
    /// Is this the highest tile of its run.
    pub top: bool,
}

pub struct FloorGrid {
    pub kind: Vec<Kind>,
    pub stair: Vec<Option<Stair>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cutaway {
    Auto,
    None,
    Low,
}

impl Cutaway {
    pub fn parse(s: &str) -> Option<Cutaway> {
        match s {
            "auto" => Some(Cutaway::Auto),
            "none" | "off" => Some(Cutaway::None),
            "low" | "all" => Some(Cutaway::Low),
            _ => None,
        }
    }
}

pub struct Building {
    pub id: String,
    pub model: Arc<Model>,
    pub colors: Vec<Rgb>,
    /// x, base y, z of the plan origin.
    pub origin: Vec3,
    pub voxel: f32,
    pub cell: f32,
    pub floor_units: f32,
    pub slab_units: f32,
    pub cols: i32,
    pub rows: i32,
    pub names: Vec<String>,
    pub grids: Vec<FloorGrid>,
    /// Floors 0..show are drawn.
    pub show: usize,
    /// Extra vertical gap between floors (exploded view), world units.
    pub gap: f32,
    pub gap_tween: Option<(f32, f32, f32, f32)>,
    pub cutaway: Cutaway,
    pub pose: Vec<Pose>,
    pub warnings: Vec<String>,
    /// Initial cut-away mode, restored by `reset` when a looping story restarts.
    home_cutaway: Cutaway,
}

const MAX_FLOORS: u16 = 8;

pub fn layer_id(building: usize, floor: usize) -> u16 {
    1 + building as u16 * MAX_FLOORS + floor as u16
}

pub fn layer_parts(layer: u16) -> Option<(usize, usize)> {
    layer.checked_sub(1).map(|l| ((l / MAX_FLOORS) as usize, (l % MAX_FLOORS) as usize))
}

fn kind_of(ch: char) -> Option<(Kind, &'static str)> {
    Some(match ch {
        '#' => (Kind::Wall, "wall"),
        'W' => (Kind::Window, "wall"),
        'D' => (Kind::Door, "floor"),
        '.' => (Kind::Floor, "floor"),
        'c' => (Kind::Floor, "carpet"),
        't' => (Kind::Floor, "tile"),
        '<' | '>' | '^' | 'v' => (Kind::Stairs, "stair"),
        ' ' => (Kind::Void, ""),
        _ => return None,
    })
}

fn stair_dir(ch: char) -> (i32, i32) {
    match ch {
        '<' => (-1, 0),
        '>' => (1, 0),
        '^' => (0, -1),
        _ => (0, 1),
    }
}

const DEFAULT_PALETTE: [(&str, &str); 9] = [
    ("wall", "#ece3d3:plaster"),
    ("trim", "#f7f2e8"),
    ("floor", "#b98550:wood"),
    ("carpet", "#6f8fc2:fabric"),
    ("tile", "#dfe8eb:tile"),
    ("slab", "#cfc7b9:plaster"),
    ("glass", "#bfe6f5*"),
    ("stair", "#a0703f:wood"),
    ("stair_top", "#c58f57:wood"),
];

fn bx(at: [i32; 3], size: [i32; 3], c: &str) -> Op {
    Op::Box { at, size, c: c.into() }
}

impl Building {
    pub fn build(def: &BuildingDef, base_y: f32) -> Result<Building> {
        if def.floors.is_empty() || def.floors.len() > MAX_FLOORS as usize {
            bail!("a building needs 1..={MAX_FLOORS} floors");
        }
        if def.tile < 1 || def.floor_height < def.slab + 10 || def.slab < 2 {
            bail!("need tile >= 1, slab >= 2 and floor_height >= slab + 10");
        }
        let (tile, fh, slab) = (def.tile, def.floor_height, def.slab);
        let rows = def.floors.iter().map(|f| f.plan.len()).max().unwrap_or(0) as i32;
        let cols = def.floors.iter().flat_map(|f| f.plan.iter().map(|r| r.chars().count())).max().unwrap_or(0) as i32;
        if rows == 0 || cols == 0 {
            bail!("empty floor plan");
        }
        let mut warnings = vec![];

        let mut palette: BTreeMap<String, String> = DEFAULT_PALETTE.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        palette.extend(def.palette.clone());

        // grids
        let mut grids: Vec<FloorGrid> = vec![];
        let mut chars: Vec<Vec<Vec<char>>> = vec![];
        for (f, floor) in def.floors.iter().enumerate() {
            let mut kind = vec![Kind::Void; (cols * rows) as usize];
            let mut grid_chars = vec![vec![' '; cols as usize]; rows as usize];
            for (z, row) in floor.plan.iter().enumerate() {
                for (x, ch) in row.chars().enumerate() {
                    let Some((k, _)) = kind_of(ch) else {
                        bail!("floor {f}: unknown plan character '{ch}' at x={x} z={z}");
                    };
                    kind[z * cols as usize + x] = k;
                    grid_chars[z][x] = ch;
                }
            }
            grids.push(FloorGrid { kind, stair: vec![None; (cols * rows) as usize] });
            chars.push(grid_chars);
        }

        let unit = def.voxel;
        let mut parts = vec![];
        for (f, floor_chars) in chars.iter().enumerate() {
            let y0 = f as i32 * fh;
            let mut ops: Vec<Op> = vec![];
            let at = |cx: i32, cz: i32| -> Option<char> {
                (cx >= 0 && cz >= 0 && cx < cols && cz < rows).then(|| floor_chars[cz as usize][cx as usize])
            };
            // stair runs
            let mut seen = vec![false; (cols * rows) as usize];
            for cz in 0..rows {
                for cx in 0..cols {
                    let ch = floor_chars[cz as usize][cx as usize];
                    if !matches!(ch, '<' | '>' | '^' | 'v') || seen[(cz * cols + cx) as usize] {
                        continue;
                    }
                    let d = stair_dir(ch);
                    // walk back to the start of the run, then forward collecting tiles
                    let (mut sx, mut sz) = (cx, cz);
                    while at(sx - d.0, sz - d.1) == Some(ch) {
                        sx -= d.0;
                        sz -= d.1;
                    }
                    let mut run = vec![];
                    let (mut px, mut pz) = (sx, sz);
                    while at(px, pz) == Some(ch) {
                        run.push((px, pz));
                        px += d.0;
                        pz += d.1;
                    }
                    let n = run.len() as i32;
                    let top_of = |i: i32| slab + ((i + 1) * fh + n / 2) / n;
                    for (i, &(rx, rz)) in run.iter().enumerate() {
                        seen[(rz * cols + rx) as usize] = true;
                        let top = top_of(i as i32);
                        let lo = if i == 0 { slab } else { top_of(i as i32 - 1) };
                        grids[f].stair[(rz * cols + rx) as usize] =
                            Some(Stair { dir: d, lo: lo as f32 * unit, hi: top as f32 * unit, top: i as i32 == n - 1 });
                        let (x0, z0) = (rx * tile, rz * tile);
                        ops.push(bx([x0, y0, z0], [tile, top, tile], "stair"));
                        ops.push(bx([x0, y0 + top - 1, z0], [tile, 1, tile], "stair_top"));
                    }
                    // the landing tile must be walkable on the next floor and the hole must be open
                    let (lx, lz) = (px, pz);
                    let landing_ok = def.floors.get(f + 1).is_some_and(|nf| {
                        nf.plan.get(lz as usize).and_then(|r| r.chars().nth(lx as usize)).and_then(kind_of).is_some_and(|(k, _)| k.walkable() && k != Kind::Stairs)
                    });
                    if !landing_ok {
                        warnings.push(format!("floor {f}: stairs at x={sx} z={sz} have no walkable landing tile at x={lx} z={lz} on floor {}", f + 1));
                    }
                    if let Some(nf) = def.floors.get(f + 1) {
                        for &(rx, rz) in &run {
                            let above = nf.plan.get(rz as usize).and_then(|r| r.chars().nth(rx as usize)).unwrap_or(' ');
                            if above != ' ' {
                                warnings.push(format!("floor {}: x={rx} z={rz} should be ' ' (hole above the stairs), found '{above}'", f + 1));
                            }
                        }
                    }
                }
            }
            for cz in 0..rows {
                for cx in 0..cols {
                    let ch = floor_chars[cz as usize][cx as usize];
                    let Some((k, key)) = kind_of(ch) else { continue };
                    let (x0, z0) = (cx * tile, cz * tile);
                    match k {
                        Kind::Void | Kind::Stairs => {}
                        Kind::Floor => {
                            ops.push(bx([x0, y0, z0], [tile, slab - 1, tile], "slab"));
                            ops.push(bx([x0, y0 + slab - 1, z0], [tile, 1, tile], key));
                        }
                        Kind::Door => {
                            ops.push(bx([x0, y0, z0], [tile, slab - 1, tile], "slab"));
                            ops.push(bx([x0, y0 + slab - 1, z0], [tile, 1, tile], key));
                            ops.push(bx([x0, y0 + slab + 9, z0], [tile, fh - slab - 9, tile], "wall"));
                            ops.push(bx([x0, y0 + fh - 1, z0], [tile, 1, tile], "trim"));
                        }
                        Kind::Wall | Kind::Window => {
                            ops.push(bx([x0, y0, z0], [tile, slab, tile], "slab"));
                            ops.push(bx([x0, y0 + slab, z0], [tile, fh - slab, tile], key));
                            if k == Kind::Window {
                                ops.push(bx([x0, y0 + slab + 4, z0], [tile, 4, tile], "glass"));
                            }
                            ops.push(bx([x0, y0 + fh - 1, z0], [tile, 1, tile], "trim"));
                        }
                    }
                }
            }
            parts.push(PartDef { name: format!("floor{f}"), parent: None, pivot: [0.0; 3], shape: ops });
        }

        let asset = AssetDef { name: def.id.clone(), voxel: unit, palette, parts, anims: BTreeMap::new() };
        let mut model = Model::bake(&asset)?;

        // wall cut-away flags: exterior walls lower when the camera is on their outside, interior
        // walls always lower
        let kind_at = |f: usize, cx: i32, cz: i32| -> Kind {
            if cx < 0 || cz < 0 || cx >= cols || cz >= rows { Kind::Void } else { grids[f].kind[(cz * cols + cx) as usize] }
        };
        for (f, part) in model.parts.iter_mut().enumerate() {
            let y_floor = f as i16 * fh as i16;
            part.cut_y = y_floor + slab as i16 + def.cut_height as i16;
            for v in part.voxels.iter_mut() {
                let (cx, cz) = ((v.p[0] as i32).div_euclid(tile), (v.p[2] as i32).div_euclid(tile));
                let k = kind_at(f, cx, cz);
                if !k.is_wall() || (v.p[1] as i32) < y_floor as i32 + slab {
                    continue;
                }
                let mut ext = 0u8;
                for (bit, (dx, dz)) in [(0, (1, 0)), (1, (-1, 0)), (2, (0, 1)), (3, (0, -1))] {
                    if kind_at(f, cx + dx, cz + dz) == Kind::Void {
                        ext |= 1 << bit;
                    }
                }
                v.cut = 0x80 | ext | if ext == 0 { 0x10 } else { 0 };
            }
        }

        let colors = model.colors.clone();
        let n = def.floors.len();
        Ok(Building {
            id: def.id.clone(),
            model: Arc::new(model),
            colors,
            origin: Vec3::new(def.pos[0], base_y, def.pos[1]),
            voxel: unit,
            cell: tile as f32 * unit,
            floor_units: fh as f32 * unit,
            slab_units: slab as f32 * unit,
            cols,
            rows,
            names: def.floors.iter().enumerate().map(|(i, f)| if f.name.is_empty() { format!("Floor {}", i + 1) } else { f.name.clone() }).collect(),
            grids,
            show: n,
            gap: 0.0,
            gap_tween: None,
            cutaway: Cutaway::parse(&def.cutaway).unwrap_or(Cutaway::Auto),
            pose: vec![Pose::default(); n],
            warnings,
            home_cutaway: Cutaway::parse(&def.cutaway).unwrap_or(Cutaway::Auto),
        })
    }

    /// Back to the authored state: all floors, no gap, the scene's cut-away mode.
    pub fn reset(&mut self) {
        self.show = self.floors();
        self.gap = 0.0;
        self.gap_tween = None;
        self.cutaway = self.home_cutaway;
        self.sync_pose();
    }

    pub fn floors(&self) -> usize {
        self.grids.len()
    }

    /// World y of the base of floor `f` (slab bottom), including the exploded gap.
    pub fn floor_y(&self, f: usize) -> f32 {
        self.origin.y + f as f32 * (self.floor_units + self.gap)
    }

    pub fn cell_of(&self, x: f32, z: f32) -> (i32, i32) {
        (((x - self.origin.x) / self.cell).floor() as i32, ((z - self.origin.z) / self.cell).floor() as i32)
    }

    pub fn kind_at(&self, f: usize, cx: i32, cz: i32) -> Kind {
        if f >= self.grids.len() || cx < 0 || cz < 0 || cx >= self.cols || cz >= self.rows {
            Kind::Void
        } else {
            self.grids[f].kind[(cz * self.cols + cx) as usize]
        }
    }

    /// Height of the walking surface above the floor base at a world position (stairs ramp).
    pub fn ground_local(&self, f: usize, x: f32, z: f32) -> f32 {
        let (cx, cz) = self.cell_of(x, z);
        if f < self.grids.len() && cx >= 0 && cz >= 0 && cx < self.cols && cz < self.rows {
            if let Some(s) = self.grids[f].stair[(cz * self.cols + cx) as usize] {
                let c = (self.origin.x + (cx as f32 + 0.5) * self.cell, self.origin.z + (cz as f32 + 0.5) * self.cell);
                let t = (0.5 + ((x - c.0) * s.dir.0 as f32 + (z - c.1) * s.dir.1 as f32) / self.cell).clamp(0.0, 1.0);
                return s.lo + (s.hi - s.lo) * t;
            }
        }
        self.slab_units
    }

    pub fn ground_y(&self, f: usize, x: f32, z: f32) -> f32 {
        self.floor_y(f) + self.ground_local(f, x, z)
    }

    /// Sets which floors are drawn (`None` = all), the exploded gap and the cut-away mode.
    pub fn configure(&mut self, show_floor: Option<i32>, gap: Option<f32>, cutaway: Option<Cutaway>, secs: f32) {
        if let Some(s) = show_floor {
            self.show = if s < 0 { self.floors() } else { (s as usize + 1).min(self.floors()) };
        }
        if let Some(c) = cutaway {
            self.cutaway = c;
        }
        if let Some(g) = gap {
            if secs > 0.0 {
                self.gap_tween = Some((self.gap, g.max(0.0), 0.0, secs));
            } else {
                self.gap = g.max(0.0);
                self.gap_tween = None;
            }
        }
        self.sync_pose();
    }

    pub fn update(&mut self, dt: f32) {
        if let Some((from, to, t, dur)) = &mut self.gap_tween {
            *t += dt;
            let u = (*t / *dur).clamp(0.0, 1.0);
            let u = u * u * (3.0 - 2.0 * u);
            self.gap = *from + (*to - *from) * u;
            if *t >= *dur {
                self.gap = *to;
                self.gap_tween = None;
            }
            self.sync_pose();
        }
    }

    fn sync_pose(&mut self) {
        for (f, p) in self.pose.iter_mut().enumerate() {
            p.pos.y = f as f32 * self.gap / self.voxel;
        }
    }

    /// Adds this building's visible geometry for a camera at `yaw` degrees.
    pub fn push(&self, id: u32, yaw: f32, mats: &mut Vec<Mat4>, mesh: &mut Mesh) {
        let facing = match self.cutaway {
            Cutaway::Low => 0x0f,
            _ => crate::mesh::facing_sides(yaw),
        };
        let placed = Placed {
            model: &self.model,
            colors: &self.colors,
            root: Mat4::from_translation(self.origin) * Mat4::from_scale(Vec3::splat(self.voxel)),
            pose: &self.pose,
            id,
            parts_shown: self.show,
            facing,
            lower_interior: self.cutaway != Cutaway::None,
            cutaway: self.cutaway != Cutaway::None,
        };
        push_model(&placed, mats, mesh);
    }

    /// Plan cells (floor, cx, cz) whose rect overlaps a world-space rectangle, for footprints.
    pub fn cells_in(&self, x0: f32, z0: f32, x1: f32, z1: f32) -> Vec<(i32, i32)> {
        let (a, b) = (self.cell_of(x0, z0), self.cell_of(x1, z1));
        let mut v = vec![];
        for cz in a.1.max(0)..=b.1.min(self.rows - 1) {
            for cx in a.0.max(0)..=b.0.min(self.cols - 1) {
                v.push((cx, cz));
            }
        }
        v
    }
}

/// Which building (by index) and floor an entity declares, resolved against a list of buildings.
pub fn find(buildings: &[Building], id: &str) -> Option<usize> {
    buildings.iter().position(|b| b.id == id)
}


#[cfg(test)]
mod tests {
    use super::*;

    fn tiny() -> BuildingDef {
        let plan = |rows: &[&str]| FloorDef { name: String::new(), plan: rows.iter().map(|s| s.to_string()).collect() };
        BuildingDef {
            id: "t".into(),
            pos: [0.0, 0.0],
            voxel: 0.25,
            tile: 2,
            floor_height: 12,
            slab: 2,
            cut_height: 3,
            cutaway: "auto".into(),
            palette: BTreeMap::new(),
            floors: vec![
                plan(&["#########", "#.......#", "#.>>>>>.#", "#.......#", "####D####"]),
                plan(&["#########", "#.......#", "#.     .#", "#.......#", "#########"]),
            ],
        }
    }

    #[test]
    fn builds_two_floors_and_ramps_the_stairs() {
        let b = Building::build(&tiny(), 0.0).unwrap();
        assert_eq!(b.floors(), 2);
        assert!(b.warnings.is_empty(), "{:?}", b.warnings);
        let low = b.ground_local(0, 1.25, 1.25);
        let high = b.ground_local(0, 3.25, 1.25);
        assert!(high > low + 1.5, "stairs ramp up: {low} -> {high}");
        assert!((b.ground_local(0, 1.75, 1.75) - b.slab_units).abs() < 1e-5);
    }

    #[test]
    fn missing_stair_hole_is_reported() {
        let mut d = tiny();
        d.floors[1].plan[2] = "#.......#".into();
        let b = Building::build(&d, 0.0).unwrap();
        assert!(b.warnings.iter().any(|w| w.contains("hole")), "{:?}", b.warnings);
    }

    #[test]
    fn hidden_floor_and_cutaway_change_the_mesh() {
        let mut b = Building::build(&tiny(), 0.0).unwrap();
        let (mut mats, mut full, mut one, mut cut_off) = (vec![], Mesh::default(), Mesh::default(), Mesh::default());
        b.push(1, 0.0, &mut mats, &mut full);
        b.configure(Some(0), None, None, 0.0);
        b.push(1, 0.0, &mut mats, &mut one);
        assert!(one.idx.len() < full.idx.len(), "hiding floor 1 must remove geometry");
        b.configure(None, None, Some(Cutaway::None), 0.0);
        b.push(1, 0.0, &mut mats, &mut cut_off);
        let mut low = Mesh::default();
        b.configure(None, None, Some(Cutaway::Low), 0.0);
        b.push(1, 0.0, &mut mats, &mut low);
        assert!(low.idx.len() < cut_off.idx.len(), "lowering walls must remove geometry");
    }

    #[test]
    fn unknown_plan_char_is_an_error() {
        let mut d = tiny();
        d.floors[0].plan[1] = "#..?....#".into();
        assert!(Building::build(&d, 0.0).is_err());
    }
}
