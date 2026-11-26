use std::path::PathBuf;

use r3dsims::building::layer_parts;
use r3dsims::gpu::{Gpu, SceneRenderer};
use r3dsims::harbor::scene_inline;
use r3dsims::world::World;

fn world() -> World {
    World::from_scene(scene_inline().unwrap(), PathBuf::new()).unwrap()
}

#[test]
fn story_finishes_without_unreachable_targets() {
    let mut w = world();
    w.story_loop = false;
    while w.story.is_some() && w.time < 300.0 {
        w.update(1.0 / 30.0);
    }
    assert!(w.story.is_none(), "story never finished (some walk_to found no path)");
    assert!((40.0..120.0).contains(&w.time), "story took {}s", w.time);
}

#[test]
fn tourist_climbs_two_stair_runs_from_the_beach_to_the_rooftop() {
    let mut w = world();
    w.story = None;
    let i = w.entity_idx("turis").unwrap();
    assert_eq!(w.entities[i].layer, 0, "starts on the beach");
    w.run_json(r#"{"do":"walk_to","actor":"turis","to":"teras_belakang","speed":3.0}"#).unwrap();
    let mut max_y = 0.0f32;
    for _ in 0..(60 * 30) {
        w.update(1.0 / 30.0);
        max_y = max_y.max(w.entities[i].pos.y);
    }
    let (b, f) = layer_parts(w.entities[i].layer).expect("ends inside a building");
    assert_eq!((w.buildings[b].id.as_str(), f), ("penginapan", 2), "ends on the rooftop");
    assert!(max_y > 1.0 + 2.0 * 3.0, "must have climbed two storeys, max y {max_y}");
}

#[test]
fn two_buildings_and_both_are_enterable_from_the_outside() {
    let mut w = world();
    w.story = None;
    assert_eq!(w.buildings.len(), 2);
    w.run_json(r##"{"do":"spawn","entity":{"id":"x","asset":"villager","pos":[20,19]}}"##).unwrap();
    w.update(1.0 / 30.0);
    w.run_json(r#"{"do":"walk_to","actor":"x","to":{"pos":[24.4,16.6],"building":"gubuk","floor":0},"speed":3.0}"#).unwrap();
    for _ in 0..(30 * 30) {
        w.update(1.0 / 30.0);
    }
    let e = &w.entities[w.entity_idx("x").unwrap()];
    let (b, _) = layer_parts(e.layer).expect("inside the hut");
    assert_eq!(w.buildings[b].id, "gubuk");
}

#[test]
fn harbour_renders_on_the_gpu() {
    let gpu = match Gpu::headless() {
        Ok(g) => g,
        Err(e) => {
            eprintln!("skipping: {e:#}");
            return;
        }
    };
    let mut r = SceneRenderer::new(&gpu.device, &gpu.queue, 200, 356);
    let mut w = world();
    for _ in 0..60 {
        w.update(1.0 / 30.0);
    }
    let rgb = r.render_rgb(&w).unwrap();
    let first = &rgb[0..3];
    assert!(rgb.chunks_exact(3).filter(|p| *p != first).count() > 5000, "scene should cover a lot of the frame");
}
