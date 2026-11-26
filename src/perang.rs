//! The "perang" template: two kingdoms side by side on one island, soldiers that march and fight, and
//! a war.json whose triggers turn viewer gifts into meteors, lightning, arrow rain, reinforcements,
//! shields and bigger macro combos. `r3dsims template perang -o war` writes it; `r3dsims live war/war.json`
//! runs it.

use std::fs;
use std::path::Path;

use anyhow::Result;
use serde_json::{Value, json};

use crate::camera::CamParams;
use crate::model::*;
use crate::templates::{bx, clip, ent, kampung_assets, pal, part, to_pretty_json, tr};
use crate::terrain::{TerrainDef, TileDef, hash3};
use crate::world::{AssetRef, EntityDef, SceneDef, SkyDef};

const W: usize = 52;
const D: usize = 30;
const CASTLE_A: [f32; 2] = [8.5, 15.0];
const CASTLE_B: [f32; 2] = [43.5, 15.0];

// ---------------------------------------------------------------- soldiers

fn soldier(name: &str, archer: bool) -> AssetDef {
    let mut body = vec![
        bx([-4, 5, -2], [8, 6, 4], "cloth"),
        bx([-4, 5, -2], [8, 1, 4], "dark"),
        bx([-3, 7, 2], [6, 3, 1], "steel"),
        bx([-1, 5, 2], [2, 2, 1], "trim"),
        bx([-6, 10, -2], [3, 2, 4], "steel"),
        bx([3, 10, -2], [3, 2, 4], "steel"),
    ];
    let mut head = vec![
        bx([-3, 11, -3], [6, 6, 6], "skin"),
        bx([-2, 13, 3], [1, 1, 1], "eye"),
        bx([1, 13, 3], [1, 1, 1], "eye"),
        bx([-1, 12, 3], [2, 1, 1], "mouth"),
    ];
    if archer {
        body.push(bx([-4, 3, -3], [8, 8, 1], "leaf"));
        body.push(bx([2, 6, -4], [2, 6, 2], "wood"));
        body.push(bx([2, 12, -4], [2, 2, 2], "trim"));
        head.push(bx([-3, 14, -3], [6, 3, 6], "leaf"));
        head.push(bx([-3, 11, -3], [6, 4, 1], "leaf"));
        head.push(bx([-3, 12, -3], [1, 4, 4], "leaf"));
        head.push(bx([2, 12, -3], [1, 4, 4], "leaf"));
    } else {
        head.push(bx([-3, 14, -3], [6, 3, 6], "steel"));
        head.push(bx([-3, 11, -3], [6, 4, 1], "steel"));
        head.push(bx([-3, 12, -3], [1, 3, 5], "steel"));
        head.push(bx([2, 12, -3], [1, 3, 5], "steel"));
        head.push(bx([-1, 17, -3], [2, 2, 6], "cloth"));
        head.push(bx([-3, 14, 3], [6, 1, 1], "steel"));
    }
    let mut forearm_r = vec![bx([-6, 5, -1], [2, 3, 2], "skin")];
    let mut forearm_l = vec![bx([4, 5, -1], [2, 3, 2], "skin")];
    if archer {
        forearm_l.push(bx([4, 3, 0], [1, 10, 1], "wood"));
        forearm_l.push(bx([4, 12, 1], [1, 1, 2], "wood"));
        forearm_l.push(bx([4, 2, 1], [1, 1, 2], "wood"));
        forearm_l.push(bx([4, 4, 3], [1, 8, 1], "string"));
    } else {
        forearm_r.push(bx([-6, 5, 1], [1, 1, 8], "steel"));
        forearm_r.push(bx([-7, 5, 1], [3, 1, 1], "trim"));
        forearm_l.push(bx([6, 3, -2], [1, 8, 5], "cloth"));
        forearm_l.push(bx([7, 6, 0], [1, 2, 1], "trim"));
        forearm_l.push(bx([6, 2, -1], [1, 1, 3], "steel"));
    }
    let parts = vec![
        part("body", None, [0.0, 5.0, 0.0], body),
        part("head", Some("body"), [0.0, 11.0, 0.0], head),
        part("arm_r", Some("body"), [-5.0, 10.0, 0.0], vec![bx([-6, 8, -1], [2, 3, 2], if archer { "leaf" } else { "steel" })]),
        part("forearm_r", Some("arm_r"), [-5.0, 8.0, 0.0], forearm_r),
        part("arm_l", Some("body"), [5.0, 10.0, 0.0], vec![bx([4, 8, -1], [2, 3, 2], if archer { "leaf" } else { "steel" })]),
        part("forearm_l", Some("arm_l"), [5.0, 8.0, 0.0], forearm_l),
        part("leg_r", Some("body"), [-2.5, 5.0, 0.0], vec![bx([-4, 1, -2], [3, 4, 4], "dark"), bx([-4, 0, -2], [3, 1, 5], "boot")]),
        part("leg_l", Some("body"), [2.5, 5.0, 0.0], vec![bx([1, 1, -2], [3, 4, 4], "dark"), bx([1, 0, -2], [3, 1, 5], "boot")]),
    ];
    let mut anims = std::collections::BTreeMap::new();
    anims.insert(
        "idle".to_string(),
        clip(
            2.2,
            true,
            vec![
                ("head", tr(&[], &[(0.0, [0.0, 0.0, 0.0]), (1.1, [0.0, 0.5, 0.0])])),
                ("arm_r", tr(&[(0.0, [0.0, 0.0, -2.0]), (1.1, [0.0, 0.0, -4.0])], &[])),
                ("arm_l", tr(&[(0.0, [0.0, 0.0, 2.0]), (1.1, [0.0, 0.0, 4.0])], &[])),
            ],
        ),
    );
    anims.insert(
        "walk".to_string(),
        clip(
            0.6,
            true,
            vec![
                ("leg_r", tr(&[(0.0, [34.0, 0.0, 0.0]), (0.3, [-34.0, 0.0, 0.0])], &[])),
                ("leg_l", tr(&[(0.0, [-34.0, 0.0, 0.0]), (0.3, [34.0, 0.0, 0.0])], &[])),
                ("arm_r", tr(&[(0.0, [-14.0, 0.0, -4.0]), (0.3, [14.0, 0.0, -4.0])], &[])),
                ("arm_l", tr(&[(0.0, [20.0, 0.0, 4.0]), (0.3, [-20.0, 0.0, 4.0])], &[])),
                ("body", tr(&[(0.0, [3.0, 0.0, 0.0]), (0.3, [3.0, 0.0, 0.0])], &[(0.0, [0.0, 0.0, 0.0]), (0.15, [0.0, 0.8, 0.0]), (0.3, [0.0, 0.0, 0.0]), (0.45, [0.0, 0.8, 0.0])])),
            ],
        ),
    );
    if archer {
        anims.insert(
            "attack".to_string(),
            clip(
                0.7,
                false,
                vec![
                    ("arm_l", tr(&[(0.0, [0.0, 0.0, 0.0]), (0.12, [-88.0, 0.0, 0.0]), (0.5, [-88.0, 0.0, 0.0]), (0.7, [0.0, 0.0, 0.0])], &[])),
                    ("arm_r", tr(&[(0.0, [0.0, 0.0, 0.0]), (0.12, [-88.0, 0.0, 0.0]), (0.4, [-88.0, 0.0, 0.0]), (0.7, [0.0, 0.0, 0.0])], &[])),
                    ("forearm_r", tr(&[(0.0, [0.0, 0.0, 0.0]), (0.12, [0.0, 0.0, 0.0]), (0.4, [60.0, 0.0, 0.0]), (0.46, [0.0, 0.0, 0.0]), (0.7, [0.0, 0.0, 0.0])], &[])),
                    ("body", tr(&[(0.0, [0.0, 0.0, 0.0]), (0.4, [-6.0, 0.0, 0.0]), (0.7, [0.0, 0.0, 0.0])], &[])),
                ],
            ),
        );
    } else {
        anims.insert(
            "attack".to_string(),
            clip(
                0.5,
                false,
                vec![
                    ("arm_r", tr(&[(0.0, [0.0, 0.0, -4.0]), (0.12, [-150.0, 0.0, -4.0]), (0.26, [-28.0, 0.0, -4.0]), (0.5, [0.0, 0.0, -4.0])], &[])),
                    ("forearm_r", tr(&[(0.0, [0.0, 0.0, 0.0]), (0.12, [-30.0, 0.0, 0.0]), (0.26, [-60.0, 0.0, 0.0]), (0.5, [0.0, 0.0, 0.0])], &[])),
                    ("body", tr(&[(0.0, [0.0, 0.0, 0.0]), (0.12, [-8.0, 0.0, 0.0]), (0.26, [16.0, 0.0, 0.0]), (0.5, [0.0, 0.0, 0.0])], &[])),
                    ("arm_l", tr(&[(0.0, [0.0, 0.0, 4.0]), (0.2, [-40.0, 0.0, 4.0]), (0.5, [0.0, 0.0, 4.0])], &[])),
                ],
            ),
        );
    }
    anims.insert(
        "die".to_string(),
        clip(
            0.7,
            false,
            vec![
                ("body", tr(&[(0.0, [0.0, 0.0, 0.0]), (0.6, [-88.0, 0.0, 0.0])], &[(0.0, [0.0, 0.0, 0.0]), (0.6, [0.0, -4.0, -2.0])])),
                ("arm_r", tr(&[(0.0, [0.0, 0.0, 0.0]), (0.5, [-110.0, 0.0, -35.0])], &[])),
                ("arm_l", tr(&[(0.0, [0.0, 0.0, 0.0]), (0.5, [-80.0, 0.0, 35.0])], &[])),
            ],
        ),
    );
    AssetDef {
        name: name.into(),
        voxel: 0.12,
        palette: pal(&[
            ("cloth", "#e0453a:fabric"),
            ("steel", "#c4ccd4:metal"),
            ("trim", "#f2d16b"),
            ("skin", "#f1c27d"),
            ("dark", "#3a3038:fabric"),
            ("boot", "#4a3426"),
            ("wood", "#8a5a2b:wood"),
            ("leaf", "#3f7a3a:fabric"),
            ("eye", "#1b1b22"),
            ("mouth", "#b5564b"),
            ("string", "#efe6d0"),
        ]),
        parts,
        anims,
    }
}

// ---------------------------------------------------------------- castle

fn castle() -> AssetDef {
    let mut s: Vec<Op> = vec![];
    // curtain wall with a courtyard
    s.push(bx([-12, 0, -12], [24, 8, 24], "stone"));
    s.push(Op::Clear { at: [-10, 2, -10], size: [20, 7, 20] });
    s.push(bx([-10, 1, -10], [20, 1, 20], "floor"));
    // crenellations along the wall top
    for i in (-12..12).step_by(2) {
        for (x, z) in [(i, -12), (i, 11), (-12, i), (11, i)] {
            s.push(bx([x, 8, z], [1, 2, 1], "stone2"));
        }
    }
    // corner towers with cone roofs and flags
    for (cx, cz) in [(-13, -13), (7, -13), (-13, 7), (7, 7)] {
        s.push(bx([cx, 0, cz], [6, 16, 6], "stone2"));
        for k in 0..4 {
            s.push(bx([cx - 1 + k, 16 + k * 2, cz - 1 + k], [8 - 2 * k, 2, 8 - 2 * k], "roof"));
        }
        s.push(bx([cx + 2, 24, cz + 2], [1, 5, 1], "wood"));
        s.push(bx([cx + 3, 27, cz + 2], [3, 2, 1], "flag"));
        s.push(bx([cx + 1, 9, cz + 6], [2, 3, 1], "win"));
    }
    // keep
    s.push(bx([-6, 0, -6], [12, 18, 12], "stone"));
    for i in (-6..6).step_by(2) {
        for (x, z) in [(i, -6), (i, 5), (-6, i), (5, i)] {
            s.push(bx([x, 18, z], [1, 2, 1], "stone2"));
        }
    }
    s.push(bx([-4, 18, -4], [8, 6, 8], "stone2"));
    for k in 0..5 {
        s.push(bx([-5 + k, 24 + k * 2, -5 + k], [10 - 2 * k, 2, 10 - 2 * k], "roof"));
    }
    s.push(bx([0, 34, 0], [1, 8, 1], "wood"));
    s.push(bx([1, 38, 0], [7, 4, 1], "flag"));
    // windows on the keep
    for (x, z, sx, sz) in [(-4, 6, 2, 1), (2, 6, 2, 1), (-4, -7, 2, 1), (2, -7, 2, 1), (6, -4, 1, 2), (6, 2, 1, 2), (-7, -4, 1, 2), (-7, 2, 1, 2)] {
        s.push(bx([x, 10, z], [sx, 3, sz], "win"));
        s.push(bx([x, 4, z], [sx, 3, sz], "win"));
    }
    // gate in the +z wall, with a timber door and an arch
    s.push(Op::Clear { at: [-3, 0, 10], size: [6, 7, 4] });
    s.push(bx([-3, 0, 10], [6, 6, 1], "door"));
    s.push(bx([-3, 6, 10], [6, 1, 3], "stone2"));
    s.push(bx([-4, 0, 10], [1, 8, 3], "stone2"));
    s.push(bx([3, 0, 10], [1, 8, 3], "stone2"));
    AssetDef {
        name: "castle".into(),
        voxel: 0.2,
        palette: pal(&[
            ("stone", "#a9a9b2:stone"),
            ("stone2", "#8a8a96:stone"),
            ("floor", "#b9a98a:stone"),
            ("roof", "#e0453a:tile"),
            ("flag", "#e0453a:fabric"),
            ("wood", "#6b4423:wood"),
            ("door", "#7a4f2a:wood"),
            ("win", "#ffd27a*"),
        ]),
        parts: vec![part("keep", None, [0.0, 0.0, 0.0], s)],
        anims: std::collections::BTreeMap::new(),
    }
}

// ---------------------------------------------------------------- effects

fn meteor() -> AssetDef {
    let s = vec![
        Op::Sphere { at: [0.0, 8.0, 0.0], r: 7.0, c: "rock".into() },
        Op::Sphere { at: [3.0, 11.0, 3.0], r: 2.2, c: "lava".into() },
        Op::Sphere { at: [-4.0, 7.0, 3.0], r: 2.0, c: "lava".into() },
        Op::Sphere { at: [1.0, 4.0, -4.0], r: 2.4, c: "lava".into() },
        bx([-3, 14, -3], [6, 6, 6], "fire"),
        bx([-2, 20, -2], [4, 6, 4], "fire2"),
        bx([-1, 26, -1], [2, 7, 2], "fire3"),
        bx([-4, 12, -1], [2, 5, 2], "fire"),
        bx([2, 12, -1], [2, 4, 2], "fire"),
    ];
    AssetDef {
        name: "meteor".into(),
        voxel: 0.12,
        palette: pal(&[("rock", "#5b4234:stone"), ("lava", "#ff7a1a!"), ("fire", "#ff8a2a!"), ("fire2", "#ffb347!"), ("fire3", "#ffe08a!")]),
        parts: vec![part("body", None, [0.0, 8.0, 0.0], s)],
        anims: std::collections::BTreeMap::new(),
    }
}

fn boom() -> AssetDef {
    AssetDef {
        name: "boom".into(),
        voxel: 0.2,
        palette: pal(&[("o", "#ff7a1a!"), ("m", "#ffb347!"), ("i", "#fff0b0!")]),
        parts: vec![part(
            "body",
            None,
            [0.0, 6.0, 0.0],
            vec![
                Op::Sphere { at: [0.0, 6.0, 0.0], r: 6.5, c: "o".into() },
                Op::Sphere { at: [0.0, 6.0, 0.0], r: 4.6, c: "m".into() },
                Op::Sphere { at: [0.0, 6.0, 0.0], r: 2.6, c: "i".into() },
                bx([-1, 0, -1], [2, 14, 2], "m"),
                bx([-8, 5, -1], [16, 2, 2], "o"),
                bx([-1, 5, -8], [2, 2, 16], "o"),
            ],
        )],
        anims: std::collections::BTreeMap::new(),
    }
}

fn arrow() -> AssetDef {
    AssetDef {
        name: "arrow".into(),
        voxel: 0.07,
        palette: pal(&[("tip", "#cfd8dc:metal"), ("shaft", "#8a5a2b:wood"), ("fl", "#f4f0e0")]),
        parts: vec![part("body", None, [0.0, 0.0, 0.0], vec![bx([0, 0, 0], [1, 3, 1], "tip"), bx([0, 3, 0], [1, 12, 1], "shaft"), bx([-1, 13, 0], [3, 3, 1], "fl")])],
        anims: std::collections::BTreeMap::new(),
    }
}

fn bolt() -> AssetDef {
    let mut s = vec![Op::Sphere { at: [0.0, 3.0, 0.0], r: 4.0, c: "core".into() }];
    let pts = [[0, 0, 0], [3, 14, 0], [-3, 28, 1], [3, 42, 0], [-2, 56, 0], [1, 70, 0], [0, 84, 0]];
    for w in pts.windows(2) {
        s.push(Op::Line { from: w[0], to: w[1], c: "bolt".into() });
        s.push(Op::Line { from: [w[0][0] + 1, w[0][1], w[0][2]], to: [w[1][0] + 1, w[1][1], w[1][2]], c: "bolt".into() });
        s.push(Op::Line { from: [w[0][0], w[0][1], w[0][2] + 1], to: [w[1][0], w[1][1], w[1][2] + 1], c: "core".into() });
    }
    AssetDef {
        name: "bolt".into(),
        voxel: 0.12,
        palette: pal(&[("bolt", "#8fd0ff!"), ("core", "#ffffff!")]),
        parts: vec![part("body", None, [0.0, 0.0, 0.0], s)],
        anims: std::collections::BTreeMap::new(),
    }
}

/// A dome of glowing lattice voxels around a castle.
fn shield() -> AssetDef {
    const R: i32 = 14;
    let mut layers: Vec<Vec<String>> = vec![];
    for y in 0..=R {
        let mut rows: Vec<String> = vec![];
        for z in -R..=R {
            let mut row = String::new();
            for x in -R..=R {
                let d = ((x * x + y * y + z * z) as f32).sqrt();
                let shell = d <= R as f32 + 0.3 && d >= R as f32 - 1.1;
                let lattice = (x + z).rem_euclid(5) == 0 || (x - z).rem_euclid(5) == 0 || y % 5 == 0;
                row.push(if shell && lattice { 'g' } else { '.' });
            }
            rows.push(row);
        }
        layers.push(rows);
    }
    AssetDef {
        name: "shield".into(),
        voxel: 0.24,
        palette: pal(&[("g", "#7fdcff!")]),
        parts: vec![part("body", None, [0.0, 0.0, 0.0], vec![Op::Grid { at: [-R, 0, -R], layers }])],
        anims: std::collections::BTreeMap::new(),
    }
}

fn sparkle() -> AssetDef {
    AssetDef {
        name: "sparkle".into(),
        voxel: 0.14,
        palette: pal(&[("g", "#6dff8a!"), ("w", "#e8fff0!")]),
        parts: vec![part("body", None, [0.0, 6.0, 0.0], vec![bx([-6, 5, -1], [13, 3, 2], "g"), bx([-1, 0, -1], [3, 13, 2], "g"), bx([-1, 5, -1], [3, 3, 2], "w"), bx([-8, 10, -1], [3, 3, 2], "w"), bx([6, 1, -1], [3, 3, 2], "w")])],
        anims: std::collections::BTreeMap::new(),
    }
}

fn rock() -> AssetDef {
    AssetDef {
        name: "rock".into(),
        voxel: 0.14,
        palette: pal(&[("a", "#8d939c:stone"), ("b", "#767c86:stone")]),
        parts: vec![part("body", None, [0.0, 0.0, 0.0], vec![Op::Sphere { at: [0.0, 3.0, 0.0], r: 4.5, c: "a".into() }, Op::Sphere { at: [4.0, 2.0, 2.0], r: 3.0, c: "b".into() }, bx([-6, 0, -6], [12, 1, 12], "b")])],
        anims: std::collections::BTreeMap::new(),
    }
}

pub(crate) fn banner() -> AssetDef {
    let mut anims = std::collections::BTreeMap::new();
    anims.insert("idle".to_string(), clip(1.6, true, vec![("flag", tr(&[(0.0, [0.0, 8.0, 0.0]), (0.8, [0.0, -8.0, 0.0])], &[]))]));
    AssetDef {
        name: "banner".into(),
        voxel: 0.12,
        palette: pal(&[("pole", "#6b4423:wood"), ("cloth", "#e0453a:fabric"), ("trim", "#f2d16b")]),
        parts: vec![
            part("pole", None, [0.0, 0.0, 0.0], vec![bx([0, 0, 0], [1, 26, 1], "pole"), bx([-1, 26, -1], [3, 2, 3], "trim")]),
            part("flag", Some("pole"), [1.0, 24.0, 0.0], vec![bx([1, 16, 0], [10, 8, 1], "cloth"), bx([1, 16, 0], [10, 1, 1], "trim"), bx([1, 23, 0], [10, 1, 1], "trim"), bx([5, 19, 0], [2, 2, 1], "trim")]),
        ],
        anims,
    }
}

fn tent() -> AssetDef {
    let mut s = vec![];
    for k in 0..6 {
        s.push(bx([-8 + k, k * 2, -8], [16 - 2 * k, 2, 16], "cloth"));
    }
    s.push(Op::Clear { at: [-2, 0, 6], size: [4, 7, 3] });
    s.push(bx([-9, 0, -8], [1, 2, 16], "dark"));
    s.push(bx([8, 0, -8], [1, 2, 16], "dark"));
    AssetDef {
        name: "tent".into(),
        voxel: 0.15,
        palette: pal(&[("cloth", "#e0453a:fabric"), ("dark", "#4a3426:wood")]),
        parts: vec![part("body", None, [0.0, 0.0, 0.0], s)],
        anims: std::collections::BTreeMap::new(),
    }
}

fn cactus() -> AssetDef {
    AssetDef {
        name: "cactus".into(),
        voxel: 0.14,
        palette: pal(&[("g", "#4e9a45"), ("d", "#3f7f38"), ("f", "#f06292")]),
        parts: vec![part(
            "body",
            None,
            [0.0, 0.0, 0.0],
            vec![bx([-2, 0, -2], [4, 16, 4], "g"), bx([-2, 0, -2], [1, 16, 4], "d"), bx([-6, 7, -1], [4, 3, 2], "g"), bx([-6, 7, -1], [2, 8, 2], "g"), bx([2, 9, -1], [4, 3, 2], "g"), bx([4, 9, -1], [2, 7, 2], "g"), bx([-1, 16, -1], [2, 2, 2], "f")],
        )],
        anims: std::collections::BTreeMap::new(),
    }
}

fn pine() -> AssetDef {
    let mut s = vec![bx([-1, 0, -1], [2, 5, 2], "trunk")];
    for k in 0..5 {
        let w = 11 - 2 * k;
        let y = 4 + k * 4;
        s.push(bx([-w / 2, y, -w / 2], [w, 3, w], "pine"));
        s.push(bx([-w / 2 + 1, y + 3, -w / 2 + 1], [w - 2, 1, w - 2], "snow"));
    }
    AssetDef {
        name: "pine".into(),
        voxel: 0.14,
        palette: pal(&[("trunk", "#6b4423:wood"), ("pine", "#2f6b3a:grass"), ("snow", "#f4f8fc:plaster")]),
        parts: vec![part("body", None, [0.0, 0.0, 0.0], s)],
        anims: std::collections::BTreeMap::new(),
    }
}

fn deadtree() -> AssetDef {
    AssetDef {
        name: "deadtree".into(),
        voxel: 0.14,
        palette: pal(&[("c", "#3a2f2a:wood"), ("e", "#ff7a1a!")]),
        parts: vec![part(
            "body",
            None,
            [0.0, 0.0, 0.0],
            vec![bx([-1, 0, -1], [3, 16, 3], "c"), bx([-5, 10, 0], [4, 2, 2], "c"), bx([-6, 12, 0], [2, 5, 2], "c"), bx([2, 12, -1], [4, 2, 2], "c"), bx([5, 14, -1], [2, 5, 2], "c"), bx([0, 16, 0], [1, 2, 1], "e"), bx([-6, 17, 0], [1, 1, 1], "e")],
        )],
        anims: std::collections::BTreeMap::new(),
    }
}

/// The four biomes: tile recolours by legend key, sky, scenery swaps, speed and hazards.
fn biomes_json() -> Value {
    let tiles = |spec: &[(&str, &str, &str)]| -> Value {
        let mut m = serde_json::Map::new();
        for (k, top, side) in spec {
            m.insert(k.to_string(), json!({"top": top, "side": side}));
        }
        Value::Object(m)
    };
    json!({
        "grass": {"name": "PADANG HIJAU", "sky": "day", "speed": 1.0,
            "terrain": tiles(&[("g", "#78b856:grass", "#8a6040:dirt"), ("G", "#6aae4c:grass", "#8a6040:dirt"), ("h", "#6aae4c:grass", "#8a6040:dirt"), ("H", "#7bbd58:grass", "#8a6040:dirt"), ("s", "#e6d29b:sand", "#cdb27a:sand"), ("d", "#b98a58:dirt", "#8a6040:dirt"), ("p", "#cdbfa5:stone", "#a8987a:stone"), ("w", "#3d95dc", "#8a6040:dirt")]),
            "props": [{"prefix": "tree_", "asset": "tree"}, {"prefix": "palm_", "asset": "palm"}]},
        "desert": {"name": "GURUN", "sky": "desert", "speed": 0.95,
            "terrain": tiles(&[("g", "#e0c07a:sand", "#c49a58:sand"), ("G", "#d6b46c:sand", "#c49a58:sand"), ("h", "#d9ae66:sand", "#c49a58:sand"), ("H", "#e3bd72:sand", "#c49a58:sand"), ("s", "#f0dca0:sand", "#d8bf80:sand"), ("d", "#b9824f:dirt", "#8a5a30:dirt"), ("p", "#c9b79a:stone", "#a8987a:stone"), ("w", "#4fb0c6", "#c49a58:sand")]),
            "props": [{"prefix": "tree_", "asset": "cactus"}, {"prefix": "palm_", "asset": "palm"}]},
        "snow": {"name": "SALJU", "sky": "snow", "speed": 0.85,
            "terrain": tiles(&[("g", "#f2f6fa:plaster", "#c9d6e0:stone"), ("G", "#e3edf6:plaster", "#c9d6e0:stone"), ("h", "#e9f1f8:plaster", "#c9d6e0:stone"), ("H", "#f4f8fc:plaster", "#c9d6e0:stone"), ("s", "#cfe0ec:stone", "#aebfcc:stone"), ("d", "#a89a8a:dirt", "#7a6a5a:dirt"), ("p", "#b8c2cc:stone", "#98a4b0:stone"), ("w", "#8fd0ee", "#9db8c8:stone")]),
            "props": [{"prefix": "tree_", "asset": "pine"}, {"prefix": "palm_", "asset": "pine"}]},
        "lava": {"name": "GUNUNG API", "sky": "lava", "speed": 1.0,
            "terrain": tiles(&[("g", "#4a3b3a:stone", "#2a2224:stone"), ("G", "#403234:stone", "#2a2224:stone"), ("h", "#554341:stone", "#2a2224:stone"), ("H", "#5d4a46:stone", "#2a2224:stone"), ("s", "#7a5a4a:stone", "#4a3228:stone"), ("d", "#6b3a2a:dirt", "#3a2018:dirt"), ("p", "#6a6068:stone", "#403840:stone"), ("w", "#ff5a1a", "#8a2a10:dirt")]),
            "props": [{"prefix": "tree_", "asset": "deadtree"}, {"prefix": "palm_", "asset": "deadtree"}],
            "hazard": {"every": 16, "do": {"type": "meteor", "count": 2, "target": "field", "damage": 80, "castle_damage": 30, "radius": 3.0}, "side": "random"}}
    })
}

/// Every asset the scene uses, once each.
pub fn assets() -> Vec<(String, AssetDef)> {
    let mut out: Vec<(String, AssetDef)> = vec![
        ("knight".into(), soldier("knight", false)),
        ("archer".into(), soldier("archer", true)),
        ("castle".into(), castle()),
        ("meteor".into(), meteor()),
        ("boom".into(), boom()),
        ("arrow".into(), arrow()),
        ("bolt".into(), bolt()),
        ("shield".into(), shield()),
        ("sparkle".into(), sparkle()),
        ("rock".into(), rock()),
        ("banner".into(), banner()),
        ("tent".into(), tent()),
        ("cactus".into(), cactus()),
        ("pine".into(), pine()),
        ("deadtree".into(), deadtree()),
    ];
    for (n, d) in kampung_assets() {
        if matches!(n, "palm" | "tree" | "lamp") {
            out.push((n.to_string(), d));
        }
    }
    out
}

// ---------------------------------------------------------------- scene

fn terrain() -> TerrainDef {
    let mut m = vec![vec!['w'; W]; D];
    for j in 0..D {
        for i in 0..W {
            let (x, z) = (i as f32 + 0.5, j as f32 + 0.5);
            let (cx, cz, hx, hz, r) = (26.0f32, 15.0f32, 23.5f32, 12.5f32, 4.0f32);
            let (qx, qz) = ((x - cx).abs() - (hx - r), (z - cz).abs() - (hz - r));
            let d = qx.max(0.0).hypot(qz.max(0.0)) + qx.max(qz).min(0.0) - r + 0.8 * hash3(i as i32, j as i32, 5);
            m[j][i] = if d > 0.0 {
                'w'
            } else if d > -1.0 {
                's'
            } else if x < 16.0 || x > 36.0 {
                if hash3(i as i32, j as i32, 3) > 0.5 { 'H' } else { 'h' }
            } else if hash3(i as i32, j as i32, 4) > 0.55 {
                'G'
            } else {
                'g'
            };
        }
    }
    // courtyards in front of the castles and the dirt road between them
    for (x0, x1) in [(3usize, 14usize), (38usize, 49usize)] {
        for row in m.iter_mut().take(20).skip(10) {
            for c in row.iter_mut().take(x1 + 1).skip(x0) {
                if matches!(*c, 'H' | 'h') {
                    *c = 'p';
                }
            }
        }
    }
    for row in m.iter_mut().take(17).skip(13) {
        for c in row.iter_mut().take(37).skip(16) {
            if matches!(*c, 'g' | 'G') {
                *c = 'd';
            }
        }
    }
    let t = |h: i32, top: &str, side: &str, jitter: f32, water: bool| TileDef { h, top: top.into(), side: Some(side.into()), jitter, water };
    let mut legend = std::collections::BTreeMap::new();
    legend.insert("g".to_string(), t(2, "#78b856:grass", "#8a6040:dirt", 0.04, false));
    legend.insert("G".to_string(), t(2, "#6aae4c:grass", "#8a6040:dirt", 0.05, false));
    legend.insert("h".to_string(), t(3, "#6aae4c:grass", "#8a6040:dirt", 0.04, false));
    legend.insert("H".to_string(), t(3, "#7bbd58:grass", "#8a6040:dirt", 0.04, false));
    legend.insert("s".to_string(), t(2, "#e6d29b:sand", "#cdb27a:sand", 0.05, false));
    legend.insert("d".to_string(), t(2, "#b98a58:dirt", "#8a6040:dirt", 0.06, false));
    legend.insert("p".to_string(), t(3, "#cdbfa5:stone", "#a8987a:stone", 0.03, false));
    legend.insert("w".to_string(), t(1, "#3d95dc", "#8a6040:dirt", 0.03, true));
    let map = m.iter().map(|r| r.iter().collect()).collect();
    TerrainDef { cell: 1.0, step: 0.5, legend, map }
}

/// Palette overrides that keep each key's material but recolour it.
fn recolor(color: &str, keys: &[&str]) -> std::collections::BTreeMap<String, String> {
    keys.iter().map(|k| (k.to_string(), color.to_string())).collect()
}

fn entities() -> Vec<EntityDef> {
    let (red, blue) = ("#e0453a", "#2f7de1");
    let mut v = vec![];
    for (id, pos, yaw, color) in [("castle_a", CASTLE_A, 90.0, red), ("castle_b", CASTLE_B, 270.0, blue)] {
        let mut e = ent(id, "castle", pos, yaw);
        e.palette = recolor(color, &["roof", "flag"]);
        v.push(e);
    }
    // camp dressing: tents, banners, lamps, rocks and trees
    for (id, pos, yaw, color) in [("tent_a1", [5.0, 6.0], 90.0, red), ("tent_a2", [5.0, 18.0], 90.0, red), ("tent_b1", [47.0, 6.0], 270.0, blue), ("tent_b2", [47.0, 18.0], 270.0, blue)] {
        let mut e = ent(id, "tent", pos, yaw);
        e.palette = recolor(color, &["cloth"]);
        v.push(e);
    }
    for (id, pos, color) in [("banner_a1", [14.5, 9.0], red), ("banner_a2", [14.5, 15.0], red), ("banner_b1", [37.5, 9.0], blue), ("banner_b2", [37.5, 15.0], blue)] {
        let mut e = ent(id, "banner", pos, 0.0);
        e.palette = recolor(color, &["cloth"]);
        v.push(e);
    }
    for (id, pos) in [("lamp_a1", [12.0, 8.0]), ("lamp_a2", [12.0, 16.0]), ("lamp_b1", [40.0, 8.0]), ("lamp_b2", [40.0, 16.0])] {
        v.push(ent(id, "lamp", pos, 0.0));
    }
    for (i, (x, z, s)) in [(19.0, 5.5, 1.0), (24.0, 17.5, 1.3), (30.0, 6.0, 0.9), (33.0, 18.0, 1.1), (22.0, 8.0, 0.7), (29.5, 15.5, 0.8), (26.0, 4.5, 1.2), (26.0, 19.5, 1.0)].into_iter().enumerate() {
        let mut e = ent(&format!("rock{i}"), "rock", [x, z], i as f32 * 47.0);
        e.scale = s;
        v.push(e);
    }
    for (i, (x, z)) in [(4.0, 3.5), (9.5, 4.0), (15.0, 3.8), (4.5, 20.5), (11.0, 20.0), (15.5, 20.2), (48.0, 3.5), (42.0, 4.0), (36.5, 3.8), (47.5, 20.5), (41.0, 20.0), (36.5, 20.2)].into_iter().enumerate() {
        let (asset, prefix) = if i % 3 == 0 { ("palm", "palm") } else { ("tree", "tree") };
        v.push(ent(&format!("{prefix}_{i}"), asset, [x, z], i as f32 * 61.0));
    }
    // the castles were placed in the middle of the deeper field; the dressing was drawn for a shallower one
    for e in v.iter_mut().skip(2) {
        e.pos[1] += 3.0;
    }
    v
}

pub fn scene() -> Result<SceneDef> {
    Ok(SceneDef {
        name: "Perang Kerajaan".into(),
        assets: assets().into_iter().map(|(n, _)| (n.clone(), AssetRef::Path(format!("assets/{n}.json")))).collect(),
        terrain: Some(terrain()),
        sky: SkyDef { preset: "day".into(), ..Default::default() },
        markers: Default::default(),
        buildings: vec![],
        entities: entities(),
        story: None,
        camera: CamParams { target: Some([26.0, 1.5, 15.8]), yaw: Some(0.0), pitch: Some(38.0), zoom: Some(30.0), ..Default::default() },
    })
}

// ---------------------------------------------------------------- the war

/// war.json: sides, units, effects, the gift menu, macros and bots.
pub fn war_json() -> Value {
    let pop = |emoji: &str, secs: f32| json!({"type": "emoji", "emoji": emoji, "size": 0.16, "at": [0.5, 0.5], "ttl": secs, "anim_in": "pop", "anim_out": "fade", "z": 5});
    let who = |color: &str| json!({"type": "text", "text": "{user}", "size": 0.04, "at": [0.5, 0.62], "color": color, "ttl": 1.8, "anim_in": "slide_up", "z": 5, "outline": "#000000"});
    json!({
        "title": "PERANG KERAJAAN",
        "scene": "scene.json",
        "castle_hp": 1000,
        "match": {"mode": "rounds", "rounds": 3, "round_secs": 90, "lobby_secs": 5, "intermission": 7, "restart_secs": 20},
        "initial_units": 5,
        "autospawn": {"every": 7.0, "count": 1},
        "max_units": 40,
        "texts": {"round": "BABAK {round}", "round_win": "KERAJAAN {name} MENANG!", "match_win": "KERAJAAN {name} JUARA!", "draw": "SERI!", "match_draw": "SERI, TIDAK ADA PEMENANG"},
        "sides": {
            "A": {"name": "MERAH", "color": "#e0453a", "castle": "castle_a", "spawn": [12.5, 15.0], "unit": "knight", "facing": 90, "castle_radius": 3.0, "zone": [4.0, 4.0, 25.0, 26.0]},
            "B": {"name": "BIRU", "color": "#2f7de1", "castle": "castle_b", "spawn": [39.5, 15.0], "unit": "knight", "facing": 270, "castle_radius": 3.0, "zone": [27.0, 4.0, 48.0, 26.0]}
        },
        "units": {
            "knight": {"asset": "knight", "hp": 45, "speed": 2.0, "dmg": 7, "range": 1.2, "cooldown": 0.9},
            "archer": {"asset": "archer", "hp": 28, "speed": 1.8, "dmg": 5, "range": 5.5, "cooldown": 1.3, "ranged": true, "castle_mult": 0.5},
            "giant": {"asset": "knight", "hp": 320, "speed": 1.4, "dmg": 28, "range": 2.0, "cooldown": 1.3, "scale": 1.9, "castle_mult": 1.0}
        },
        "actions": {
            "meteor": {"type": "meteor", "count": 1, "radius": 3.2, "damage": 150, "castle_damage": 110},
            "meteor_storm": {"type": "meteor", "count": 8, "radius": 3.6, "damage": 160, "castle_damage": 120, "gap": 0.22},
            "arrow_rain": {"type": "arrows", "count": 22, "radius": 3.2, "damage": 10, "castle_damage": 5},
            "lightning": {"type": "lightning", "count": 3, "damage": 170, "castle_damage": 60},
            "reinforce": {"type": "spawn", "unit": "knight", "count": 4},
            "archers": {"type": "spawn", "unit": "archer", "count": 4},
            "giant": {"type": "spawn", "unit": "giant", "count": 1},
            "heal": {"type": "heal", "amount": 300},
            "shield": {"type": "shield", "secs": 10},
            "boost": {"type": "boost", "secs": 10, "speed": 1.4, "damage": 1.6},
            "quake": {"type": "quake", "damage": 40, "castle_damage": 60},
            "tsunami": {"type": "push", "strength": 14, "radius": 40, "damage": 30, "target": "front"},
            "thunderstorm": {"type": "lightning", "count": 9, "damage": 150, "castle_damage": 70}
        },
        "gift_aliases": {"mawar": "rose", "donat": "doughnut", "jari hati": "fingerheart", "love": "fingerheart", "parfum": "perfume", "singa": "lion", "galaksi": "galaxy", "paus": "whale", "semesta": "universe"},
        "triggers": [
            {"name": "rose", "on": "gift:rose", "side": "A", "do": "meteor", "label": "ROSE = METEOR MERAH", "icon": ":rose:", "overlay": [pop("☄️", 1.4), who("#ffffff")]},
            {"name": "tiktok", "on": "gift:tiktok", "side": "B", "do": "meteor", "label": "TIKTOK = METEOR BIRU", "icon": ":comet:", "overlay": [pop("☄️", 1.4), who("#ffffff")]},
            {"name": "fingerheart", "on": "gift:fingerheart", "side": "A", "do": "reinforce", "per_count": true, "label": "FINGER HEART = PASUKAN MERAH", "icon": ":heart:"},
            {"name": "doughnut", "on": "gift:doughnut", "side": "B", "do": "reinforce", "per_count": true, "label": "DOUGHNUT = PASUKAN BIRU", "icon": ":party:"},
            {"name": "handhearts", "on": "gift:handhearts", "side": "A", "do": "lightning", "label": "HAND HEARTS = PETIR MERAH", "icon": ":bolt:", "overlay": [pop("⚡", 1.2)]},
            {"name": "perfume", "on": "gift:perfume", "side": "B", "do": "arrow_rain", "label": "PERFUME = HUJAN PANAH BIRU", "icon": ":bow:"},
            {"name": "galaxy", "on": "gift:galaxy", "side": "B", "macro": "apocalypse", "label": "GALAXY = ULTIMATE KIAMAT BIRU", "icon": ":glowing_star:", "cooldown": 10},
            {"name": "whale", "on": "gift:whale", "side": "B", "macro": "leviathan", "label": "PAUS = ULTIMATE TSUNAMI BIRU", "icon": ":wave:", "cooldown": 15},
            {"name": "universe", "on": "gift:universe", "side": "A", "macro": "thunder_god", "label": "UNIVERSE = ULTIMATE BADAI PETIR", "icon": ":bolt:", "cooldown": 12},
            {"name": "lion", "on": "gift:lion", "side": "trailing", "macro": "royal_guard", "label": "LION = PENGAWAL (YANG KALAH)", "icon": ":lion:", "cooldown": 8},
            {"name": "join_red", "on": "comment:~merah", "side": "A", "do": {"type": "spawn", "count": 1}, "cooldown": 1.5, "legend": false, "announce": "{user} BERGABUNG KE MERAH"},
            {"name": "join_blue", "on": "comment:~biru", "side": "B", "do": {"type": "spawn", "count": 1}, "cooldown": 1.5, "legend": false, "announce": "{user} BERGABUNG KE BIRU"},
            {"name": "likes", "on": "like", "side": "event", "do": {"type": "spawn", "count": 1}, "min_count": 10, "cooldown": 3, "legend": false, "announce": "{user} LIKE x{count}"}
        ],
        "macros": {
            "apocalypse": [
                {"do": "announce", "text": "{user} MEMANGGIL KIAMAT!", "secs": 2.6, "color": "#ffcc33"},
                {"do": "act", "action": {"type": "cinematic", "pov": "enemy", "secs": 8, "look": "sky", "fov": 78, "shake": 0.35, "tint": "#ff7a1a30", "vignette": 0.5}, "side": "event"},
                {"do": "wait", "secs": 1.2},
                {"do": "act", "action": "meteor_storm", "side": "event"},
                {"do": "wait", "secs": 1.8},
                {"do": "act", "action": "quake", "side": "event"}
            ],
            "leviathan": [
                {"do": "announce", "text": "{user} MELEPAS PAUS RAKSASA!", "secs": 2.6, "color": "#4aa8ff"},
                {"do": "act", "action": {"type": "cinematic", "pov": "enemy", "secs": 7, "look": "enemy", "fov": 82, "pitch": 4, "shake": 0.22, "tint": "#2a7de150", "vignette": 0.55}, "side": "event"},
                {"do": "wait", "secs": 1.6},
                {"do": "act", "action": "tsunami", "side": "event"},
                {"do": "wait", "secs": 1.2},
                {"do": "act", "action": "quake", "side": "event"}
            ],
            "thunder_god": [
                {"do": "announce", "text": "{user} MENURUNKAN BADAI PETIR!", "secs": 2.6, "color": "#bfe3ff"},
                {"do": "act", "action": {"type": "cinematic", "pov": "enemy", "secs": 7, "look": "sky", "fov": 70, "pitch": -18, "shake": 0.15, "tint": "#0a1a4a40", "vignette": 0.6, "flashes": 2.5}, "side": "event"},
                {"do": "wait", "secs": 1.0},
                {"do": "act", "action": "thunderstorm", "side": "event"}
            ],
            "royal_guard": [
                {"do": "announce", "text": "{user}: PENGAWAL RAJA {side}!", "secs": 2.4},
                {"do": "act", "action": "shield", "side": "event"},
                {"do": "act", "action": "boost", "side": "event"},
                {"do": "act", "action": "giant", "side": "event"},
                {"do": "act", "action": "heal", "side": "event"}
            ],
            "comeback": [
                {"do": "if", "cond": {"hp_pct_lt": {"side": "A", "value": 25}}, "then": [{"do": "act", "action": "shield", "side": "A"}, {"do": "announce", "text": "MERAH TERDESAK! PERISAI AKTIF", "secs": 2.0, "color": "#e0453a"}]},
                {"do": "if", "cond": {"hp_pct_lt": {"side": "B", "value": 25}}, "then": [{"do": "act", "action": "shield", "side": "B"}, {"do": "announce", "text": "BIRU TERDESAK! PERISAI AKTIF", "secs": 2.0, "color": "#2f7de1"}]}
            ]
        },
        "bots": [
            {"name": "comeback_watch", "macro": "comeback", "every": 14}
        ],
        "autopilot": {"idle_secs": 6, "interval": [2.5, 6.0], "actions": [
            {"weight": 4, "do": "reinforce", "side": "random"},
            {"weight": 2, "do": "archers", "side": "random"},
            {"weight": 2, "do": "meteor", "side": "leader"},
            {"weight": 1, "do": "arrow_rain", "side": "random"},
            {"weight": 1, "do": "lightning", "side": "leader"}
        ]},
        "biome": "grass",
        "biome_rotation": {"every": "round", "list": ["grass", "desert", "snow", "lava"]},
        "biomes": biomes_json(),
        "overlay": []
    })
}

pub fn write(dir: &Path) -> Result<()> {
    fs::create_dir_all(dir.join("assets"))?;
    for (name, def) in assets() {
        fs::write(dir.join("assets").join(format!("{name}.json")), to_pretty_json(&def)?)?;
    }
    fs::write(dir.join("scene.json"), to_pretty_json(&scene()?)?)?;
    fs::write(dir.join("war.json"), serde_json::to_string_pretty(&war_json())?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assets_bake() {
        for (n, d) in assets() {
            Model::bake(&d).unwrap_or_else(|e| panic!("{n}: {e:#}"));
        }
    }

    #[test]
    fn war_json_parses() {
        let def: crate::war::WarDef = serde_json::from_value(war_json()).expect("war.json schema");
        assert_eq!(def.sides.len(), 2);
        assert!(def.triggers.len() >= 8);
    }
}
