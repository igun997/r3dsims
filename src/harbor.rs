//! The "pelabuhan" template: the complex example. A fishing harbour island with a dock and boats,
//! a hill with a lighthouse, kampung houses and a stall, a three-storey guesthouse (two stair
//! runs, lobby / rooms / rooftop terrace) and a small fisherman's hut, all in one scene, with a
//! story that walks a tourist from the beach up to the rooftop through the cut-away building.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use anyhow::Result;
use serde_json::{Value, json};

use crate::building::{BuildingDef, FloorDef};
use crate::camera::CamParams;
use crate::model::*;
use crate::story::{StageDef, StoryDef, Target};
use crate::templates::{bx, clip, ent, kampung_assets, pal, part, person, to_pretty_json, tr};
use crate::terrain::{TerrainDef, TileDef, hash3};
use crate::world::{AssetRef, EntityDef, SceneDef, SkyDef};

const W: usize = 36;
const D: usize = 30;
const INN: [f32; 2] = [10.0, 6.0];
const HUT: [f32; 2] = [22.0, 15.0];

// ---------------------------------------------------------------- new assets

fn boat() -> AssetDef {
    let mut s = vec![
        bx([-4, 0, -8], [8, 4, 15], "hull"),
        bx([-3, 0, 7], [6, 4, 2], "hull"),
        bx([-2, 0, 9], [4, 4, 2], "hull"),
        bx([-1, 1, 11], [2, 3, 1], "hull"),
        Op::Clear { at: [-3, 2, -7], size: [6, 3, 13] },
        bx([-3, 1, -7], [6, 1, 13], "deck"),
        bx([-4, 3, -8], [8, 1, 15], "trim"),
        bx([-3, 4, -7], [6, 5, 5], "cabin"),
        bx([-4, 9, -8], [8, 1, 7], "roof"),
        bx([0, 4, 2], [1, 13, 1], "mast"),
        bx([0, 8, 3], [1, 8, 5], "sail"),
        bx([-1, 7, -3], [2, 2, 1], "lamp"),
    ];
    s.push(bx([-2, 5, -3], [4, 2, 1], "glass"));
    let mut anims = BTreeMap::new();
    anims.insert(
        "idle".to_string(),
        clip(4.0, true, vec![("hull", tr(&[(0.0, [1.5, 0.0, 2.5]), (2.0, [-1.5, 0.0, -2.5])], &[(0.0, [0.0, 0.3, 0.0]), (2.0, [0.0, -0.3, 0.0])]))]),
    );
    AssetDef {
        name: "boat".into(),
        voxel: 0.2,
        palette: pal(&[
            ("hull", "#2f6a9a:wood"),
            ("deck", "#b98550:wood"),
            ("trim", "#f4efe6:wood"),
            ("cabin", "#e8dcc8:wood"),
            ("roof", "#c4553a:metal"),
            ("mast", "#6b4423:wood"),
            ("sail", "#f6efe0:fabric"),
            ("lamp", "#ffd27a*"),
            ("glass", "#9fd3ea"),
        ]),
        parts: vec![part("hull", None, [0.0, 2.0, 0.0], s)],
        anims,
    }
}

fn lighthouse() -> AssetDef {
    let mut tower = vec![];
    for y in 0..28 {
        let hw = 5 - y / 9;
        tower.push(bx([-hw, y, -hw], [hw * 2, 1, hw * 2], if (y / 4) % 2 == 0 { "red" } else { "white" }));
    }
    tower.push(bx([-6, 28, -6], [12, 1, 12], "dark"));
    for (x, z, sx, sz) in [(-6, -6, 12, 1), (-6, 5, 12, 1), (-6, -5, 1, 10), (5, -5, 1, 10)] {
        tower.push(bx([x, 29, z], [sx, 2, sz], "rail"));
    }
    tower.push(bx([-3, 29, -3], [6, 5, 6], "glass"));
    for k in 0..5 {
        tower.push(bx([-4 + k, 34 + k, -4 + k], [8 - 2 * k, 1, 8 - 2 * k], "cap"));
    }
    let beam = vec![bx([3, 30, -1], [11, 2, 2], "beam"), bx([-12, 30, -1], [9, 2, 2], "beam")];
    let mut anims = BTreeMap::new();
    let mut c = clip(6.0, true, vec![("beam", tr(&[(0.0, [0.0, 0.0, 0.0]), (2.0, [0.0, 120.0, 0.0]), (4.0, [0.0, 240.0, 0.0]), (6.0, [0.0, 360.0, 0.0])], &[]))]);
    c.ease = "linear".into();
    anims.insert("idle".to_string(), c);
    AssetDef {
        name: "lighthouse".into(),
        voxel: 0.25,
        palette: pal(&[
            ("red", "#d9402f:plaster"),
            ("white", "#f4efe6:plaster"),
            ("dark", "#3b3b44:metal"),
            ("rail", "#3b3b44:metal"),
            ("glass", "#ffe9a8*"),
            ("cap", "#3b3b44:metal"),
            ("beam", "#fff2b0!"),
        ]),
        parts: vec![part("tower", None, [0.0; 3], tower), part("beam", Some("tower"), [0.0, 31.0, 0.0], beam)],
        anims,
    }
}

fn crate_asset() -> AssetDef {
    AssetDef {
        name: "crate".into(),
        voxel: 0.12,
        palette: pal(&[("w", "#b98550:wood"), ("d", "#8a5e34:wood")]),
        parts: vec![part("body", None, [0.0; 3], vec![bx([-4, 0, -4], [8, 7, 8], "w"), bx([-4, 3, -4], [8, 1, 8], "d"), bx([-4, 0, -4], [1, 7, 1], "d"), bx([3, 0, 3], [1, 7, 1], "d")])],
        anims: BTreeMap::new(),
    }
}

fn barrel() -> AssetDef {
    AssetDef {
        name: "barrel".into(),
        voxel: 0.12,
        palette: pal(&[("w", "#8a5e34:wood"), ("b", "#3b3b44:metal")]),
        parts: vec![part(
            "body",
            None,
            [0.0; 3],
            vec![bx([-3, 0, -2], [6, 9, 4], "w"), bx([-2, 0, -3], [4, 9, 6], "w"), bx([-3, 2, -2], [6, 1, 4], "b"), bx([-2, 2, -3], [4, 1, 6], "b"), bx([-3, 6, -2], [6, 1, 4], "b"), bx([-2, 6, -3], [4, 1, 6], "b")],
        )],
        anims: BTreeMap::new(),
    }
}

/// Every asset the scene uses, once each.
pub fn assets() -> Vec<(String, AssetDef)> {
    let mut out: Vec<(String, AssetDef)> = vec![];
    let mut add = |n: &str, d: AssetDef| {
        if !out.iter().any(|(k, _)| k == n) {
            out.push((n.to_string(), d));
        }
    };
    for (n, d) in kampung_assets() {
        add(n, d);
    }
    for (n, d) in crate::house::assets() {
        add(n, d);
    }
    add("boat", boat());
    add("lighthouse", lighthouse());
    add("crate", crate_asset());
    add("barrel", barrel());
    out
}

// ---------------------------------------------------------------- terrain

fn terrain() -> (TerrainDef, Vec<Vec<char>>) {
    let mut m = vec![vec!['w'; W]; D];
    for j in 0..D {
        for i in 0..W {
            let (x, z) = (i as f32 + 0.5, j as f32 + 0.5);
            let d = ((x - 17.0) / 14.0).powi(2) + ((z - 13.0) / 10.5).powi(2) + 0.05 * hash3(i as i32, j as i32, 9);
            let hd = ((x - 29.5).powi(2) + (z - 8.5).powi(2)).sqrt();
            m[j][i] = if d >= 1.0 {
                'w'
            } else if d > 0.8 {
                's'
            } else if hd < 2.8 {
                'H'
            } else if hd < 4.8 {
                'h'
            } else if hash3(i as i32, j as i32, 3) > 0.45 {
                'G'
            } else {
                'g'
            };
        }
    }
    let land = |c: char| !matches!(c, 'w' | 's');
    for j in 6..13 {
        for i in 10..22 {
            m[j][i] = 'p';
        }
    }
    for j in 13..17 {
        for i in 9..24 {
            if land(m[j][i]) || m[j][i] == 's' {
                m[j][i] = 'p';
            }
        }
    }
    for j in 15..19 {
        for i in 22..27 {
            m[j][i] = 'p';
        }
    }
    for (i0, i1, j0, j1) in [(3usize, 8usize, 14usize, 15usize), (24, 30, 15, 16), (29, 30, 9, 16)] {
        for row in m.iter_mut().take(j1 + 1).skip(j0) {
            for c in row.iter_mut().take(i1 + 1).skip(i0) {
                if matches!(*c, 'g' | 'G') {
                    *c = 'd';
                }
            }
        }
    }
    for j in 22..28 {
        for i in 16..19 {
            m[j][i] = 'k';
        }
    }
    for j in 26..28 {
        for i in 13..22 {
            m[j][i] = 'k';
        }
    }
    let t = |h: i32, top: &str, side: &str, jitter: f32, water: bool| TileDef { h, top: top.into(), side: Some(side.into()), jitter, water };
    let mut legend = BTreeMap::new();
    legend.insert("g".to_string(), t(2, "#78b856:grass", "#8a6040:dirt", 0.04, false));
    legend.insert("G".to_string(), t(2, "#6aae4c:grass", "#8a6040:dirt", 0.05, false));
    legend.insert("h".to_string(), t(3, "#6aae4c:grass", "#8a6040:dirt", 0.05, false));
    legend.insert("H".to_string(), t(4, "#5a9e42:grass", "#8a6040:dirt", 0.05, false));
    legend.insert("s".to_string(), t(2, "#e6d29b:sand", "#cdb27a:sand", 0.05, false));
    legend.insert("d".to_string(), t(2, "#b98a58:dirt", "#8a6040:dirt", 0.06, false));
    legend.insert("p".to_string(), t(2, "#cdbfa5:stone", "#a8987a:stone", 0.04, false));
    legend.insert("k".to_string(), t(2, "#a9743f:wood", "#6b4423:wood", 0.03, false));
    legend.insert("w".to_string(), t(1, "#3d95dc", "#8a6040:dirt", 0.03, true));
    let map_rows = m.iter().map(|r| r.iter().collect()).collect();
    (TerrainDef { cell: 1.0, step: 0.5, legend, map: map_rows }, m)
}

// ---------------------------------------------------------------- buildings

type Grid = Vec<Vec<char>>;

fn blank(w: usize, h: usize) -> Grid {
    vec![vec![' '; w]; h]
}
fn fill(g: &mut Grid, x0: usize, z0: usize, x1: usize, z1: usize, ch: char) {
    for row in g.iter_mut().take(z1 + 1).skip(z0) {
        for c in row.iter_mut().take(x1 + 1).skip(x0) {
            *c = ch;
        }
    }
}
fn outline(g: &mut Grid) {
    let (w, h) = (g[0].len(), g.len());
    for x in 0..w {
        g[0][x] = '#';
        g[h - 1][x] = '#';
    }
    for row in g.iter_mut() {
        row[0] = '#';
        row[w - 1] = '#';
    }
}
fn put(g: &mut Grid, cells: &[(usize, usize)], ch: char) {
    for &(x, z) in cells {
        g[z][x] = ch;
    }
}
fn windows(g: &mut Grid, front: &[usize], back: &[usize], left: &[usize], right: &[usize]) {
    let (w, h) = (g[0].len(), g.len());
    for &x in front {
        g[h - 1][x] = 'W';
    }
    for &x in back {
        g[0][x] = 'W';
    }
    for &z in left {
        g[z][0] = 'W';
    }
    for &z in right {
        g[z][w - 1] = 'W';
    }
}
fn rows(g: &Grid) -> Vec<String> {
    g.iter().map(|r| r.iter().collect()).collect()
}

fn inn_floors() -> Vec<FloorDef> {
    const CW: usize = 24;
    const CD: usize = 14;
    // floor 0: lobby (left), restaurant and kitchen (right), first stair run along the lobby
    let mut f0 = blank(CW, CD);
    fill(&mut f0, 1, 1, CW - 2, CD - 2, '.');
    fill(&mut f0, 13, 1, 22, 12, 't');
    outline(&mut f0);
    fill(&mut f0, 12, 1, 12, 12, '#');
    put(&mut f0, &[(12, 7), (12, 8)], 'D');
    put(&mut f0, &[(10, CD - 1), (11, CD - 1)], 'D');
    fill(&mut f0, 2, 10, 9, 11, '>');
    windows(&mut f0, &[3, 4, 14, 15, 19, 20], &[4, 5, 15, 16], &[4, 5], &[3, 4, 9, 10]);

    // floor 1: guest rooms A (left) and B (right), lounge, hole over run 1, second run on the right
    let mut f1 = blank(CW, CD);
    fill(&mut f1, 1, 1, CW - 2, CD - 2, '.');
    fill(&mut f1, 1, 1, 11, 7, 'c');
    fill(&mut f1, 13, 1, 22, 7, 'c');
    outline(&mut f1);
    fill(&mut f1, 12, 1, 12, 12, '#');
    put(&mut f1, &[(12, 4), (12, 5), (12, 10), (12, 11)], 'D');
    fill(&mut f1, 1, 8, 11, 8, '#');
    put(&mut f1, &[(5, 8), (6, 8)], 'D');
    fill(&mut f1, 13, 8, 22, 8, '#');
    put(&mut f1, &[(17, 8), (18, 8)], 'D');
    fill(&mut f1, 2, 10, 9, 11, ' ');
    fill(&mut f1, 13, 10, 20, 11, '>');
    windows(&mut f1, &[3, 4, 8, 9, 14, 15, 19, 20], &[4, 5, 15, 16], &[4, 5, 11, 12], &[3, 4, 11, 12]);

    // floor 2: rooftop terrace, open plan, hole over run 2
    let mut f2 = blank(CW, CD);
    fill(&mut f2, 1, 1, CW - 2, CD - 2, '.');
    outline(&mut f2);
    fill(&mut f2, 13, 10, 20, 11, ' ');
    windows(&mut f2, &[4, 5, 10, 11, 17, 18], &[4, 5, 10, 11, 17, 18], &[4, 5, 9, 10], &[4, 5, 9, 10]);
    vec![
        FloorDef { name: "Lobi".into(), plan: rows(&f0) },
        FloorDef { name: "Kamar".into(), plan: rows(&f1) },
        FloorDef { name: "Teras".into(), plan: rows(&f2) },
    ]
}

fn hut_floors() -> Vec<FloorDef> {
    let mut g = blank(10, 8);
    fill(&mut g, 1, 1, 8, 6, '.');
    outline(&mut g);
    put(&mut g, &[(4, 7), (5, 7)], 'D');
    windows(&mut g, &[1, 8], &[3, 6], &[3], &[3]);
    vec![FloorDef { name: "Gubuk".into(), plan: rows(&g) }]
}

fn building(id: &str, pos: [f32; 2], floors: Vec<FloorDef>) -> BuildingDef {
    BuildingDef { id: id.into(), pos, voxel: 0.25, tile: 2, floor_height: 12, slab: 2, cut_height: 3, cutaway: "auto".into(), palette: BTreeMap::new(), floors }
}

fn at(origin: [f32; 2], cx: f32, cz: f32) -> [f32; 2] {
    [origin[0] + (cx + 0.5) * 0.5, origin[1] + (cz + 0.5) * 0.5]
}

// ---------------------------------------------------------------- entities

struct Furnisher {
    id: &'static str,
    origin: [f32; 2],
}

impl Furnisher {
    fn put(&self, id: &str, asset: &str, cell: (f32, f32), yaw: f32, fp: [f32; 2], floor: usize) -> EntityDef {
        let mut e = ent(id, asset, at(self.origin, cell.0, cell.1), yaw);
        e.footprint = fp;
        e.building = Some(self.id.into());
        e.floor = floor;
        e
    }
    fn who(&self, id: &str, label: &str, cell: (f32, f32), yaw: f32, shirt: &str, hair: &str, scale: f32, floor: usize) -> EntityDef {
        let mut e = person(id, label, at(self.origin, cell.0, cell.1), yaw, shirt, hair, scale);
        e.building = Some(self.id.into());
        e.floor = floor;
        e
    }
}

fn outdoor(id: &str, asset: &str, pos: [f32; 2], yaw: f32, fp: [f32; 2]) -> EntityDef {
    let mut e = ent(id, asset, pos, yaw);
    e.footprint = fp;
    e
}

fn entities(map: &[Vec<char>]) -> Vec<EntityDef> {
    let mut v: Vec<EntityDef> = vec![];
    // landmarks
    v.push(outdoor("mercusuar", "lighthouse", [29.5, 8.5], 0.0, [1.6, 1.6]));
    v.push(outdoor("rumah_barat1", "house", [5.5, 9.5], 90.0, [3.6, 4.4]));
    v.push(outdoor("rumah_barat2", "house", [5.5, 19.0], 90.0, [3.6, 4.4]));
    v.push(outdoor("warung", "warung", [15.0, 19.6], 180.0, [3.6, 2.6]));
    v.push(outdoor("sumur", "well", [20.5, 20.5], 0.0, [1.9, 1.9]));
    for (i, p) in [[9.5, 16.8], [13.0, 16.8], [20.5, 16.8], [23.5, 13.8]].iter().enumerate() {
        v.push(ent(&format!("lampu{}", i + 1), "lamp", *p, 0.0));
    }
    for (i, p) in [[15.6, 22.6], [19.4, 22.6], [13.4, 25.6], [20.6, 25.6]].iter().enumerate() {
        v.push(ent(&format!("lampu_dermaga{}", i + 1), "lamp", *p, 0.0));
    }
    // the dock
    let mut c1 = outdoor("peti1", "crate", [16.5, 23.5], 10.0, [0.6, 0.6]);
    c1.scale = 1.0;
    v.push(c1);
    v.push(outdoor("peti2", "crate", [18.4, 24.4], -20.0, [0.6, 0.6]));
    v.push(outdoor("peti3", "crate", [18.4, 23.7], 5.0, [0.6, 0.6]));
    v.push(outdoor("drum1", "barrel", [16.4, 25.0], 0.0, [0.6, 0.6]));
    for (id, p, yaw) in [("perahu1", [14.2, 24.3], 5.0), ("perahu2", [20.8, 24.0], -15.0), ("perahu3", [15.0, 29.0 - 1.0], 80.0)] {
        let mut b = ent(id, "boat", p, yaw);
        b.elevation = -0.18;
        v.push(b);
    }
    // palms on the beach, trees on grass
    let near = |v: &[EntityDef], p: [f32; 2], d: f32| v.iter().any(|e| ((e.pos[0] - p[0]).powi(2) + (e.pos[1] - p[1]).powi(2)).sqrt() < d);
    let (mut palms, mut trees) = (0, 0);
    for j in 1..D - 1 {
        for i in 1..W - 1 {
            let p = [i as f32 + 0.5 + 0.3 * hash3(i as i32, j as i32, 1), j as f32 + 0.5 + 0.3 * hash3(i as i32, j as i32, 2)];
            let r = hash3(i as i32, j as i32, 5);
            match map[j][i] {
                's' if palms < 14 && r > 0.5 && !near(&v, p, 4.0) => {
                    palms += 1;
                    let mut e = ent(&format!("palem{palms}"), "palm", p, 360.0 * (r + 1.0) / 2.0);
                    e.footprint = [0.6, 0.6];
                    v.push(e);
                }
                'g' | 'G' if trees < 10 && r > 0.6 && !near(&v, p, 4.2) => {
                    let clear = (-2i32..=2).all(|dj| (-2i32..=2).all(|di| !matches!(map[(j as i32 + dj) as usize][(i as i32 + di) as usize], 'd' | 'p' | 'k')));
                    if clear {
                        trees += 1;
                        let mut e = ent(&format!("pohon{trees}"), "tree", p, 360.0 * (r + 1.0) / 2.0);
                        e.footprint = [0.8, 0.8];
                        v.push(e);
                    }
                }
                _ => {}
            }
        }
    }

    // the guesthouse, floor by floor
    let f = Furnisher { id: "penginapan", origin: INN };
    v.extend([
        f.put("resepsi", "counter", (6.0, 2.0), 0.0, [3.0, 0.9], 0),
        f.put("rak_lobi", "bookshelf", (1.8, 5.0), 90.0, [0.5, 1.2], 0),
        f.put("sofa_lobi", "sofa", (6.0, 6.0), 180.0, [2.0, 0.9], 0),
        f.put("meja_lobi", "coffee_table", (6.0, 4.6), 0.0, [1.4, 1.0], 0),
        f.put("tanaman_lobi1", "plant", (10.5, 2.0), 0.0, [0.6, 0.6], 0),
        f.put("tanaman_lobi2", "plant", (1.7, 12.0), 0.0, [0.6, 0.6], 0),
        f.put("lampu_lobi", "floor_lamp", (10.5, 5.0), 0.0, [0.4, 0.4], 0),
        f.put("dapur", "counter", (17.0, 1.8), 0.0, [3.0, 0.9], 0),
        f.put("kulkas", "fridge", (21.5, 2.0), 0.0, [0.8, 0.8], 0),
        f.put("meja_resto1", "table", (16.0, 6.0), 0.0, [1.6, 1.2], 0),
        f.put("kursi_r1", "chair", (13.9, 6.0), 90.0, [0.6, 0.6], 0),
        f.put("kursi_r2", "chair", (18.1, 6.0), -90.0, [0.6, 0.6], 0),
        f.put("meja_resto2", "table", (19.0, 10.4), 0.0, [1.6, 1.2], 0),
        f.put("tanaman_resto", "plant", (22.0, 12.0), 0.0, [0.6, 0.6], 0),
        // floor 1
        f.put("kasur_a", "bed", (4.0, 4.0), 0.0, [1.9, 2.5], 1),
        f.put("lemari_a", "wardrobe", (1.8, 6.0), 90.0, [0.8, 1.6], 1),
        f.put("meja_a", "table", (9.0, 3.0), 90.0, [1.6, 1.2], 1),
        f.put("kursi_a", "chair", (7.2, 3.0), -90.0, [0.6, 0.6], 1),
        f.put("kasur_b", "bed", (20.0, 4.0), 0.0, [1.9, 2.5], 1),
        f.put("lemari_b", "wardrobe", (13.8, 6.0), -90.0, [0.8, 1.6], 1),
        f.put("sofa_lantai1", "sofa", (6.0, 12.0), 0.0, [2.0, 0.9], 1),
        f.put("tv_lantai1", "tv_unit", (1.8, 9.5), 90.0, [0.6, 2.0], 1),
        f.put("tanaman_k1", "plant", (11.0, 9.2), 0.0, [0.6, 0.6], 1),
        f.put("lampu_k1", "floor_lamp", (22.0, 12.0), 0.0, [0.4, 0.4], 1),
        // floor 2, rooftop
        f.put("sofa_teras1", "sofa", (5.0, 3.0), 0.0, [2.0, 0.9], 2),
        f.put("sofa_teras2", "sofa", (5.0, 8.0), 180.0, [2.0, 0.9], 2),
        f.put("meja_teras", "coffee_table", (5.0, 5.5), 0.0, [1.4, 1.0], 2),
        f.put("meja_teras2", "table", (16.0, 4.0), 0.0, [1.6, 1.2], 2),
        f.put("kursi_t1", "chair", (13.9, 4.0), 90.0, [0.6, 0.6], 2),
        f.put("kursi_t2", "chair", (18.1, 4.0), -90.0, [0.6, 0.6], 2),
        f.put("tanaman_t1", "plant", (1.7, 1.8), 0.0, [0.6, 0.6], 2),
        f.put("tanaman_t2", "plant", (22.0, 1.8), 0.0, [0.6, 0.6], 2),
        f.put("tanaman_t3", "plant", (22.0, 12.0), 0.0, [0.6, 0.6], 2),
        f.put("lampu_t1", "floor_lamp", (10.5, 1.8), 0.0, [0.4, 0.4], 2),
        f.put("lampu_t2", "floor_lamp", (10.5, 12.0), 0.0, [0.4, 0.4], 2),
    ]);
    v.push(f.who("pemilik", "@pemilik", (8.5, 2.0), 0.0, "#8a4c9a", "#201510", 1.0, 0));
    v.push(f.who("koki", "@koki", (17.0, 3.2), 0.0, "#f4efe6", "#3b2a20", 1.0, 0));
    v.push(f.who("tamu_kamar", "@tamu", (7.0, 6.0), 90.0, "#4caf50", "#7a4a1e", 1.0, 1));

    // the fisherman's hut
    let h = Furnisher { id: "gubuk", origin: HUT };
    v.extend([
        h.put("kasur_gubuk", "bed", (2.0, 2.2), 0.0, [1.9, 2.5], 0),
        h.put("meja_gubuk", "table", (6.5, 3.0), 90.0, [1.6, 1.2], 0),
        h.put("kursi_gubuk", "chair", (5.2, 3.0), -90.0, [0.6, 0.6], 0),
        h.put("tanaman_gubuk", "plant", (8.0, 1.5), 0.0, [0.6, 0.6], 0),
    ]);

    // people outside
    v.push(person("nelayan", "@nelayan", [17.5, 25.8], 0.0, "#f0b429", "#2a1a10", 1.0));
    v.push(person("turis", "@turis", [4.5, 15.2], 90.0, "#e0584a", "#3b2a20", 1.0));
    v.push(person("penjual", "@penjual", [15.0, 18.0], 0.0, "#3c8fd9", "#201510", 1.0));
    v.push(person("anak", "@anak", [12.0, 15.5], 90.0, "#4caf50", "#2a1a10", 0.8));
    v
}

fn markers() -> BTreeMap<String, Target> {
    let floor = |b: &str, cx: f32, cz: f32, org: [f32; 2], f: usize| Target::At { pos: at(org, cx, cz), building: b.into(), floor: f };
    let mut m = BTreeMap::new();
    m.insert("pintu_penginapan".to_string(), Target::Pos([15.5, 13.9]));
    m.insert("dermaga_kiri".to_string(), Target::Pos([16.5, 25.4]));
    m.insert("dermaga_ujung".to_string(), Target::Pos([17.6, 26.8]));
    m.insert("warung_depan".to_string(), Target::Pos([15.0, 17.3]));
    m.insert("resepsionis_depan".to_string(), floor("penginapan", 6.0, 3.6, INN, 0));
    m.insert("tangga1_atas".to_string(), floor("penginapan", 10.5, 10.5, INN, 1));
    m.insert("tangga2_atas".to_string(), floor("penginapan", 21.5, 10.5, INN, 2));
    m.insert("kamar_a".to_string(), floor("penginapan", 6.5, 5.0, INN, 1));
    m.insert("teras_depan".to_string(), floor("penginapan", 8.0, 5.5, INN, 2));
    m.insert("teras_belakang".to_string(), floor("penginapan", 19.0, 8.0, INN, 2));
    m
}

// ---------------------------------------------------------------- story

fn story() -> Result<StoryDef> {
    let scenes: Vec<Value> = vec![
        json!({"name": "fajar", "steps": [
            {"do": "camera", "op": "set", "target": [17, 1.5, 14], "yaw": 35, "pitch": 38, "zoom": 36},
            {"do": "set_sky", "preset": "dawn"},
            {"do": "par", "steps": [
                {"do": "camera", "op": "move", "yaw": 62, "zoom": 31, "secs": 7},
                {"do": "set_sky", "preset": "day", "secs": 6},
                {"do": "seq", "steps": [
                    {"do": "wait", "secs": 0.8},
                    {"do": "walk_to", "actor": "nelayan", "to": "dermaga_ujung", "speed": 1.6},
                    {"do": "play", "actor": "nelayan", "anim": "wave"}]},
                {"do": "seq", "steps": [
                    {"do": "wait", "secs": 1.5},
                    {"do": "say", "actor": "penjual", "text": "IKAN SEGAR!", "secs": 2.2}]},
                {"do": "seq", "steps": [
                    {"do": "walk_to", "actor": "anak", "to": "warung_depan", "speed": 1.8},
                    {"do": "play", "actor": "anak", "anim": "cheer", "times": 2}]}
            ]}
        ]}),
        json!({"name": "turis", "steps": [
            {"do": "building", "id": "penginapan", "show_floor": 0},
            {"do": "camera", "op": "move", "target": [15, 1.6, 11], "yaw": 40, "pitch": 46, "zoom": 17, "secs": 2.0},
            {"do": "walk_to", "actor": "turis", "to": "pintu_penginapan", "speed": 2.4},
            {"do": "camera", "op": "follow", "actor": "turis", "smooth": 4},
            {"do": "walk_to", "actor": "turis", "to": "resepsionis_depan", "speed": 2.2},
            {"do": "par", "steps": [
                {"do": "say", "actor": "turis", "text": "ADA KAMAR KOSONG?", "secs": 2.0},
                {"do": "play", "actor": "turis", "anim": "talk", "secs": 2.0}]},
            {"do": "par", "steps": [
                {"do": "say", "actor": "pemilik", "text": "ADA! LANTAI 2 DAN TERAS", "secs": 2.4},
                {"do": "play", "actor": "pemilik", "anim": "wave"}]},
            {"do": "walk_to", "actor": "turis", "to": "tangga1_atas", "speed": 2.3},
            {"do": "building", "id": "penginapan", "show_floor": 1},
            {"do": "walk_to", "actor": "turis", "to": "kamar_a", "speed": 2.3},
            {"do": "par", "steps": [
                {"do": "say", "actor": "turis", "text": "KAMARNYA BAGUS!", "secs": 1.8},
                {"do": "play", "actor": "turis", "anim": "cheer", "times": 2}]},
            {"do": "walk_to", "actor": "turis", "to": "tangga2_atas", "speed": 2.3},
            {"do": "building", "id": "penginapan", "show_floor": -1},
            {"do": "walk_to", "actor": "turis", "to": "teras_belakang", "speed": 2.3},
            {"do": "camera", "op": "unfollow"},
            {"do": "par", "steps": [
                {"do": "camera", "op": "move", "target": [16, 5.5, 9.5], "yaw": 120, "pitch": 42, "zoom": 20, "secs": 3.0},
                {"do": "say", "actor": "turis", "text": "PEMANDANGANNYA!", "secs": 2.4},
                {"do": "play", "actor": "turis", "anim": "cheer", "times": 3}]}
        ]}),
        json!({"name": "lantai", "steps": [
            {"do": "camera", "op": "move", "target": [16, 3.5, 9.5], "yaw": 30, "pitch": 40, "zoom": 24, "secs": 1.6},
            {"do": "building", "id": "penginapan", "gap": 3.4, "secs": 1.8},
            {"do": "par", "steps": [
                {"do": "camera", "op": "move", "target": [16, 5.0, 9.5], "yaw": 150, "pitch": 26, "zoom": 26, "secs": 5.0},
                {"do": "wait", "secs": 5.0}]},
            {"do": "building", "id": "penginapan", "gap": 0, "secs": 1.6}
        ]}),
        json!({"name": "senja", "steps": [
            {"do": "camera", "op": "move", "target": [18, 1.5, 16], "yaw": 80, "pitch": 32, "zoom": 33, "secs": 3.0},
            {"do": "set_sky", "preset": "sunset", "secs": 3.0},
            {"do": "par", "steps": [
                {"do": "walk_to", "actor": "nelayan", "to": "dermaga_kiri", "speed": 1.8},
                {"do": "say", "actor": "penjual", "text": "HAMPIR MALAM", "secs": 2.0}]},
            {"do": "set_sky", "preset": "night", "secs": 3.0},
            {"do": "par", "steps": [
                {"do": "camera", "op": "move", "target": [24, 3.5, 11], "yaw": 110, "pitch": 28, "zoom": 24, "secs": 4.0},
                {"do": "building", "id": "penginapan", "gap": 2.6, "secs": 2.0}]},
            {"do": "wait", "secs": 1.2}
        ]}),
    ];
    Ok(StoryDef { looping: true, reset_on_loop: true, scenes: serde_json::from_value::<Vec<StageDef>>(Value::Array(scenes))? })
}

pub fn scene() -> Result<SceneDef> {
    let (terrain, map) = terrain();
    Ok(SceneDef {
        name: "Pelabuhan Nelayan".into(),
        assets: assets().into_iter().map(|(n, _)| (n.clone(), AssetRef::Path(format!("assets/{n}.json")))).collect(),
        terrain: Some(terrain),
        sky: SkyDef { preset: "day".into(), ..Default::default() },
        markers: markers(),
        buildings: vec![building("penginapan", INN, inn_floors()), building("gubuk", HUT, hut_floors())],
        entities: entities(&map),
        story: Some(story()?),
        camera: CamParams { target: Some([17.0, 1.5, 14.0]), yaw: Some(35.0), pitch: Some(38.0), zoom: Some(36.0), ..Default::default() },
    })
}

pub fn scene_inline() -> Result<SceneDef> {
    let mut s = scene()?;
    s.assets = assets().into_iter().map(|(n, d)| (n, AssetRef::Inline(Box::new(d)))).collect();
    Ok(s)
}

pub fn write(dir: &Path) -> Result<()> {
    fs::create_dir_all(dir.join("assets"))?;
    for (name, def) in assets() {
        fs::write(dir.join("assets").join(format!("{name}.json")), to_pretty_json(&def)?)?;
    }
    fs::write(dir.join("scene.json"), to_pretty_json(&scene()?)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::building::Building;

    #[test]
    fn assets_bake_and_buildings_are_consistent() {
        for (n, d) in assets() {
            Model::bake(&d).unwrap_or_else(|e| panic!("{n}: {e:#}"));
        }
        for (id, pos, floors) in [("penginapan", INN, inn_floors()), ("gubuk", HUT, hut_floors())] {
            let b = Building::build(&building(id, pos, floors), 1.0).unwrap();
            assert!(b.warnings.is_empty(), "{id}: {:?}", b.warnings);
        }
    }
}
