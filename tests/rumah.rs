use std::path::PathBuf;

use r3dsims::gpu::{Gpu, SceneRenderer};
use r3dsims::house::scene_inline;
use r3dsims::world::World;

fn world() -> World {
    World::from_scene(scene_inline().unwrap(), PathBuf::new()).unwrap()
}

#[test]
fn story_finishes_and_nobody_gets_stuck() {
    let mut w = world();
    w.story_loop = false;
    while w.story.is_some() && w.time < 200.0 {
        w.update(1.0 / 30.0);
    }
    assert!(w.story.is_none(), "story never finished (a walk_to probably found no path)");
    assert!((25.0..70.0).contains(&w.time), "story took {}s", w.time);
}

#[test]
fn a_child_walks_down_the_stairs_to_the_kitchen() {
    let mut w = world();
    w.story_loop = false;
    w.story = None;
    let upstairs = w.entities[w.entity_idx("anak").unwrap()].layer;
    w.run_json(r#"{"do":"walk_to","actor":"anak","to":"meja_makan_bawah","speed":3.0}"#).unwrap();
    let mut min_y = f32::MAX;
    let mut max_y = f32::MIN;
    for _ in 0..(40 * 30) {
        w.update(1.0 / 30.0);
        let e = &w.entities[w.entity_idx("anak").unwrap()];
        min_y = min_y.min(e.pos.y);
        max_y = max_y.max(e.pos.y);
    }
    let anak = &w.entities[w.entity_idx("anak").unwrap()];
    assert_ne!(anak.layer, upstairs, "anak must end on another floor");
    assert_eq!(anak.layer, w.entities[w.entity_idx("ibu").unwrap()].layer, "same floor as the kitchen");
    assert!(max_y - min_y > 2.0, "the walk must descend a storey, y {min_y}..{max_y}");
    let ibu = &w.entities[w.entity_idx("ibu").unwrap()];
    assert!((anak.pos - ibu.pos).length() < 5.0, "anak should be in the kitchen, at {:?}", anak.pos);
}

#[test]
fn building_steps_hide_floors_and_hidden_residents() {
    let mut w = world();
    w.run_json(r#"{"do":"building","id":"rumah","show_floor":0}"#).unwrap();
    w.update(1.0 / 30.0);
    let anak = &w.entities[w.entity_idx("anak").unwrap()];
    assert!(!w.entity_visible(anak), "residents upstairs are hidden with their floor");
    let ibu = &w.entities[w.entity_idx("ibu").unwrap()];
    assert!(w.entity_visible(ibu));
    w.run_json(r#"{"do":"building","id":"rumah","show_floor":-1,"gap":3.0,"secs":1.0}"#).unwrap();
    for _ in 0..40 {
        w.update(1.0 / 30.0);
    }
    assert!((w.buildings[0].gap - 3.0).abs() < 1e-3);
    let anak_y = w.entities[w.entity_idx("anak").unwrap()].pos.y;
    assert!(anak_y > 3.0 + 1.0, "residents ride their floor when it lifts, y = {anak_y}");
}

#[test]
fn dollhouse_renders_and_hiding_a_floor_changes_the_picture() {
    let gpu = match Gpu::headless() {
        Ok(g) => g,
        Err(e) => {
            eprintln!("skipping: {e:#}");
            return;
        }
    };
    let mut r = SceneRenderer::new(&gpu.device, &gpu.queue, 180, 320);
    let mut w = world();
    w.story = None;
    w.update(1.0 / 30.0);
    let all = r.render_rgb(&w).unwrap();
    w.run_json(r#"{"do":"building","id":"rumah","show_floor":0}"#).unwrap();
    w.update(1.0 / 30.0);
    let one = r.render_rgb(&w).unwrap();
    let diff = all.iter().zip(&one).filter(|(a, b)| a != b).count();
    assert!(diff > 500, "hiding the upper floor must visibly change the frame (changed bytes: {diff})");
}
