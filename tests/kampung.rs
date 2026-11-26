use std::path::PathBuf;

use glam::Vec2;
use r3dsims::gpu::{Gpu, SceneRenderer};
use r3dsims::nav::Place;
use r3dsims::story::Step;
use r3dsims::templates::kampung_scene_inline;
use r3dsims::world::World;

fn world() -> World {
    World::from_scene(kampung_scene_inline().unwrap(), PathBuf::new()).unwrap()
}

#[test]
fn story_finishes_in_reasonable_time() {
    let mut w = world();
    w.story_loop = false;
    while w.story.is_some() && w.time < 200.0 {
        w.update(1.0 / 30.0);
    }
    assert!(w.story.is_none(), "story never finished");
    assert!((30.0..60.0).contains(&w.time), "story took {}s", w.time);
}

#[test]
fn walking_goes_around_a_house_not_through_it() {
    let w = world();
    // rumah_a stands at (7.5, 6.5) with a 4.4 x 3.6 footprint; walk from its front to its back.
    let from = Place { pos: Vec2::new(7.5, 10.0), layer: 0 };
    let to = Place { pos: Vec2::new(7.5, 2.5), layer: 0 };
    let pts = w.nav.path(from, to).expect("a path exists");
    assert!(pts.len() >= 2, "expected a detour, got {pts:?}");
    let mut prev = from.pos;
    for p in pts {
        for k in 0..=20 {
            let q = prev.lerp(p.pos, k as f32 / 20.0);
            let inside = (q.x - 7.5).abs() < 2.0 && (q.y - 6.5).abs() < 1.6;
            assert!(!inside, "path crosses the house at {q:?}");
        }
        prev = p.pos;
    }
}

#[test]
fn live_json_commands_drive_actors_and_camera() {
    let mut w = world();
    w.run_json(r#"{"do":"teleport","actor":"ani","to":[13,9]}"#).unwrap();
    w.run_json(r#"{"op":"set","yaw":200}"#).unwrap();
    w.update(1.0 / 30.0);
    let i = w.entity_idx("ani").unwrap();
    assert!((w.entities[i].pos.x - 13.0).abs() < 0.5, "ani at {:?}", w.entities[i].pos);
    assert_eq!(w.rig.cam.yaw, 200.0);
    assert!(w.run_json("not json").is_err());
}

#[test]
fn spawned_follower_gets_own_palette_and_label() {
    let mut w = world();
    let step: Step = serde_json::from_str(
        r##"{"do":"spawn","entity":{"id":"rina","asset":"villager","pos":[12,9],"label":"@rina","palette":{"b":"#ff00ff"}}}"##,
    )
    .unwrap();
    w.run(step);
    w.update(1.0 / 30.0);
    let i = w.entity_idx("rina").expect("spawned");
    let model = &w.entities[i].model;
    let shirt = model.color_names["b"] as usize;
    assert_eq!(w.entities[i].colors[shirt], [255, 0, 255]);
    assert_ne!(model.colors[shirt], [255, 0, 255], "the shared model must stay untouched");
}

#[test]
fn gpu_frames_are_deterministic_and_not_blank() {
    let gpu = match Gpu::headless() {
        Ok(g) => g,
        Err(e) => {
            eprintln!("skipping: {e:#}");
            return;
        }
    };
    let frame = || {
        let mut w = world();
        for _ in 0..90 {
            w.update(1.0 / 30.0);
        }
        let mut r = SceneRenderer::new(&gpu.device, &gpu.queue, 120, 213);
        r.render_rgb(&w).unwrap()
    };
    let (a, b) = (frame(), frame());
    assert_eq!(a, b, "same world must render identically");
    let first = &a[0..3];
    assert!(a.chunks_exact(3).any(|p| p != first), "frame is a single colour");
}
