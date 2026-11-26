//! Macro scripts for bots. A macro is a JSON list of steps run by the war: emit synthetic viewer
//! events, fire actions, wait, loop, branch on the battle state, pick at random, call other macros.
//! Triggers run macros when a webhook event arrives; bots run them when the stream is idle or on a
//! timer, so the arena never sits still.
//!
//! ```json
//! "macros": {
//!   "fill": [
//!     {"do": "random", "choices": [
//!       {"weight": 3, "steps": [{"do": "act", "action": "reinforce", "side": "random"}]},
//!       {"weight": 1, "steps": [{"do": "act", "action": "meteor_small", "side": "trailing"}]}
//!     ]},
//!     {"do": "wait", "secs": 4}
//!   ]
//! }
//! ```

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::overlay::Element;
use crate::war::{Action, ActionRef, Event, War};
use crate::world::World;

fn d_secs() -> f32 {
    2.5
}
fn one() -> f32 {
    1.0
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Choice {
    #[serde(default = "one")]
    pub weight: f32,
    pub steps: Vec<MStep>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "do", rename_all = "snake_case")]
pub enum MStep {
    /// Fire a named (or inline) action on `side` (A, B, random, leader, trailing, event).
    Act {
        action: ActionRef,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        side: Option<String>,
    },
    /// Feed a synthetic viewer event through the triggers, exactly like a webhook would.
    Event(Event),
    Wait {
        secs: f32,
    },
    Repeat {
        times: u32,
        steps: Vec<MStep>,
    },
    Forever {
        steps: Vec<MStep>,
    },
    Random {
        choices: Vec<Choice>,
    },
    If {
        cond: Cond,
        #[serde(default)]
        then: Vec<MStep>,
        #[serde(default, rename = "else")]
        otherwise: Vec<MStep>,
    },
    /// Block until `cond` holds (or `timeout` seconds pass).
    Until {
        cond: Cond,
        #[serde(default)]
        timeout: Option<f32>,
    },
    Announce {
        text: String,
        #[serde(default = "d_secs")]
        secs: f32,
        #[serde(default)]
        color: Option<String>,
    },
    Overlay {
        #[serde(default)]
        add: Vec<Element>,
        #[serde(default)]
        remove: Vec<String>,
        #[serde(default)]
        clear: bool,
    },
    /// Any story / camera step of the diorama (`{"op":"orbit","dyaw":90,"secs":2}` style).
    Step {
        step: Value,
    },
    Call {
        #[serde(rename = "macro")]
        name: String,
    },
    Set {
        var: String,
        value: f64,
    },
    Add {
        var: String,
        #[serde(default = "one_f64")]
        by: f64,
    },
    Stop,
}

fn one_f64() -> f64 {
    1.0
}

/// Conditions for `if` / `until` / bots. Sides are `"A"` or `"B"`.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Cond {
    HpPctLt { side: String, value: f32 },
    HpPctGt { side: String, value: f32 },
    UnitsLt { side: String, value: u32 },
    UnitsGt { side: String, value: u32 },
    /// current round number equals / is at least
    Round(u32),
    RoundGte(u32),
    /// lobby | fighting | round_end | match_end
    Phase(String),
    /// which side has more castle HP ("A" / "B")
    Leading(String),
    IdleSecsGt(f32),
    /// seconds into the round
    RoundSecsGt(f32),
    Var { name: String, gte: Option<f64>, lte: Option<f64>, eq: Option<f64> },
    Chance(f32),
    Not(Box<Cond>),
    All(Vec<Cond>),
    Any(Vec<Cond>),
}

/// What a running macro knows about the event that started it.
#[derive(Clone, Debug, Default)]
pub struct Ctx {
    pub event: Option<Event>,
    pub side: Option<usize>,
}

enum Kind {
    Seq,
    Repeat(u32),
    Forever,
}

struct Frame {
    steps: Vec<MStep>,
    idx: usize,
    kind: Kind,
}

pub struct Runner {
    pub name: String,
    stack: Vec<Frame>,
    wait: f32,
    until: Option<(Cond, Option<f32>)>,
    pub ctx: Ctx,
    pub done: bool,
    calls: u32,
}

const MAX_INSTANT_STEPS: u32 = 400;
const MAX_CALL_DEPTH: u32 = 24;

impl Runner {
    pub fn new(name: &str, steps: Vec<MStep>, ctx: Ctx) -> Runner {
        Runner { name: name.into(), stack: vec![Frame { steps, idx: 0, kind: Kind::Seq }], wait: 0.0, until: None, ctx, done: false, calls: 0 }
    }

    pub fn tick(&mut self, dt: f32, war: &mut War, world: &mut World) {
        if self.done {
            return;
        }
        if self.wait > 0.0 {
            self.wait -= dt;
            if self.wait > 0.0 {
                return;
            }
        }
        if let Some((cond, left)) = &mut self.until {
            let timed_out = match left {
                Some(t) => {
                    *t -= dt;
                    *t <= 0.0
                }
                None => false,
            };
            if !timed_out && !war.eval(cond, &self.ctx) {
                return;
            }
            self.until = None;
        }
        let mut budget = MAX_INSTANT_STEPS;
        while budget > 0 {
            let Some(frame) = self.stack.last_mut() else {
                self.done = true;
                return;
            };
            if frame.idx >= frame.steps.len() {
                match &mut frame.kind {
                    Kind::Seq => {
                        self.stack.pop();
                    }
                    Kind::Repeat(left) => {
                        if *left > 1 {
                            *left -= 1;
                            frame.idx = 0;
                        } else {
                            self.stack.pop();
                        }
                    }
                    Kind::Forever => frame.idx = 0,
                }
                budget -= 1;
                continue;
            }
            let step = frame.steps[frame.idx].clone();
            frame.idx += 1;
            budget -= 1;
            match step {
                MStep::Wait { secs } => {
                    self.wait = secs.max(0.0);
                    if self.wait > 0.0 {
                        return;
                    }
                }
                MStep::Repeat { times, steps } => {
                    if times > 0 && !steps.is_empty() {
                        self.stack.push(Frame { steps, idx: 0, kind: Kind::Repeat(times) });
                    }
                }
                MStep::Forever { steps } => {
                    if !steps.is_empty() {
                        self.stack.push(Frame { steps, idx: 0, kind: Kind::Forever });
                    }
                }
                MStep::Random { choices } => {
                    let total: f32 = choices.iter().map(|c| c.weight.max(0.0)).sum();
                    if total > 0.0 {
                        let mut r = war.rand() * total;
                        for c in choices {
                            r -= c.weight.max(0.0);
                            if r <= 0.0 {
                                self.stack.push(Frame { steps: c.steps, idx: 0, kind: Kind::Seq });
                                break;
                            }
                        }
                    }
                }
                MStep::If { cond, then, otherwise } => {
                    let branch = if war.eval(&cond, &self.ctx) { then } else { otherwise };
                    if !branch.is_empty() {
                        self.stack.push(Frame { steps: branch, idx: 0, kind: Kind::Seq });
                    }
                }
                MStep::Until { cond, timeout } => {
                    if !war.eval(&cond, &self.ctx) {
                        self.until = Some((cond, timeout));
                        return;
                    }
                }
                MStep::Act { action, side } => {
                    let s = war.resolve_side(side.as_deref().unwrap_or("event"), &self.ctx);
                    war.exec(world, &action, s, &self.ctx);
                }
                MStep::Event(ev) => war.inject(world, ev),
                MStep::Announce { text, secs, color } => {
                    let s = war.resolve_side("event", &self.ctx);
                    war.exec(world, &ActionRef::Inline(Box::new(Action::Announce { text, secs, color })), s, &self.ctx);
                }
                MStep::Overlay { add, remove, clear } => {
                    let s = war.resolve_side("event", &self.ctx);
                    war.exec(world, &ActionRef::Inline(Box::new(Action::Overlay { add, remove, clear })), s, &self.ctx);
                }
                MStep::Step { step } => war.world_step(world, &step),
                MStep::Call { name } => {
                    self.calls += 1;
                    if self.calls > MAX_CALL_DEPTH * 50 {
                        war.warn(format!("macro '{}': too many calls, stopped", self.name));
                        self.done = true;
                        return;
                    }
                    match war.macro_steps(&name) {
                        Some(steps) => self.stack.push(Frame { steps, idx: 0, kind: Kind::Seq }),
                        None => war.warn(format!("macro '{}': call to unknown macro '{name}'", self.name)),
                    }
                    if self.stack.len() as u32 > MAX_CALL_DEPTH {
                        war.warn(format!("macro '{}': nested deeper than {MAX_CALL_DEPTH}, stopped", self.name));
                        self.done = true;
                        return;
                    }
                }
                MStep::Set { var, value } => war.set_var(&var, value),
                MStep::Add { var, by } => {
                    let v = war.get_var(&var) + by;
                    war.set_var(&var, v);
                }
                MStep::Stop => {
                    self.done = true;
                    return;
                }
            }
        }
        // ran a whole budget of steps without waiting: a `forever` with no wait inside. Yield a frame.
        war.warn(format!("macro '{}': {MAX_INSTANT_STEPS} steps without a wait; pausing 0.2s (add a wait to the loop)", self.name));
        self.wait = 0.2;
    }
}
