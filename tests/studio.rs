//! A studio is a live show that is not a game: the kampung diorama with viewers talking to it.

use std::path::PathBuf;

use r3dsims::client::Client;
use r3dsims::live::{Live, LiveOpts};
use r3dsims::show::{Show, is_show_file, load_show};
use r3dsims::studio;
use r3dsims::war::{Event, Hooks};
use r3dsims::world::World;
use serde_json::{Value, json};

fn project(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("r3d_studio_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    studio::write_template(&dir).unwrap();
    dir
}

fn load(tag: &str) -> (World, Box<dyn Show>) {
    load_show(&project(tag).join("studio.json")).expect("template studio loads and validates")
}

fn sim(show: &mut dyn Show, world: &mut World, secs: f32) {
    let dt = 1.0 / 30.0;
    for _ in 0..(secs / dt) as usize {
        show.update(dt, world);
        world.update(dt);
    }
}

fn ev(kind: &str, user: &str, gift: &str, count: u32, text: &str) -> Event {
    Event { kind: kind.into(), user: user.into(), gift: gift.into(), count, diamonds: if kind == "gift" { 1 } else { 0 }, text: text.into(), side: None, id: None }
}

#[test]
fn the_template_is_a_studio_and_not_a_war() {
    let dir = project("kind");
    let v: Value = serde_json::from_str(&std::fs::read_to_string(dir.join("studio.json")).unwrap()).unwrap();
    assert!(is_show_file(&v) && v.get("sides").is_none());
    let (_world, show) = load_show(&dir.join("studio.json")).unwrap();
    assert_eq!(show.kind(), "studio");
    assert!(show.camps().is_none(), "a studio has no camps, so no win/loss history");
    let st = show.state_json();
    assert_eq!(st["kind"], json!("studio"));
    assert_eq!(st["sides"], json!({}));
    assert_eq!(st["goal"]["target"], json!(200.0));
}

#[test]
fn chat_likes_and_gifts_drive_the_scene() {
    let (mut world, mut show) = load("drive");
    sim(show.as_mut(), &mut world, 0.5);

    let r = show.handle_event(&mut world, ev("comment", "Sari", "", 1, "halo kampung"));
    assert_eq!(r["matched"], json!(["obrolan"]));
    let r = show.handle_event(&mut world, ev("comment", "Sari", "", 1, "lagi"));
    assert_eq!(r["matched"], json!(["obrolan (cooldown)"]), "chat is rate limited so a flood cannot bury the scene");
    sim(show.as_mut(), &mut world, 0.3);
    assert_eq!(show.state_json()["vars"]["chats"], json!(1.0));

    show.handle_event(&mut world, ev("like", "Sari", "", 5, ""));
    sim(show.as_mut(), &mut world, 0.2);
    let st = show.state_json();
    assert_eq!(st["vars"]["likes"], json!(5.0), "a like event of count 5 adds 5 to the counter");

    let r = show.handle_event(&mut world, ev("gift", "Budi", "Rose", 1, ""));
    assert_eq!(r["matched"], json!(["mawar"]), "stop: true keeps the catch-all thank-you away");
    sim(show.as_mut(), &mut world, 0.3);
    assert!(!show.state_json()["overlay_elements"].as_array().unwrap().is_empty(), "the rose effect is an overlay");
    assert_eq!(show.state_json()["vars"]["gifts"], json!(1.0));

    let r = show.handle_event(&mut world, ev("gift", "Budi", "Lion", 1, ""));
    assert_eq!(r["matched"], json!(["hadiah"]), "an unknown gift falls to the catch-all");

    // the leaderboard: a gift counts its diamonds, any other event one point (Sari: 2 comments + 1 like, Budi: 2 gifts)
    let top = show.state_json()["top"].clone();
    assert_eq!((top[0]["user"].clone(), top[0]["points"].clone()), (json!("Sari"), json!(3)));
    assert_eq!((top[1]["user"].clone(), top[1]["points"].clone()), (json!("Budi"), json!(2)));
}

#[test]
fn reaching_the_goal_runs_its_macro_and_moves_the_target() {
    let (mut world, mut show) = load("goal");
    show.handle_event(&mut world, ev("like", "A", "", 250, ""));
    sim(show.as_mut(), &mut world, 0.5);
    let g = show.state_json()["goal"].clone();
    assert_eq!(g["reached"], json!(1));
    assert_eq!(g["target"], json!(400.0), "a repeating goal asks for another 200");
    assert!(show.vars()["goal_frac"].parse::<f32>().unwrap() < 0.3, "the bar starts over");
}

#[test]
fn published_bots_and_macros_work_and_game_steps_are_refused() {
    let (mut world, mut show) = load("publish");
    let h: Hooks = serde_json::from_value(json!({
        "macros": {"tick": [{"do": "add", "var": "ticks"}]},
        "bots": [{"name": "t", "macro": "tick", "every": 1.0}]
    }))
    .unwrap();
    show.publish(&world, h).expect("publish");
    sim(show.as_mut(), &mut world, 3.5);
    let ticks = show.state_json()["vars"]["ticks"].as_f64().unwrap();
    assert!((2.0..=4.0).contains(&ticks), "a bot every second: {ticks}");

    let bad: Hooks = serde_json::from_value(json!({"macros": {"m": [{"do": "act", "action": "meteor"}]}})).unwrap();
    let err = show.publish(&world, bad).unwrap_err().to_string();
    assert!(err.contains("act"), "{err}");
    let bad: Hooks = serde_json::from_value(json!({"macros": {"m": [{"do": "step", "step": {"do": "say", "actor": "tidak_ada", "text": "x"}}]}})).unwrap();
    let err = show.publish(&world, bad).unwrap_err().to_string();
    assert!(err.contains("tidak_ada"), "{err}");
    let bad: Hooks = serde_json::from_value(json!({"actions": {"a": {"type": "meteor"}}})).unwrap();
    assert!(show.publish(&world, bad).is_err(), "a studio has no game actions");
}

#[test]
fn a_silent_stream_is_kept_alive_by_the_idle_bot() {
    let (mut world, mut show) = load("idle");
    show.publish(
        &world,
        serde_json::from_value(json!({
            "macros": {"hello": [{"do": "add", "var": "greeted"}]},
            "bots": [{"name": "sapa", "macro": "hello", "when_idle": 5.0}]
        }))
        .unwrap(),
    )
    .unwrap();
    sim(show.as_mut(), &mut world, 4.0);
    assert!(show.state_json()["vars"]["greeted"].is_null() || show.state_json()["vars"]["greeted"] == json!(0.0));
    sim(show.as_mut(), &mut world, 3.0);
    assert_eq!(show.state_json()["vars"]["greeted"], json!(1.0), "after 5 quiet seconds the bot speaks once");
    show.handle_event(&mut world, ev("like", "A", "", 1, ""));
    sim(show.as_mut(), &mut world, 3.0);
    assert_eq!(show.state_json()["vars"]["greeted"], json!(1.0), "a viewer event restarts the quiet");
}

#[test]
fn a_fed_counter_moves_the_scene_through_a_watch() {
    let dir = project("feed");
    let path = dir.join("studio.json");
    let mut v: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    v["macros"]["pasang"] = json!([
        {"do": "step", "step": {"do": "spawn", "entity": {"id": "lencana", "asset": "house", "pos": ["={var.x}", "={var.z}"], "label": "{text.nama}"}}},
        {"do": "add", "var": "ran"}
    ]);
    v["watch"] = json!([{"var": "x", "macro": "pasang"}]);
    std::fs::write(&path, v.to_string()).unwrap();
    let (mut world, mut show) = load_show(&path).unwrap();

    // a bad feed is refused whole and changes nothing
    assert!(show.set_vars(&json!({"set": {"bad name": 1}})).is_err());
    assert!(show.set_vars(&json!({"set": {"x": "nope"}})).is_err());
    assert!(show.set_vars(&json!({"set": {"x": 5}, "text": {"nama": 3}})).is_err());
    assert!(show.set_vars(&json!({})).is_err());
    sim(show.as_mut(), &mut world, 0.5);
    assert!(show.state_json()["vars"]["x"].is_null(), "nothing was applied");

    show.set_vars(&json!({"set": {"x": 20.0, "z": 9.5}, "text": {"nama": "Al-Hidayah"}})).unwrap();
    sim(show.as_mut(), &mut world, 1.0);
    let i = world.entity_idx("lencana").expect("the watch ran its macro and spawned the entity");
    assert_eq!((world.entities[i].pos.x, world.entities[i].pos.z), (20.0, 9.5), "a \"={{var.x}}\" string became a number");
    assert_eq!(world.entities[i].label.as_deref(), Some("Al-Hidayah"));
    assert_eq!(show.state_json()["vars"]["ran"], json!(1.0));
    assert_eq!(show.state_json()["texts"]["nama"], json!("Al-Hidayah"));
    assert_eq!(show.vars()["text.nama"], "Al-Hidayah");

    // the same value again is not a change; a new one is
    show.set_vars(&json!({"set": {"x": 20.0}})).unwrap();
    sim(show.as_mut(), &mut world, 0.5);
    assert_eq!(show.state_json()["vars"]["ran"], json!(1.0));
    show.set_vars(&json!({"add": {"x": 1}})).unwrap();
    sim(show.as_mut(), &mut world, 0.5);
    assert_eq!(show.state_json()["vars"]["ran"], json!(2.0));
}

#[test]
fn pause_resume_and_reset() {
    let (mut world, mut show) = load("ctl");
    show.handle_event(&mut world, ev("like", "A", "", 7, ""));
    sim(show.as_mut(), &mut world, 0.2);
    show.control(&mut world, "pause").unwrap();
    assert_eq!(show.state_json()["phase"], json!("paused"));
    show.control(&mut world, "resume").unwrap();
    assert_eq!(show.state_json()["phase"], json!("running"));
    show.control(&mut world, "reset").unwrap();
    assert_eq!(show.state_json()["vars"]["likes"], json!(0.0));
    assert!(show.control(&mut world, "run:nope").is_err());
    show.control(&mut world, "run:sapaan").unwrap();
    assert!(show.control(&mut world, "next_round").is_err(), "rounds belong to the war");
}

fn opts(dir: &std::path::Path) -> LiveOpts {
    LiveOpts {
        show: dir.join("studio.json"),
        listen: "127.0.0.1:0".into(),
        token: None,
        out_w: 360,
        out_h: 640,
        pixel: 2,
        clay: true,
        history: Some(dir.join("sessions.jsonl")),
        fps: 20,
        mjpeg: true,
        quality: 70,
        out: None,
        bitrate_k: 1000,
        emoji_dir: None,
        seconds: Some(5.0),
        realtime: true,
        log_events: false,
        tiktok: r3dsims::tiktok::TikTokOpts::default(),
        allow_no_token: false,
    }
}

#[test]
fn the_live_server_streams_a_studio_with_the_same_endpoints() {
    let dir = project("live");
    let live = Live::start(opts(&dir)).unwrap();
    let addr = live.addr().to_string();
    let runner = std::thread::spawn(move || live.run());
    let c = Client::new(&format!("http://{addr}"), None).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(600));

    let res = c.post("/webhook", r#"[{"type":"comment","user":"Ani","text":"halo"},{"type":"like","user":"Ani","count":3},{"type":"gift","user":"Ani","gift":"Rose"}]"#).unwrap();
    let res: Value = serde_json::from_str(&res).unwrap();
    assert_eq!(res["results"][0]["matched"], json!(["obrolan"]));
    assert_eq!(res["results"][2]["matched"], json!(["mawar"]));
    let hooks: Value = serde_json::from_str(&c.get("/hooks").unwrap()).unwrap();
    assert!(hooks["macros"]["balon"].is_array());

    std::thread::sleep(std::time::Duration::from_millis(900));
    let state: Value = serde_json::from_str(&c.get("/state").unwrap()).unwrap();
    assert_eq!(state["kind"], json!("studio"));
    assert_eq!(state["events_seen"], json!(3));
    assert_eq!(state["vars"]["likes"], json!(3.0));
    let (code, jpg) = c.request_bytes("GET", "/frame.jpg", None).unwrap();
    assert_eq!(code, 200);
    assert!(jpg.len() > 1000 && jpg.starts_with(&[0xFF, 0xD8]));
    let hist: Value = serde_json::from_str(&c.get("/history").unwrap()).unwrap();
    assert_eq!(hist["enabled"], json!(false), "no camps, no win/loss history");
    let (code, text) = c.request("POST", "/hooks", Some(r#"{"triggers":[{"on":"gift:x","do":"meteor"}]}"#)).unwrap();
    assert_eq!(code, 400, "{text}");

    let st = runner.join().unwrap().unwrap();
    assert!(st["time"].as_f64().unwrap() >= 5.0);
}
