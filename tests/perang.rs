use std::path::PathBuf;

use r3dsims::client::Client;
use r3dsims::live::{Live, LiveOpts};
use r3dsims::perang;
use r3dsims::war::{Event, Hooks, War};
use r3dsims::world::World;
use serde_json::{Value, json};

fn project(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("r3d_perang_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    perang::write(&dir).unwrap();
    dir
}

fn load(tag: &str) -> (World, War) {
    War::load(&project(tag).join("war.json")).expect("template war loads and validates")
}

fn sim(war: &mut War, world: &mut World, secs: f32) {
    let dt = 1.0 / 30.0;
    for _ in 0..(secs / dt) as usize {
        war.update(dt, world);
        world.update(dt);
    }
}

fn gift(name: &str) -> Event {
    Event { kind: "gift".into(), user: "Budi".into(), gift: name.into(), count: 1, diamonds: 1, text: String::new(), side: None, id: None }
}

fn hp(war: &War, side: &str) -> f64 {
    war.state_json()["sides"][side]["hp"].as_f64().unwrap()
}

fn publish(war: &mut War, world: &World, v: Value) {
    let h: Hooks = serde_json::from_value(v).unwrap();
    war.publish(world, h).expect("publish");
}

fn start(war: &mut War, world: &mut World) {
    war.control(world, "start").unwrap();
    sim(war, world, 0.5);
}

#[test]
fn a_castle_meteor_from_a_rose_hits_side_a_only() {
    let (mut world, mut war) = load("meteor");
    start(&mut war, &mut world);
    publish(
        &mut war,
        &world,
        json!({"triggers": [{"name": "rose", "on": "gift:rose", "side": "A", "do": {"type": "meteor", "target": "castle", "damage": 100, "castle_damage": 200}}]}),
    );
    let r = war.handle_event(&mut world, gift("Rose"));
    assert_eq!(r["matched"], json!(["rose"]));
    sim(&mut war, &mut world, 3.0);
    assert_eq!(hp(&war, "A"), 800.0, "meteor should take 200 off castle A");
    assert_eq!(hp(&war, "B"), 1000.0);
}

#[test]
fn platform_spellings_and_aliases_reach_the_same_trigger() {
    let (mut world, mut war) = load("alias");
    start(&mut war, &mut world);
    let ev = Event::from_json(&json!({"event": "gift", "uniqueId": "budi", "nickname": "Budi!", "giftName": "Mawar", "repeatCount": 3, "diamondCount": 1, "msgId": "x1"})).unwrap();
    assert_eq!((ev.user.as_str(), ev.gift.as_str(), ev.count), ("Budi!", "Mawar", 3));
    let r = war.handle_event(&mut world, ev.clone());
    assert_eq!(r["matched"], json!(["rose"]), "alias mawar -> rose: {r}");
    let again = war.handle_event(&mut world, ev);
    assert_eq!(again["duplicate"], json!(true), "same connector id must not fire twice");
}

#[test]
fn rounds_end_with_a_winner_and_the_match_with_a_champion() {
    let (mut world, mut war) = load("match");
    publish(&mut war, &world, json!({"triggers": [{"name": "doom", "on": "gift:doom", "side": "B", "do": {"type": "meteor", "count": 10, "target": "castle", "castle_damage": 150, "gap": 0.1}}]}));
    let mut champion = Value::Null;
    for _ in 0..40 {
        sim(&mut war, &mut world, 10.0);
        let st = war.state_json();
        if st["phase"] == "match_end" {
            champion = st["match_winner"].clone();
            break;
        }
        if st["phase"] == "fighting" && st["round_secs_left"].as_f64().unwrap() > 60.0 {
            war.handle_event(&mut world, gift("doom"));
        }
    }
    assert_eq!(champion, json!("A"), "B's castle was destroyed in two rounds");
    let st = war.state_json();
    assert_eq!(st["sides"]["A"]["round_wins"], json!(2));
}

#[test]
fn endless_mode_keeps_going_until_stopped() {
    let (mut world, war) = load("endless");
    let mut def = war.def.clone();
    def.match_.mode = "endless".into();
    def.match_.round_secs = 12.0;
    def.match_.lobby_secs = 1.0;
    def.match_.intermission = 2.0;
    let mut war = War::new(def, &mut world).unwrap();
    sim(&mut war, &mut world, 60.0);
    let st = war.state_json();
    assert!(st["round"].as_u64().unwrap() >= 3, "endless should have moved past round 3: {st}");
    assert_ne!(st["phase"], json!("match_end"));
    war.control(&mut world, "stop").unwrap();
    sim(&mut war, &mut world, 20.0);
    assert_eq!(war.state_json()["phase"], json!("match_end"));
}

#[test]
fn publish_is_atomic_and_reports_every_problem() {
    let (mut world, mut war) = load("atomic");
    let before = war.hooks_json();
    let h: Hooks = serde_json::from_value(json!({
        "triggers": [{"name": "ok", "on": "gift:ok", "side": "A", "do": "meteor"}, {"name": "bad", "on": "gift:bad", "side": "A", "do": "no_such_action"}],
        "macros": {"m": [{"do": "call", "macro": "ghost"}]}
    }))
    .unwrap();
    let err = war.publish(&world, h).unwrap_err().to_string();
    assert!(err.contains("no_such_action") && err.contains("ghost"), "{err}");
    assert_eq!(war.hooks_json(), before, "a failed publish must change nothing");
    let _ = &mut world;
}

#[test]
fn macros_wait_branch_and_count() {
    let (mut world, mut war) = load("macro");
    start(&mut war, &mut world);
    publish(
        &mut war,
        &world,
        json!({
            "macros": {"m1": [
                {"do": "set", "var": "x", "value": 3},
                {"do": "wait", "secs": 1.0},
                {"do": "add", "var": "x", "by": 2},
                {"do": "if", "cond": {"var": {"name": "x", "gte": 5}}, "then": [{"do": "set", "var": "big", "value": 1}], "else": [{"do": "set", "var": "big", "value": -1}]}
            ]},
            "triggers": [{"name": "m", "on": "gift:m", "macro": "m1", "side": "A"}]
        }),
    );
    war.handle_event(&mut world, gift("m"));
    sim(&mut war, &mut world, 0.3);
    assert_eq!(war.get_var("x"), 3.0);
    sim(&mut war, &mut world, 1.2);
    assert_eq!((war.get_var("x"), war.get_var("big")), (5.0, 1.0));
}

#[test]
fn bots_play_when_the_stream_is_idle() {
    let (mut world, mut war) = load("bot");
    start(&mut war, &mut world);
    publish(&mut war, &world, json!({
        "macros": {"tick": [{"do": "add", "var": "ticks"}]},
        "bots": [{"name": "t", "macro": "tick", "when_idle": 2.0}]
    }));
    sim(&mut war, &mut world, 7.5);
    let idle_ticks = war.get_var("ticks");
    assert!(idle_ticks >= 2.0, "bot should have run while idle, ran {idle_ticks}");
    war.handle_event(&mut world, gift("anything"));
    let before = war.get_var("ticks");
    sim(&mut war, &mut world, 1.5);
    assert_eq!(war.get_var("ticks"), before, "a webhook event resets the idle clock");
}

#[test]
fn overlay_commands_add_patch_and_expire() {
    let (mut world, mut war) = load("overlay");
    let r = war.overlay_op(&json!({"add": [{"id": "hello", "type": "text", "text": "HI {user}", "ttl": 1.0}, {"id": "fire", "type": "emoji", "emoji": ":fire:"}]})).unwrap();
    assert_eq!(r["added"], json!(["hello", "fire"]));
    war.overlay_op(&json!({"patch": {"hello": {"text": "BYE"}}})).unwrap();
    sim(&mut war, &mut world, 1.3);
    let st = war.state_json();
    assert_eq!(st["overlay_elements"], json!(["fire"]), "ttl element is gone, the other stays");
    assert!(war.overlay_op(&json!({"patch": {"nope": {"text": "x"}}})).is_err());
}

fn http(addr: &str, tok: Option<&str>) -> Client {
    Client::new(&format!("http://{addr}"), tok.map(str::to_string)).unwrap()
}

#[test]
fn live_stream_takes_webhooks_publishes_hooks_and_serves_frames() {
    let dir = project("live");
    let live = Live::start(LiveOpts {
        war: dir.join("war.json"),
        listen: "127.0.0.1:0".into(),
        token: Some("secret".into()),
        out_w: 480,
        out_h: 270,
        pixel: 2,
        clay: true,
        fps: 20,
        mjpeg: true,
        quality: 70,
        out: None,
        bitrate_k: 1000,
        emoji_dir: None,
        seconds: Some(7.0),
        realtime: true,
        log_events: false,
    })
    .unwrap();
    let addr = live.addr().to_string();
    let runner = std::thread::spawn(move || live.run());

    let anon = http(&addr, None);
    let me = http(&addr, Some("secret"));
    assert_eq!(anon.get("/health").unwrap(), "ok");

    let (code, _) = anon.request("POST", "/webhook", Some(r#"{"type":"gift","gift":"rose"}"#)).unwrap();
    assert_eq!(code, 401, "a token is required for webhooks");

    std::thread::sleep(std::time::Duration::from_millis(600));
    let out = me.post("/hooks", &json!({"triggers": [{"name": "t1", "on": "gift:zap", "side": "B", "do": "lightning"}]}).to_string()).unwrap();
    assert!(serde_json::from_str::<Value>(&out).unwrap()["ok"].as_bool().unwrap());
    let (code, text) = me.request("POST", "/hooks", Some(r#"{"triggers":[{"on":"gift:x","side":"A","do":"nope"}]}"#)).unwrap();
    assert_eq!(code, 400);
    assert!(text.contains("nope"), "{text}");

    me.post("/control", r#"{"op":"start"}"#).unwrap();
    let res = me.post("/webhook", r#"[{"type":"gift","user":"Ani","gift":"Zap","count":2},{"type":"gift","user":"Ani","gift":"Rose"}]"#).unwrap();
    let res: Value = serde_json::from_str(&res).unwrap();
    assert_eq!(res["results"][0]["matched"], json!(["t1"]));
    assert_eq!(res["results"][1]["matched"], json!(["rose"]));
    me.post("/overlay", r#"{"type":"emoji","emoji":"🌹","at":[0.5,0.4],"size":0.2}"#).unwrap();

    std::thread::sleep(std::time::Duration::from_millis(900));
    let state: Value = serde_json::from_str(&anon.get("/state").unwrap()).unwrap();
    assert_eq!(state["phase"], json!("fighting"));
    assert_eq!(state["events_seen"], json!(2));
    assert!(!state["overlay_elements"].as_array().unwrap().is_empty());
    let (code, jpg) = anon.request_bytes("GET", "/frame.jpg", None).unwrap();
    assert_eq!(code, 200);
    assert!(jpg.len() > 1000 && jpg.starts_with(&[0xFF, 0xD8]), "frame.jpg is {} bytes, starts {:?}", jpg.len(), &jpg[..jpg.len().min(4)]);

    let st = runner.join().unwrap().unwrap();
    assert!(st["time"].as_f64().unwrap() >= 7.0);
}
