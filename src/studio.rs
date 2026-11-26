//! Studio: a show that is not a game. Take any scene (a village, a harbour, a room, your own) and let the stream's viewers
//! act on it: chat becomes speech bubbles, gifts become effects, likes fill a goal bar, a bot keeps it alive when nobody
//! writes. There are no sides, rounds or winners; what happens is whatever the macros say.
//!
//! `studio.json`:
//!
//! ```json
//! {
//!   "title": "KAMPUNG LIVE",
//!   "scene": "scene.json",
//!   "vars": {"likes": 0},
//!   "goal": {"var": "likes", "target": 200, "label": "TARGET LIKE", "on_reach": "party"},
//!   "triggers": [
//!     {"on": "comment", "macro": "bubble", "cooldown": 1.5},
//!     {"on": "like", "macro": "count_likes"},
//!     {"on": "gift:rose", "macro": "rose", "label": "ROSE = TARI MAWAR", "icon": ":rose:", "stop": true}
//!   ],
//!   "macros": {
//!     "bubble": [{"do": "step", "step": {"do": "say", "actor": "ani", "text": "{user}: {text}", "secs": 4}}],
//!     "count_likes": [{"do": "add", "var": "likes", "from": "count"}]
//!   },
//!   "bots": [{"name": "greet", "macro": "greet", "when_idle": 20}]
//! }
//! ```
//!
//! Macros use the same steps as a war's (`wait`, `repeat`, `random`, `if`, `overlay`, `step` for any story or camera
//! command, `set`, `add`, `call`, `announce`), minus what needs a game: `act` and the conditions about sides and rounds. In
//! the text of a trigger, an overlay or a `step`, `{user} {text} {gift} {count} {diamonds}` name the viewer event that
//! started it and `{var.NAME}` a counter.

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::path::Path;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::hud::{self, HudItem, MenuItem};
use crate::overlay::{Element, Overlay, Vars, interp_value, interpolate};
use crate::script::{Cond, Ctx, Host, MStep, Runner};
use crate::show::Show;
use crate::war::{ActionRef, Bot, Event, Hooks, Trigger, norm, pattern_matches, trigger_name};
use crate::world::World;

fn yes() -> bool {
    true
}
fn d_title() -> String {
    "LIVE".into()
}
fn d_goal_label() -> String {
    "TARGET".into()
}
fn d_goal_color() -> String {
    "#f2a31b".into()
}

/// A counter with a bar: when `var` reaches `target` the `on_reach` macro runs and, with `repeat`, the target grows by
/// itself again.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Goal {
    pub var: String,
    pub target: f64,
    #[serde(default = "d_goal_label")]
    pub label: String,
    #[serde(default = "d_goal_color")]
    pub color: String,
    #[serde(default = "yes")]
    pub repeat: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_reach: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct StudioDef {
    #[serde(default = "d_title")]
    pub title: String,
    /// scene file, relative to this file
    pub scene: String,
    #[serde(default = "yes")]
    pub hud: bool,
    /// a story scene to play at the start (a scene with its own looping story starts it by itself)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub play: Option<String>,
    #[serde(default)]
    pub vars: BTreeMap<String, f64>,
    #[serde(default)]
    pub triggers: Vec<Trigger>,
    #[serde(default)]
    pub macros: BTreeMap<String, Vec<MStep>>,
    #[serde(default)]
    pub bots: Vec<Bot>,
    #[serde(default)]
    pub gift_aliases: BTreeMap<String, String>,
    #[serde(default)]
    pub overlay: Vec<Element>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub goal: Option<Goal>,
}

struct Banner {
    text: String,
    color: String,
    left: f32,
    total: f32,
}

pub struct Studio {
    def: StudioDef,
    overlay: Overlay,
    runners: Vec<Runner>,
    vars: BTreeMap<String, f64>,
    ticker: VecDeque<(String, String)>,
    banner: Option<Banner>,
    board: HashMap<String, u32>,
    seen: VecDeque<String>,
    cooldowns: HashMap<String, f32>,
    bot_t: HashMap<String, f32>,
    idle: f32,
    time: f32,
    paused: bool,
    events_seen: u64,
    goal_target: f64,
    goal_hits: u32,
    rng: u64,
    warned: Vec<String>,
}

fn num(n: f64) -> String {
    if n.fract() == 0.0 { format!("{}", n as i64) } else { format!("{n:.1}") }
}

impl Studio {
    /// Reads `studio.json` and the scene it names.
    pub fn load(path: &Path) -> Result<(World, Studio)> {
        let txt = std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
        let def: StudioDef = serde_json::from_str(&txt).with_context(|| format!("parse {}", path.display()))?;
        let base = path.parent().map(Path::to_path_buf).unwrap_or_default();
        let mut world = World::load(&base.join(&def.scene)).with_context(|| format!("scene {}", def.scene))?;
        let studio = Studio::new(def, &mut world)?;
        Ok((world, studio))
    }

    pub fn new(mut def: StudioDef, world: &mut World) -> Result<Studio> {
        validate(&def, world)?;
        for t in def.triggers.iter_mut() {
            t.name = trigger_name(t);
        }
        let goal_target = def.goal.as_ref().map_or(0.0, |g| g.target);
        let mut s = Studio {
            overlay: Overlay::new(),
            runners: vec![],
            vars: def.vars.clone(),
            ticker: VecDeque::new(),
            banner: None,
            board: HashMap::new(),
            seen: VecDeque::new(),
            cooldowns: HashMap::new(),
            bot_t: HashMap::new(),
            idle: 0.0,
            time: 0.0,
            paused: false,
            events_seen: 0,
            goal_target,
            goal_hits: 0,
            rng: 0x9E37_79B9_7F4A_7C15,
            warned: vec![],
            def,
        };
        s.start_scene(world)?;
        Ok(s)
    }

    /// The first overlay elements and the story scene named by `play`.
    fn start_scene(&mut self, world: &mut World) -> Result<()> {
        for e in self.def.overlay.clone() {
            self.overlay.add(e);
        }
        if let Some(p) = self.def.play.clone() {
            world.run_json(&json!({"do": "play_scene", "name": p}).to_string()).with_context(|| format!("play '{p}'"))?;
        }
        Ok(())
    }

    fn next_rand(&mut self) -> f32 {
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.rng = x;
        ((x >> 40) as f32) / (1u64 << 24) as f32
    }

    fn push_ticker(&mut self, text: String, color: &str) {
        self.ticker.push_front((text, color.to_string()));
        self.ticker.truncate(5);
    }

    fn start_macro(&mut self, name: &str, ctx: Ctx) {
        match self.def.macros.get(name).cloned() {
            Some(steps) => {
                if self.runners.len() < 64 {
                    self.runners.push(Runner::new(name, steps, ctx));
                }
            }
            None => self.note(format!("unknown macro '{name}'")),
        }
    }

    fn note(&mut self, msg: String) {
        if !self.warned.contains(&msg) && self.warned.len() < 50 {
            eprintln!("[warn] studio: {msg}");
            self.warned.push(msg);
        }
    }

    fn norm_gift(&self, g: &str) -> String {
        let n = norm(g);
        for (k, v) in &self.def.gift_aliases {
            if norm(k) == n {
                return norm(v);
            }
        }
        n
    }

    /// The `{placeholders}` a macro, trigger or overlay can use for the event that started it.
    fn ctx_vars(&self, ctx: &Ctx) -> Vars {
        let mut v = self.vars();
        if let Some(e) = &ctx.event {
            v.insert("user".into(), e.user.chars().take(40).collect());
            v.insert("gift".into(), e.gift.clone());
            v.insert("count".into(), e.count.to_string());
            v.insert("diamonds".into(), e.diamonds.to_string());
            v.insert("text".into(), e.text.chars().take(70).collect());
            v.insert("kind".into(), e.kind.clone());
        }
        v
    }

    fn process_event(&mut self, world: &mut World, ev: Event, external: bool) -> Value {
        let _ = world;
        if let Some(id) = &ev.id {
            if self.seen.contains(id) {
                return json!({"duplicate": true});
            }
            self.seen.push_back(id.clone());
            if self.seen.len() > 2048 {
                self.seen.pop_front();
            }
        }
        if external {
            self.idle = 0.0;
            self.events_seen += 1;
        }
        let points = if ev.kind == "gift" { (ev.diamonds.max(1) as u64 * ev.count as u64).min(1_000_000) as u32 } else { 1 };
        *self.board.entry(ev.user.clone()).or_insert(0) += points;
        let gift_norm = self.norm_gift(&ev.gift);
        let mut matched: Vec<String> = vec![];
        let triggers = self.def.triggers.clone();
        for t in &triggers {
            if !t.on.list().iter().any(|p| pattern_matches(p, &ev, &gift_norm)) {
                continue;
            }
            if ev.count < t.min_count || ev.diamonds < t.min_diamonds {
                continue;
            }
            if self.cooldowns.get(&t.name).copied().unwrap_or(0.0) > 0.0 {
                matched.push(format!("{} (cooldown)", t.name));
                continue;
            }
            if t.cooldown > 0.0 {
                self.cooldowns.insert(t.name.clone(), t.cooldown);
            }
            let ctx = Ctx { event: Some(ev.clone()), side: None };
            if let Some(m) = &t.macro_ {
                self.start_macro(m, ctx.clone());
            }
            let mut vars = self.ctx_vars(&ctx);
            vars.insert("label".into(), t.label.clone().unwrap_or_default());
            vars.insert("icon".into(), t.icon.clone().unwrap_or_default());
            for e in &t.overlay {
                if let Err(err) = self.overlay.add_templated(e, &vars) {
                    self.note(format!("trigger '{}': overlay element: {err}", t.name));
                }
            }
            // chat shows as a bubble and likes as the goal bar; the ticker is for gifts, follows and the like, or whatever a trigger announces
            if !matches!(ev.kind.as_str(), "comment" | "like") || t.announce.is_some() || t.label.is_some() {
                let line = match (&t.announce, &t.label) {
                    (Some(a), _) => interpolate(a, &vars),
                    (None, Some(_)) => interpolate("{icon} {user}: {label}", &vars),
                    (None, None) => interpolate("{user} {gift} x{count}", &vars),
                };
                self.push_ticker(line.trim().to_string(), "#ffe27a");
            }
            matched.push(t.name.clone());
            if t.stop {
                break;
            }
        }
        json!({"matched": matched, "phase": self.phase_name()})
    }

    fn phase_name(&self) -> &'static str {
        if self.paused { "paused" } else { "running" }
    }

    fn run_runners(&mut self, dt: f32, world: &mut World) {
        if self.runners.is_empty() {
            return;
        }
        let mut rs = std::mem::take(&mut self.runners);
        for r in rs.iter_mut() {
            r.tick(dt, self, world);
        }
        rs.retain(|r| !r.done);
        rs.append(&mut self.runners);
        self.runners = rs;
        self.runners.truncate(64);
    }

    fn run_bots(&mut self, dt: f32) {
        for b in self.def.bots.clone() {
            let t = self.bot_t.entry(b.name.clone()).or_insert(0.0);
            *t += dt;
            let t = *t;
            let due = b.every.is_some_and(|e| t >= e) || b.when_idle.is_some_and(|i| self.idle >= i && t >= i);
            if !due {
                continue;
            }
            if let Some(c) = &b.when {
                if !self.eval(c, &Ctx::default()) {
                    continue;
                }
            }
            let name = format!("bot:{}", b.name);
            if self.runners.iter().any(|r| r.name == name) {
                continue;
            }
            self.bot_t.insert(b.name.clone(), 0.0);
            if let Some(steps) = self.def.macros.get(&b.macro_).cloned() {
                self.runners.push(Runner::new(&name, steps, Ctx::default()));
            }
        }
    }

    fn check_goal(&mut self) {
        let Some(g) = self.def.goal.clone() else { return };
        if !g.repeat && self.goal_hits > 0 {
            return;
        }
        let value = self.get_var(&g.var);
        if value < self.goal_target {
            return;
        }
        self.goal_hits += 1;
        if g.repeat {
            while value >= self.goal_target {
                self.goal_target += g.target.max(1.0);
            }
        }
        match &g.on_reach {
            Some(m) => self.start_macro(m, Ctx::default()),
            None => self.banner = Some(Banner { text: format!("{} TERCAPAI!", g.label), color: g.color.clone(), left: 4.0, total: 4.0 }),
        }
        self.push_ticker(format!("{} {} TERCAPAI", g.label, num(value)), &g.color);
    }

    fn reset(&mut self, world: &mut World) -> Result<()> {
        self.vars = self.def.vars.clone();
        self.runners.clear();
        self.overlay.clear();
        self.ticker.clear();
        self.banner = None;
        self.board.clear();
        self.cooldowns.clear();
        self.bot_t.clear();
        self.goal_target = self.def.goal.as_ref().map_or(0.0, |g| g.target);
        self.goal_hits = 0;
        self.start_scene(world)
    }

    fn menu(&self) -> Vec<MenuItem> {
        let mut v = vec![];
        for t in &self.def.triggers {
            let show = t.legend.unwrap_or(t.label.is_some());
            if let (true, Some(l)) = (show, &t.label) {
                v.push((t.icon.clone().unwrap_or_default(), l.clone(), "#ffffff".to_string()));
            }
        }
        v
    }

    fn top(&self, n: usize) -> Vec<(&String, &u32)> {
        let mut top: Vec<(&String, &u32)> = self.board.iter().collect();
        top.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
        top.truncate(n);
        top
    }
}

impl Host for Studio {
    fn eval(&mut self, c: &Cond, ctx: &Ctx) -> bool {
        match c {
            Cond::Var { name, gte, lte, eq } => {
                let v = self.get_var(name);
                gte.is_none_or(|x| v >= x) && lte.is_none_or(|x| v <= x) && eq.is_none_or(|x| (v - x).abs() < 1e-9)
            }
            Cond::Chance(p) => self.next_rand() < *p,
            Cond::IdleSecsGt(s) => self.idle > *s,
            Cond::Not(c) => !self.eval(c, ctx),
            Cond::All(v) => v.iter().all(|c| self.eval(c, ctx)),
            Cond::Any(v) => v.iter().any(|c| self.eval(c, ctx)),
            // refused when the file is loaded: these need a game
            _ => false,
        }
    }

    fn rand(&mut self) -> f32 {
        self.next_rand()
    }

    fn act(&mut self, _world: &mut World, _action: &ActionRef, _side: Option<&str>, _ctx: &Ctx) {
        self.note("`act` needs a war; a studio has no game actions".into());
    }

    fn inject(&mut self, world: &mut World, ev: Event) {
        self.process_event(world, ev, false);
    }

    fn announce(&mut self, _world: &mut World, text: String, secs: f32, color: Option<String>, ctx: &Ctx) {
        let vars = self.ctx_vars(ctx);
        let secs = secs.max(0.5);
        self.banner = Some(Banner { text: interpolate(&text, &vars), color: color.unwrap_or_else(|| "#ffffff".into()), left: secs, total: secs });
    }

    fn overlay(&mut self, _world: &mut World, add: Vec<Element>, remove: Vec<String>, clear: bool, ctx: &Ctx) {
        let vars = self.ctx_vars(ctx);
        if clear {
            self.overlay.clear();
        }
        for id in remove {
            self.overlay.remove(&id);
        }
        for e in add {
            if let Err(err) = self.overlay.add_templated(&e, &vars) {
                self.note(format!("overlay element: {err}"));
            }
        }
    }

    fn world_step(&mut self, world: &mut World, step: &Value, ctx: &Ctx) {
        let mut step = step.clone();
        interp_value(&mut step, &self.ctx_vars(ctx));
        if let Err(e) = world.run_json(&step.to_string()) {
            self.note(format!("world step: {e:#}"));
        }
    }

    fn macro_steps(&self, name: &str) -> Option<Vec<MStep>> {
        self.def.macros.get(name).cloned()
    }

    fn get_var(&self, name: &str) -> f64 {
        self.vars.get(name).copied().unwrap_or(0.0)
    }

    fn set_var(&mut self, name: &str, v: f64) {
        self.vars.insert(name.to_string(), v);
    }

    fn warn(&mut self, msg: String) {
        self.note(msg);
    }
}

impl Show for Studio {
    fn kind(&self) -> &'static str {
        "studio"
    }

    fn title(&self) -> String {
        self.def.title.clone()
    }

    fn update(&mut self, dt: f32, world: &mut World) {
        self.time += dt;
        self.idle += dt;
        self.overlay.tick(dt);
        self.cooldowns.retain(|_, v| {
            *v -= dt;
            *v > 0.0
        });
        if let Some(b) = &mut self.banner {
            b.left -= dt;
            if b.left <= 0.0 {
                self.banner = None;
            }
        }
        if self.paused {
            return;
        }
        self.run_runners(dt, world);
        self.run_bots(dt);
        self.check_goal();
    }

    fn handle_event(&mut self, world: &mut World, ev: Event) -> Value {
        self.process_event(world, ev, true)
    }

    fn control(&mut self, world: &mut World, op: &str) -> Result<Value> {
        match op {
            "pause" => self.paused = true,
            "resume" | "start" => self.paused = false,
            "reset" => self.reset(world)?,
            other if other.starts_with("run:") => {
                let name = other[4..].split(':').next().unwrap_or_default().to_string();
                if !self.def.macros.contains_key(&name) {
                    bail!("no macro named '{name}'");
                }
                self.start_macro(&name, Ctx::default());
            }
            other => bail!("unknown control '{other}' for a studio (pause, resume, reset, run:NAME)"),
        }
        Ok(json!({"phase": self.phase_name(), "paused": self.paused}))
    }

    fn publish(&mut self, world: &World, h: Hooks) -> Result<Value> {
        if !h.actions.is_empty() || !h.units.is_empty() {
            bail!("a studio has no actions or units: publish triggers, macros and bots");
        }
        let mut def = self.def.clone();
        let mut added = vec![];
        let mut removed = vec![];
        if h.replace {
            if !h.triggers.is_empty() {
                def.triggers.clear();
            }
            if !h.macros.is_empty() {
                def.macros.clear();
            }
            if !h.bots.is_empty() {
                def.bots.clear();
            }
        }
        for name in &h.remove {
            let before = def.triggers.len() + def.macros.len() + def.bots.len();
            def.triggers.retain(|t| &trigger_name(t) != name);
            def.macros.remove(name);
            def.bots.retain(|b| &b.name != name);
            if def.triggers.len() + def.macros.len() + def.bots.len() < before {
                removed.push(name.clone());
            }
        }
        for (k, v) in h.macros {
            def.macros.insert(k.clone(), v);
            added.push(format!("macro:{k}"));
        }
        for mut t in h.triggers {
            t.name = trigger_name(&t);
            def.triggers.retain(|x| x.name != t.name);
            added.push(format!("trigger:{}", t.name));
            def.triggers.push(t);
        }
        for b in h.bots {
            def.bots.retain(|x| x.name != b.name);
            added.push(format!("bot:{}", b.name));
            def.bots.push(b);
        }
        for (k, v) in h.gift_aliases {
            def.gift_aliases.insert(k, v);
        }
        for t in def.triggers.iter_mut() {
            t.name = trigger_name(t);
        }
        validate(&def, world)?;
        self.def = def;
        Ok(json!({"added": added, "removed": removed, "triggers": self.def.triggers.len(), "macros": self.def.macros.len(), "bots": self.def.bots.len()}))
    }

    fn overlay_op(&mut self, body: &Value) -> Result<Value> {
        let vars = self.vars();
        self.overlay.apply_op(body, &vars)
    }

    fn overlay(&self) -> &Overlay {
        &self.overlay
    }

    fn vars(&self) -> Vars {
        let mut v = Vars::new();
        v.insert("title".into(), self.def.title.clone());
        let secs = self.time as u64;
        v.insert("time".into(), format!("{}:{:02}", secs / 60, secs % 60));
        v.insert("events".into(), self.events_seen.to_string());
        for i in 0..5 {
            let t = self.ticker.get(i);
            v.insert(format!("ticker.{}", i + 1), t.map(|t| t.0.clone()).unwrap_or_default());
            v.insert(format!("tickercolor.{}", i + 1), t.map(|t| t.1.clone()).unwrap_or_else(|| "#ffffff".into()));
        }
        let top = self.top(3);
        for i in 0..3 {
            v.insert(format!("top{}", i + 1), top.get(i).map(|(u, p)| format!("{u} {p}")).unwrap_or_default());
        }
        if let Some(b) = &self.banner {
            v.insert("banner".into(), b.text.clone());
        }
        for (k, val) in &self.vars {
            v.insert(format!("var.{k}"), num(*val));
        }
        if let Some(g) = &self.def.goal {
            let value = self.get_var(&g.var);
            let step = g.target.max(1.0);
            let base = if g.repeat { self.goal_target - step } else { 0.0 };
            let span = if g.repeat { step } else { self.goal_target.max(1.0) };
            v.insert("goal_value".into(), num(value));
            v.insert("goal_target".into(), num(self.goal_target));
            v.insert("goal_label".into(), g.label.clone());
            v.insert("goal_color".into(), g.color.clone());
            v.insert("goal_frac".into(), format!("{:.4}", ((value - base) / span).clamp(0.0, 1.0)));
        }
        v
    }

    fn hud(&self, w: usize, h: usize) -> Vec<HudItem> {
        if !self.def.hud {
            return vec![];
        }
        let banner = self.banner.as_ref().map(|b| (b.text.clone(), b.color.clone(), b.total - b.left, b.left));
        hud::build_studio(&self.vars(), &self.menu(), banner, w, h)
    }

    fn state_json(&self) -> Value {
        let vars: BTreeMap<&String, f64> = self.vars.iter().map(|(k, v)| (k, *v)).collect();
        json!({
            "kind": "studio",
            "mode": "studio",
            "title": self.def.title,
            "phase": self.phase_name(),
            "paused": self.paused,
            "time": (self.time * 10.0).round() / 10.0,
            "idle_secs": (self.idle * 10.0).round() / 10.0,
            "events_seen": self.events_seen,
            "vars": vars,
            "goal": self.def.goal.as_ref().map(|g| json!({"var": g.var, "label": g.label, "value": self.get_var(&g.var), "target": self.goal_target, "reached": self.goal_hits})),
            "sides": {},
            "autopilot": false,
            "cinematic": false,
            "biome": Value::Null,
            "running_macros": self.runners.iter().map(|r| r.name.clone()).collect::<Vec<_>>(),
            "overlay_elements": self.overlay.list().iter().map(|e| e.id.clone()).collect::<Vec<_>>(),
            "ticker": self.ticker.iter().map(|t| t.0.clone()).collect::<Vec<_>>(),
            "top": self.top(5).iter().map(|(u, p)| json!({"user": u, "points": p})).collect::<Vec<_>>(),
        })
    }

    fn hooks_json(&self) -> Value {
        json!({
            "triggers": self.def.triggers,
            "actions": {},
            "macros": self.def.macros,
            "bots": self.def.bots,
            "units": {},
            "gift_aliases": self.def.gift_aliases,
            "biomes": [],
        })
    }

    fn take_log(&mut self) -> Vec<Value> {
        vec![]
    }

    fn camps(&self) -> Option<[(String, String); 2]> {
        None
    }
}

// ---------------------------------------------------------------- checks

fn cond_name(c: &Cond) -> String {
    let d = format!("{c:?}");
    d.split(['(', ' ', '{']).next().unwrap_or("").to_string()
}

fn check_cond(c: &Cond, ctx: &str, errs: &mut Vec<String>) {
    match c {
        Cond::Var { .. } | Cond::Chance(_) | Cond::IdleSecsGt(_) => {}
        Cond::Not(c) => check_cond(c, ctx, errs),
        Cond::All(v) | Cond::Any(v) => v.iter().for_each(|c| check_cond(c, ctx, errs)),
        other => errs.push(format!("{ctx}: condition {} is about a game; a studio has var, chance, idle_secs_gt, not, all, any", cond_name(other))),
    }
}

/// Every `actor` named in a story step must exist in the scene (unless it is a `{placeholder}`).
fn check_actors(v: &Value, world: &World, ctx: &str, errs: &mut Vec<String>) {
    match v {
        Value::Object(o) => {
            if let Some(Value::String(a)) = o.get("actor") {
                if !a.contains('{') && world.entity_idx(a).is_none() {
                    errs.push(format!("{ctx}: unknown actor '{a}' (not an entity of the scene)"));
                }
            }
            o.values().for_each(|x| check_actors(x, world, ctx, errs));
        }
        Value::Array(a) => a.iter().for_each(|x| check_actors(x, world, ctx, errs)),
        _ => {}
    }
}

fn check_steps(steps: &[MStep], def: &StudioDef, world: &World, ctx: &str, errs: &mut Vec<String>) {
    for s in steps {
        match s {
            MStep::Act { .. } => errs.push(format!("{ctx}: `act` fires a game action; a studio uses overlay, step, announce, set and add")),
            MStep::Call { name } => {
                if !def.macros.contains_key(name) {
                    errs.push(format!("{ctx}: calls unknown macro '{name}'"));
                }
            }
            MStep::Repeat { steps, .. } | MStep::Forever { steps } => check_steps(steps, def, world, ctx, errs),
            MStep::Random { choices } => choices.iter().for_each(|c| check_steps(&c.steps, def, world, ctx, errs)),
            MStep::If { cond, then, otherwise } => {
                check_cond(cond, ctx, errs);
                check_steps(then, def, world, ctx, errs);
                check_steps(otherwise, def, world, ctx, errs);
            }
            MStep::Until { cond, .. } => check_cond(cond, ctx, errs),
            MStep::Step { step } => {
                let ok = serde_json::from_value::<crate::story::Step>(step.clone()).is_ok() || serde_json::from_value::<crate::camera::CameraCmd>(step.clone()).is_ok();
                if !ok {
                    errs.push(format!("{ctx}: {step} is not a story or camera step"));
                }
                check_actors(step, world, ctx, errs);
            }
            _ => {}
        }
    }
}

fn validate(def: &StudioDef, world: &World) -> Result<()> {
    let mut errs: Vec<String> = vec![];
    for t in &def.triggers {
        let n = trigger_name(t);
        if t.action.is_some() {
            errs.push(format!("trigger '{n}': a studio has no game actions (\"do\"); use a macro"));
        }
        if let Some(m) = &t.macro_ {
            if !def.macros.contains_key(m) {
                errs.push(format!("trigger '{n}': unknown macro '{m}'"));
            }
        }
    }
    for (name, steps) in &def.macros {
        check_steps(steps, def, world, &format!("macro '{name}'"), &mut errs);
    }
    for b in &def.bots {
        if !def.macros.contains_key(&b.macro_) {
            errs.push(format!("bot '{}': unknown macro '{}'", b.name, b.macro_));
        }
        if let Some(c) = &b.when {
            check_cond(c, &format!("bot '{}'", b.name), &mut errs);
        }
    }
    if let Some(g) = &def.goal {
        if g.target <= 0.0 {
            errs.push("goal: target must be above 0".into());
        }
        if let Some(m) = &g.on_reach {
            if !def.macros.contains_key(m) {
                errs.push(format!("goal: unknown macro '{m}'"));
            }
        }
    }
    if errs.is_empty() { Ok(()) } else { bail!("{}", errs.join("\n")) }
}

// ---------------------------------------------------------------- template

/// Writes `scene.json` + assets (the kampung diorama) and a `studio.json` that makes it a live show: chat becomes speech
/// bubbles of the villagers, gifts become effects, likes fill a goal bar, a bot greets when nobody writes.
pub fn write_template(dir: &Path) -> Result<()> {
    crate::templates::write_kampung(dir)?;
    std::fs::write(dir.join("studio.json"), TEMPLATE).with_context(|| format!("write {}", dir.join("studio.json").display()))?;
    Ok(())
}

const TEMPLATE: &str = r##"{
  "title": "KAMPUNG LIVE",
  "scene": "scene.json",
  "vars": {"likes": 0, "gifts": 0, "chats": 0},
  "goal": {"var": "likes", "target": 200, "label": "TARGET LIKE", "color": "#ff5c8a", "on_reach": "target_tercapai"},
  "gift_aliases": {"mawar": "rose", "donat": "doughnut", "galaksi": "galaxy"},
  "triggers": [
    {"name": "obrolan", "on": "comment", "macro": "balon", "cooldown": 1.2},
    {"name": "like", "on": "like", "macro": "hitung_like"},
    {"name": "ikut", "on": "follow", "macro": "sambut", "announce": "{user} IKUT KAMPUNG!"},
    {"name": "mawar", "on": "gift:rose", "macro": "tari_mawar", "label": "ROSE = TARI MAWAR", "icon": ":rose:", "stop": true},
    {"name": "donat", "on": "gift:doughnut", "macro": "traktir", "label": "DONAT = TRAKTIR WARGA", "icon": ":party:", "stop": true},
    {"name": "galaksi", "on": "gift:galaxy", "macro": "malam_bintang", "label": "GALAXY = MALAM BINTANG", "icon": ":glowing_star:", "cooldown": 25, "stop": true},
    {"name": "hadiah", "on": "gift:*", "macro": "terima_kasih"}
  ],
  "macros": {
    "balon": [
      {"do": "add", "var": "chats"},
      {"do": "random", "choices": [
        {"steps": [{"do": "step", "step": {"do": "say", "actor": "ani", "text": "{user}: {text}", "secs": 4}}]},
        {"steps": [{"do": "step", "step": {"do": "say", "actor": "budi", "text": "{user}: {text}", "secs": 4}}]},
        {"steps": [{"do": "step", "step": {"do": "say", "actor": "citra", "text": "{user}: {text}", "secs": 4}}]},
        {"steps": [{"do": "step", "step": {"do": "say", "actor": "dodi", "text": "{user}: {text}", "secs": 4}}]}
      ]}
    ],
    "hitung_like": [{"do": "add", "var": "likes", "from": "count"}],
    "sambut": [
      {"do": "step", "step": {"do": "play", "actor": "ani", "anim": "wave", "times": 2}},
      {"do": "step", "step": {"do": "say", "actor": "ani", "text": "SELAMAT DATANG {user}!", "secs": 3}}
    ],
    "tari_mawar": [
      {"do": "overlay", "add": [
        {"type": "emoji", "emoji": ":rose:", "at": [0.5, 0.42], "size": 0.2, "ttl": 2.6, "anim_in": "pop", "anim_out": "fade"},
        {"type": "text", "text": "{user} KIRIM MAWAR", "at": [0.5, 0.58], "size": 0.03, "ttl": 2.6, "anim_in": "pop", "anim_out": "fade"}
      ]},
      {"do": "step", "step": {"do": "play", "actor": "citra", "anim": "cheer", "times": 3}},
      {"do": "step", "step": {"do": "say", "actor": "citra", "text": "MAKASIH {user}!", "secs": 3}},
      {"do": "add", "var": "gifts"}
    ],
    "traktir": [
      {"do": "overlay", "add": [{"type": "emoji", "emoji": ":party:", "at": [0.5, 0.42], "size": 0.2, "ttl": 2.6, "anim_in": "pop", "anim_out": "fade"}]},
      {"do": "step", "step": {"do": "play", "actor": "ani", "anim": "cheer", "times": 2}},
      {"do": "step", "step": {"do": "play", "actor": "budi", "anim": "cheer", "times": 2}},
      {"do": "step", "step": {"do": "play", "actor": "citra", "anim": "cheer", "times": 2}},
      {"do": "step", "step": {"do": "play", "actor": "dodi", "anim": "cheer", "times": 2}},
      {"do": "announce", "text": "{user} TRAKTIR SEMUA WARGA!", "secs": 2.5, "color": "#ffd27a"},
      {"do": "add", "var": "gifts"}
    ],
    "malam_bintang": [
      {"do": "announce", "text": "{user} MEMBUAT MALAM BINTANG!", "secs": 3, "color": "#9fc4ff"},
      {"do": "step", "step": {"do": "set_sky", "preset": "night", "secs": 3}},
      {"do": "step", "step": {"op": "orbit", "dyaw": 90, "secs": 10}},
      {"do": "overlay", "add": [{"type": "emoji", "emoji": ":glowing_star:", "at": [0.5, 0.3], "size": 0.18, "ttl": 4, "anim_in": "pop", "anim_out": "fade"}]},
      {"do": "add", "var": "gifts"},
      {"do": "wait", "secs": 14},
      {"do": "step", "step": {"do": "set_sky", "preset": "day", "secs": 4}}
    ],
    "terima_kasih": [
      {"do": "announce", "text": "TERIMA KASIH {user}! {gift} x{count}", "secs": 2.2, "color": "#ffe27a"},
      {"do": "add", "var": "gifts"}
    ],
    "target_tercapai": [
      {"do": "announce", "text": "TARGET LIKE TERCAPAI!", "secs": 3, "color": "#ff9fbf"},
      {"do": "overlay", "add": [
        {"type": "emoji", "emoji": ":party:", "at": [0.25, 0.4], "size": 0.16, "ttl": 3, "anim_in": "pop", "anim_out": "fade"},
        {"type": "emoji", "emoji": ":party:", "at": [0.75, 0.4], "size": 0.16, "ttl": 3, "anim_in": "pop", "anim_out": "fade"},
        {"type": "emoji", "emoji": ":heart:", "at": [0.5, 0.45], "size": 0.2, "ttl": 3, "anim_in": "pop", "anim_out": "fade"}
      ]},
      {"do": "step", "step": {"do": "play", "actor": "ani", "anim": "cheer", "times": 3}},
      {"do": "step", "step": {"do": "play", "actor": "dodi", "anim": "cheer", "times": 3}}
    ],
    "sapaan": [
      {"do": "random", "choices": [
        {"steps": [{"do": "step", "step": {"do": "say", "actor": "ani", "text": "AYO KIRIM KOMENTAR!", "secs": 3}}]},
        {"steps": [{"do": "step", "step": {"do": "say", "actor": "budi", "text": "JANGAN LUPA LIKE YA!", "secs": 3}}]},
        {"steps": [{"do": "step", "step": {"do": "say", "actor": "citra", "text": "KAMPUNG INI RAMAI KALAU KAMU IKUT", "secs": 3}}]},
        {"steps": [{"do": "step", "step": {"do": "play", "actor": "dodi", "anim": "wave", "times": 2}}]}
      ]}
    ]
  },
  "bots": [{"name": "sapa", "macro": "sapaan", "when_idle": 15}]
}
"##;
