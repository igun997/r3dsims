//! Story scenes. A story is a list of named scenes; a scene is a list of `Step`s run in order.
//! `par` runs children together, `seq` / `repeat` nest. The exact same `Step` JSON is accepted
//! live (viewer stdin, `--commands`), so an AI agent drives the diorama with the format it already knows.

use glam::Vec2;
use serde::{Deserialize, Serialize};

use crate::camera::CameraCmd;
use crate::nav::{PathPt, Place};
use crate::world::{EntityDef, World};

fn yes() -> bool {
    true
}
fn one_u() -> u32 {
    1
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(untagged)]
pub enum Target {
    /// entity id or marker name
    Name(String),
    /// world [x, z] on the ground
    Pos([f32; 2]),
    /// world [x, z] on a floor of a building: `{"pos":[7,9],"building":"rumah","floor":1}`
    At { pos: [f32; 2], building: String, floor: usize },
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct StageDef {
    pub name: String,
    pub steps: Vec<Step>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct StoryDef {
    #[serde(default, rename = "loop")]
    pub looping: bool,
    /// When looping, put actors and camera back to their starting state.
    #[serde(default = "yes")]
    pub reset_on_loop: bool,
    pub scenes: Vec<StageDef>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "do", rename_all = "snake_case")]
pub enum Step {
    /// Walk (pathfinding around houses/water). Stops `stop` units short of an actor target (default 1.2)
    /// and turns to face it. `via` = extra waypoints.
    WalkTo {
        actor: String,
        to: Target,
        speed: Option<f32>,
        stop: Option<f32>,
        #[serde(default)]
        via: Vec<Target>,
    },
    Face { actor: String, to: Target },
    /// Play a clip once (`times`) or for `secs`; blocks until finished.
    Play {
        actor: String,
        anim: String,
        #[serde(default = "one_u")]
        times: u32,
        secs: Option<f32>,
    },
    /// Set the actor's looping base animation (replaces "idle"). Does not block.
    LoopAnim { actor: String, anim: String },
    Wait { secs: f32 },
    /// Speech bubble for `secs` (default scales with text length); blocks meanwhile.
    Say { actor: String, text: String, secs: Option<f32> },
    Teleport { actor: String, to: Target },
    Seq { steps: Vec<Step> },
    Par { steps: Vec<Step> },
    Repeat { times: u32, steps: Vec<Step> },
    Camera(CameraCmd),
    Spawn { entity: EntityDef },
    Remove { actor: String },
    /// Run another named scene inline.
    PlayScene { name: String },
    /// "day" | "sunset" | "dawn" | "night"
    SetSky { preset: String, secs: Option<f32> },
    /// Dollhouse view: `show_floor` N hides every floor above N (-1 = all), `gap` pulls floors
    /// apart (exploded view, blends over `secs`), `cutaway` is auto | none | low. Blocks for `secs`.
    Building {
        id: String,
        show_floor: Option<i32>,
        gap: Option<f32>,
        cutaway: Option<String>,
        secs: Option<f32>,
    },
    StopStory,
}

pub enum Node {
    Seq { steps: Vec<Step>, idx: usize, cur: Option<Box<Node>> },
    Par { kids: Vec<Option<Node>> },
    Repeat { steps: Vec<Step>, left: u32, cur: Option<Box<Node>> },
    Wait { left: f32 },
    Walk(Box<Walk>),
    Face { actor: String, to: Target },
    Play { actor: String, anim: String, times: u32, secs: Option<f32>, left: Option<f32> },
    Say { actor: String, text: String, secs: Option<f32>, left: Option<f32> },
    Cam { cmd: CameraCmd, left: Option<f32> },
    Bld { id: String, show_floor: Option<i32>, gap: Option<f32>, cutaway: Option<String>, secs: f32, left: Option<f32> },
    Instant(Step),
}

pub struct Walk {
    actor: String,
    to: Target,
    via: Vec<Target>,
    speed: f32,
    stop: Option<f32>,
    st: Option<WalkState>,
}

struct WalkState {
    pts: Vec<PathPt>,
    i: usize,
    face: Option<Vec2>,
    arrived: bool,
}

const TURN_WALK: f32 = 720.0;
const TURN_FACE: f32 = 540.0;

fn angle_diff(to: f32, from: f32) -> f32 {
    (to - from + 540.0).rem_euclid(360.0) - 180.0
}

fn trim_end(pts: &mut Vec<PathPt>, stop: f32) {
    let mut rem = stop;
    while pts.len() >= 2 {
        let (a, b) = (pts[pts.len() - 2], pts[pts.len() - 1]);
        if a.layer != b.layer {
            break;
        }
        let seg = (b.pos - a.pos).length();
        if seg > rem {
            let n = pts.len();
            pts[n - 1].pos = b.pos + (a.pos - b.pos).normalize_or_zero() * rem;
            break;
        }
        rem -= seg;
        pts.pop();
    }
}

/// Turns toward `want` (degrees). Returns time left over if the turn finished within `rem`.
fn turn(yaw: &mut f32, want: f32, rate: f32, rem: f32) -> Option<f32> {
    let diff = angle_diff(want, *yaw);
    let step = rate * rem;
    if diff.abs() <= step {
        *yaw = want;
        Some(rem - diff.abs() / rate)
    } else {
        *yaw += step * diff.signum();
        None
    }
}

fn timed(left: &mut Option<f32>, dt: f32, init: impl FnOnce() -> f32) -> Option<f32> {
    let l = left.get_or_insert_with(init);
    *l -= dt;
    (*l <= 0.0).then(|| -*l)
}

impl Node {
    pub fn from_step(s: &Step, w: &mut World) -> Node {
        match s {
            Step::Seq { steps } => Node::Seq { steps: steps.clone(), idx: 0, cur: None },
            Step::Par { steps } => Node::Par { kids: steps.iter().map(|s| Some(Node::from_step(s, w))).collect() },
            Step::Repeat { times, steps } => Node::Repeat { steps: steps.clone(), left: *times, cur: None },
            Step::Wait { secs } => Node::Wait { left: *secs },
            Step::WalkTo { actor, to, speed, stop, via } => Node::Walk(Box::new(Walk {
                actor: actor.clone(),
                to: to.clone(),
                via: via.clone(),
                speed: speed.unwrap_or(2.0).max(0.05),
                stop: *stop,
                st: None,
            })),
            Step::Face { actor, to } => Node::Face { actor: actor.clone(), to: to.clone() },
            Step::Play { actor, anim, times, secs } => {
                Node::Play { actor: actor.clone(), anim: anim.clone(), times: (*times).max(1), secs: *secs, left: None }
            }
            Step::Say { actor, text, secs } => {
                Node::Say { actor: actor.clone(), text: text.clone(), secs: *secs, left: None }
            }
            Step::Camera(cmd) => Node::Cam { cmd: cmd.clone(), left: None },
            Step::Building { id, show_floor, gap, cutaway, secs } => Node::Bld {
                id: id.clone(),
                show_floor: *show_floor,
                gap: *gap,
                cutaway: cutaway.clone(),
                secs: secs.unwrap_or(0.0),
                left: None,
            },
            Step::PlayScene { name } => match w.stage_steps(name) {
                Some(steps) => Node::Seq { steps, idx: 0, cur: None },
                None => {
                    w.warn(format!("play_scene: unknown scene '{name}'"));
                    Node::Wait { left: 0.0 }
                }
            },
            other => Node::Instant(other.clone()),
        }
    }

    /// Advances by `dt`. `Some(leftover)` = finished with that much time unused; `None` = still running.
    pub fn tick(&mut self, dt: f32, w: &mut World) -> Option<f32> {
        match self {
            Node::Wait { left } => {
                *left -= dt;
                (*left <= 0.0).then(|| -*left)
            }
            Node::Seq { steps, idx, cur } => {
                let mut rem = dt;
                loop {
                    if cur.is_none() {
                        if *idx >= steps.len() {
                            return Some(rem);
                        }
                        *cur = Some(Box::new(Node::from_step(&steps[*idx], w)));
                    }
                    match cur.as_mut().unwrap().tick(rem, w) {
                        Some(l) => {
                            rem = l;
                            *cur = None;
                            *idx += 1;
                        }
                        None => return None,
                    }
                }
            }
            Node::Repeat { steps, left, cur } => {
                let mut rem = dt;
                loop {
                    if cur.is_none() {
                        if *left == 0 || steps.is_empty() {
                            return Some(rem);
                        }
                        *cur = Some(Box::new(Node::from_step(&Step::Seq { steps: steps.clone() }, w)));
                    }
                    match cur.as_mut().unwrap().tick(rem, w) {
                        Some(l) => {
                            *left -= 1;
                            rem = l;
                            *cur = None;
                        }
                        None => return None,
                    }
                }
            }
            Node::Par { kids } => {
                let mut min_left = f32::INFINITY;
                let mut running = false;
                for k in kids.iter_mut() {
                    if let Some(n) = k {
                        match n.tick(dt, w) {
                            Some(l) => {
                                min_left = min_left.min(l);
                                *k = None;
                            }
                            None => running = true,
                        }
                    }
                }
                if running {
                    None
                } else if min_left.is_finite() {
                    Some(min_left)
                } else {
                    Some(dt)
                }
            }
            Node::Walk(walk) => tick_walk(walk, dt, w),
            Node::Face { actor, to } => {
                let Some(ei) = w.entity_idx(actor) else {
                    w.warn(format!("face: unknown actor '{actor}'"));
                    return Some(dt);
                };
                let Some(goal) = w.resolve(to) else {
                    w.warn(format!("face: cannot resolve target {to:?}"));
                    return Some(dt);
                };
                let e = &mut w.entities[ei];
                let d = goal - Vec2::new(e.pos.x, e.pos.z);
                if d.length() < 0.01 {
                    return Some(dt);
                }
                turn(&mut e.yaw, d.x.atan2(d.y).to_degrees(), TURN_FACE, dt)
            }
            Node::Play { actor, anim, times, secs, left } => {
                let Some(ei) = w.entity_idx(actor) else {
                    w.warn(format!("play: unknown actor '{actor}'"));
                    return Some(dt);
                };
                if left.is_none() {
                    let e = &mut w.entities[ei];
                    let Some(dur) = e.clip(anim).map(|c| c.duration) else {
                        let msg = format!("play: actor '{actor}' has no animation '{anim}'");
                        w.warn(msg);
                        return Some(dt);
                    };
                    e.anim.oneshot = Some(anim.clone());
                    e.anim.oneshot_loop = *times > 1 || secs.is_some();
                    e.anim.t = 0.0;
                    *left = Some(secs.unwrap_or(dur * *times as f32));
                }
                let r = timed(left, dt, || 0.0);
                if r.is_some() {
                    let e = &mut w.entities[ei];
                    e.anim.oneshot = None;
                    e.anim.oneshot_loop = false;
                }
                r
            }
            Node::Say { actor, text, secs, left } => {
                let Some(ei) = w.entity_idx(actor) else {
                    w.warn(format!("say: unknown actor '{actor}'"));
                    return Some(dt);
                };
                if left.is_none() {
                    w.entities[ei].speech = Some(text.clone());
                    *left = Some(secs.unwrap_or((text.chars().count() as f32 * 0.07).max(1.5)));
                }
                let r = timed(left, dt, || 0.0);
                if r.is_some() {
                    w.entities[ei].speech = None;
                }
                r
            }
            Node::Cam { cmd, left } => {
                if left.is_none() {
                    *left = Some(w.camera_cmd(cmd));
                }
                timed(left, dt, || 0.0)
            }
            Node::Bld { id, show_floor, gap, cutaway, secs, left } => {
                if left.is_none() {
                    *left = Some(w.building_cmd(id, *show_floor, *gap, cutaway.as_deref(), *secs));
                }
                timed(left, dt, || 0.0)
            }
            Node::Instant(step) => {
                exec_instant(step, w);
                Some(dt)
            }
        }
    }
}

fn tick_walk(n: &mut Walk, dt: f32, w: &mut World) -> Option<f32> {
    let Some(ei) = w.entity_idx(&n.actor) else {
        w.warn(format!("walk_to: unknown actor '{}'", n.actor));
        return Some(dt);
    };
    if n.st.is_none() {
        let start = Place { pos: Vec2::new(w.entities[ei].pos.x, w.entities[ei].pos.z), layer: w.entities[ei].layer };
        let Some(goal) = w.resolve_place(&n.to) else {
            w.warn(format!("walk_to: cannot resolve target {:?}", n.to));
            return Some(dt);
        };
        let mut waypoints: Vec<Place> = n.via.iter().filter_map(|v| w.resolve_place(v)).collect();
        waypoints.push(goal);
        let mut pts = vec![PathPt { pos: start.pos, layer: start.layer }];
        let mut cur = start;
        for wp in waypoints {
            let Some(seg) = w.nav.path(cur, wp) else {
                w.warn(format!("walk_to: no path for '{}' to {:?}", n.actor, n.to));
                return Some(dt);
            };
            pts.extend(seg);
            let last = *pts.last().unwrap();
            cur = Place { pos: last.pos, layer: last.layer };
        }
        let is_actor = matches!(&n.to, Target::Name(name) if w.entity_idx(name).is_some());
        let stop = n.stop.unwrap_or(if is_actor { 1.2 } else { 0.05 });
        trim_end(&mut pts, stop);
        let face = matches!(n.to, Target::Name(_)).then_some(goal.pos);
        n.st = Some(WalkState { pts, i: 1, face, arrived: false });
    }
    let st = n.st.as_mut().unwrap();
    let e = &mut w.entities[ei];
    let mut rem = dt;
    if !st.arrived {
        let mut budget = n.speed * dt;
        e.moving = st.i < st.pts.len();
        while st.i < st.pts.len() {
            let cur = Vec2::new(e.pos.x, e.pos.z);
            let t = st.pts[st.i].pos;
            let d = t - cur;
            let len = d.length();
            if len > 1e-4 {
                let want = d.x.atan2(d.y).to_degrees();
                let diff = angle_diff(want, e.yaw);
                let step = TURN_WALK * dt;
                e.yaw = if diff.abs() <= step { want } else { e.yaw + step * diff.signum() };
            }
            if len <= budget {
                e.pos.x = t.x;
                e.pos.z = t.y;
                e.layer = st.pts[st.i].layer;
                budget -= len;
                st.i += 1;
            } else {
                let dir = d / len;
                e.pos.x += dir.x * budget;
                e.pos.z += dir.y * budget;
                return None;
            }
        }
        e.moving = false;
        st.arrived = true;
        rem = budget / n.speed;
    }
    if let Some(g) = st.face {
        let d = g - Vec2::new(e.pos.x, e.pos.z);
        if d.length() > 0.01 {
            return turn(&mut e.yaw, d.x.atan2(d.y).to_degrees(), TURN_FACE, rem);
        }
    }
    Some(rem)
}

fn exec_instant(step: &Step, w: &mut World) {
    match step {
        Step::LoopAnim { actor, anim } => match w.entity_idx(actor) {
            Some(i) => {
                if w.entities[i].clip(anim).is_none() {
                    w.warn(format!("loop_anim: actor '{actor}' has no animation '{anim}'"));
                } else {
                    w.entities[i].anim.base = anim.clone();
                }
            }
            None => w.warn(format!("loop_anim: unknown actor '{actor}'")),
        },
        Step::Teleport { actor, to } => match (w.entity_idx(actor), w.resolve_place(to)) {
            (Some(i), Some(p)) => {
                let e = &mut w.entities[i];
                e.pos.x = p.pos.x;
                e.pos.z = p.pos.y;
                e.layer = p.layer;
                e.snapped = false;
            }
            _ => w.warn(format!("teleport: cannot resolve actor '{actor}' or target")),
        },
        Step::Spawn { entity } => {
            if let Err(err) = w.spawn(entity) {
                w.warn(format!("spawn '{}': {err:#}", entity.id));
            }
        }
        Step::Remove { actor } => {
            if !w.remove(actor) {
                w.warn(format!("remove: unknown actor '{actor}'"));
            }
        }
        Step::SetSky { preset, secs } => w.set_sky(preset, *secs),
        Step::StopStory => w.stop_story = true,
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn step_json_forms() {
        let s: Step = serde_json::from_str(r#"{"do":"walk_to","actor":"a","to":"b","stop":1.5}"#).unwrap();
        assert!(matches!(s, Step::WalkTo { .. }));
        let s: Step = serde_json::from_str(r#"{"do":"walk_to","actor":"a","to":[3,4]}"#).unwrap();
        assert!(matches!(s, Step::WalkTo { to: Target::Pos([x, _]), .. } if x == 3.0));
        let s: Step = serde_json::from_str(r#"{"do":"camera","op":"orbit","dyaw":90,"secs":2}"#).unwrap();
        assert!(matches!(s, Step::Camera(CameraCmd::Orbit { .. })));
        let s: Step = serde_json::from_str(r#"{"do":"par","steps":[{"do":"wait","secs":1}]}"#).unwrap();
        assert!(matches!(s, Step::Par { .. }));
    }

    #[test]
    fn trim_shortens_last_segment() {
        let pt = |x: f32, layer: u16| PathPt { pos: Vec2::new(x, 0.0), layer };
        let mut p = vec![pt(0.0, 0), pt(10.0, 0)];
        trim_end(&mut p, 2.0);
        assert!((p[1].pos.x - 8.0).abs() < 1e-5);
        let mut p = vec![pt(0.0, 0), pt(1.0, 0)];
        trim_end(&mut p, 2.0);
        assert_eq!(p.len(), 1);
        // never shorten across a layer change (stairs, doors)
        let mut p = vec![pt(0.0, 0), pt(1.0, 1)];
        trim_end(&mut p, 2.0);
        assert_eq!(p.len(), 2);
    }

    #[test]
    fn angle_wraps() {
        assert!((angle_diff(10.0, 350.0) - 20.0).abs() < 1e-4);
        assert!((angle_diff(350.0, 10.0) + 20.0).abs() < 1e-4);
    }
}
