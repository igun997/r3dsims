//! Sample assets and the "kampung" diorama, built from Rust so the same builder API is available to
//! anyone writing their own assets. `r3dsims template kampung --out DIR` writes them as editable JSON.

use std::collections::BTreeMap;
use std::f32::consts::TAU;
use std::fs;
use std::path::Path;

use anyhow::Result;
use serde_json::{Value, json};

use crate::camera::CamParams;
use crate::model::*;
use crate::story::{StageDef, StoryDef, Target};
use crate::terrain::{TerrainDef, TileDef, hash3};
use crate::world::{AssetRef, EntityDef, SceneDef, SkyDef};

// ---------------------------------------------------------------- builder helpers

pub(crate) fn pal(items: &[(&str, &str)]) -> BTreeMap<String, String> {
    items.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
}

pub(crate) fn bx(at: [i32; 3], size: [i32; 3], c: &str) -> Op {
    Op::Box { at, size, c: c.into() }
}

pub(crate) fn part(name: &str, parent: Option<&str>, pivot: [f32; 3], shape: Vec<Op>) -> PartDef {
    PartDef { name: name.into(), parent: parent.map(Into::into), pivot, shape }
}

pub(crate) type Keys<'a> = &'a [(f32, [f32; 3])];

pub(crate) fn tr(rot: Keys, pos: Keys) -> TrackDef {
    TrackDef { rot: rot.to_vec(), pos: pos.to_vec() }
}

pub(crate) fn clip(duration: f32, looping: bool, tracks: Vec<(&str, TrackDef)>) -> ClipDef {
    ClipDef { duration, looping, ease: "smooth".into(), tracks: tracks.into_iter().map(|(k, v)| (k.to_string(), v)).collect() }
}

// ---------------------------------------------------------------- assets

pub fn villager() -> AssetDef {
    let parts = vec![
        part("body", None, [0.0, 5.0, 0.0], vec![bx([-4, 5, -2], [8, 6, 4], "b"), bx([-4, 5, -2], [8, 1, 4], "k")]),
        part(
            "head",
            Some("body"),
            [0.0, 11.0, 0.0],
            vec![
                bx([-3, 11, -3], [6, 6, 6], "s"),
                bx([-3, 15, -3], [6, 2, 6], "h"),
                bx([-3, 11, -3], [6, 4, 2], "h"),
                bx([-3, 14, 2], [6, 1, 1], "h"),
                bx([-3, 12, -1], [1, 3, 3], "h"),
                bx([2, 12, -1], [1, 3, 3], "h"),
                bx([-2, 13, 2], [1, 1, 1], "e"),
                bx([1, 13, 2], [1, 1, 1], "e"),
                bx([-1, 12, 2], [2, 1, 1], "m"),
            ],
        ),
        part("arm_r", Some("body"), [-5.0, 10.0, 0.0], vec![bx([-6, 8, -1], [2, 3, 2], "b")]),
        part("forearm_r", Some("arm_r"), [-5.0, 8.0, 0.0], vec![bx([-6, 5, -1], [2, 3, 2], "s")]),
        part("arm_l", Some("body"), [5.0, 10.0, 0.0], vec![bx([4, 8, -1], [2, 3, 2], "b")]),
        part("forearm_l", Some("arm_l"), [5.0, 8.0, 0.0], vec![bx([4, 5, -1], [2, 3, 2], "s")]),
        part("leg_r", Some("body"), [-2.5, 5.0, 0.0], vec![bx([-4, 1, -2], [3, 4, 4], "p"), bx([-4, 0, -2], [3, 1, 5], "o")]),
        part("leg_l", Some("body"), [2.5, 5.0, 0.0], vec![bx([1, 1, -2], [3, 4, 4], "p"), bx([1, 0, -2], [3, 1, 5], "o")]),
    ];
    let mut anims = BTreeMap::new();
    anims.insert(
        "idle".to_string(),
        clip(
            2.4,
            true,
            vec![
                ("head", tr(&[], &[(0.0, [0.0, 0.0, 0.0]), (1.2, [0.0, 0.5, 0.0])])),
                ("arm_r", tr(&[(0.0, [0.0, 0.0, -2.0]), (1.2, [0.0, 0.0, -5.0])], &[])),
                ("arm_l", tr(&[(0.0, [0.0, 0.0, 2.0]), (1.2, [0.0, 0.0, 5.0])], &[])),
            ],
        ),
    );
    anims.insert(
        "walk".to_string(),
        clip(
            0.7,
            true,
            vec![
                ("leg_r", tr(&[(0.0, [32.0, 0.0, 0.0]), (0.35, [-32.0, 0.0, 0.0])], &[])),
                ("leg_l", tr(&[(0.0, [-32.0, 0.0, 0.0]), (0.35, [32.0, 0.0, 0.0])], &[])),
                ("arm_r", tr(&[(0.0, [-28.0, 0.0, 0.0]), (0.35, [28.0, 0.0, 0.0])], &[])),
                ("arm_l", tr(&[(0.0, [28.0, 0.0, 0.0]), (0.35, [-28.0, 0.0, 0.0])], &[])),
                (
                    "body",
                    tr(
                        &[(0.0, [0.0, 4.0, 0.0]), (0.35, [0.0, -4.0, 0.0])],
                        &[(0.0, [0.0, 0.0, 0.0]), (0.175, [0.0, 0.8, 0.0]), (0.35, [0.0, 0.0, 0.0]), (0.525, [0.0, 0.8, 0.0])],
                    ),
                ),
                ("head", tr(&[(0.0, [0.0, -3.0, 0.0]), (0.35, [0.0, 3.0, 0.0])], &[])),
            ],
        ),
    );
    anims.insert(
        "wave".to_string(),
        clip(
            1.6,
            false,
            vec![
                ("arm_r", tr(&[(0.0, [0.0, 0.0, 0.0]), (0.25, [0.0, 0.0, -95.0]), (1.3, [0.0, 0.0, -95.0]), (1.6, [0.0, 0.0, 0.0])], &[])),
                (
                    "forearm_r",
                    tr(
                        &[
                            (0.0, [0.0, 0.0, 0.0]),
                            (0.25, [0.0, 0.0, -70.0]),
                            (0.4, [0.0, 0.0, -105.0]),
                            (0.55, [0.0, 0.0, -65.0]),
                            (0.7, [0.0, 0.0, -105.0]),
                            (0.85, [0.0, 0.0, -65.0]),
                            (1.0, [0.0, 0.0, -105.0]),
                            (1.15, [0.0, 0.0, -65.0]),
                            (1.3, [0.0, 0.0, -85.0]),
                            (1.6, [0.0, 0.0, 0.0]),
                        ],
                        &[],
                    ),
                ),
                ("head", tr(&[(0.0, [0.0, 0.0, 0.0]), (0.3, [0.0, 0.0, -6.0]), (1.3, [0.0, 0.0, -6.0]), (1.6, [0.0, 0.0, 0.0])], &[])),
            ],
        ),
    );
    anims.insert(
        "cheer".to_string(),
        clip(
            0.9,
            true,
            vec![
                ("arm_r", tr(&[(0.0, [0.0, 0.0, -150.0]), (0.45, [0.0, 0.0, -172.0])], &[])),
                ("arm_l", tr(&[(0.0, [0.0, 0.0, 150.0]), (0.45, [0.0, 0.0, 172.0])], &[])),
                (
                    "body",
                    tr(&[], &[(0.0, [0.0, 0.0, 0.0]), (0.225, [0.0, 2.5, 0.0]), (0.45, [0.0, 0.0, 0.0]), (0.675, [0.0, 2.5, 0.0])]),
                ),
                ("leg_r", tr(&[(0.0, [0.0, 0.0, 0.0]), (0.225, [-12.0, 0.0, 0.0]), (0.45, [0.0, 0.0, 0.0])], &[])),
                ("leg_l", tr(&[(0.0, [0.0, 0.0, 0.0]), (0.225, [-12.0, 0.0, 0.0]), (0.45, [0.0, 0.0, 0.0])], &[])),
            ],
        ),
    );
    anims.insert(
        "talk".to_string(),
        clip(
            1.2,
            true,
            vec![
                ("head", tr(&[(0.0, [0.0, 0.0, 0.0]), (0.3, [6.0, 0.0, 0.0]), (0.6, [0.0, 0.0, 0.0]), (0.9, [4.0, 0.0, 0.0])], &[])),
                ("arm_r", tr(&[(0.0, [-30.0, 0.0, 0.0]), (0.6, [-48.0, 0.0, 0.0])], &[])),
                ("forearm_r", tr(&[(0.0, [-20.0, 0.0, 0.0]), (0.6, [-40.0, 0.0, 0.0])], &[])),
            ],
        ),
    );
    AssetDef {
        name: "villager".into(),
        voxel: 0.1,
        palette: pal(&[
            ("s", "#f1c27d"),
            ("h", "#3b2a20"),
            ("b", "#e0584a:fabric"),
            ("p", "#35507a:fabric"),
            ("o", "#2b2b33"),
            ("e", "#1b1b22"),
            ("m", "#b5564b"),
            ("k", "#4a3426"),
        ]),
        parts,
        anims,
    }
}

pub fn chicken() -> AssetDef {
    let parts = vec![
        part("body", None, [0.0, 3.0, 0.0], vec![bx([-2, 3, -3], [4, 4, 6], "w"), bx([-1, 5, -4], [2, 3, 1], "w"), bx([-1, 7, -5], [2, 1, 1], "d")]),
        part(
            "head",
            Some("body"),
            [0.0, 6.0, 2.0],
            vec![
                bx([-1, 6, 1], [2, 3, 3], "w"),
                bx([0, 9, 1], [1, 1, 3], "r"),
                bx([-1, 7, 4], [2, 1, 1], "y"),
                bx([0, 6, 4], [1, 1, 1], "r"),
                bx([-1, 8, 3], [1, 1, 1], "k"),
                bx([0, 8, 3], [1, 1, 1], "k"),
            ],
        ),
        part("wing_r", Some("body"), [-2.0, 6.0, 0.0], vec![bx([-3, 3, -2], [1, 3, 3], "d")]),
        part("wing_l", Some("body"), [2.0, 6.0, 0.0], vec![bx([2, 3, -2], [1, 3, 3], "d")]),
        part("leg_r", Some("body"), [-1.0, 3.0, 0.0], vec![bx([-1, 0, 0], [1, 3, 1], "y"), bx([-1, 0, 1], [1, 1, 1], "y")]),
        part("leg_l", Some("body"), [1.0, 3.0, 0.0], vec![bx([0, 0, 0], [1, 3, 1], "y"), bx([0, 0, 1], [1, 1, 1], "y")]),
    ];
    let mut anims = BTreeMap::new();
    anims.insert(
        "idle".to_string(),
        clip(
            3.0,
            true,
            vec![(
                "head",
                tr(
                    &[(0.0, [0.0, 0.0, 0.0]), (1.3, [0.0, 0.0, 0.0]), (1.5, [55.0, 0.0, 0.0]), (1.7, [0.0, 0.0, 0.0]), (1.9, [55.0, 0.0, 0.0]), (2.1, [0.0, 0.0, 0.0])],
                    &[],
                ),
            )],
        ),
    );
    anims.insert(
        "walk".to_string(),
        clip(
            0.5,
            true,
            vec![
                ("leg_r", tr(&[(0.0, [38.0, 0.0, 0.0]), (0.25, [-38.0, 0.0, 0.0])], &[])),
                ("leg_l", tr(&[(0.0, [-38.0, 0.0, 0.0]), (0.25, [38.0, 0.0, 0.0])], &[])),
                ("body", tr(&[], &[(0.0, [0.0, 0.0, 0.0]), (0.125, [0.0, 0.6, 0.0]), (0.25, [0.0, 0.0, 0.0]), (0.375, [0.0, 0.6, 0.0])])),
                ("head", tr(&[], &[(0.0, [0.0, 0.0, 0.8]), (0.25, [0.0, 0.0, -0.4])])),
            ],
        ),
    );
    AssetDef {
        name: "chicken".into(),
        voxel: 0.07,
        palette: pal(&[("w", "#f4efe6"), ("d", "#d8cdb8"), ("r", "#d9382c"), ("y", "#f0b429"), ("k", "#1b1b22")]),
        parts,
        anims,
    }
}

pub fn house() -> AssetDef {
    let mut b = vec![];
    for (x, z) in [(-8, -6), (7, -6), (-8, 7), (7, 7), (-1, -6), (-1, 7)] {
        b.push(bx([x, 0, z], [1, 2, 1], "st"));
    }
    b.push(bx([-8, 2, -6], [16, 1, 14], "fl"));
    for y in 3..11 {
        b.push(bx([-7, y, -5], [14, 1, 10], if y % 2 == 0 { "w1" } else { "w2" }));
    }
    for (x, z) in [(-7, -5), (6, -5), (-7, 4), (6, 4)] {
        b.push(bx([x, 3, z], [1, 8, 1], "wp"));
    }
    for x in [-8, 7] {
        b.push(bx([x, 3, 7], [1, 8, 1], "wp"));
    }
    b.push(Op::Clear { at: [-2, 3, 4], size: [4, 7, 1] });
    b.push(bx([-6, 6, 4], [3, 3, 1], "gl"));
    b.push(bx([3, 6, 4], [3, 3, 1], "gl"));
    b.push(bx([6, 6, -2], [1, 3, 3], "gl"));
    b.push(bx([-7, 6, -2], [1, 3, 3], "gl"));
    for k in 0..7 {
        let (z0, n) = (-5 + k, 10 - 2 * k);
        if n <= 0 {
            break;
        }
        b.push(bx([-7, 11 + k, z0], [1, 1, n], "w1"));
        b.push(bx([6, 11 + k, z0], [1, 1, n], "w1"));
    }
    for k in 0..8 {
        let (z0, z1) = (-7 + k, 8 - k);
        if z1 - z0 <= 0 {
            break;
        }
        b.push(bx([-9, 11 + k, z0], [18, 1, z1 - z0], if k % 2 == 0 { "rf1" } else { "rf2" }));
    }
    b.push(bx([-3, 1, 8], [6, 2, 1], "sp"));
    b.push(bx([-3, 0, 9], [6, 2, 1], "sp"));
    b.push(bx([-3, 0, 10], [6, 1, 1], "sp"));
    let parts = vec![
        part("body", None, [0.0; 3], b),
        part("door", Some("body"), [-2.0, 3.0, 4.5], vec![bx([-2, 3, 4], [4, 7, 1], "dr"), bx([1, 5, 5], [1, 1, 1], "kn")]),
    ];
    let mut anims = BTreeMap::new();
    anims.insert(
        "open".to_string(),
        clip(
            3.0,
            false,
            vec![("door", tr(&[(0.0, [0.0, 0.0, 0.0]), (0.5, [0.0, -100.0, 0.0]), (2.4, [0.0, -100.0, 0.0]), (3.0, [0.0, 0.0, 0.0])], &[]))],
        ),
    );
    AssetDef {
        name: "house".into(),
        voxel: 0.25,
        palette: pal(&[
            ("st", "#6b4423:wood"),
            ("fl", "#8b5e34:wood"),
            ("w1", "#b9824f:wood"),
            ("w2", "#a9743f:wood"),
            ("wp", "#7a4f2c:wood"),
            ("gl", "#ffd27a*"),
            ("rf1", "#c4553a:brick"),
            ("rf2", "#b34a31:brick"),
            ("sp", "#9a7a56:stone"),
            ("dr", "#5a3a22:wood"),
            ("kn", "#f0c040:metal"),
        ]),
        parts,
        anims,
    }
}

pub fn palm() -> AssetDef {
    let mut trunk = vec![];
    for y in 0..14 {
        let dx = ((y as f32 / 14.0).powi(2) * 3.0).round() as i32;
        trunk.push(bx([dx - 1, y, -1], [2, 1, 2], if y % 2 == 0 { "t1" } else { "t2" }));
    }
    let mut crown = vec![bx([2, 14, -1], [2, 1, 2], "l1"), Op::Sphere { at: [3.5, 15.0, 0.5], r: 1.8, c: "l1".into() }];
    for i in 0..7 {
        let a = i as f32 * TAU / 7.0;
        let (c, s) = (a.cos(), a.sin());
        let r = |k: f32| [3 + (c * k).round() as i32, 0, (s * k).round() as i32];
        let (p1, p2) = (r(4.0), r(8.0));
        crown.push(Op::Line { from: [3, 15, 0], to: [p1[0], 16, p1[2]], c: "l1".into() });
        crown.push(Op::Line { from: [p1[0], 16, p1[2]], to: [p2[0], 13, p2[2]], c: "l2".into() });
        crown.push(Op::Line { from: [3, 14, 0], to: [p1[0], 15, p1[2]], c: "l2".into() });
        crown.push(Op::Line { from: [p1[0], 15, p1[2]], to: [p2[0], 12, p2[2]], c: "l1".into() });
    }
    crown.push(bx([2, 13, -1], [2, 1, 1], "co"));
    crown.push(bx([4, 13, 0], [1, 1, 1], "co"));
    let mut anims = BTreeMap::new();
    anims.insert(
        "idle".to_string(),
        clip(4.0, true, vec![("crown", tr(&[(0.0, [1.0, 0.0, 1.5]), (2.0, [-1.0, 0.0, -1.5])], &[]))]),
    );
    AssetDef {
        name: "palm".into(),
        voxel: 0.25,
        palette: pal(&[("t1", "#8a6a45:wood"), ("t2", "#745634:wood"), ("l1", "#3f9a45:grass"), ("l2", "#2f7d3a:grass"), ("co", "#5a3b1e")]),
        parts: vec![part("trunk", None, [0.0; 3], trunk), part("crown", Some("trunk"), [3.0, 14.0, 0.0], crown)],
        anims,
    }
}

pub fn tree() -> AssetDef {
    let crown = vec![
        Op::Sphere { at: [0.0, 9.5, 0.0], r: 5.2, c: "l1".into() },
        Op::Sphere { at: [3.5, 8.5, 1.5], r: 3.2, c: "l2".into() },
        Op::Sphere { at: [-3.5, 8.5, -1.0], r: 3.4, c: "l2".into() },
        Op::Sphere { at: [0.5, 12.5, 0.0], r: 3.4, c: "l3".into() },
        bx([-4, 9, 3], [1, 1, 1], "fr"),
        bx([3, 12, 3], [1, 1, 1], "fr"),
        bx([4, 8, -2], [1, 1, 1], "fr"),
        bx([-2, 7, 4], [1, 1, 1], "fr"),
    ];
    let mut anims = BTreeMap::new();
    anims.insert(
        "idle".to_string(),
        clip(5.0, true, vec![("crown", tr(&[(0.0, [0.8, 0.0, 1.0]), (2.5, [-0.8, 0.0, -1.0])], &[]))]),
    );
    AssetDef {
        name: "tree".into(),
        voxel: 0.25,
        palette: pal(&[("t1", "#7a5230:wood"), ("t2", "#664326:wood"), ("l1", "#4a9b3f:grass"), ("l2", "#3b8535:grass"), ("l3", "#5fb34b:grass"), ("fr", "#e0483a")]),
        parts: vec![
            part("trunk", None, [0.0; 3], vec![bx([-1, 0, -1], [2, 6, 2], "t1"), bx([-2, 0, -2], [4, 1, 4], "t2")]),
            part("crown", Some("trunk"), [0.0, 6.0, 0.0], crown),
        ],
        anims,
    }
}

pub fn warung() -> AssetDef {
    let mut b = vec![];
    for (x, z) in [(-6, -4), (5, -4), (-6, 3), (5, 3)] {
        b.push(bx([x, 0, z], [1, 9, 1], "wp"));
    }
    b.push(bx([-5, 0, -4], [10, 8, 1], "w2"));
    b.push(bx([-5, 0, 2], [10, 4, 2], "ct"));
    b.push(bx([-5, 4, 2], [10, 1, 3], "pl"));
    for (i, c) in ["f1", "f2", "f3", "f1", "f2", "f3", "f1"].iter().enumerate() {
        b.push(bx([-4 + i as i32 * 1 + (i as i32 / 2), 5, 3], [1, 1, 1], c));
    }
    b.push(bx([-4, 1, -3], [8, 1, 1], "pl"));
    b.push(bx([-4, 4, -3], [8, 1, 1], "pl"));
    for (z0, y) in [(-5, 10), (-1, 9), (3, 8)] {
        for s in 0..7 {
            b.push(bx([-7 + s * 2, y, z0], [2, 1, 4], if s % 2 == 0 { "a1" } else { "a2" }));
        }
    }
    b.push(bx([0, 7, 2], [1, 1, 1], "gl"));
    AssetDef {
        name: "warung".into(),
        voxel: 0.25,
        palette: pal(&[
            ("wp", "#6b4423:wood"),
            ("pl", "#a67c4e:wood"),
            ("w2", "#c9a46a:wood"),
            ("ct", "#8a5a34:wood"),
            ("a1", "#d9402f:fabric"),
            ("a2", "#f6efe0:fabric"),
            ("gl", "#ffd27a*"),
            ("f1", "#e04a3a"),
            ("f2", "#f0b429"),
            ("f3", "#62b94a"),
        ]),
        parts: vec![part("body", None, [0.0; 3], b)],
        anims: BTreeMap::new(),
    }
}

pub fn lamp() -> AssetDef {
    AssetDef {
        name: "lamp".into(),
        voxel: 0.2,
        palette: pal(&[("d", "#2c2c38:metal"), ("g", "#ffe39a*")]),
        parts: vec![part(
            "body",
            None,
            [0.0; 3],
            vec![
                bx([-1, 0, -1], [3, 1, 3], "d"),
                bx([0, 1, 0], [1, 11, 1], "d"),
                bx([0, 12, 0], [3, 1, 1], "d"),
                bx([2, 10, -1], [3, 2, 3], "g"),
                bx([2, 12, -1], [3, 1, 3], "d"),
            ],
        )],
        anims: BTreeMap::new(),
    }
}

pub fn well() -> AssetDef {
    let mut b = vec![];
    for y in 0..5 {
        b.push(bx([-4, y, -4], [8, 1, 8], if y % 2 == 0 { "st" } else { "st2" }));
    }
    b.push(Op::Clear { at: [-3, 1, -3], size: [6, 4, 6] });
    b.push(bx([-3, 1, -3], [6, 1, 6], "wa"));
    b.push(bx([-4, 5, -1], [1, 9, 2], "wd"));
    b.push(bx([3, 5, -1], [1, 9, 2], "wd"));
    b.push(bx([-5, 14, -2], [10, 1, 4], "wd"));
    b.push(bx([-6, 15, -3], [12, 1, 6], "rf"));
    b.push(bx([-5, 16, -2], [10, 1, 4], "rf"));
    b.push(bx([-4, 17, -1], [8, 1, 2], "rf"));
    let mut anims = BTreeMap::new();
    anims.insert(
        "idle".to_string(),
        clip(3.0, true, vec![("bucket", tr(&[(0.0, [4.0, 0.0, 0.0]), (1.5, [-4.0, 0.0, 0.0])], &[]))]),
    );
    AssetDef {
        name: "well".into(),
        voxel: 0.2,
        palette: pal(&[("st", "#8d8d96:stone"), ("st2", "#7a7a84:stone"), ("wa", "#3d95dc"), ("wd", "#7a4f2c:wood"), ("rf", "#b34a31:brick"), ("rp", "#cdb88a:fabric"), ("bk", "#6b4423:wood")]),
        parts: vec![
            part("body", None, [0.0; 3], b),
            part("bucket", Some("body"), [0.0, 14.0, 0.0], vec![bx([0, 8, 0], [1, 6, 1], "rp"), bx([-1, 6, -1], [3, 2, 3], "bk")]),
        ],
        anims,
    }
}

// ---------------------------------------------------------------- kampung scene

const N: usize = 26;

fn land_metric(i: usize, j: usize) -> f32 {
    let dx = ((i as f32 + 0.5) - 13.0) / 11.8;
    let dz = ((j as f32 + 0.5) - 13.0) / 11.8;
    dx.abs().powi(3) + dz.abs().powi(3) + 0.05 * hash3(i as i32, j as i32, 9)
}

fn kampung_map() -> Vec<Vec<char>> {
    let mut m = vec![vec!['w'; N]; N];
    for j in 0..N {
        for i in 0..N {
            let d = land_metric(i, j);
            let (cx, cz) = (i as f32 + 0.5, j as f32 + 0.5);
            let plaza = ((cx - 13.0).powi(2) + (cz - 13.0).powi(2)).sqrt() < 3.4;
            let road = ((12..=13).contains(&i) && (3..=23).contains(&j)) || ((12..=13).contains(&j) && (3..=23).contains(&i));
            let hd = ((cx - 5.5).powi(2) + (cz - 20.5).powi(2)).sqrt();
            m[j][i] = if d >= 1.0 {
                'w'
            } else if d > 0.80 {
                's'
            } else if plaza {
                'p'
            } else if road {
                'd'
            } else if hd < 3.4 {
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
    // short dirt paths from the roads to the front doors
    for (x, z0, z1) in [(7usize, 9usize, 11usize), (19, 9, 11), (19, 14, 17)] {
        for z in z0..=z1 {
            if matches!(m[z][x], 'g' | 'G') {
                m[z][x] = 'd';
            }
        }
    }
    m
}

fn kampung_terrain(map: &[Vec<char>]) -> TerrainDef {
    let t = |h: i32, top: &str, side: Option<&str>, jitter: f32, water: bool| TileDef { h, top: top.into(), side: side.map(Into::into), jitter, water };
    let mut legend = BTreeMap::new();
    legend.insert("g".to_string(), t(2, "#78b856:grass", Some("#8a6040:dirt"), 0.04, false));
    legend.insert("G".to_string(), t(2, "#6aae4c:grass", Some("#8a6040:dirt"), 0.05, false));
    legend.insert("h".to_string(), t(3, "#6aae4c:grass", Some("#8a6040:dirt"), 0.05, false));
    legend.insert("H".to_string(), t(4, "#5a9e42:grass", Some("#8a6040:dirt"), 0.05, false));
    legend.insert("s".to_string(), t(2, "#e6d29b:sand", Some("#cdb27a:sand"), 0.05, false));
    legend.insert("d".to_string(), t(2, "#b98a58:dirt", Some("#8a6040:dirt"), 0.06, false));
    legend.insert("p".to_string(), t(2, "#cdbfa5:stone", Some("#a8987a:stone"), 0.04, false));
    legend.insert("w".to_string(), t(1, "#3d95dc", Some("#8a6040"), 0.03, true));
    TerrainDef { cell: 1.0, step: 0.5, legend, map: map.iter().map(|r| r.iter().collect()).collect() }
}

pub(crate) fn ent(id: &str, asset: &str, pos: [f32; 2], yaw: f32) -> EntityDef {
    EntityDef {
        id: id.into(),
        asset: asset.into(),
        pos,
        yaw,
        scale: 1.0,
        palette: BTreeMap::new(),
        anims: BTreeMap::new(),
        footprint: [0.0, 0.0],
        label: None,
        base_anim: None,
        elevation: 0.0,
        building: None,
        floor: 0,
    }
}

pub(crate) fn person(id: &str, label: &str, pos: [f32; 2], yaw: f32, shirt: &str, hair: &str, scale: f32) -> EntityDef {
    let mut e = ent(id, "villager", pos, yaw);
    e.label = Some(label.into());
    e.scale = scale;
    e.palette = pal(&[("b", shirt), ("h", hair)]);
    e
}

fn kampung_entities(map: &[Vec<char>]) -> Vec<EntityDef> {
    let mut v: Vec<EntityDef> = vec![];
    let solid = |id: &str, asset: &str, pos: [f32; 2], yaw: f32, fp: [f32; 2]| {
        let mut e = ent(id, asset, pos, yaw);
        e.footprint = fp;
        e
    };
    v.push(solid("sumur", "well", [13.0, 13.0], 0.0, [1.9, 1.9]));
    v.push(solid("rumah_a", "house", [7.5, 6.5], 0.0, [4.4, 3.6]));
    v.push(solid("rumah_b", "house", [19.5, 6.5], 0.0, [4.4, 3.6]));
    v.push(solid("rumah_c", "house", [19.5, 20.0], 180.0, [4.4, 3.6]));
    v.push(solid("rumah_d", "house", [5.5, 20.5], 180.0, [4.4, 3.6]));
    v.push(solid("warung", "warung", [17.0, 17.2], 180.0, [3.6, 2.6]));
    for (i, p) in [[11.0, 11.2], [15.0, 11.2], [11.0, 14.8], [15.0, 14.8]].iter().enumerate() {
        v.push(ent(&format!("lampu{}", i + 1), "lamp", *p, 0.0));
    }

    // palms on the beach, trees on grass, placed deterministically but naturally
    let near = |v: &[EntityDef], p: [f32; 2], d: f32| v.iter().any(|e| ((e.pos[0] - p[0]).powi(2) + (e.pos[1] - p[1]).powi(2)).sqrt() < d);
    let (mut palms, mut trees) = (0, 0);
    for j in 1..N - 1 {
        for i in 1..N - 1 {
            let p = [i as f32 + 0.5 + 0.3 * hash3(i as i32, j as i32, 1), j as f32 + 0.5 + 0.3 * hash3(i as i32, j as i32, 2)];
            let r = hash3(i as i32, j as i32, 5);
            match map[j][i] {
                's' if palms < 9 && r > 0.55 && !near(&v, p, 4.5) => {
                    palms += 1;
                    let mut e = ent(&format!("palem{palms}"), "palm", p, 360.0 * (r + 1.0) / 2.0);
                    e.footprint = [0.6, 0.6];
                    v.push(e);
                }
                'g' | 'G' if trees < 9 && r > 0.62 && !near(&v, p, 4.2) => {
                    let ring_ok = (-2i32..=2).all(|dj| (-2i32..=2).all(|di| !matches!(map[(j as i32 + dj) as usize][(i as i32 + di) as usize], 'd' | 'p')));
                    if ring_ok {
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
    v.push(person("ani", "@ani", [7.5, 10.2], 0.0, "#e0584a", "#3b2a20", 1.0));
    v.push(person("budi", "@budi", [17.0, 14.8], 90.0, "#3c8fd9", "#7a4a1e", 1.0));
    v.push(person("citra", "@citra", [19.5, 10.6], 200.0, "#f0b429", "#201510", 1.0));
    v.push(person("dodi", "@dodi", [6.5, 16.0], 120.0, "#4caf50", "#2a1a10", 0.8));
    v.push(ent("ayam1", "chicken", [10.5, 17.5], 40.0));
    v.push(ent("ayam2", "chicken", [15.0, 19.5], 250.0));
    v
}

fn kampung_story() -> Result<StoryDef> {
    let scenes: Vec<Value> = vec![
        json!({"name": "pagi", "steps": [
            {"do": "par", "steps": [
                {"do": "camera", "op": "move", "yaw": 62, "zoom": 31, "secs": 7.5},
                {"do": "seq", "steps": [{"do": "wait", "secs": 0.8}, {"do": "say", "actor": "ani", "text": "SELAMAT PAGI KAMPUNG!", "secs": 2.4}]},
                {"do": "seq", "steps": [{"do": "wait", "secs": 0.8}, {"do": "play", "actor": "ani", "anim": "wave"}]},
                {"do": "seq", "steps": [{"do": "wait", "secs": 3.4}, {"do": "play", "actor": "budi", "anim": "wave"}]},
                {"do": "seq", "steps": [{"do": "play", "actor": "rumah_a", "anim": "open"}]},
                {"do": "seq", "steps": [
                    {"do": "walk_to", "actor": "ayam1", "to": [12.5, 18.8], "speed": 1.0},
                    {"do": "wait", "secs": 1.0},
                    {"do": "walk_to", "actor": "ayam1", "to": [9.5, 18.3], "speed": 1.0}]},
                {"do": "seq", "steps": [
                    {"do": "wait", "secs": 2.0},
                    {"do": "walk_to", "actor": "ayam2", "to": [13.8, 20.5], "speed": 1.0}]}
            ]}
        ]}),
        json!({"name": "menyapa", "steps": [
            {"do": "camera", "op": "move", "target": [7.5, 1.2, 10.0], "zoom": 15, "pitch": 33, "yaw": 125, "secs": 2.2},
            {"do": "camera", "op": "follow", "actor": "ani", "smooth": 4},
            {"do": "walk_to", "actor": "ani", "to": "budi", "stop": 1.4, "speed": 2.3},
            {"do": "par", "steps": [
                {"do": "play", "actor": "ani", "anim": "wave", "times": 2},
                {"do": "say", "actor": "ani", "text": "HALO BUDI!", "secs": 2.2},
                {"do": "seq", "steps": [
                    {"do": "wait", "secs": 0.9},
                    {"do": "face", "actor": "budi", "to": "ani"},
                    {"do": "par", "steps": [
                        {"do": "say", "actor": "budi", "text": "HALO ANI! MAU BELI APA?", "secs": 2.6},
                        {"do": "play", "actor": "budi", "anim": "talk", "secs": 2.6}]}]}
            ]},
            {"do": "wait", "secs": 0.4}
        ]}),
        json!({"name": "kumpul", "steps": [
            {"do": "camera", "op": "unfollow"},
            {"do": "camera", "op": "move", "target": [13, 1, 13], "zoom": 22, "yaw": 20, "pitch": 34, "secs": 2.5},
            {"do": "par", "steps": [
                {"do": "walk_to", "actor": "ani", "to": [10.9, 13.6], "speed": 2.4},
                {"do": "walk_to", "actor": "budi", "to": [15.1, 13.6], "speed": 2.4},
                {"do": "walk_to", "actor": "citra", "to": [13.0, 10.8], "speed": 2.4},
                {"do": "walk_to", "actor": "dodi", "to": [13.6, 15.3], "speed": 2.4},
                {"do": "seq", "steps": [{"do": "wait", "secs": 3.0}, {"do": "say", "actor": "citra", "text": "AKU IKUT YA!", "secs": 2.0}]}
            ]},
            {"do": "par", "steps": [
                {"do": "face", "actor": "ani", "to": "sumur"},
                {"do": "face", "actor": "budi", "to": "sumur"},
                {"do": "face", "actor": "citra", "to": "sumur"},
                {"do": "face", "actor": "dodi", "to": "sumur"}]},
            {"do": "par", "steps": [
                {"do": "play", "actor": "ani", "anim": "cheer", "times": 3},
                {"do": "play", "actor": "budi", "anim": "cheer", "times": 3},
                {"do": "play", "actor": "citra", "anim": "cheer", "times": 3},
                {"do": "play", "actor": "dodi", "anim": "cheer", "times": 3},
                {"do": "camera", "op": "orbit", "dyaw": 110, "secs": 3.5},
                {"do": "say", "actor": "dodi", "text": "HORE!", "secs": 2.0}]}
        ]}),
        json!({"name": "sore", "steps": [
            {"do": "set_sky", "preset": "sunset", "secs": 3.5},
            {"do": "camera", "op": "move", "yaw": 150, "pitch": 26, "zoom": 27, "secs": 4.0},
            {"do": "say", "actor": "ani", "text": "YUK PULANG, SUDAH SORE!", "secs": 2.2},
            {"do": "set_sky", "preset": "night", "secs": 3.0},
            {"do": "camera", "op": "move", "yaw": 215, "pitch": 30, "zoom": 33, "secs": 3.5},
            {"do": "wait", "secs": 1.5}
        ]}),
    ];
    Ok(StoryDef { looping: true, reset_on_loop: true, scenes: serde_json::from_value::<Vec<StageDef>>(Value::Array(scenes))? })
}

pub fn kampung_assets() -> Vec<(&'static str, AssetDef)> {
    vec![
        ("villager", villager()),
        ("chicken", chicken()),
        ("house", house()),
        ("palm", palm()),
        ("tree", tree()),
        ("warung", warung()),
        ("lamp", lamp()),
        ("well", well()),
    ]
}

/// Same scene with every asset embedded, so it needs no files (used by the live app's built-in demo).
pub fn kampung_scene_inline() -> Result<SceneDef> {
    let mut scene = kampung_scene()?;
    scene.assets = kampung_assets().into_iter().map(|(n, d)| (n.to_string(), AssetRef::Inline(Box::new(d)))).collect();
    Ok(scene)
}

pub fn kampung_scene() -> Result<SceneDef> {
    let map = kampung_map();
    let mut markers = BTreeMap::new();
    markers.insert("plaza".to_string(), Target::Pos([13.0, 13.0]));
    markers.insert("warung_depan".to_string(), Target::Pos([17.0, 14.8]));
    markers.insert("pintu_a".to_string(), Target::Pos([7.5, 10.0]));
    markers.insert("pintu_b".to_string(), Target::Pos([19.5, 10.0]));
    Ok(SceneDef {
        name: "Kampung Bahagia".into(),
        assets: kampung_assets().into_iter().map(|(n, _)| (n.to_string(), AssetRef::Path(format!("assets/{n}.json")))).collect(),
        terrain: Some(kampung_terrain(&map)),
        sky: SkyDef { preset: "day".into(), ..Default::default() },
        markers,
        buildings: vec![],
        entities: kampung_entities(&map),
        story: Some(kampung_story()?),
        camera: CamParams { target: Some([13.0, 1.0, 13.0]), yaw: Some(45.0), pitch: Some(30.0), ..Default::default() },
    })
}

// ---------------------------------------------------------------- JSON output

fn strip_nulls(v: &mut Value) {
    match v {
        Value::Object(m) => {
            m.retain(|_, x| !x.is_null());
            m.values_mut().for_each(strip_nulls);
        }
        Value::Array(a) => a.iter_mut().for_each(strip_nulls),
        _ => {}
    }
}

fn fmt(v: &Value, ind: usize, out: &mut String) {
    let compact = serde_json::to_string(v).unwrap();
    if compact.len() + ind * 2 <= 110 || !matches!(v, Value::Array(_) | Value::Object(_)) {
        out.push_str(&compact);
        return;
    }
    let pad = "  ".repeat(ind + 1);
    match v {
        Value::Array(a) => {
            out.push_str("[\n");
            for (i, x) in a.iter().enumerate() {
                out.push_str(&pad);
                fmt(x, ind + 1, out);
                out.push_str(if i + 1 < a.len() { ",\n" } else { "\n" });
            }
            out.push_str(&"  ".repeat(ind));
            out.push(']');
        }
        Value::Object(m) => {
            out.push_str("{\n");
            let n = m.len();
            for (i, (k, x)) in m.iter().enumerate() {
                out.push_str(&pad);
                out.push_str(&serde_json::to_string(k).unwrap());
                out.push_str(": ");
                fmt(x, ind + 1, out);
                out.push_str(if i + 1 < n { ",\n" } else { "\n" });
            }
            out.push_str(&"  ".repeat(ind));
            out.push('}');
        }
        _ => unreachable!(),
    }
}

/// Pretty JSON that keeps short arrays/objects on one line (voxel ops, keyframes) and drops nulls.
pub fn to_pretty_json<T: serde::Serialize>(x: &T) -> Result<String> {
    let mut v = serde_json::to_value(x)?;
    strip_nulls(&mut v);
    let mut s = String::new();
    fmt(&v, 0, &mut s);
    s.push('\n');
    Ok(s)
}

pub fn write_kampung(dir: &Path) -> Result<()> {
    fs::create_dir_all(dir.join("assets"))?;
    for (name, def) in kampung_assets() {
        fs::write(dir.join("assets").join(format!("{name}.json")), to_pretty_json(&def)?)?;
    }
    fs::write(dir.join("scene.json"), to_pretty_json(&kampung_scene()?)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_template_assets_bake() {
        for (name, def) in kampung_assets() {
            let m = Model::bake(&def).unwrap_or_else(|e| panic!("{name}: {e:#}"));
            assert!(m.voxel_count() > 20, "{name}");
        }
    }

    #[test]
    fn scene_roundtrips_through_json() {
        let json = to_pretty_json(&kampung_scene().unwrap()).unwrap();
        let back: SceneDef = serde_json::from_str(&json).unwrap();
        assert_eq!(back.entities.len(), kampung_scene().unwrap().entities.len());
        assert!(back.story.is_some());
    }
}
