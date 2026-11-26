//! The "demo" template: a parody of a street demonstration. Citizens (RAKYAT) rally on a plaza with a
//! stage, the government (PEMERINTAH) holds its building across the road. Nobody is hurt: a beaten
//! person just faints. Viewer gifts become protest tactics and government counter-measures: speeches that
//! raise morale, food packs that heal, barricades, molotov bottles, water cannon and tear gas, foreign aid
//! or corruption that heals the government, sabotage by intelligence agents, bribes (rice packs) that make
//! protesters change sides. Endless mode, day and night biomes.

use std::collections::BTreeMap;
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
const STAGE: [f32; 2] = [8.5, 15.0];
const GOV: [f32; 2] = [43.5, 15.0];

fn none() -> BTreeMap<String, ClipDef> {
    BTreeMap::new()
}

// ---------------------------------------------------------------- people

/// role: "warga" (placard), "mahasiswa" (cap and backpack), "polisi" (helmet, riot shield, baton)
fn person(name: &str, role: &str) -> AssetDef {
    let police = role == "polisi";
    let student = role == "mahasiswa";
    let mut body = vec![bx([-4, 5, -2], [8, 6, 4], "cloth"), bx([-4, 5, -2], [8, 1, 4], "belt")];
    let mut head = vec![bx([-3, 11, -3], [6, 6, 6], "skin"), bx([-2, 13, 3], [1, 1, 1], "eye"), bx([1, 13, 3], [1, 1, 1], "eye"), bx([-1, 12, 3], [2, 1, 1], "mouth")];
    if police {
        body.push(bx([-4, 7, 2], [8, 3, 1], "vest"));
        body.push(bx([2, 9, 2], [1, 1, 1], "trim"));
        head.push(bx([-3, 14, -3], [6, 3, 6], "helm"));
        head.push(bx([-3, 13, 3], [6, 1, 1], "helm"));
        head.push(bx([-3, 11, -3], [6, 4, 1], "helm"));
    } else if student {
        body.push(bx([-3, 5, -4], [6, 6, 2], "bag"));
        head.push(bx([-3, 15, -3], [6, 2, 6], "cap"));
        head.push(bx([-3, 14, 3], [6, 1, 2], "cap"));
        head.push(bx([-3, 11, -3], [6, 4, 1], "hair"));
    } else {
        head.push(bx([-3, 15, -3], [6, 2, 6], "hair"));
        head.push(bx([-3, 11, -3], [6, 4, 1], "hair"));
        head.push(bx([-3, 12, -3], [1, 3, 3], "hair"));
        head.push(bx([2, 12, -3], [1, 3, 3], "hair"));
    }
    let mut forearm_r = vec![bx([-6, 5, -1], [2, 3, 2], "skin")];
    let mut forearm_l = vec![bx([4, 5, -1], [2, 3, 2], "skin")];
    if police {
        forearm_r.push(bx([-6, 5, 1], [1, 1, 7], "baton"));
        forearm_l.push(bx([6, 2, -3], [1, 11, 7], "shield"));
        forearm_l.push(bx([6, 7, -3], [1, 1, 7], "trim"));
    } else {
        // the placard: a stick, a board and a few scribbles that read as slogans
        forearm_r.push(bx([-7, 6, 0], [1, 12, 1], "stick"));
        forearm_r.push(bx([-12, 15, 0], [10, 7, 1], "sign"));
        forearm_r.push(bx([-11, 20, 1], [8, 1, 1], "mark"));
        forearm_r.push(bx([-11, 18, 1], [6, 1, 1], "mark"));
        forearm_r.push(bx([-11, 16, 1], [7, 1, 1], "mark2"));
    }
    let sleeve = if police { "vest" } else { "cloth" };
    let parts = vec![
        part("body", None, [0.0, 5.0, 0.0], body),
        part("head", Some("body"), [0.0, 11.0, 0.0], head),
        part("arm_r", Some("body"), [-5.0, 10.0, 0.0], vec![bx([-6, 8, -1], [2, 3, 2], sleeve)]),
        part("forearm_r", Some("arm_r"), [-5.0, 8.0, 0.0], forearm_r),
        part("arm_l", Some("body"), [5.0, 10.0, 0.0], vec![bx([4, 8, -1], [2, 3, 2], sleeve)]),
        part("forearm_l", Some("arm_l"), [5.0, 8.0, 0.0], forearm_l),
        part("leg_r", Some("body"), [-2.5, 5.0, 0.0], vec![bx([-4, 1, -2], [3, 4, 4], "pants"), bx([-4, 0, -2], [3, 1, 5], "shoe")]),
        part("leg_l", Some("body"), [2.5, 5.0, 0.0], vec![bx([1, 1, -2], [3, 4, 4], "pants"), bx([1, 0, -2], [3, 1, 5], "shoe")]),
    ];
    let mut anims = none();
    anims.insert(
        "idle".into(),
        clip(2.2, true, vec![("head", tr(&[], &[(0.0, [0.0, 0.0, 0.0]), (1.1, [0.0, 0.5, 0.0])])), ("arm_l", tr(&[(0.0, [0.0, 0.0, 2.0]), (1.1, [0.0, 0.0, 4.0])], &[]))]),
    );
    anims.insert(
        "walk".into(),
        clip(
            0.6,
            true,
            vec![
                ("leg_r", tr(&[(0.0, [34.0, 0.0, 0.0]), (0.3, [-34.0, 0.0, 0.0])], &[])),
                ("leg_l", tr(&[(0.0, [-34.0, 0.0, 0.0]), (0.3, [34.0, 0.0, 0.0])], &[])),
                ("arm_l", tr(&[(0.0, [20.0, 0.0, 4.0]), (0.3, [-20.0, 0.0, 4.0])], &[])),
                ("arm_r", tr(&[(0.0, [-6.0, 0.0, -4.0]), (0.3, [6.0, 0.0, -4.0])], &[])),
                ("body", tr(&[], &[(0.0, [0.0, 0.0, 0.0]), (0.15, [0.0, 0.8, 0.0]), (0.3, [0.0, 0.0, 0.0]), (0.45, [0.0, 0.8, 0.0])])),
            ],
        ),
    );
    anims.insert(
        "attack".into(),
        clip(
            0.5,
            false,
            vec![
                ("arm_r", tr(&[(0.0, [0.0, 0.0, -4.0]), (0.12, [-150.0, 0.0, -4.0]), (0.26, [-28.0, 0.0, -4.0]), (0.5, [0.0, 0.0, -4.0])], &[])),
                ("body", tr(&[(0.0, [0.0, 0.0, 0.0]), (0.12, [-8.0, 0.0, 0.0]), (0.26, [14.0, 0.0, 0.0]), (0.5, [0.0, 0.0, 0.0])], &[])),
            ],
        ),
    );
    anims.insert(
        "cheer".into(),
        clip(
            0.8,
            true,
            vec![
                ("arm_r", tr(&[(0.0, [0.0, 0.0, -150.0]), (0.4, [0.0, 0.0, -172.0])], &[])),
                ("arm_l", tr(&[(0.0, [0.0, 0.0, 150.0]), (0.4, [0.0, 0.0, 172.0])], &[])),
                ("body", tr(&[], &[(0.0, [0.0, 0.0, 0.0]), (0.2, [0.0, 2.5, 0.0]), (0.4, [0.0, 0.0, 0.0]), (0.6, [0.0, 2.5, 0.0])])),
            ],
        ),
    );
    // nobody is hurt in this one: a beaten person sits down dizzy
    anims.insert(
        "faint".into(),
        clip(
            0.7,
            false,
            vec![
                ("body", tr(&[(0.0, [0.0, 0.0, 0.0]), (0.5, [0.0, 0.0, 84.0])], &[(0.0, [0.0, 0.0, 0.0]), (0.5, [0.0, -3.0, 0.0])])),
                ("arm_r", tr(&[(0.0, [0.0, 0.0, 0.0]), (0.5, [-40.0, 0.0, -40.0])], &[])),
                ("arm_l", tr(&[(0.0, [0.0, 0.0, 0.0]), (0.5, [-40.0, 0.0, 40.0])], &[])),
            ],
        ),
    );
    AssetDef {
        name: name.into(),
        voxel: 0.12,
        palette: pal(&[
            ("cloth", "#f2a31b:fabric"),
            ("belt", "#3a2f2a"),
            ("pants", "#3d4a63:fabric"),
            ("shoe", "#2b2b33"),
            ("skin", "#e8b98a"),
            ("hair", "#2a1f1a"),
            ("eye", "#1b1b22"),
            ("mouth", "#b5564b"),
            ("stick", "#8a5a2b:wood"),
            ("sign", "#fff4d6"),
            ("mark", "#d84a3a"),
            ("mark2", "#2f5fa8"),
            ("cap", "#c0392b:fabric"),
            ("bag", "#3f7a3a:fabric"),
            ("vest", "#22304a:fabric"),
            ("helm", "#1d2a42:metal"),
            ("trim", "#e8e8f0"),
            ("baton", "#16161c"),
            ("shield", "#9fc4e8"),
        ]),
        parts,
        anims: if police { anims } else { anims },
    }
}

fn barricade() -> AssetDef {
    AssetDef {
        name: "barikade".into(),
        voxel: 0.14,
        palette: pal(&[("wood", "#a9743f:wood"), ("wood2", "#8a5a2b:wood"), ("tire", "#1c1c20"), ("cone", "#ff7a1a"), ("white", "#f4f4f4"), ("tape", "#f2d21b")]),
        parts: vec![part(
            "body",
            None,
            [0.0, 0.0, 0.0],
            vec![
                bx([-8, 0, -2], [16, 2, 4], "wood"),
                bx([-7, 2, -2], [14, 2, 4], "wood2"),
                bx([-6, 4, -2], [12, 2, 4], "wood"),
                bx([-10, 0, -3], [3, 5, 6], "tire"),
                bx([7, 0, -3], [3, 5, 6], "tire"),
                bx([-1, 6, -1], [3, 3, 3], "cone"),
                bx([-1, 8, -1], [3, 1, 3], "white"),
                bx([-8, 3, 2], [16, 1, 1], "tape"),
            ],
        )],
        anims: none(),
    }
}

// ---------------------------------------------------------------- buildings

fn stage() -> AssetDef {
    AssetDef {
        name: "panggung".into(),
        voxel: 0.2,
        palette: pal(&[
            ("deck", "#8a6a4a:wood"),
            ("deck2", "#6b4f35:wood"),
            ("pole", "#3c3c44:metal"),
            ("flag", "#f2a31b:fabric"),
            ("trim", "#ffffff"),
            ("roof", "#f2a31b:tile"),
            ("speaker", "#1a1a20"),
            ("lamp", "#fff0b0*"),
        ]),
        parts: vec![part(
            "body",
            None,
            [0.0, 0.0, 0.0],
            vec![
                bx([-12, 0, -10], [24, 3, 20], "deck"),
                bx([-4, 0, 10], [8, 2, 2], "deck2"),
                bx([-4, 2, 10], [8, 1, 1], "deck2"),
                bx([-12, 3, -10], [2, 16, 2], "pole"),
                bx([10, 3, -10], [2, 16, 2], "pole"),
                bx([-10, 10, -10], [20, 8, 1], "flag"),
                bx([-10, 10, -10], [20, 1, 1], "trim"),
                bx([-10, 17, -10], [20, 1, 1], "trim"),
                bx([-6, 12, -10], [12, 2, 1], "trim"),
                bx([-12, 19, -10], [24, 1, 9], "roof"),
                bx([-11, 3, -3], [3, 6, 3], "speaker"),
                bx([8, 3, -3], [3, 6, 3], "speaker"),
                bx([-2, 3, -2], [4, 5, 3], "deck2"),
                bx([0, 3, 2], [1, 7, 1], "pole"),
                bx([0, 10, 2], [1, 1, 2], "pole"),
                bx([-6, 18, -8], [2, 1, 2], "lamp"),
                bx([4, 18, -8], [2, 1, 2], "lamp"),
            ],
        )],
        anims: none(),
    }
}

fn government() -> AssetDef {
    let mut s = vec![
        bx([-14, 0, -9], [28, 2, 18], "step"),
        bx([-12, 2, -8], [24, 1, 16], "step2"),
        bx([-11, 3, -7], [22, 14, 12], "wall"),
        bx([-12, 17, -7], [24, 2, 14], "wall2"),
    ];
    for i in 0..6 {
        s.push(bx([-10 + i * 4, 3, 5], [2, 14, 2], "col"));
    }
    for k in 0..5 {
        s.push(bx([-11 + k * 2, 19 + k * 2, 3], [22 - 4 * k, 2, 3], "roof"));
    }
    s.push(Op::Sphere { at: [0.0, 22.0, -3.0], r: 6.0, c: "dome".into() });
    s.push(bx([0, 27, -3], [1, 10, 1], "pole"));
    s.push(bx([1, 33, -3], [6, 4, 1], "flag"));
    for i in 0..5 {
        s.push(bx([-8 + i * 4, 8, 4], [2, 5, 1], "win"));
    }
    s.push(bx([-1, 3, 4], [3, 6, 1], "door"));
    AssetDef {
        name: "gedung".into(),
        voxel: 0.2,
        palette: pal(&[
            ("step", "#b9b4ab:stone"),
            ("step2", "#cfc9bd:stone"),
            ("wall", "#e9e4d8:plaster"),
            ("wall2", "#d8d2c4:plaster"),
            ("col", "#f4f1ea:stone"),
            ("roof", "#2f5fa8:tile"),
            ("dome", "#d6b24a:metal"),
            ("pole", "#6b6b73:metal"),
            ("flag", "#2f5fa8:fabric"),
            ("win", "#ffd27a*"),
            ("door", "#5b3a24:wood"),
        ]),
        parts: vec![part("body", None, [0.0, 0.0, 0.0], s)],
        anims: none(),
    }
}

// ---------------------------------------------------------------- effects

fn blob(name: &str, voxel: f32, spheres: &[([f32; 3], f32, &str)], palette: &[(&str, &str)]) -> AssetDef {
    AssetDef {
        name: name.into(),
        voxel,
        palette: pal(palette),
        parts: vec![part("body", None, [0.0, 0.0, 0.0], spheres.iter().map(|(at, r, c)| Op::Sphere { at: *at, r: *r, c: (*c).into() }).collect())],
        anims: none(),
    }
}

fn bottle() -> AssetDef {
    AssetDef {
        name: "botol".into(),
        voxel: 0.1,
        palette: pal(&[("glass", "#6fbf73"), ("rag", "#e8e0d0"), ("flame", "#ff7a1a!"), ("flame2", "#ffd24a!")]),
        parts: vec![part(
            "body",
            None,
            [0.0, 6.0, 0.0],
            vec![
                bx([-2, 0, -2], [4, 8, 4], "glass"),
                bx([-1, 8, -1], [2, 4, 2], "glass"),
                bx([-1, 12, -1], [2, 2, 2], "rag"),
                bx([-2, 14, -2], [4, 4, 4], "flame"),
                bx([-1, 18, -1], [2, 4, 2], "flame2"),
                bx([-3, 15, -1], [2, 3, 2], "flame"),
            ],
        )],
        anims: none(),
    }
}

fn fire() -> AssetDef {
    let mut s = vec![];
    for (x, z, h) in [(-6, -2, 6), (-2, 3, 8), (3, -4, 7), (5, 2, 5), (0, 0, 10), (-5, 5, 5), (4, 6, 6), (-1, -6, 6)] {
        s.push(bx([x, 0, z], [3, h, 3], "f1"));
        s.push(bx([x, h, z], [2, 3, 2], "f2"));
    }
    AssetDef { name: "api".into(), voxel: 0.1, palette: pal(&[("f1", "#ff7a1a!"), ("f2", "#ffd24a!")]), parts: vec![part("body", None, [0.0, 0.0, 0.0], s)], anims: none() }
}

fn rice_pack() -> AssetDef {
    AssetDef {
        name: "nasi".into(),
        voxel: 0.16,
        palette: pal(&[("paper", "#b88a55"), ("rice", "#fffaf0"), ("leaf", "#4e9a45"), ("tali", "#e8e0d0")]),
        parts: vec![part("body", None, [0.0, 3.0, 0.0], vec![bx([-5, 0, -4], [10, 6, 8], "paper"), bx([-5, 5, -4], [10, 1, 8], "leaf"), bx([-4, 6, -3], [8, 2, 6], "rice"), bx([-5, 2, -1], [10, 1, 2], "tali")])],
        anims: none(),
    }
}

fn cash() -> AssetDef {
    AssetDef {
        name: "uang".into(),
        voxel: 0.16,
        palette: pal(&[("bill", "#5fae52"), ("bill2", "#7cc46d"), ("band", "#e8e0d0"), ("gold", "#f2c230!")]),
        parts: vec![part(
            "body",
            None,
            [0.0, 3.0, 0.0],
            vec![bx([-5, 0, -3], [10, 2, 6], "bill"), bx([-5, 2, -3], [10, 2, 6], "bill2"), bx([-1, 0, -3], [2, 4, 6], "band"), Op::Sphere { at: [7.0, 4.0, 0.0], r: 3.0, c: "gold".into() }],
        )],
        anims: none(),
    }
}

fn heart() -> AssetDef {
    AssetDef {
        name: "sparkle".into(),
        voxel: 0.14,
        palette: pal(&[("h", "#ff5a7a!"), ("w", "#ffd0dc!")]),
        parts: vec![part(
            "body",
            None,
            [0.0, 6.0, 0.0],
            vec![
                Op::Sphere { at: [-3.0, 8.0, 0.0], r: 3.4, c: "h".into() },
                Op::Sphere { at: [3.0, 8.0, 0.0], r: 3.4, c: "h".into() },
                bx([-6, 5, -2], [12, 2, 4], "h"),
                bx([-4, 3, -2], [8, 2, 4], "h"),
                bx([-2, 1, -2], [4, 2, 4], "h"),
                bx([-4, 9, -2], [2, 2, 2], "w"),
            ],
        )],
        anims: none(),
    }
}

/// Every asset the scene uses, once each.
pub fn assets() -> Vec<(String, AssetDef)> {
    let mut out: Vec<(String, AssetDef)> = vec![
        ("warga".into(), person("warga", "warga")),
        ("mahasiswa".into(), person("mahasiswa", "mahasiswa")),
        ("polisi".into(), person("polisi", "polisi")),
        ("barikade".into(), barricade()),
        ("panggung".into(), stage()),
        ("gedung".into(), government()),
        ("puff".into(), blob("puff", 0.2, &[([0.0, 6.0, 0.0], 6.5, "a"), ([0.0, 6.0, 0.0], 4.6, "b"), ([5.0, 4.0, 2.0], 3.5, "a"), ([-5.0, 5.0, -2.0], 3.5, "a")], &[("a", "#d9dbe4!"), ("b", "#ffffff!")])),
        ("botol".into(), bottle()),
        ("api".into(), fire()),
        ("gas".into(), blob("gas", 0.1, &[([0.0, 6.0, 0.0], 7.0, "a"), ([6.0, 5.0, 3.0], 5.0, "b"), ([-6.0, 5.0, -3.0], 5.5, "b"), ([2.0, 10.0, -4.0], 4.5, "a")], &[("a", "#c7d3c0"), ("b", "#dbe5d4")])),
        ("semprot".into(), blob("semprot", 0.2, &[([0.0, 4.0, 0.0], 5.0, "a"), ([0.0, 4.0, 0.0], 3.2, "b"), ([5.0, 3.0, 0.0], 2.5, "a"), ([-5.0, 3.0, 0.0], 2.5, "a")], &[("a", "#9fd3ff!"), ("b", "#ffffff!")])),
        ("nasi".into(), rice_pack()),
        ("uang".into(), cash()),
        ("sparkle".into(), heart()),
        ("banner".into(), crate::perang::banner()),
    ];
    for (n, d) in kampung_assets() {
        if matches!(n, "tree" | "lamp" | "warung") {
            out.push((n.to_string(), d));
        }
    }
    out
}

// ---------------------------------------------------------------- scene

fn terrain() -> TerrainDef {
    let mut m = vec![vec!['a'; W]; D];
    for j in 0..D {
        for i in 0..W {
            let x = i as f32 + 0.5;
            m[j][i] = if j < 3 || j >= D - 3 {
                'k'
            } else if x < 14.0 {
                'p'
            } else if x > 38.0 {
                if hash3(i as i32, j as i32, 7) > 0.8 { 'f' } else { 'q' }
            } else if j == 15 && i % 2 == 0 {
                'm'
            } else {
                'a'
            };
        }
    }
    let t = |h: i32, top: &str, side: &str, jitter: f32| TileDef { h, top: top.into(), side: Some(side.into()), jitter, water: false };
    let mut legend = BTreeMap::new();
    legend.insert("p".to_string(), t(2, "#cfc7b8:stone", "#9a9285:stone", 0.03));
    legend.insert("a".to_string(), t(2, "#4a4d55:stone", "#2f3138:stone", 0.02));
    legend.insert("m".to_string(), t(2, "#e8e8e8:stone", "#2f3138:stone", 0.02));
    legend.insert("q".to_string(), t(3, "#b9b9c4:stone", "#8c8c98:stone", 0.03));
    legend.insert("f".to_string(), t(3, "#5fae52:grass", "#8c8c98:stone", 0.04));
    legend.insert("k".to_string(), t(3, "#b9b2a5:stone", "#8a8478:stone", 0.03));
    TerrainDef { cell: 1.0, step: 0.5, legend, map: m.iter().map(|r| r.iter().collect()).collect() }
}

fn recolor(color: &str, keys: &[&str]) -> BTreeMap<String, String> {
    keys.iter().map(|k| (k.to_string(), color.to_string())).collect()
}

fn entities() -> Vec<EntityDef> {
    let (yellow, navy) = ("#f2a31b", "#2f5fa8");
    let mut v = vec![];
    let mut e = ent("panggung", "panggung", STAGE, 90.0);
    e.palette = recolor(yellow, &["flag", "roof"]);
    v.push(e);
    let mut e = ent("gedung", "gedung", GOV, 270.0);
    e.palette = recolor(navy, &["flag", "roof"]);
    v.push(e);
    for (id, pos, color) in [("spanduk_a1", [14.5, 8.0], yellow), ("spanduk_a2", [14.5, 22.0], yellow), ("spanduk_b1", [37.5, 8.0], navy), ("spanduk_b2", [37.5, 22.0], navy)] {
        let mut e = ent(id, "banner", pos, 0.0);
        e.palette = recolor(color, &["cloth"]);
        v.push(e);
    }
    for (i, x) in [4.0f32, 12.0, 20.0, 28.0, 36.0, 44.0].into_iter().enumerate() {
        v.push(ent(&format!("lamp_n{i}"), "lamp", [x, 2.0], 0.0));
        v.push(ent(&format!("lamp_s{i}"), "lamp", [x, D as f32 - 2.0], 0.0));
    }
    for (i, x) in [8.0f32, 16.0, 24.0, 32.0, 40.0, 48.0].into_iter().enumerate() {
        v.push(ent(&format!("tree_n{i}"), "tree", [x, 1.2], i as f32 * 53.0));
        v.push(ent(&format!("tree_s{i}"), "tree", [x, D as f32 - 1.2], i as f32 * 71.0));
    }
    v.push(ent("warung1", "warung", [13.0, 4.2], 0.0));
    v.push(ent("warung2", "warung", [13.0, D as f32 - 4.2], 180.0));
    v
}

pub fn scene() -> Result<SceneDef> {
    Ok(SceneDef {
        name: "Demo Rakyat".into(),
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

// ---------------------------------------------------------------- the demonstration

fn biomes_json() -> Value {
    let tiles = |spec: &[(&str, &str, &str)]| -> Value {
        let mut m = serde_json::Map::new();
        for (k, top, side) in spec {
            m.insert(k.to_string(), json!({"top": top, "side": side}));
        }
        Value::Object(m)
    };
    json!({
        "siang": {"name": "SIANG", "sky": "day", "speed": 1.0, "terrain": tiles(&[
            ("p", "#cfc7b8:stone", "#9a9285:stone"), ("a", "#4a4d55:stone", "#2f3138:stone"), ("m", "#e8e8e8:stone", "#2f3138:stone"),
            ("q", "#b9b9c4:stone", "#8c8c98:stone"), ("f", "#5fae52:grass", "#8c8c98:stone"), ("k", "#b9b2a5:stone", "#8a8478:stone")])},
        "sore": {"name": "SORE", "sky": "sunset", "speed": 1.0, "terrain": tiles(&[
            ("p", "#d8b896:stone", "#a08468:stone"), ("a", "#53474c:stone", "#352c30:stone"), ("m", "#f0d8c0:stone", "#352c30:stone"),
            ("q", "#c4aaa8:stone", "#947c7c:stone"), ("f", "#6f9a4a:grass", "#947c7c:stone"), ("k", "#c4a890:stone", "#907c68:stone")])},
        "malam": {"name": "MALAM", "sky": "night", "speed": 0.95, "terrain": tiles(&[
            ("p", "#8e8aa6:stone", "#625f7a:stone"), ("a", "#2c2e3a:stone", "#1c1d26:stone"), ("m", "#b8b8cc:stone", "#1c1d26:stone"),
            ("q", "#7a7a96:stone", "#565670:stone"), ("f", "#2f5a3a:grass", "#565670:stone"), ("k", "#6e6a80:stone", "#524f64:stone")])}
    })
}

/// war.json: a street demonstration, endless, day and night.
pub fn war_json() -> Value {
    let orasi_texts = json!(["TURUNKAN HARGA!", "BERAS MURAH!", "WIFI GRATIS!", "JANGAN MAGER!", "KERJA LAYAK!", "DENGARKAN KAMI!", "HIDUP RAKYAT!"]);
    let pop = |emoji: &str, secs: f32| json!({"type": "emoji", "emoji": emoji, "size": 0.16, "at": [0.5, 0.5], "ttl": secs, "anim_in": "pop", "anim_out": "fade", "z": 5});
    let who = |color: &str| json!({"type": "text", "text": "{user}", "size": 0.04, "at": [0.5, 0.62], "color": color, "ttl": 1.8, "anim_in": "slide_up", "z": 5, "outline": "#000000"});
    json!({
        "title": "DEMO RAKYAT VS PEMERINTAH",
        "scene": "scene.json",
        "castle_hp": 1000,
        "match": {"mode": "endless", "round_secs": 75, "lobby_secs": 5, "intermission": 6},
        "initial_units": 5,
        "autospawn": {"every": 6.0, "count": 1},
        "max_units": 40,
        "texts": {
            "round": "HARI KE-{round}", "round_win": "{name} MENANG HARI INI!", "match_win": "{name} JUARA!", "draw": "SERI, SAMA-SAMA CAPEK",
            "match_draw": "SERI", "label_hp": "DUKUNGAN", "label_units": "MASSA", "label_wins": "MENANG", "label_losses": "KALAH", "label_round": "HARI",
            "hint": "MODE OTOMATIS: KIRIM GIFT BIAR RAMAI!"
        },
        "sides": {
            "A": {"name": "RAKYAT", "color": "#f2a31b", "castle": "panggung", "spawn": [12.5, 15.0], "unit": "warga", "facing": 90, "castle_radius": 3.0, "zone": [4.0, 4.0, 25.0, 26.0]},
            "B": {"name": "PEMERINTAH", "color": "#3f74c4", "castle": "gedung", "spawn": [39.5, 15.0], "unit": "polisi", "facing": 270, "castle_radius": 3.2, "zone": [27.0, 4.0, 48.0, 26.0]}
        },
        "units": {
            "warga": {"asset": "warga", "hp": 40, "speed": 1.8, "dmg": 4, "range": 1.2, "cooldown": 1.0, "defeat_anim": "faint"},
            "mahasiswa": {"asset": "mahasiswa", "hp": 30, "speed": 2.4, "dmg": 3, "range": 1.2, "cooldown": 0.8, "defeat_anim": "faint"},
            "polisi": {"asset": "polisi", "hp": 55, "speed": 1.7, "dmg": 6, "range": 1.4, "cooldown": 1.1, "castle_mult": 0.5, "defeat_anim": "faint"},
            "barikade": {"asset": "barikade", "hp": 220, "speed": 0, "dmg": 0, "structure": true, "tint": ""}
        },
        "fx_assets": {"boom": "puff", "meteor": "botol", "fire": "api", "cloud": "gas", "push": "semprot"},
        "actions": {
            "orasi": {"type": "chant", "texts": orasi_texts, "count": 6, "secs": 3.2, "anim": "cheer", "boost_secs": 12},
            "nasi_bungkus": {"type": "heal", "amount": 260, "asset": "nasi"},
            "molotov": {"type": "meteor", "count": 1, "asset": "botol", "radius": 2.4, "damage": 60, "castle_damage": 70, "target": "front", "burn_secs": 6, "burn_dps": 8, "burn_asset": "api"},
            "barikade": {"type": "spawn", "unit": "barikade", "count": 3, "at": "front"},
            "tambah_warga": {"type": "spawn", "unit": "warga", "count": 4},
            "tambah_mahasiswa": {"type": "spawn", "unit": "mahasiswa", "count": 4},
            "tambah_aparat": {"type": "spawn", "unit": "polisi", "count": 4},
            "bantuan_asing": {"type": "heal", "amount": 300, "asset": "uang"},
            "water_cannon": {"type": "push", "strength": 8.5, "radius": 5.5, "target": "front"},
            "gas_air_mata": {"type": "cloud", "radius": 4.0, "secs": 9, "slow": 0.45, "dps": 5, "target": "front", "asset": "gas"},
            "intelijen": {"type": "sabotage", "slow_secs": 8, "slow": 0.6, "structures": true},
            "sogokan": {"type": "convert", "count": 3, "radius": 7.5, "target": "front"},
            "massa": {"type": "spawn", "unit": "warga", "count": 10, "at": "front"},
            "water_cannon_besar": {"type": "push", "strength": 15, "radius": 9, "damage": 30, "target": "front"},
            "gas_besar": {"type": "cloud", "radius": 6.5, "secs": 10, "slow": 0.4, "dps": 9, "target": "front", "asset": "gas"},
            "tembakan_gas": {"type": "meteor", "count": 7, "gap": 0.22, "asset": "gas", "radius": 2.8, "damage": 55, "castle_damage": 70, "target": "front"},
            "dorong_massa": {"type": "push", "strength": 14, "radius": 10, "damage": 30, "target": "front"},
            "aparat_membelot": {"type": "convert", "count": 5, "radius": 12, "target": "front"},
            "hujan_botol": {"type": "meteor", "count": 7, "gap": 0.22, "asset": "botol", "radius": 2.6, "damage": 50, "castle_damage": 70, "target": "front", "burn_secs": 5, "burn_dps": 7, "burn_asset": "api"},
            "maju_aparat": {"type": "chant", "texts": ["MAJU TERTIBKAN!", "AMANKAN LOKASI!", "SIAP KOMANDAN!"], "count": 6, "secs": 3.0, "anim": "cheer", "boost_secs": 10},
            "panggung_dikepung": {"type": "heal", "amount": -240, "asset": "gas"},
            "gedung_dikepung": {"type": "heal", "amount": -240, "asset": "api"}
        },
        "gift_aliases": {"mawar": "rose", "donat": "doughnut", "jari hati": "fingerheart", "parfum": "perfume", "singa": "lion", "galaksi": "galaxy", "paus": "whale", "semesta": "universe", "money gun": "moneygun", "uang": "moneygun"},
        "triggers": [
            {"name": "rose", "on": "gift:rose", "side": "A", "macro": "orasi_besar", "label": "ROSE = ORASI (MORAL NAIK)", "icon": ":party:", "cooldown": 4},
            {"name": "tiktok", "on": "gift:tiktok", "side": "B", "do": "molotov", "label": "TIKTOK = MOLOTOV KE PEMERINTAH", "icon": ":fire:", "per_count": true, "max_repeat": 3, "overlay": [pop("🔥", 1.2), who("#ffffff")]},
            {"name": "fingerheart", "on": "gift:fingerheart", "side": "A", "do": "nasi_bungkus", "label": "FINGER HEART = NASI BUNGKUS", "icon": ":heart:", "overlay": [pop("🍙", 1.4)]},
            {"name": "handhearts", "on": "gift:handhearts", "side": "A", "do": "barikade", "label": "HAND HEARTS = BARIKADE", "icon": ":brick:"},
            {"name": "doughnut", "on": "gift:doughnut", "side": "A", "do": "tambah_warga", "per_count": true, "label": "DOUGHNUT = +4 WARGA", "icon": ":blue_square:"},
            {"name": "perfume", "on": "gift:perfume", "side": "B", "macro": "bantuan_asing", "label": "PERFUME = BANTUAN ASING/KORUPSI", "icon": ":money:", "cooldown": 5},
            {"name": "galaxy", "on": "gift:galaxy", "side": "A", "macro": "penertiban", "label": "GALAXY = ULTIMATE PENERTIBAN", "icon": ":wave:", "cooldown": 12},
            {"name": "whale", "on": "gift:whale", "side": "B", "macro": "aksi_massa", "label": "PAUS = ULTIMATE AKSI MASSA", "icon": ":crown:", "cooldown": 15},
            {"name": "lion", "on": "gift:lion", "side": "A", "do": "intelijen", "label": "LION = INTELIJEN MENYUSUP", "icon": ":warning:", "cooldown": 6, "overlay": [pop("🕵️", 1.4)]},
            {"name": "universe", "on": "gift:universe", "side": "A", "do": "sogokan", "label": "UNIVERSE = NASI BUNGKUS SOGOKAN", "icon": ":gift:", "cooldown": 8},
            {"name": "moneygun", "on": "gift:moneygun", "side": "B", "do": "tambah_aparat", "per_count": true, "label": "MONEY GUN = +4 APARAT", "icon": ":shield:"},
            {"name": "join_rakyat", "on": ["comment:~rakyat", "comment:~demo"], "side": "A", "do": {"type": "spawn", "unit": "mahasiswa", "count": 1}, "cooldown": 1.5, "legend": false, "announce": "{user} IKUT DEMO"},
            {"name": "join_aparat", "on": "comment:~aparat", "side": "B", "do": {"type": "spawn", "count": 1}, "cooldown": 1.5, "legend": false, "announce": "{user} JAGA GEDUNG"},
            {"name": "likes", "on": "like", "side": "event", "do": {"type": "spawn", "count": 1}, "min_count": 10, "cooldown": 3, "legend": false, "announce": "{user} LIKE x{count}"}
        ],
        "macros": {
            "orasi_besar": [
                {"do": "announce", "text": "{user} BERORASI DI ATAS PANGGUNG!", "secs": 2.4, "color": "#f2a31b"},
                {"do": "act", "action": "orasi", "side": "A"}
            ],
            "bantuan_asing": [
                {"do": "random", "choices": [
                    {"weight": 7, "steps": [
                        {"do": "announce", "text": "BANTUAN ASING MASUK KE KAS NEGARA", "secs": 2.4, "color": "#3f74c4"},
                        {"do": "act", "action": "bantuan_asing", "side": "B"}]},
                    {"weight": 3, "steps": [
                        {"do": "announce", "text": "DANA DIKORUPSI! RAKYAT MAKIN MARAH", "secs": 2.6, "color": "#ff6a4a"},
                        {"do": "act", "action": {"type": "heal", "amount": 150, "asset": "uang"}, "side": "B"},
                        {"do": "act", "action": "orasi", "side": "A"}]}
                ]}
            ],
            "penertiban": [
                {"do": "announce", "text": "PENERTIBAN! WATER CANNON, GAS AIR MATA, TEMBAKAN GAS", "secs": 2.8, "color": "#3f74c4"},
                {"do": "act", "action": {"type": "cinematic", "pov": "enemy", "secs": 8, "look": "enemy", "fov": 76, "shake": 0.22, "tint": "#2f5fa845", "vignette": 0.55, "bob": 1.2}, "side": "event"},
                {"do": "act", "action": "maju_aparat", "side": "B"},
                {"do": "wait", "secs": 1.2},
                {"do": "act", "action": "water_cannon_besar", "side": "A"},
                {"do": "wait", "secs": 0.8},
                {"do": "act", "action": "gas_besar", "side": "A"},
                {"do": "wait", "secs": 0.6},
                {"do": "act", "action": "tembakan_gas", "side": "A"},
                {"do": "act", "action": "panggung_dikepung", "side": "A"},
                {"do": "wait", "secs": 1.6},
                {"do": "act", "action": "water_cannon_besar", "side": "A"}
            ],
            "aksi_massa": [
                {"do": "announce", "text": "{user} MENGGERAKKAN AKSI SERIBU MASSA!", "secs": 2.8, "color": "#f2a31b"},
                {"do": "act", "action": {"type": "cinematic", "pov": "enemy", "secs": 8, "look": "enemy", "fov": 80, "shake": 0.14, "tint": "#f2a31b30", "vignette": 0.45, "bob": 1.6}, "side": "event"},
                {"do": "wait", "secs": 1.0},
                {"do": "act", "action": "massa", "side": "A"},
                {"do": "act", "action": "orasi", "side": "A"},
                {"do": "wait", "secs": 0.8},
                {"do": "act", "action": "dorong_massa", "side": "B"},
                {"do": "wait", "secs": 0.6},
                {"do": "act", "action": "hujan_botol", "side": "B"},
                {"do": "wait", "secs": 1.6},
                {"do": "act", "action": "aparat_membelot", "side": "B"},
                {"do": "act", "action": "gedung_dikepung", "side": "B"},
                {"do": "act", "action": "dorong_massa", "side": "B"}
            ],
            "solidaritas": [
                {"do": "if", "cond": {"hp_pct_lt": {"side": "A", "value": 25}}, "then": [{"do": "announce", "text": "SOLIDARITAS! WARGA DATANG MEMBANTU", "secs": 2.2, "color": "#f2a31b"}, {"do": "act", "action": "tambah_warga", "side": "A"}, {"do": "act", "action": "orasi", "side": "A"}]},
                {"do": "if", "cond": {"hp_pct_lt": {"side": "B", "value": 25}}, "then": [{"do": "announce", "text": "DANA DARURAT CAIR", "secs": 2.2, "color": "#3f74c4"}, {"do": "act", "action": "bantuan_asing", "side": "B"}, {"do": "act", "action": "tambah_aparat", "side": "B"}]}
            ]
        },
        "bots": [{"name": "solidaritas_watch", "macro": "solidaritas", "every": 15}],
        "autopilot": {"idle_secs": 6, "interval": [2.5, 6.0], "actions": [
            {"weight": 3, "do": "tambah_warga", "side": "A"},
            {"weight": 3, "do": "tambah_aparat", "side": "B"},
            {"weight": 2, "do": "tambah_mahasiswa", "side": "A"},
            {"weight": 2, "do": "orasi", "side": "A"},
            {"weight": 1, "do": "nasi_bungkus", "side": "A"},
            {"weight": 1, "do": "bantuan_asing", "side": "B"},
            {"weight": 1, "do": "molotov", "side": "B"},
            {"weight": 1, "do": "water_cannon", "side": "A"},
            {"weight": 1, "do": "gas_air_mata", "side": "A"},
            {"weight": 1, "do": "barikade", "side": "A"},
            {"weight": 1, "do": "sogokan", "side": "A"}
        ]},
        "biome": "siang",
        "biome_rotation": {"every": "round", "list": ["siang", "malam"]},
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
        assert_eq!(def.match_.mode, "endless");
        assert!(def.biomes.contains_key("malam") && def.triggers.len() >= 10);
    }
}
