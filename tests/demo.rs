use std::path::PathBuf;

use r3dsims::demo;
use r3dsims::script::Ctx;
use r3dsims::war::{Action, ActionRef, Event, War};
use r3dsims::world::World;
use serde_json::{Value, json};

fn load(tag: &str) -> (World, War) {
    let dir: PathBuf = std::env::temp_dir().join(format!("r3d_demo_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    demo::write(&dir).unwrap();
    War::load(&dir.join("war.json")).expect("demo war loads and validates")
}

fn sim(war: &mut War, world: &mut World, secs: f32) {
    let dt = 1.0 / 30.0;
    for _ in 0..(secs / dt) as usize {
        war.update(dt, world);
        world.update(dt);
    }
}

fn run(war: &mut War, world: &mut World, name: &str, side: usize) {
    war.exec(world, &ActionRef::Name(name.into()), side, &Ctx::default());
}

fn inline(v: Value) -> ActionRef {
    ActionRef::Inline(Box::new(serde_json::from_value::<Action>(v).unwrap()))
}

fn units(war: &War, side: &str) -> u64 {
    war.state_json()["sides"][side]["units"].as_u64().unwrap()
}

fn gift(name: &str) -> Event {
    Event { kind: "gift".into(), user: "Budi".into(), gift: name.into(), count: 1, diamonds: 1, text: String::new(), side: None, id: None }
}

#[test]
fn an_unwatched_stream_plays_itself_through_day_and_night() {
    let (mut world, mut war) = load("auto");
    // nobody ever sends anything: lobby -> round -> winner -> next round, by itself
    sim(&mut war, &mut world, 24.0);
    let st = war.state_json();
    assert_eq!(st["phase"], json!("fighting"));
    assert_eq!(st["autopilot"], json!(true), "autopilot should have taken over: {st}");
    assert!(st["ticker"].as_array().unwrap().iter().any(|t| t.as_str().unwrap().starts_with("AUTO")), "autopilot leaves ticker lines: {st}");
    assert!(units(&war, "A") + units(&war, "B") >= 6, "soldiers on the street");
    assert_eq!(st["biome"], json!("siang"));
    sim(&mut war, &mut world, 90.0);
    let st = war.state_json();
    assert!(st["round"].as_u64().unwrap() >= 2, "round 2 should have started: {st}");
    assert_eq!(st["biome"], json!("malam"), "the second round is at night");
    assert!(st["last_round_winner"].is_string(), "a round has a winner or a draw: {st}");
    assert_eq!(st["match_winner"], Value::Null, "endless mode never ends by itself");
}

#[test]
fn a_real_event_hands_control_back_from_the_autopilot() {
    let (mut world, mut war) = load("handback");
    sim(&mut war, &mut world, 14.0);
    assert_eq!(war.state_json()["autopilot"], json!(true));
    war.handle_event(&mut world, gift("anything"));
    assert_eq!(war.state_json()["autopilot"], json!(false));
}

#[test]
fn bribes_make_protesters_change_sides_and_barricades_are_not_soldiers() {
    let (mut world, mut war) = load("convert");
    war.control(&mut world, "start").unwrap();
    sim(&mut war, &mut world, 6.0);
    let (a0, b0) = (units(&war, "A"), units(&war, "B"));
    run(&mut war, &mut world, "sogokan", 0);
    sim(&mut war, &mut world, 0.2);
    let (a1, b1) = (units(&war, "A"), units(&war, "B"));
    assert!(a1 < a0 && b1 >= b0 + 2, "converted soldiers move from A to B: {a0},{b0} -> {a1},{b1}");

    // three barricades go up in front of the line
    run(&mut war, &mut world, "barikade", 0);
    sim(&mut war, &mut world, 2.0);
    let barricades = world.entities.iter().filter(|e| e.model.name == "barikade").count();
    assert_eq!(barricades, 3, "three barricades stand");
}

#[test]
fn speeches_make_soldiers_shout_slogans_for_a_while() {
    let (mut world, mut war) = load("chant");
    war.control(&mut world, "start").unwrap();
    sim(&mut war, &mut world, 4.0);
    run(&mut war, &mut world, "orasi", 0);
    war.handle_event(&mut world, gift("anything")); // a viewer is there: the autopilot stays out of it
    sim(&mut war, &mut world, 0.3);
    let shouting = world.entities.iter().filter(|e| e.speech.is_some()).count();
    assert!(shouting >= 3, "{shouting} soldiers are shouting");
    sim(&mut war, &mut world, 5.0);
    assert_eq!(world.entities.iter().filter(|e| e.speech.is_some()).count(), 0, "the bubbles go away");
}

#[test]
fn healing_costs_nothing_but_sabotage_strips_a_boost() {
    let (mut world, mut war) = load("heal");
    war.control(&mut world, "start").unwrap();
    sim(&mut war, &mut world, 1.0);
    run(&mut war, &mut world, "molotov", 1);
    sim(&mut war, &mut world, 4.0);
    let hurt = war.state_json()["sides"]["B"]["hp"].as_f64().unwrap();
    run(&mut war, &mut world, "bantuan_asing", 1);
    let healed = war.state_json()["sides"]["B"]["hp"].as_f64().unwrap();
    assert!(healed >= hurt, "foreign aid never hurts: {hurt} -> {healed}");
    war.exec(&mut world, &inline(json!({"type": "boost", "secs": 20})), 0, &Ctx::default());
    assert!(war.state_json()["sides"]["A"]["boost_secs"].as_f64().unwrap() > 10.0);
    run(&mut war, &mut world, "intelijen", 0);
    assert_eq!(war.state_json()["sides"]["A"]["boost_secs"].as_f64().unwrap(), 0.0);
}

#[test]
fn only_ultimates_move_the_camera() {
    let (mut world, mut war) = load("camera");
    war.control(&mut world, "start").unwrap();
    sim(&mut war, &mut world, 6.0);
    let home = world.rig.cam;
    assert!(home.ortho);
    // ordinary skills: the camera does not move
    for name in ["molotov", "orasi", "water_cannon", "gas_air_mata", "sogokan", "intelijen"] {
        run(&mut war, &mut world, name, 1);
        sim(&mut war, &mut world, 0.5);
        assert!(world.rig.cam.ortho && (world.rig.cam.target - home.target).length() < 1e-4, "{name} must not touch the camera");
    }
    assert_eq!(war.state_json()["cinematic"], json!(false));
    // an ultimate: first person from a camp, then back to the broadcast view
    war.handle_event(&mut world, gift("Whale"));
    sim(&mut war, &mut world, 3.0);
    assert_eq!(war.state_json()["cinematic"], json!(true));
    assert!(!world.rig.cam.ortho, "first person is a perspective camera");
    assert!(world.rig.cam.dist < 10.0 && world.rig.cam.fov > 60.0);
    // the viewpoint is the camp of the attacker (RAKYAT: the stage, x about 8.5), raised above its gate, not a soldier
    let eye = world.rig.cam.eye();
    assert!(eye.x > 9.0 && eye.x < 15.0 && eye.y > 3.5, "eye at the citizens' camp gate: {eye:?}");
    assert!(war.state_json()["overlay_elements"].as_array().unwrap().iter().any(|e| e == "cine.tint"), "the ultimate tints the screen");
    sim(&mut war, &mut world, 8.5);
    assert_eq!(war.state_json()["cinematic"], json!(false));
    assert!(world.rig.cam.ortho && (world.rig.cam.target - home.target).length() < 1e-3, "back to the home camera");
}

#[test]
fn biomes_recolour_ground_sky_and_the_night_gets_dark() {
    let (mut world, mut war) = load("biome");
    let day_sky = world.sky.top;
    let day_plaza = world.terrain_def.legend["p"].top.clone();
    war.control(&mut world, "biome:malam").unwrap();
    sim(&mut war, &mut world, 4.0);
    assert_ne!(world.terrain_def.legend["p"].top, day_plaza, "the plaza changes colour at night");
    assert!(world.sky.top.length() < day_sky.length() * 0.5, "night sky is much darker");
    assert!(war.control(&mut world, "biome:nowhere").is_err());
}
