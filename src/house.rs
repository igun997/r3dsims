//! The "rumah" template: a two-storey dollhouse seen from above without a roof. Furniture assets
//! are built from Rust like the kampung ones; the plan, furniture placement and story are data.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use anyhow::Result;
use serde_json::{Value, json};

use crate::building::{BuildingDef, FloorDef};
use crate::camera::CamParams;
use crate::model::*;
use crate::story::{StageDef, StoryDef, Target};
use crate::templates::{bx, ent, pal, part, person, to_pretty_json, villager};
use crate::terrain::{TerrainDef, TileDef};
use crate::world::{AssetRef, EntityDef, SceneDef, SkyDef};

const ORIGIN: [f32; 2] = [3.0, 3.0];
const COLS: usize = 24;
const ROWS: usize = 16;

fn single(name: &str, voxel: f32, palette: BTreeMap<String, String>, shape: Vec<Op>) -> AssetDef {
    AssetDef { name: name.into(), voxel, palette, parts: vec![part("body", None, [0.0; 3], shape)], anims: BTreeMap::new() }
}

fn sphere(at: [f32; 3], r: f32, c: &str) -> Op {
    Op::Sphere { at, r, c: c.into() }
}

fn sofa() -> AssetDef {
    single(
        "sofa",
        0.1,
        pal(&[("a", "#5b7fa6:fabric"), ("b", "#4d6e94:fabric"), ("c", "#6a90b8:fabric"), ("l", "#3b2f27:wood")]),
        vec![
            bx([-10, 1, -4], [20, 3, 9], "a"),
            bx([-10, 4, -4], [20, 6, 2], "b"),
            bx([-10, 4, -2], [2, 3, 7], "b"),
            bx([8, 4, -2], [2, 3, 7], "b"),
            bx([-8, 4, -2], [7, 2, 6], "c"),
            bx([1, 4, -2], [7, 2, 6], "c"),
            bx([-10, 0, -4], [2, 1, 2], "l"),
            bx([8, 0, -4], [2, 1, 2], "l"),
            bx([-10, 0, 3], [2, 1, 2], "l"),
            bx([8, 0, 3], [2, 1, 2], "l"),
        ],
    )
}

fn table() -> AssetDef {
    let mut s = vec![bx([-8, 7, -6], [16, 1, 12], "top")];
    for (x, z) in [(-8, -6), (7, -6), (-8, 5), (7, 5)] {
        s.push(bx([x, 0, z], [1, 7, 1], "leg"));
    }
    s.push(bx([-2, 8, -2], [4, 1, 4], "bowl"));
    s.push(bx([-1, 9, -1], [2, 1, 2], "fruit"));
    single("table", 0.1, pal(&[("top", "#b98550:wood"), ("leg", "#8a5e34:wood"), ("bowl", "#e8e0d0:stone"), ("fruit", "#d9402f")]), s)
}

fn coffee_table() -> AssetDef {
    let mut s = vec![bx([-7, 3, -5], [14, 1, 10], "top")];
    for (x, z) in [(-7, -5), (6, -5), (-7, 4), (6, 4)] {
        s.push(bx([x, 0, z], [1, 3, 1], "leg"));
    }
    s.push(bx([-3, 4, -2], [6, 1, 4], "book"));
    single("coffee_table", 0.1, pal(&[("top", "#c9a46a:wood"), ("leg", "#8a5e34:wood"), ("book", "#d9645a")]), s)
}

fn chair() -> AssetDef {
    let mut s = vec![bx([-3, 4, -3], [6, 1, 6], "seat"), bx([-3, 5, -3], [6, 5, 1], "seat")];
    for (x, z) in [(-3, -3), (2, -3), (-3, 2), (2, 2)] {
        s.push(bx([x, 0, z], [1, 4, 1], "leg"));
    }
    single("chair", 0.1, pal(&[("seat", "#d6b07a:wood"), ("leg", "#8a5e34:wood")]), s)
}

fn tv_unit() -> AssetDef {
    single(
        "tv_unit",
        0.1,
        pal(&[("w", "#6b4a2e:wood"), ("f", "#202228:metal"), ("s", "#3a5a7a*")]),
        vec![bx([-10, 0, -3], [20, 5, 6], "w"), bx([-9, 5, -1], [18, 11, 1], "f"), bx([-8, 6, 0], [16, 9, 1], "s"), bx([-2, 5, -2], [4, 1, 3], "f")],
    )
}

fn bookshelf() -> AssetDef {
    let mut s = vec![bx([-6, 0, -2], [12, 18, 4], "w"), Op::Clear { at: [-5, 1, -1], size: [10, 4, 3] }, Op::Clear { at: [-5, 6, -1], size: [10, 4, 3] }, Op::Clear { at: [-5, 11, -1], size: [10, 4, 3] }];
    for (i, c) in ["b1", "b2", "b3", "b1", "b3", "b2", "b1", "b2"].iter().enumerate() {
        s.push(bx([-5 + i as i32, 1, -1], [1, 3, 2], c));
        s.push(bx([-5 + i as i32, 6, -1], [1, if i % 2 == 0 { 3 } else { 2 }, 2], c));
    }
    s.push(bx([-5, 11, -1], [3, 3, 2], "b2"));
    single("bookshelf", 0.1, pal(&[("w", "#8a5e34:wood"), ("b1", "#d9645a"), ("b2", "#4c8fc4"), ("b3", "#e0b53a")]), s)
}

fn plant() -> AssetDef {
    single(
        "plant",
        0.1,
        pal(&[("pot", "#b5643c:stone"), ("soil", "#4a3426:dirt"), ("l1", "#3f9a45:grass"), ("l2", "#2f7d3a:grass")]),
        vec![bx([-3, 0, -3], [6, 5, 6], "pot"), bx([-2, 5, -2], [4, 1, 4], "soil"), sphere([0.0, 9.0, 0.0], 4.5, "l1"), sphere([2.0, 12.0, 1.0], 3.0, "l2"), sphere([-2.0, 11.0, -1.0], 2.5, "l2")],
    )
}

fn floor_lamp() -> AssetDef {
    single(
        "floor_lamp",
        0.1,
        pal(&[("d", "#2c2c38:metal"), ("g", "#ffe9b0*")]),
        vec![bx([-2, 0, -2], [4, 1, 4], "d"), bx([0, 1, 0], [1, 15, 1], "d"), bx([-3, 15, -3], [7, 5, 7], "g")],
    )
}

fn counter() -> AssetDef {
    single(
        "counter",
        0.1,
        pal(&[("c", "#f0ece4:wood"), ("t", "#6e7480:stone"), ("k", "#26282e:metal"), ("s", "#c4c9d0:metal")]),
        vec![
            bx([-15, 0, -4], [30, 8, 8], "c"),
            bx([-15, 8, -4], [30, 1, 9], "t"),
            bx([-9, 9, -2], [8, 1, 5], "k"),
            bx([-8, 10, -1], [2, 1, 2], "s"),
            bx([-5, 10, 0], [2, 1, 2], "s"),
            bx([4, 8, -2], [7, 1, 5], "s"),
            bx([8, 9, -1], [1, 4, 1], "s"),
            bx([-15, 4, 4], [30, 1, 1], "k"),
        ],
    )
}

fn fridge() -> AssetDef {
    single(
        "fridge",
        0.1,
        pal(&[("f", "#dfe5ea:metal"), ("k", "#7a8088:metal")]),
        vec![bx([-4, 0, -4], [8, 18, 8], "f"), bx([-4, 11, 4], [8, 1, 1], "k"), bx([2, 13, 4], [1, 4, 1], "k"), bx([2, 5, 4], [1, 4, 1], "k")],
    )
}

fn bed() -> AssetDef {
    let mut s = vec![
        bx([-9, 0, -12], [18, 3, 24], "frame"),
        bx([-8, 3, -11], [16, 3, 22], "sheet"),
        bx([-9, 3, -12], [18, 9, 1], "frame"),
        bx([-8, 6, 0], [16, 1, 11], "blanket"),
        bx([-7, 6, -10], [6, 2, 4], "pillow"),
        bx([1, 6, -10], [6, 2, 4], "pillow"),
    ];
    for i in 0..4 {
        s.push(bx([-8 + i * 4, 7, 0], [2, 1, 11], "stripe"));
    }
    single("bed", 0.1, pal(&[("frame", "#8a5e34:wood"), ("sheet", "#f2efe8:fabric"), ("blanket", "#d9645a:fabric"), ("stripe", "#e98a80:fabric"), ("pillow", "#ffffff:fabric")]), s)
}

fn wardrobe() -> AssetDef {
    single(
        "wardrobe",
        0.1,
        pal(&[("w", "#a8794a:wood"), ("d", "#7a5430:wood"), ("k", "#f0c040:metal")]),
        vec![bx([-8, 0, -4], [16, 24, 8], "w"), bx([0, 2, 4], [1, 20, 1], "d"), bx([-2, 12, 4], [1, 3, 1], "k"), bx([1, 12, 4], [1, 3, 1], "k"), bx([-8, 24, -4], [16, 1, 8], "d")],
    )
}

fn bathtub() -> AssetDef {
    single(
        "bathtub",
        0.1,
        pal(&[("w", "#f2f5f7:metal"), ("water", "#7ec8f0"), ("m", "#aeb4bc:metal")]),
        vec![
            bx([-14, 0, -7], [28, 9, 14], "w"),
            Op::Clear { at: [-12, 3, -5], size: [24, 7, 10] },
            bx([-12, 3, -5], [24, 4, 10], "water"),
            bx([-14, 9, -1], [2, 4, 2], "m"),
            bx([-14, 12, -1], [4, 1, 2], "m"),
        ],
    )
}

fn toilet() -> AssetDef {
    single(
        "toilet",
        0.1,
        pal(&[("w", "#f4f6f8:metal"), ("s", "#d8dde2:metal")]),
        vec![bx([-3, 0, -2], [6, 4, 6], "w"), bx([-3, 4, -2], [6, 1, 6], "s"), bx([-3, 5, -5], [6, 7, 3], "w"), bx([-1, 12, -4], [2, 1, 1], "s")],
    )
}

fn sink() -> AssetDef {
    single(
        "sink",
        0.1,
        pal(&[("c", "#e8e2d6:wood"), ("w", "#f4f6f8:metal"), ("m", "#bfe6f5*"), ("t", "#aeb4bc:metal")]),
        vec![bx([-6, 0, -3], [12, 8, 6], "c"), bx([-6, 8, -3], [12, 1, 6], "w"), bx([-1, 9, -3], [2, 3, 1], "t"), bx([-5, 11, -4], [10, 9, 1], "m")],
    )
}

pub fn assets() -> Vec<(&'static str, AssetDef)> {
    vec![
        ("villager", villager()),
        ("sofa", sofa()),
        ("table", table()),
        ("coffee_table", coffee_table()),
        ("chair", chair()),
        ("tv_unit", tv_unit()),
        ("bookshelf", bookshelf()),
        ("plant", plant()),
        ("floor_lamp", floor_lamp()),
        ("counter", counter()),
        ("fridge", fridge()),
        ("bed", bed()),
        ("wardrobe", wardrobe()),
        ("bathtub", bathtub()),
        ("toilet", toilet()),
        ("sink", sink()),
    ]
}

// ---------------------------------------------------------------- plan

type Grid = Vec<Vec<char>>;

fn blank() -> Grid {
    vec![vec![' '; COLS]; ROWS]
}

fn fill(g: &mut Grid, x0: usize, z0: usize, x1: usize, z1: usize, ch: char) {
    for row in g.iter_mut().take(z1 + 1).skip(z0) {
        for c in row.iter_mut().take(x1 + 1).skip(x0) {
            *c = ch;
        }
    }
}

fn outline(g: &mut Grid, x0: usize, z0: usize, x1: usize, z1: usize) {
    for x in x0..=x1 {
        g[z0][x] = '#';
        g[z1][x] = '#';
    }
    for row in g.iter_mut().take(z1 + 1).skip(z0) {
        row[x0] = '#';
        row[x1] = '#';
    }
}

fn set(g: &mut Grid, cells: &[(usize, usize)], ch: char) {
    for &(x, z) in cells {
        g[z][x] = ch;
    }
}

fn rows(g: &Grid) -> Vec<String> {
    g.iter().map(|r| r.iter().collect()).collect()
}

fn windows(g: &mut Grid, front: &[usize], back: &[usize], left: &[usize], right: &[usize]) {
    for &x in front {
        g[ROWS - 1][x] = 'W';
    }
    for &x in back {
        g[0][x] = 'W';
    }
    for &z in left {
        g[z][0] = 'W';
    }
    for &z in right {
        g[z][COLS - 1] = 'W';
    }
}

fn ground_floor() -> Vec<String> {
    let mut g = blank();
    fill(&mut g, 1, 1, COLS - 2, ROWS - 2, '.');
    fill(&mut g, 12, 1, 22, 7, 't');
    outline(&mut g, 0, 0, COLS - 1, ROWS - 1);
    fill(&mut g, 11, 1, 11, ROWS - 2, '#');
    set(&mut g, &[(11, 3), (11, 4), (11, 10), (11, 11)], 'D');
    fill(&mut g, 12, 8, 22, 8, '#');
    set(&mut g, &[(16, 8), (17, 8)], 'D');
    set(&mut g, &[(8, ROWS - 1), (9, ROWS - 1)], 'D');
    fill(&mut g, 13, 12, 20, 13, '>');
    windows(&mut g, &[3, 4, 14, 15, 19, 20], &[4, 5, 15, 16], &[4, 5, 10, 11], &[3, 4, 11, 12]);
    rows(&g)
}

fn upper_floor() -> Vec<String> {
    let mut g = blank();
    fill(&mut g, 1, 1, COLS - 2, ROWS - 2, '.');
    fill(&mut g, 1, 1, 10, ROWS - 2, 'c');
    fill(&mut g, 12, 1, 22, 7, 't');
    outline(&mut g, 0, 0, COLS - 1, ROWS - 1);
    fill(&mut g, 11, 1, 11, ROWS - 2, '#');
    set(&mut g, &[(11, 10), (11, 11)], 'D');
    fill(&mut g, 12, 8, 22, 8, '#');
    set(&mut g, &[(16, 8), (17, 8)], 'D');
    fill(&mut g, 13, 12, 20, 13, ' ');
    windows(&mut g, &[3, 4, 8, 9, 14, 15, 19, 20], &[4, 5, 15, 16], &[4, 5, 10, 11], &[3, 4, 11, 12]);
    rows(&g)
}

pub fn building() -> BuildingDef {
    BuildingDef {
        id: "rumah".into(),
        pos: ORIGIN,
        voxel: 0.25,
        tile: 2,
        floor_height: 12,
        slab: 2,
        cut_height: 3,
        cutaway: "auto".into(),
        palette: BTreeMap::new(),
        floors: vec![
            FloorDef { name: "Lantai 1".into(), plan: ground_floor() },
            FloorDef { name: "Lantai 2".into(), plan: upper_floor() },
        ],
    }
}

/// World [x, z] of the centre of plan cell (cx, cz); half-cell offsets allowed.
fn at(cx: f32, cz: f32) -> [f32; 2] {
    [ORIGIN[0] + (cx + 0.5) * 0.5, ORIGIN[1] + (cz + 0.5) * 0.5]
}

fn terrain() -> TerrainDef {
    let (w, d) = (18usize, 14usize);
    let mut map = vec![vec!['g'; w]; d];
    for (z, row) in map.iter_mut().enumerate() {
        for (x, c) in row.iter_mut().enumerate() {
            if (2..16).contains(&x) && (2..12).contains(&z) {
                *c = 'p';
            }
        }
    }
    let tile = |top: &str, side: &str| TileDef { h: 2, top: top.into(), side: Some(side.into()), jitter: 0.04, water: false };
    let mut legend = BTreeMap::new();
    legend.insert("g".to_string(), tile("#78b856:grass", "#8a6040:dirt"));
    legend.insert("p".to_string(), tile("#cdbfa5:stone", "#a8987a:stone"));
    TerrainDef { cell: 1.0, step: 0.5, legend, map: map.iter().map(|r| r.iter().collect()).collect() }
}

fn furnish(id: &str, asset: &str, cell: (f32, f32), yaw: f32, footprint: [f32; 2], floor: usize) -> EntityDef {
    let mut e = ent(id, asset, at(cell.0, cell.1), yaw);
    e.footprint = footprint;
    e.building = Some("rumah".into());
    e.floor = floor;
    e
}

fn resident(id: &str, label: &str, cell: (f32, f32), yaw: f32, shirt: &str, hair: &str, scale: f32, floor: usize) -> EntityDef {
    let mut e = person(id, label, at(cell.0, cell.1), yaw, shirt, hair, scale);
    e.building = Some("rumah".into());
    e.floor = floor;
    e
}

fn entities() -> Vec<EntityDef> {
    vec![
        // living room
        furnish("sofa", "sofa", (5.0, 3.0), 0.0, [2.0, 0.9], 0),
        furnish("meja_tamu", "coffee_table", (5.0, 6.5), 0.0, [1.4, 1.0], 0),
        furnish("tv", "tv_unit", (5.0, 13.0), 180.0, [2.0, 0.6], 0),
        furnish("rak", "bookshelf", (1.8, 8.0), 90.0, [0.5, 1.2], 0),
        furnish("tanaman1", "plant", (9.0, 2.0), 0.0, [0.6, 0.6], 0),
        furnish("lampu1", "floor_lamp", (2.0, 2.0), 0.0, [0.4, 0.4], 0),
        // kitchen
        furnish("dapur", "counter", (17.0, 1.8), 0.0, [3.0, 0.9], 0),
        furnish("kulkas", "fridge", (21.0, 2.0), 0.0, [0.8, 0.8], 0),
        furnish("meja_makan", "table", (17.0, 4.8), 0.0, [1.6, 1.2], 0),
        furnish("kursi1", "chair", (14.6, 4.8), 90.0, [0.6, 0.6], 0),
        furnish("kursi2", "chair", (19.4, 4.8), -90.0, [0.6, 0.6], 0),
        furnish("tanaman2", "plant", (12.6, 14.0), 0.0, [0.6, 0.6], 0),
        // bedroom
        furnish("kasur", "bed", (4.0, 4.0), 0.0, [1.9, 2.5], 1),
        furnish("lemari", "wardrobe", (1.8, 11.0), 90.0, [0.8, 1.6], 1),
        furnish("meja_belajar", "table", (8.2, 12.2), 180.0, [1.6, 1.2], 1),
        furnish("kursi3", "chair", (8.2, 10.4), 0.0, [0.6, 0.6], 1),
        furnish("lampu2", "floor_lamp", (9.5, 2.0), 0.0, [0.4, 0.4], 1),
        // bathroom and landing
        furnish("bak", "bathtub", (16.0, 2.4), 0.0, [2.8, 1.4], 1),
        furnish("kloset", "toilet", (21.0, 2.5), 0.0, [0.6, 0.7], 1),
        furnish("wastafel", "sink", (12.8, 5.5), 90.0, [0.6, 1.2], 1),
        furnish("tanaman3", "plant", (22.0, 14.0), 0.0, [0.6, 0.6], 1),
        // the family
        resident("ibu", "@ibu", (15.0, 3.2), 0.0, "#e0584a", "#201510", 1.0, 0),
        resident("bapak", "@bapak", (7.5, 9.0), 0.0, "#4c8fc4", "#3b2a20", 1.0, 0),
        resident("anak", "@anak", (6.0, 8.0), 180.0, "#f0b429", "#2a1a10", 0.8, 1),
    ]
}

fn markers() -> BTreeMap<String, Target> {
    let at_floor = |cx: f32, cz: f32, floor: usize| Target::At { pos: at(cx, cz), building: "rumah".into(), floor };
    let mut m = BTreeMap::new();
    m.insert("meja_makan_bawah".to_string(), at_floor(17.0, 6.6, 0));
    m.insert("sofa_depan".to_string(), at_floor(5.0, 5.0, 0));
    m.insert("kamar_tidur".to_string(), at_floor(7.0, 7.0, 1));
    m.insert("kamar_mandi".to_string(), at_floor(18.0, 5.0, 1));
    m.insert("halaman".to_string(), Target::Pos([9.0, 11.6]));
    m.insert("tangga_atas".to_string(), at_floor(21.0, 12.5, 1));
    m
}

fn story() -> Result<StoryDef> {
    let scenes: Vec<Value> = vec![
        json!({"name": "pagi", "steps": [
            {"do": "building", "id": "rumah", "show_floor": 0},
            {"do": "camera", "op": "set", "target": [9, 1.6, 7], "yaw": 38, "pitch": 50, "zoom": 17},
            {"do": "par", "steps": [
                {"do": "camera", "op": "move", "yaw": 55, "zoom": 15, "secs": 4.6},
                {"do": "seq", "steps": [
                    {"do": "wait", "secs": 0.5},
                    {"do": "par", "steps": [
                        {"do": "say", "actor": "ibu", "text": "SARAPAN SIAP!", "secs": 2.2},
                        {"do": "play", "actor": "ibu", "anim": "wave"}]}]},
                {"do": "seq", "steps": [
                    {"do": "wait", "secs": 2.0},
                    {"do": "par", "steps": [
                        {"do": "say", "actor": "bapak", "text": "IYA, SEBENTAR!", "secs": 1.9},
                        {"do": "play", "actor": "bapak", "anim": "talk", "secs": 1.9}]}]}
            ]},
            {"do": "building", "id": "rumah", "show_floor": -1},
            {"do": "camera", "op": "move", "target": [9, 3.2, 7], "yaw": 35, "pitch": 50, "zoom": 17, "secs": 1.0},
            {"do": "par", "steps": [
                {"do": "say", "actor": "anak", "text": "SELAMAT PAGI!", "secs": 1.7},
                {"do": "play", "actor": "anak", "anim": "wave"}]},
            {"do": "walk_to", "actor": "anak", "to": "tangga_atas", "speed": 2.4},
            {"do": "building", "id": "rumah", "show_floor": 0},
            {"do": "par", "steps": [
                {"do": "camera", "op": "move", "target": [10, 1.6, 8], "yaw": 40, "pitch": 48, "zoom": 15, "secs": 1.0},
                {"do": "walk_to", "actor": "anak", "to": "meja_makan_bawah", "speed": 2.4},
                {"do": "walk_to", "actor": "bapak", "to": "meja_makan_bawah", "speed": 2.0, "stop": 1.4}]},
            {"do": "par", "steps": [
                {"do": "say", "actor": "anak", "text": "AKU LAPAR!", "secs": 1.8},
                {"do": "play", "actor": "anak", "anim": "cheer", "times": 2}]}
        ]}),
        json!({"name": "lantai", "steps": [
            {"do": "camera", "op": "move", "target": [9, 1.6, 7], "yaw": 28, "pitch": 54, "zoom": 16, "secs": 1.4},
            {"do": "say", "actor": "ibu", "text": "LANTAI 1", "secs": 1.2},
            {"do": "building", "id": "rumah", "show_floor": -1, "gap": 3.2, "secs": 1.6},
            {"do": "par", "steps": [
                {"do": "camera", "op": "move", "target": [9, 3.2, 7], "yaw": 150, "pitch": 28, "zoom": 22, "secs": 4.5},
                {"do": "wait", "secs": 4.5}]},
            {"do": "building", "id": "rumah", "gap": 0, "secs": 1.4}
        ]}),
        json!({"name": "naik", "steps": [
            {"do": "building", "id": "rumah", "show_floor": 0},
            {"do": "camera", "op": "move", "target": [10, 1.6, 8], "yaw": 40, "pitch": 48, "zoom": 15, "secs": 1.2},
            {"do": "camera", "op": "follow", "actor": "bapak", "smooth": 4},
            {"do": "walk_to", "actor": "bapak", "to": "tangga_atas", "speed": 2.2},
            {"do": "building", "id": "rumah", "show_floor": -1},
            {"do": "walk_to", "actor": "bapak", "to": "kamar_tidur", "speed": 2.2},
            {"do": "par", "steps": [
                {"do": "say", "actor": "bapak", "text": "MAU ISTIRAHAT DULU", "secs": 2.2},
                {"do": "play", "actor": "bapak", "anim": "wave"}]},
            {"do": "camera", "op": "unfollow"}
        ]}),
        json!({"name": "malam", "steps": [
            {"do": "set_sky", "preset": "sunset", "secs": 3.0},
            {"do": "camera", "op": "move", "target": [9, 3.0, 7], "yaw": 70, "pitch": 40, "zoom": 19, "secs": 3.0},
            {"do": "set_sky", "preset": "night", "secs": 2.5},
            {"do": "building", "id": "rumah", "gap": 3.2, "secs": 1.4},
            {"do": "par", "steps": [
                {"do": "say", "actor": "ibu", "text": "SELAMAT MALAM", "secs": 2.4},
                {"do": "play", "actor": "ibu", "anim": "wave"}]},
            {"do": "wait", "secs": 1.0}
        ]}),
    ];
    Ok(StoryDef { looping: true, reset_on_loop: true, scenes: serde_json::from_value::<Vec<StageDef>>(Value::Array(scenes))? })
}

pub fn scene() -> Result<SceneDef> {
    Ok(SceneDef {
        name: "Rumah Dua Lantai".into(),
        assets: assets().into_iter().map(|(n, _)| (n.to_string(), AssetRef::Path(format!("assets/{n}.json")))).collect(),
        terrain: Some(terrain()),
        sky: SkyDef { preset: "day".into(), ..Default::default() },
        markers: markers(),
        buildings: vec![building()],
        entities: entities(),
        story: Some(story()?),
        camera: CamParams { target: Some([9.0, 2.5, 7.0]), yaw: Some(35.0), pitch: Some(50.0), zoom: Some(19.0), ..Default::default() },
    })
}

pub fn scene_inline() -> Result<SceneDef> {
    let mut s = scene()?;
    s.assets = assets().into_iter().map(|(n, d)| (n.to_string(), AssetRef::Inline(Box::new(d)))).collect();
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

    #[test]
    fn plans_are_rectangular_and_stairs_line_up() {
        let b = building();
        for f in &b.floors {
            assert_eq!(f.plan.len(), ROWS);
            assert!(f.plan.iter().all(|r| r.chars().count() == COLS));
        }
        crate::building::Building::build(&b, 1.0).unwrap();
    }

    #[test]
    fn all_assets_bake() {
        for (n, d) in assets() {
            let m = Model::bake(&d).unwrap_or_else(|e| panic!("{n}: {e:#}"));
            assert!(m.voxel_count() > 10, "{n}");
        }
    }
}
