//! A show is what `r3dsims live` runs inside the stream: the part that decides what viewer events do. The server around it
//! (picture, dashboard, TikTok and webhook input, video output, overlays) knows nothing about games.
//!
//! * `War` (`war.json`, has `sides`): two camps, rounds, a winner, wins and losses. A game.
//! * `Studio` (`studio.json`, no `sides`): triggers, macros, bots, overlays, story steps, counters and a goal bar on any scene.
//!   Chat as speech bubbles, gifts as effects, likes filling a bar, a diorama that lives: no game at all.
//!
//! Anything else can implement `Show` the same way.

use std::path::Path;

use anyhow::{Context, Result, bail};
use serde_json::Value;

use crate::hud::{self, HudItem};
use crate::overlay::{Overlay, Vars};
use crate::studio::Studio;
use crate::war::{Event, Hooks, War};
use crate::world::World;

pub trait Show: Send {
    /// `war` or `studio`; the dashboard shows only the cards that make sense for the kind
    fn kind(&self) -> &'static str;
    fn title(&self) -> String;
    /// advance the show by `dt` seconds (the world itself is updated by the caller)
    fn update(&mut self, dt: f32, world: &mut World);
    /// one viewer event, from a webhook, the TikTok connector or a macro; the answer lists which triggers fired
    fn handle_event(&mut self, world: &mut World, ev: Event) -> Value;
    fn control(&mut self, world: &mut World, op: &str) -> Result<Value>;
    /// triggers, macros and bots published while the stream runs
    fn publish(&mut self, world: &World, hooks: Hooks) -> Result<Value>;
    fn overlay_op(&mut self, body: &Value) -> Result<Value>;
    fn overlay(&self) -> &Overlay;
    /// every value the HUD and overlay text can show: `{title}`, `{ticker.1}`, `{top1}`, `{var.NAME}` ...
    fn vars(&self) -> Vars;
    /// the built-in HUD for a frame of `w` x `h` pixels (empty when the show turns it off)
    fn hud(&self, w: usize, h: usize) -> Vec<HudItem>;
    fn state_json(&self) -> Value;
    fn hooks_json(&self) -> Value;
    /// results worth keeping in the history file (the rounds of a game); a show without results returns none
    fn take_log(&mut self) -> Vec<Value>;
    /// the two camps of a game, for the history header; `None` = nothing to keep a history of
    fn camps(&self) -> Option<[(String, String); 2]>;
    /// counters and texts fed from outside (`POST /vars`); only a show that keeps counters accepts them
    fn set_vars(&mut self, _body: &Value) -> Result<Value> {
        bail!("this show keeps no counters to feed; use a studio")
    }
    /// a vector stage instead of the GPU diorama: when true `live` draws `render_frame` on the CPU and never opens a GPU
    fn has_stage(&self) -> bool {
        false
    }
    /// one frame of the stage as packed RGB, `w` x `h` pixels
    fn render_frame(&mut self, _w: usize, _h: usize) -> Option<Result<Vec<u8>>> {
        None
    }
}

impl Show for War {
    fn kind(&self) -> &'static str {
        "war"
    }

    fn title(&self) -> String {
        self.state_json()["title"].as_str().unwrap_or("").to_string()
    }

    fn update(&mut self, dt: f32, world: &mut World) {
        War::update(self, dt, world)
    }

    fn handle_event(&mut self, world: &mut World, ev: Event) -> Value {
        War::handle_event(self, world, ev)
    }

    fn control(&mut self, world: &mut World, op: &str) -> Result<Value> {
        War::control(self, world, op)
    }

    fn publish(&mut self, world: &World, hooks: Hooks) -> Result<Value> {
        War::publish(self, world, hooks)
    }

    fn overlay_op(&mut self, body: &Value) -> Result<Value> {
        War::overlay_op(self, body)
    }

    fn overlay(&self) -> &Overlay {
        &self.overlay
    }

    fn vars(&self) -> Vars {
        War::vars(self)
    }

    fn hud(&self, w: usize, h: usize) -> Vec<HudItem> {
        if self.hud_enabled() { hud::build(self, w, h) } else { vec![] }
    }

    fn state_json(&self) -> Value {
        War::state_json(self)
    }

    fn hooks_json(&self) -> Value {
        War::hooks_json(self)
    }

    fn take_log(&mut self) -> Vec<Value> {
        War::take_log(self)
    }

    fn camps(&self) -> Option<[(String, String); 2]> {
        let st = War::state_json(self);
        let sides: Vec<(String, String)> = st["sides"].as_object()?.iter().map(|(k, v)| (k.clone(), v["name"].as_str().unwrap_or(k).to_string())).collect();
        Some([sides.first().cloned().unwrap_or_default(), sides.get(1).cloned().unwrap_or_default()])
    }
}

/// Which kind of show a JSON file is: a war has `sides`, a studio does not. A scene file (it has `terrain`) is neither.
pub fn is_show_file(v: &Value) -> bool {
    v.get("scene").is_some_and(Value::is_string) && v.get("terrain").is_none()
}

/// Reads a show file and the scene it names.
pub fn load_show(path: &Path) -> Result<(World, Box<dyn Show>)> {
    let txt = std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let v: Value = serde_json::from_str(&txt).with_context(|| format!("parse {}", path.display()))?;
    if !is_show_file(&v) {
        bail!("{} is not a show file: it needs a \"scene\" (the scene.json to play). A game has \"sides\" too; without them it is a studio", path.display());
    }
    if v.get("sides").is_some() {
        let (world, war) = War::load(path)?;
        Ok((world, Box::new(war)))
    } else {
        let (world, studio) = Studio::load(path)?;
        Ok((world, Box::new(studio)))
    }
}
