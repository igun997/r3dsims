//! The war: two kingdoms side by side, soldiers that march and fight on their own, and a table of
//! *triggers* that turn viewer events (a gift, likes, a comment) into effects that already exist in the
//! diorama: meteors, arrow rain, lightning, reinforcements, shields, healing. Rounds or endless, a
//! winner every round and (in rounds mode) at the end of the match.
//!
//! Everything is data (`war.json`), and the data can be published into the running stream:
//! `POST /hooks` replaces or adds triggers, macros, bots and actions without a restart.

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::path::Path;

use anyhow::{Context, Result, anyhow, bail};
use glam::{Vec2, Vec3};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::overlay::{Element, Overlay, Vars, interpolate};
use crate::script::{Cond, Ctx, Host, MStep, Runner};
use crate::world::{EntityDef, TilePatch, World};
use crate::camera::{CamParams, Camera, CameraCmd};

fn one() -> f32 {
    1.0
}
fn one_u() -> u32 {
    1
}
fn yes() -> bool {
    true
}

// ---------------------------------------------------------------- config

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SideDef {
    pub name: String,
    /// `#rrggbb`: HUD, soldier tabards, announcements
    pub color: String,
    /// entity id of the castle in the scene
    #[serde(alias = "base", alias = "objective")]
    pub castle: String,
    /// where reinforcements appear, world [x, z]
    pub spawn: [f32; 2],
    /// default soldier type (key of `units`)
    pub unit: String,
    /// soldier yaw while marching toward the enemy (0 = +z, 90 = +x, 270 = -x)
    #[serde(default)]
    pub facing: f32,
    #[serde(default = "d_castle_radius")]
    pub castle_radius: f32,
    /// this kingdom's ground, `[x0, z0, x1, z1]`; meteors and arrows aimed at the side land here
    pub zone: [f32; 4],
}

fn d_castle_radius() -> f32 {
    2.6
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct UnitDef {
    /// asset key in the scene (clips used: idle, walk, attack, die)
    pub asset: String,
    #[serde(default = "d_unit_hp")]
    pub hp: f32,
    #[serde(default = "d_unit_speed")]
    pub speed: f32,
    #[serde(default = "d_unit_dmg")]
    pub dmg: f32,
    #[serde(default = "d_unit_range")]
    pub range: f32,
    #[serde(default = "d_unit_cd")]
    pub cooldown: f32,
    #[serde(default = "one")]
    pub scale: f32,
    /// shoots arrows (cosmetic projectile) instead of swinging
    #[serde(default)]
    pub ranged: bool,
    /// damage multiplier against a castle
    #[serde(default = "d_castle_mult")]
    pub castle_mult: f32,
    /// palette key recoloured with the side colour ("" = none)
    #[serde(default = "d_tint")]
    pub tint: String,
    #[serde(default)]
    pub elevation: f32,
    /// a thing, not a fighter (barricade, tent): never moves or attacks, the other side tears it down;
    /// not counted as a soldier
    #[serde(default)]
    pub structure: bool,
    /// clip played when it is beaten (default "die"; a non-violent asset can define "faint")
    #[serde(default = "d_defeat")]
    pub defeat_anim: String,
}

fn d_unit_hp() -> f32 {
    40.0
}
fn d_unit_speed() -> f32 {
    2.0
}
fn d_unit_dmg() -> f32 {
    7.0
}
fn d_unit_range() -> f32 {
    1.1
}
fn d_unit_cd() -> f32 {
    0.9
}
fn d_castle_mult() -> f32 {
    0.6
}
fn d_tint() -> String {
    "cloth".into()
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct MatchDef {
    /// "rounds" (best of N, a final winner) or "endless" (a winner every round, never ends until `stop`)
    #[serde(default = "d_mode")]
    pub mode: String,
    #[serde(default = "d_rounds")]
    pub rounds: u32,
    #[serde(default = "d_round_secs")]
    pub round_secs: f32,
    #[serde(default = "d_lobby")]
    pub lobby_secs: f32,
    #[serde(default = "d_intermission")]
    pub intermission: f32,
    /// seconds after the final winner before a new match starts by itself (default 15); null = wait for `control start`
    #[serde(default = "d_restart")]
    pub restart_secs: Option<f32>,
}

impl Default for MatchDef {
    fn default() -> Self {
        MatchDef { mode: d_mode(), rounds: d_rounds(), round_secs: d_round_secs(), lobby_secs: d_lobby(), intermission: d_intermission(), restart_secs: d_restart() }
    }
}

fn d_mode() -> String {
    "rounds".into()
}
fn d_rounds() -> u32 {
    3
}
fn d_round_secs() -> f32 {
    90.0
}
fn d_lobby() -> f32 {
    5.0
}
fn d_intermission() -> f32 {
    7.0
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AutoSpawn {
    pub every: f32,
    #[serde(default = "one_u")]
    pub count: u32,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(untagged)]
pub enum OneOrMany {
    One(String),
    Many(Vec<String>),
}

impl OneOrMany {
    pub fn list(&self) -> Vec<&str> {
        match self {
            OneOrMany::One(s) => vec![s.as_str()],
            OneOrMany::Many(v) => v.iter().map(String::as_str).collect(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(untagged)]
pub enum ActionRef {
    Name(String),
    Inline(Box<Action>),
}

/// The effects available to triggers and macros. `side` is where the effect lands: the victim for
/// harmful ones (meteor, arrows, lightning, quake), the beneficiary for the rest.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Action {
    Meteor {
        #[serde(default = "one_u")]
        count: u32,
        #[serde(default = "d_m_radius")]
        radius: f32,
        #[serde(default = "d_m_dmg")]
        damage: f32,
        #[serde(default = "d_m_cdmg")]
        castle_damage: f32,
        /// seconds between meteors of one gift
        #[serde(default = "d_m_gap")]
        gap: f32,
        /// auto | castle | field | units | front
        #[serde(default)]
        target: String,
        /// also hurts the other side's soldiers inside the blast
        #[serde(default)]
        friendly_fire: bool,
        #[serde(default = "one")]
        scale: f32,
        /// scene asset that falls (default: the `meteor` effect asset), e.g. a bottle
        #[serde(default)]
        asset: String,
        /// leave a burning patch where it lands for this many seconds (0 = none)
        #[serde(default)]
        burn_secs: f32,
        /// damage per second inside the patch
        #[serde(default)]
        burn_dps: f32,
        /// scene asset of the patch (default `fire`)
        #[serde(default)]
        burn_asset: String,
    },
    Arrows {
        #[serde(default = "d_a_count")]
        count: u32,
        #[serde(default = "d_a_radius")]
        radius: f32,
        #[serde(default = "d_a_dmg")]
        damage: f32,
        #[serde(default = "d_a_cdmg")]
        castle_damage: f32,
        #[serde(default)]
        target: String,
    },
    Lightning {
        #[serde(default = "one_u")]
        count: u32,
        #[serde(default = "d_l_dmg")]
        damage: f32,
        #[serde(default = "d_l_cdmg")]
        castle_damage: f32,
    },
    Spawn {
        /// unit key; empty = the side's default soldier
        #[serde(default)]
        unit: String,
        #[serde(default = "d_s_count")]
        count: u32,
        /// where they appear: spawn (default, the side's gate), front (just ahead of its line), center
        #[serde(default)]
        at: String,
    },
    /// Soldiers of `side` shout slogans (speech bubbles) and cheer; `boost_secs` also rallies them.
    Chant {
        #[serde(default)]
        texts: Vec<String>,
        #[serde(default = "d_chant_count")]
        count: u32,
        #[serde(default = "d_chant_secs")]
        secs: f32,
        /// clip to play (default "cheer"; ignored when the asset has none)
        #[serde(default)]
        anim: String,
        #[serde(default)]
        boost_secs: f32,
    },
    /// Shoves soldiers of `side` away from a point (water cannon, shockwave).
    Push {
        #[serde(default = "d_push_strength")]
        strength: f32,
        #[serde(default = "d_push_radius")]
        radius: f32,
        #[serde(default)]
        damage: f32,
        #[serde(default)]
        target: String,
    },
    /// A lingering cloud (tear gas, smoke, fog) that slows, and optionally wears down, soldiers of `side` inside it.
    Cloud {
        #[serde(default = "d_cloud_radius")]
        radius: f32,
        #[serde(default = "d_cloud_secs")]
        secs: f32,
        /// speed multiplier inside (0.5 = half speed)
        #[serde(default = "d_cloud_slow")]
        slow: f32,
        /// damage per second inside
        #[serde(default)]
        dps: f32,
        /// false: it never takes a soldier's last HP
        #[serde(default)]
        lethal: bool,
        /// scene asset of the cloud (default `cloud`)
        #[serde(default)]
        asset: String,
        #[serde(default)]
        target: String,
    },
    /// Soldiers of `side` near a point change to the other side.
    Convert {
        #[serde(default = "d_conv_count")]
        count: u32,
        #[serde(default = "d_conv_radius")]
        radius: f32,
        #[serde(default)]
        target: String,
    },
    /// Switch the biome: a key of `biomes`, or "random".
    Biome {
        name: String,
    },
    /// First-person camera from a camp for a few seconds. Use it in ultimates only: ordinary skills never move
    /// the camera. `tint`, `vignette`, `shake` and `flashes` give each ultimate its own look.
    Cinematic {
        /// whose camp the camera sits in: enemy (the side opposite the action's side, default), self, A or B
        #[serde(default)]
        pov: String,
        #[serde(default = "d_cine_secs")]
        secs: f32,
        /// enemy (straight ahead, default), sky (look up), castle (at the enemy castle)
        #[serde(default)]
        look: String,
        #[serde(default = "d_cine_fov")]
        fov: f32,
        /// degrees below the horizon (negative = up); overrides `look`'s default
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pitch: Option<f32>,
        /// head bob while the soldier walks
        #[serde(default = "one")]
        bob: f32,
        #[serde(default)]
        shake: f32,
        /// camp (default): the camera sits at the camp, above the gate of that side's objective, and sweeps slowly over
        /// the field. front / random: ride a soldier of that camp instead (head bob applies).
        #[serde(default)]
        unit: String,
        /// full-screen colour `#rrggbbaa`
        #[serde(default)]
        tint: String,
        /// darkened screen edges, 0..1
        #[serde(default)]
        vignette: f32,
        /// white lightning flashes per second
        #[serde(default)]
        flashes: f32,
    },
    /// Throws the side's soldiers off balance: strips shield and boost, slows them, tears down structures.
    Sabotage {
        /// seconds of slowdown (0 = none)
        #[serde(default)]
        slow_secs: f32,
        /// speed multiplier while slowed
        #[serde(default = "d_sab_slow")]
        slow: f32,
        /// also destroy the side's structures (barricades)
        #[serde(default = "yes")]
        structures: bool,
    },
    /// Restores castle HP (a negative amount costs HP); `asset` is the little effect that floats up.
    Heal {
        #[serde(default = "d_heal")]
        amount: f32,
        #[serde(default)]
        asset: String,
    },
    Shield {
        #[serde(default = "d_shield")]
        secs: f32,
    },
    Boost {
        #[serde(default = "d_boost_secs")]
        secs: f32,
        #[serde(default = "d_boost_speed")]
        speed: f32,
        #[serde(default = "d_boost_dmg")]
        damage: f32,
    },
    Quake {
        #[serde(default = "d_q_dmg")]
        damage: f32,
        #[serde(default = "d_q_cdmg")]
        castle_damage: f32,
    },
    /// Any story / camera step of the diorama.
    Step {
        step: Value,
    },
    Overlay {
        #[serde(default)]
        add: Vec<Element>,
        #[serde(default)]
        remove: Vec<String>,
        #[serde(default)]
        clear: bool,
    },
    Announce {
        text: String,
        #[serde(default = "d_ann_secs")]
        secs: f32,
        #[serde(default)]
        color: Option<String>,
    },
    Seq {
        actions: Vec<ActionRef>,
    },
    Macro {
        name: String,
    },
}

fn d_m_radius() -> f32 {
    3.0
}
fn d_m_dmg() -> f32 {
    140.0
}
fn d_m_cdmg() -> f32 {
    120.0
}
fn d_m_gap() -> f32 {
    0.3
}
fn d_a_count() -> u32 {
    12
}
fn d_a_radius() -> f32 {
    3.0
}
fn d_a_dmg() -> f32 {
    10.0
}
fn d_a_cdmg() -> f32 {
    4.0
}
fn d_l_dmg() -> f32 {
    160.0
}
fn d_l_cdmg() -> f32 {
    50.0
}
fn d_s_count() -> u32 {
    3
}
fn d_heal() -> f32 {
    200.0
}
fn d_shield() -> f32 {
    8.0
}
fn d_boost_secs() -> f32 {
    10.0
}
fn d_boost_speed() -> f32 {
    1.35
}
fn d_boost_dmg() -> f32 {
    1.5
}
fn d_q_dmg() -> f32 {
    35.0
}
fn d_q_cdmg() -> f32 {
    40.0
}
fn d_sab_slow() -> f32 {
    0.6
}
fn d_cine_secs() -> f32 {
    6.0
}
fn d_cine_fov() -> f32 {
    72.0
}
fn d_ann_secs() -> f32 {
    2.5
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Trigger {
    /// Identity for publish / remove; defaults to the first `on` pattern.
    #[serde(default)]
    pub name: String,
    /// `gift:rose`, `gift:*`, `like`, `comment:~word`, `follow`, `share`, `join`, `subscribe`, `custom:NAME`
    pub on: OneOrMany,
    /// A, B, event (the event's own `side`, else random), random, leader, trailing
    #[serde(default = "d_event")]
    pub side: String,
    #[serde(default, rename = "do", skip_serializing_if = "Option::is_none")]
    pub action: Option<ActionRef>,
    #[serde(default, rename = "macro", skip_serializing_if = "Option::is_none")]
    pub macro_: Option<String>,
    #[serde(default)]
    pub min_count: u32,
    #[serde(default)]
    pub min_diamonds: u32,
    /// fire the action once per gift count (capped by `max_repeat`)
    #[serde(default)]
    pub per_count: bool,
    #[serde(default = "d_max_repeat")]
    pub max_repeat: u32,
    #[serde(default)]
    pub cooldown: f32,
    /// ticker line; `{user} {gift} {count} {side} {label}` are replaced
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub announce: Option<String>,
    /// shown in the ticker and the on-screen gift menu
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    /// list in the on-screen gift menu (default: yes when it has a label)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub legend: Option<bool>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub overlay: Vec<Element>,
    /// do not look at the triggers after this one
    #[serde(default)]
    pub stop: bool,
}

fn d_event() -> String {
    "event".into()
}
fn d_max_repeat() -> u32 {
    6
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Bot {
    pub name: String,
    #[serde(rename = "macro")]
    pub macro_: String,
    /// start when no webhook event arrived for this many seconds
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub when_idle: Option<f32>,
    /// start every this many seconds
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub every: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub when: Option<Cond>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct WarDef {
    #[serde(default = "d_title")]
    pub title: String,
    /// scene file, relative to this file
    pub scene: String,
    pub sides: BTreeMap<String, SideDef>,
    #[serde(default = "d_castle_hp")]
    pub castle_hp: f32,
    #[serde(default, rename = "match")]
    pub match_: MatchDef,
    #[serde(default)]
    pub units: BTreeMap<String, UnitDef>,
    /// soldiers each side starts a round with
    #[serde(default = "d_initial")]
    pub initial_units: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub autospawn: Option<AutoSpawn>,
    #[serde(default)]
    pub actions: BTreeMap<String, Action>,
    #[serde(default)]
    pub triggers: Vec<Trigger>,
    #[serde(default)]
    pub macros: BTreeMap<String, Vec<MStep>>,
    #[serde(default)]
    pub bots: Vec<Bot>,
    /// platform gift name -> trigger gift name (`"mawar": "rose"`)
    #[serde(default)]
    pub gift_aliases: BTreeMap<String, String>,
    /// overlay elements present from the start
    #[serde(default)]
    pub overlay: Vec<Element>,
    /// draw the built-in war HUD (bars, round, timer, ticker, gift menu, banner)
    #[serde(default = "yes")]
    pub hud: bool,
    /// override on-screen texts: round, round_win, draw, match_win, match_draw, lobby
    #[serde(default)]
    pub texts: BTreeMap<String, String>,
    #[serde(default = "d_seed")]
    pub seed: u64,
    /// soldiers alive per side; more are refused
    #[serde(default = "d_max_units")]
    pub max_units: usize,
    /// scene asset names for effects: meteor, boom, arrow, bolt, shield, sparkle
    #[serde(default)]
    pub fx_assets: BTreeMap<String, String>,
    /// terrain / sky / prop themes the war can switch between (`biome` action, `control biome:NAME`, rotation)
    #[serde(default)]
    pub biomes: BTreeMap<String, BiomeDef>,
    /// biome at the start (a key of `biomes`)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub biome: Option<String>,
    /// change biome by itself: every round or every N seconds
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub biome_rotation: Option<Rotation>,
    /// keeps the war going when nobody sends anything (default on; `null` turns it off)
    #[serde(default = "d_autopilot")]
    pub autopilot: Option<Autopilot>,
}

fn d_title() -> String {
    "PERANG KERAJAAN".into()
}
fn d_castle_hp() -> f32 {
    1000.0
}
fn d_initial() -> u32 {
    3
}
fn d_seed() -> u64 {
    20240607
}
fn d_max_units() -> usize {
    36
}

fn d_defeat() -> String {
    "die".into()
}
fn d_restart() -> Option<f32> {
    Some(15.0)
}
fn d_chant_count() -> u32 {
    4
}
fn d_chant_secs() -> f32 {
    3.0
}
fn d_push_strength() -> f32 {
    7.0
}
fn d_push_radius() -> f32 {
    4.5
}
fn d_cloud_radius() -> f32 {
    3.5
}
fn d_cloud_secs() -> f32 {
    8.0
}
fn d_cloud_slow() -> f32 {
    0.5
}
fn d_conv_count() -> u32 {
    2
}
fn d_conv_radius() -> f32 {
    6.0
}
fn d_random() -> String {
    "random".into()
}
fn d_round_every() -> String {
    "round".into()
}
fn d_rot_secs() -> f32 {
    45.0
}
fn d_ap_idle() -> f32 {
    6.0
}
fn d_ap_interval() -> [f32; 2] {
    [2.5, 6.0]
}
fn d_autopilot() -> Option<Autopilot> {
    Some(Autopilot { idle_secs: d_ap_idle(), interval: d_ap_interval(), actions: vec![] })
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PropSwap {
    /// entities whose id starts with this
    pub prefix: String,
    pub asset: String,
}

/// Something the biome does by itself while a round is fought (eruptions, storms).
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Hazard {
    pub every: f32,
    #[serde(rename = "do")]
    pub action: ActionRef,
    #[serde(default = "d_random")]
    pub side: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct BiomeDef {
    /// shown on screen (`{biome}`)
    #[serde(default)]
    pub name: String,
    /// sky preset: day, dawn, sunset, night, desert, snow, lava
    #[serde(default)]
    pub sky: String,
    /// legend tile key -> new top / side colour
    #[serde(default)]
    pub terrain: BTreeMap<String, TilePatch>,
    /// swap the asset of scenery entities by id prefix
    #[serde(default)]
    pub props: Vec<PropSwap>,
    /// speed multiplier for every soldier
    #[serde(default = "one")]
    pub speed: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hazard: Option<Hazard>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Rotation {
    /// "round" or "secs"
    #[serde(default = "d_round_every")]
    pub every: String,
    #[serde(default = "d_rot_secs")]
    pub secs: f32,
    /// biome keys in order (default: all, sorted)
    #[serde(default)]
    pub list: Vec<String>,
    #[serde(default)]
    pub random: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AutoMove {
    #[serde(default = "one")]
    pub weight: f32,
    #[serde(rename = "do")]
    pub action: ActionRef,
    #[serde(default = "d_random")]
    pub side: String,
}

/// Plays the war by itself while the stream is quiet, so an empty live never sits still.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Autopilot {
    /// seconds without a webhook event before it takes over
    #[serde(default = "d_ap_idle")]
    pub idle_secs: f32,
    /// seconds between its moves, [min, max]
    #[serde(default = "d_ap_interval")]
    pub interval: [f32; 2],
    /// its moves; empty = reinforce a random side, or fire any named action on a random side
    #[serde(default)]
    pub actions: Vec<AutoMove>,
}

// ---------------------------------------------------------------- events

/// A viewer event as the connector app delivers it. `from_json` also reads the common platform
/// spellings (`uniqueId`, `nickname`, `giftName`, `repeatCount`, `diamondCount`, `comment`).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Event {
    #[serde(rename = "type", default = "d_kind")]
    pub kind: String,
    #[serde(default = "d_user")]
    pub user: String,
    #[serde(default)]
    pub gift: String,
    #[serde(default = "one_u")]
    pub count: u32,
    #[serde(default)]
    pub diamonds: u32,
    #[serde(default)]
    pub text: String,
    /// "A" / "B": the side the viewer supports, when the connector knows it
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub side: Option<String>,
    /// connector-side id; repeats of the same id are ignored
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
}

fn d_kind() -> String {
    "gift".into()
}
fn d_user() -> String {
    "viewer".into()
}

impl Event {
    pub fn from_json(v: &Value) -> Result<Event> {
        let o = v.as_object().ok_or_else(|| anyhow!("event must be a JSON object"))?;
        let s = |keys: &[&str]| keys.iter().find_map(|k| o.get(*k).and_then(|x| x.as_str()).filter(|x| !x.is_empty())).map(str::to_string);
        let n = |keys: &[&str]| {
            keys.iter().find_map(|k| o.get(*k).and_then(|x| x.as_u64().or_else(|| x.as_f64().map(|f| f.max(0.0) as u64)).or_else(|| x.as_str().and_then(|t| t.parse().ok()))))
        };
        let kind = s(&["type", "event", "kind"]).unwrap_or_else(d_kind).to_ascii_lowercase();
        Ok(Event {
            kind,
            user: s(&["user", "nickname", "uniqueId", "username", "userId"]).unwrap_or_else(d_user),
            gift: s(&["gift", "giftName", "gift_name", "name"]).unwrap_or_default(),
            count: n(&["count", "repeatCount", "repeat_count", "amount"]).unwrap_or(1).clamp(1, 100_000) as u32,
            diamonds: n(&["diamonds", "diamondCount", "diamond_count"]).unwrap_or(0).min(u32::MAX as u64) as u32,
            text: s(&["text", "comment"]).unwrap_or_default(),
            side: s(&["side", "team"]),
            id: s(&["id", "msgId", "eventId"]),
        })
    }
}

pub(crate) fn norm(s: &str) -> String {
    s.to_lowercase().chars().filter(|c| !matches!(c, ' ' | '_' | '-')).collect()
}

pub(crate) fn pattern_matches(pat: &str, ev: &Event, gift_norm: &str) -> bool {
    let (kind, arg) = match pat.split_once(':') {
        Some((k, a)) => (k, Some(a)),
        None => (pat, None),
    };
    if !kind.trim().eq_ignore_ascii_case(&ev.kind) {
        return false;
    }
    match arg.map(str::trim) {
        None | Some("*") | Some("") => true,
        Some(a) if a.starts_with('~') => ev.text.to_lowercase().contains(&a[1..].to_lowercase()),
        Some(a) => norm(a) == gift_norm,
    }
}

// ---------------------------------------------------------------- runtime state

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Phase {
    Lobby(f32),
    Fighting,
    RoundEnd(f32),
    MatchEnd(f32),
}

impl Phase {
    pub fn name(&self) -> &'static str {
        match self {
            Phase::Lobby(_) => "lobby",
            Phase::Fighting => "fighting",
            Phase::RoundEnd(_) => "round_end",
            Phase::MatchEnd(_) => "match_end",
        }
    }
}

/// Results of this run of the stream ("session"): rounds won per side, draws, matches.
#[derive(Default, Clone)]
struct Session {
    rounds: u32,
    draws: u32,
    wins: [u32; 2],
    matches: u32,
    match_draws: u32,
    match_wins: [u32; 2],
    started_unix: u64,
}

#[derive(Default, Clone)]
struct SideState {
    hp: f32,
    wins: u32,
    shield: f32,
    boost: f32,
    boost_speed: f32,
    boost_dmg: f32,
    /// seconds since the castle fell
    destroyed: Option<f32>,
    shield_ent: Option<String>,
    /// castle HP left at the end of each finished round, summed (tiebreak)
    hp_sum: f32,
    /// seconds of slowdown left, and its speed multiplier (sabotage)
    slow_t: f32,
    slow_f: f32,
    /// seconds left of "the camp is under attack" (a unit just hit its castle)
    hit_t: f32,
    /// castle HP as drawn on the bar: eases toward the real value instead of jumping
    hp_disp: f32,
}

#[derive(Clone)]
struct Unit {
    id: String,
    side: usize,
    pos: Vec2,
    hp: f32,
    cd: f32,
    dying: Option<f32>,
    anim_left: f32,
    moving: bool,
    yaw: f32,
    /// shove velocity (water cannon, shockwave); decays
    push: Vec2,
    speech_left: f32,
    def: UnitDef,
}

#[derive(Clone)]
enum Land {
    None,
    Meteor { side: usize, radius: f32, dmg: f32, cdmg: f32, ff: bool, burn: Option<(f32, f32, String)> },
    Arrow { side: usize, dmg: f32, cdmg: f32 },
}

#[derive(Clone)]
enum FxKind {
    Fall { from: Vec3, to: Vec2, dur: f32, land: Land },
    Flash { dur: f32, max: f32 },
    Bolt { dur: f32 },
    Rise { dur: f32 },
    /// expanding (or, with from > to, closing) ground ring: cast marks and meteor telegraphs
    Ring { dur: f32, from: f32, to: f32 },
}

struct Fx {
    id: String,
    kind: FxKind,
    t: f32,
}

/// Enemy soldiers this close to a castle's edge (world units) count as an attack on the base.
const DEFEND_RANGE: f32 = 7.0;

/// A freshly spawned thing growing in with a little overshoot.
struct Pop {
    id: String,
    t: f32,
    dur: f32,
}

/// A castle swelling and settling like a spring when a skill lands on or for its camp.
struct Bounce {
    id: String,
    t: f32,
    dur: f32,
    base: f32,
    amp: f32,
}

fn ease_out_back(u: f32) -> f32 {
    let (c1, c3) = (1.70158f32, 2.70158f32);
    let v = u.clamp(0.0, 1.0) - 1.0;
    1.0 + c3 * v * v * v + c1 * v * v
}

enum What {
    Meteor { target: String, radius: f32, dmg: f32, cdmg: f32, ff: bool, scale: f32, asset: String, burn: Option<(f32, f32, String)> },
    Arrow { target: String, radius: f32, dmg: f32, cdmg: f32 },
    Bolt { dmg: f32, cdmg: f32 },
    Spawn { unit: String, at: String },
    Boom { pos: Vec2, max: f32 },
    Exec { action: ActionRef, ctx: Ctx },
}

struct Job {
    at: f32,
    side: usize,
    what: What,
}

/// A running first-person camera.
struct Cine {
    t: f32,
    total: f32,
    pov: usize,
    look: String,
    fov: f32,
    pitch: Option<f32>,
    bob: f32,
    shake: f32,
    unit_mode: String,
    unit: Option<String>,
    flashes: f32,
    flash_t: f32,
    home: Camera,
}

fn wrap180(a: f32) -> f32 {
    (a + 540.0).rem_euclid(360.0) - 180.0
}

/// A perspective camera that frames the scene like the ortho camera `h` does, so the swoop into a
/// first-person view starts from the picture the viewer is already looking at.
fn persp_equiv(h: &Camera) -> Camera {
    if !h.ortho {
        return *h;
    }
    let d = 90.0f32;
    let fov = 2.0 * ((h.zoom * 0.5) / d).atan().to_degrees();
    Camera { target: h.target, yaw: h.yaw, pitch: h.pitch, dist: d, zoom: h.zoom, fov, ortho: false }
}

/// A lingering area effect (tear gas, smoke) on the ground.
struct Cloud {
    id: String,
    pos: Vec2,
    radius: f32,
    left: f32,
    total: f32,
    slow: f32,
    dps: f32,
    lethal: bool,
    /// whose soldiers it affects
    side: usize,
}

struct Banner {
    text: String,
    color: String,
    left: f32,
    total: f32,
}

#[derive(Clone)]
struct Tick {
    text: String,
    color: String,
}

/// The whole war. `update` it every frame together with the world.
pub struct War {
    pub def: WarDef,
    pub overlay: Overlay,
    sides: [SideState; 2],
    side_keys: [String; 2],
    phase: Phase,
    round: u32,
    round_time: f32,
    time: f32,
    units: Vec<Unit>,
    fx: Vec<Fx>,
    jobs: Vec<Job>,
    runners: Vec<Runner>,
    vars: HashMap<String, f64>,
    rng: u64,
    seq: u64,
    ticker: VecDeque<Tick>,
    banner: Option<Banner>,
    shake: f32,
    idle: f32,
    seen: VecDeque<String>,
    cooldowns: HashMap<String, f32>,
    board: HashMap<String, u32>,
    stopped: bool,
    paused: bool,
    match_winner: Option<Option<usize>>,
    last_round_winner: Option<Option<usize>>,
    session: Session,
    /// round / match results waiting to be written to the history file
    log: Vec<Value>,
    pops: Vec<Pop>,
    bounces: Vec<Bounce>,
    auto_t: f32,
    bot_t: HashMap<String, f32>,
    pub events_seen: u64,
    warned: Vec<String>,
    clouds: Vec<Cloud>,
    biome: Option<String>,
    biome_t: f32,
    hazard_t: f32,
    ap_t: f32,
    ap_next: f32,
    cine: Option<Cine>,
    /// ground position of the first-person eye while a cinematic runs (effects too close to it are hidden)
    cine_eye: Option<Vec2>,
}

fn side_idx(s: &str) -> Option<usize> {
    match s.to_ascii_lowercase().as_str() {
        "a" => Some(0),
        "b" => Some(1),
        _ => None,
    }
}

const SIDE_SPECS: [&str; 6] = ["a", "b", "event", "random", "leader", "trailing"];

impl War {
    /// Reads `war.json` and the scene it names.
    pub fn load(path: &Path) -> Result<(World, War)> {
        let txt = std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
        let def: WarDef = serde_json::from_str(&txt).with_context(|| format!("parse {}", path.display()))?;
        let base = path.parent().map(Path::to_path_buf).unwrap_or_default();
        let mut world = World::load(&base.join(&def.scene)).with_context(|| format!("scene {}", def.scene))?;
        let war = War::new(def, &mut world)?;
        Ok((world, war))
    }

    pub fn new(mut def: WarDef, world: &mut World) -> Result<War> {
        validate(&def, world)?;
        for t in def.triggers.iter_mut() {
            t.name = trigger_name(t);
        }
        let mut keys = def.sides.keys().cloned();
        let side_keys = [keys.next().unwrap_or_default(), keys.next().unwrap_or_default()];
        let mut war = War {
            overlay: Overlay::new(),
            sides: [SideState::default(), SideState::default()],
            side_keys,
            phase: Phase::Lobby(def.match_.lobby_secs.max(0.0)),
            round: 1,
            round_time: 0.0,
            time: 0.0,
            units: vec![],
            fx: vec![],
            jobs: vec![],
            runners: vec![],
            vars: HashMap::new(),
            rng: def.seed.max(1),
            seq: 0,
            ticker: VecDeque::new(),
            banner: None,
            shake: 0.0,
            idle: 0.0,
            seen: VecDeque::new(),
            cooldowns: HashMap::new(),
            board: HashMap::new(),
            stopped: false,
            paused: false,
            match_winner: None,
            last_round_winner: None,
            session: Session { started_unix: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0), ..Default::default() },
            log: vec![],
            pops: vec![],
            bounces: vec![],
            auto_t: 0.0,
            bot_t: HashMap::new(),
            events_seen: 0,
            warned: vec![],
            clouds: vec![],
            biome: None,
            biome_t: 0.0,
            hazard_t: 0.0,
            ap_t: 0.0,
            ap_next: 4.0,
            cine: None,
            cine_eye: None,
            def,
        };
        for s in war.sides.iter_mut() {
            s.hp = war.def.castle_hp;
        }
        let els = war.def.overlay.clone();
        for e in els {
            war.overlay.add(e);
        }
        if let Some(b) = war.def.biome.clone() {
            war.set_biome(world, &b)?;
        }
        let lobby = war.text("lobby", &Vars::new());
        war.banner = Some(Banner { text: lobby, color: "#ffffff".into(), left: war.def.match_.lobby_secs, total: 0.4 });
        Ok(war)
    }

    // ------------------------------------------------------------ small helpers

    pub fn warn(&mut self, msg: String) {
        if !self.warned.contains(&msg) {
            eprintln!("[warn] {msg}");
            self.warned.push(msg);
        }
    }

    pub fn rand(&mut self) -> f32 {
        let mut x = self.rng;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.rng = x;
        ((x.wrapping_mul(0x2545F4914F6CDD1D) >> 40) as f32) / (1u64 << 24) as f32
    }

    fn range(&mut self, a: f32, b: f32) -> f32 {
        a + (b - a) * self.rand()
    }

    fn next_id(&mut self, prefix: &str) -> String {
        self.seq += 1;
        format!("{prefix}{}", self.seq)
    }

    pub fn phase(&self) -> Phase {
        self.phase
    }

    pub fn shake(&self) -> f32 {
        self.shake
    }

    pub fn get_var(&self, name: &str) -> f64 {
        self.vars.get(name).copied().unwrap_or(0.0)
    }

    pub fn set_var(&mut self, name: &str, v: f64) {
        self.vars.insert(name.to_string(), v);
    }

    pub fn macro_steps(&self, name: &str) -> Option<Vec<MStep>> {
        self.def.macros.get(name).cloned()
    }

    fn text(&self, key: &str, vars: &Vars) -> String {
        let default = match key {
            "lobby" => "BERSIAP...",
            "round" => "BABAK {round}",
            "round_win" => "{name} MENANG!",
            "draw" => "SERI!",
            "match_win" => "{name} JUARA!",
            "match_draw" => "SERI! TIDAK ADA PEMENANG",
            "label_hp" => "HP",
            "label_wins" => "MENANG",
            "label_losses" => "KALAH",
            "label_round" => "BABAK",
            "hint" => "MODE OTOMATIS: KIRIM GIFT UNTUK IKUT CAMPUR!",
            _ => "",
        };
        interpolate(self.def.texts.get(key).map(String::as_str).unwrap_or(default), vars)
    }

    fn side_def(&self, s: usize) -> &SideDef {
        &self.def.sides[&self.side_keys[s]]
    }

    fn side_name(&self, s: usize) -> String {
        self.side_def(s).name.clone()
    }

    fn side_color(&self, s: usize) -> String {
        self.side_def(s).color.clone()
    }

    fn asset(&self, fx: &str) -> String {
        self.def.fx_assets.get(fx).cloned().unwrap_or_else(|| fx.to_string())
    }

    fn castle_pos(&self, world: &World, s: usize) -> Vec2 {
        match world.entity_idx(&self.side_def(s).castle) {
            Some(i) => Vec2::new(world.entities[i].pos.x, world.entities[i].pos.z),
            None => Vec2::from(self.side_def(s).spawn),
        }
    }

    fn alive(&self, s: usize) -> usize {
        self.units.iter().filter(|u| u.side == s && u.dying.is_none() && !u.def.structure).count()
    }

    pub fn resolve_side(&mut self, spec: &str, ctx: &Ctx) -> usize {
        match spec.to_ascii_lowercase().as_str() {
            "a" => 0,
            "b" => 1,
            "event" => ctx
                .side
                .or_else(|| ctx.event.as_ref().and_then(|e| e.side.as_deref()).and_then(|s| self.side_lookup(s)))
                .unwrap_or_else(|| usize::from(self.rand() > 0.5)),
            "leader" => usize::from(self.sides[1].hp > self.sides[0].hp),
            "trailing" => usize::from(self.sides[1].hp <= self.sides[0].hp),
            _ => usize::from(self.rand() > 0.5),
        }
    }

    fn side_lookup(&self, s: &str) -> Option<usize> {
        side_idx(s).or_else(|| (0..2).find(|&i| self.side_name(i).eq_ignore_ascii_case(s)))
    }

    // ------------------------------------------------------------ conditions

    pub fn eval(&mut self, c: &Cond, ctx: &Ctx) -> bool {
        let sd = |w: &War, s: &str| side_idx(s).or_else(|| w.side_lookup(s)).unwrap_or(0);
        match c {
            Cond::HpPctLt { side, value } => {
                let i = sd(self, side);
                self.sides[i].hp / self.def.castle_hp * 100.0 < *value
            }
            Cond::HpPctGt { side, value } => {
                let i = sd(self, side);
                self.sides[i].hp / self.def.castle_hp * 100.0 > *value
            }
            Cond::UnitsLt { side, value } => {
                let i = sd(self, side);
                (self.alive(i) as u32) < *value
            }
            Cond::UnitsGt { side, value } => {
                let i = sd(self, side);
                self.alive(i) as u32 > *value
            }
            Cond::Round(n) => self.round == *n,
            Cond::RoundGte(n) => self.round >= *n,
            Cond::Phase(p) => self.phase.name().eq_ignore_ascii_case(p),
            Cond::Leading(s) => {
                let i = sd(self, s);
                self.sides[i].hp > self.sides[1 - i].hp
            }
            Cond::IdleSecsGt(t) => self.idle > *t,
            Cond::RoundSecsGt(t) => matches!(self.phase, Phase::Fighting) && self.round_time > *t,
            Cond::Var { name, gte, lte, eq } => {
                let v = self.get_var(name);
                gte.is_none_or(|x| v >= x) && lte.is_none_or(|x| v <= x) && eq.is_none_or(|x| (v - x).abs() < 1e-9)
            }
            Cond::Chance(p) => self.rand() < *p,
            Cond::Not(c) => !self.eval(c, ctx),
            Cond::All(v) => v.iter().all(|c| self.eval(c, ctx)),
            Cond::Any(v) => v.iter().any(|c| self.eval(c, ctx)),
        }
    }

    // ------------------------------------------------------------ entities

    fn spawn_entity(&mut self, world: &mut World, asset: &str, pos: Vec2, yaw: f32, scale: f32, elevation: f32, palette: BTreeMap<String, String>, prefix: &str) -> Option<String> {
        let id = self.next_id(prefix);
        let d = EntityDef {
            id: id.clone(),
            asset: asset.to_string(),
            pos: pos.to_array(),
            yaw,
            scale,
            palette,
            anims: BTreeMap::new(),
            footprint: [0.0, 0.0],
            label: None,
            base_anim: None,
            elevation,
            building: None,
            floor: 0,
        };
        match world.spawn(&d) {
            Ok(()) => Some(id),
            Err(e) => {
                self.warn(format!("effect asset '{asset}': {e:#}"));
                None
            }
        }
    }

    fn spawn_unit(&mut self, world: &mut World, side: usize, kind: &str, at: &str) -> bool {
        let Some(def) = self.def.units.get(kind).cloned() else {
            self.warn(format!("unknown unit '{kind}'"));
            return false;
        };
        if def.structure {
            if self.units.iter().filter(|u| u.def.structure && u.dying.is_none()).count() >= 24 {
                return false;
            }
        } else if self.alive(side) >= self.def.max_units {
            return false;
        }
        let sd = self.side_def(side).clone();
        let zone = sd.zone;
        let dir = (self.castle_pos(world, 1 - side).x - self.castle_pos(world, side).x).signum();
        let own_front = self.units.iter().filter(|u| u.side == side && !u.def.structure && u.dying.is_none()).map(|u| u.pos.x * dir).fold(f32::NEG_INFINITY, f32::max);
        let z = self.range(zone[1] + 1.0, zone[3] - 1.0);
        let base_x = match at {
            "front" => {
                let fx = if own_front.is_finite() { own_front * dir + dir * 2.5 } else { sd.spawn[0] + dir * 5.0 };
                fx.clamp(zone[0] + 1.0, zone[2] - 1.0)
            }
            "center" => (self.castle_pos(world, 0).x + self.castle_pos(world, 1).x) / 2.0,
            _ => sd.spawn[0],
        };
        let pos = Vec2::new(base_x + self.range(-0.8, 0.8), z);
        let mut palette = BTreeMap::new();
        if !def.tint.is_empty() {
            palette.insert(def.tint.clone(), sd.color.clone());
        }
        let Some(id) = self.spawn_entity(world, &def.asset, pos, sd.facing, def.scale, def.elevation, palette, "u") else { return false };
        let cd = self.rand() * def.cooldown;
        self.pops.push(Pop { id: id.clone(), t: 0.0, dur: 0.4 });
        if let Some(k) = world.entity_idx(&id) {
            world.entities[k].scale = 0.02;
        }
        self.units.push(Unit { id, side, pos, hp: def.hp, cd, dying: None, anim_left: 0.0, moving: false, yaw: sd.facing, push: Vec2::ZERO, speech_left: 0.0, def });
        true
    }

    fn remove_entity(world: &mut World, id: &str) {
        world.remove(id);
    }

    // ------------------------------------------------------------ cinematic camera

    fn end_cine(&mut self, world: &mut World) {
        if let Some(c) = self.cine.take() {
            world.rig.cam = c.home;
            self.cine_eye = None;
            self.overlay.remove("cine.tint");
            self.overlay.remove("cine.vig");
        }
    }

    fn start_cine(&mut self, world: &mut World, c: Cine, tint: &str, vignette: f32) {
        world.rig.follow = None;
        world.rig.spin = 0.0;
        // a camera step still tweening would fight us: a no-op Set cancels it
        world.rig.apply(&CameraCmd::Set(CamParams::default()), None);
        let secs = c.total;
        let mut c = c;
        c.home = world.rig.cam;
        self.cine = Some(c);
        if !tint.is_empty() {
            self.add_json_overlay(json!({"id": "cine.tint", "type": "rect", "full": true, "color": tint, "ttl": secs, "anim_in": "fade", "anim_out": "fade", "anim_secs": 0.8, "z": -1}));
        }
        if vignette > 0.0 {
            self.add_json_overlay(json!({"id": "cine.vig", "type": "vignette", "strength": vignette, "ttl": secs, "anim_in": "fade", "anim_out": "fade", "anim_secs": 0.8, "z": -1}));
        }
    }

    fn add_json_overlay(&mut self, v: Value) {
        match serde_json::from_value::<Element>(v) {
            Ok(e) => {
                self.overlay.add(e);
            }
            Err(e) => self.warn(format!("cinematic overlay: {e}")),
        }
    }

    /// A soldier of `pov` to see through: the one closest to the enemy that is still well short of the enemy
    /// objective, so the fight is in front of the lens and not a wall. None = stand at the gate instead.
    fn pick_pov_unit(&mut self, world: &World, pov: usize, mode: &str) -> Option<String> {
        let enemy = self.castle_pos(world, 1 - pov);
        let mut cands: Vec<usize> = (0..self.units.len())
            .filter(|&i| self.units[i].side == pov && self.units[i].dying.is_none() && !self.units[i].def.structure && self.units[i].pos.distance(enemy) >= 10.0)
            .collect();
        if cands.is_empty() || matches!(mode, "" | "camp" | "castle") {
            return None;
        }
        let i = if mode == "random" {
            cands[(self.rand() * cands.len() as f32) as usize % cands.len()]
        } else {
            cands.sort_by(|&a, &b| self.units[a].pos.distance(enemy).total_cmp(&self.units[b].pos.distance(enemy)));
            cands[cands.len() / 3]
        };
        Some(self.units[i].id.clone())
    }

    /// The first-person camera for this instant: at the soldier's eyes, looking along its heading.
    fn fpv_camera(&mut self, world: &World, c: &mut Cine, home_p: &Camera) -> Camera {
        let enemy = self.castle_pos(world, 1 - c.pov);
        let alive = c.unit.as_ref().is_some_and(|id| self.units.iter().any(|u| &u.id == id && u.dying.is_none() && u.pos.distance(enemy) >= 6.0));
        if !alive {
            let mode = c.unit_mode.clone();
            c.unit = self.pick_pov_unit(world, c.pov, &mode);
        }
        let mut moving = false;
        let (eye, yaw_u) = match c.unit.as_ref().and_then(|id| self.units.iter().find(|u| &u.id == id)) {
            Some(u) => {
                let hy = world.entity_idx(&u.id).map(|i| world.entities[i].pos.y + world.entities[i].elevation + world.entities[i].model.height * world.entities[i].scale * 0.92).unwrap_or(1.6);
                let f = Vec3::new(u.yaw.to_radians().sin(), 0.0, u.yaw.to_radians().cos());
                moving = u.moving;
                (Vec3::new(u.pos.x, hy + 0.3, u.pos.y) + f * 0.55, u.yaw)
            }
            None => {
                // the camp itself: raised above the gate of its objective, slowly sweeping the field
                let cp = self.castle_pos(world, c.pov);
                let facing = self.side_def(c.pov).facing + 9.0 * (c.t * 0.5).sin();
                let gy = world.terrain.height_at(cp.x, cp.y) + 3.6;
                let f = Vec3::new(facing.to_radians().sin(), 0.0, facing.to_radians().cos());
                (Vec3::new(cp.x, gy, cp.y) + f * (self.side_def(c.pov).castle_radius + 0.6 + 0.5 * (c.t * 0.7).sin()), facing)
            }
        };
        let t = c.t;
        let (mut y, mut p) = match c.look.as_str() {
            "castle" => {
                let cp = self.castle_pos(world, 1 - c.pov);
                let l = (Vec3::new(cp.x, 4.0, cp.y) - eye).normalize_or_zero();
                ((-l.x).atan2(-l.z).to_degrees(), (-l.y).clamp(-1.0, 1.0).asin().to_degrees())
            }
            "sky" => (yaw_u + 180.0, c.pitch.unwrap_or(-30.0)),
            _ => (yaw_u + 180.0, c.pitch.unwrap_or(6.0)),
        };
        if c.bob > 0.0 && c.unit.is_some() {
            p += c.bob * if moving { 1.0 } else { 0.3 } * 0.9 * (t * 9.0).sin();
        }
        y = home_p.yaw + wrap180(y - home_p.yaw);
        let (yr, pr) = (y.to_radians(), p.to_radians());
        let v = Vec3::new(pr.cos() * yr.sin(), pr.sin(), pr.cos() * yr.cos());
        let d = 5.0;
        let mut target = eye - v * d;
        if c.shake > 0.0 {
            target += Vec3::new((t * 47.0).sin(), (t * 53.0).cos(), (t * 41.0).sin()) * c.shake * 0.25;
        }
        Camera { target, yaw: y, pitch: p, dist: d, zoom: home_p.zoom, fov: c.fov, ortho: false }
    }

    fn step_cine(&mut self, world: &mut World, dt: f32) {
        let Some(mut c) = self.cine.take() else { return };
        c.t += dt;
        if c.t >= c.total {
            world.rig.cam = c.home;
            self.cine_eye = None;
            return;
        }
        world.rig.follow = None;
        world.rig.spin = 0.0;
        let ease = |u: f32| {
            let u = u.clamp(0.0, 1.0);
            u * u * (3.0 - 2.0 * u)
        };
        let k = ease(c.t / 0.9).min(ease((c.total - c.t) / 0.9));
        let home_p = persp_equiv(&c.home);
        let goal = self.fpv_camera(world, &mut c, &home_p);
        world.rig.cam = Camera::lerp(&home_p, &goal, k);
        let eye = world.rig.cam.eye();
        self.cine_eye = Some(Vec2::new(eye.x, eye.z));
        if c.flashes > 0.0 {
            c.flash_t -= dt;
            if c.flash_t <= 0.0 {
                c.flash_t = (1.0 / c.flashes) * (0.5 + self.rand());
                self.add_json_overlay(json!({"type": "rect", "full": true, "color": "#ffffffd0", "ttl": 0.2, "anim_out": "fade", "anim_secs": 0.18, "z": 8}));
                self.shake = self.shake.max(0.3);
            }
        }
        self.cine = Some(c);
    }

    fn clear_field(&mut self, world: &mut World) {
        for u in self.units.drain(..) {
            world.remove(&u.id);
        }
        for f in self.fx.drain(..) {
            world.remove(&f.id);
        }
        for s in self.sides.iter_mut() {
            if let Some(id) = s.shield_ent.take() {
                world.remove(&id);
            }
        }
        for c in self.clouds.drain(..) {
            world.remove(&c.id);
        }
        self.end_cine(world);
    }

    // ------------------------------------------------------------ damage

    fn hit_castle(&mut self, s: usize, dmg: f32) {
        if dmg <= 0.0 || self.sides[s].destroyed.is_some() || !matches!(self.phase, Phase::Fighting) {
            return;
        }
        if self.sides[s].shield > 0.0 {
            self.shake = self.shake.max(0.08);
            return;
        }
        self.sides[s].hp = (self.sides[s].hp - dmg).max(0.0);
        self.shake = self.shake.max((dmg / 250.0).min(0.35));
        if self.sides[s].hp <= 0.0 {
            self.sides[s].destroyed = Some(0.0);
            self.ticker_push(format!("{} HANCUR!", self.side_name(s)), self.side_color(s));
            self.end_round(Some(1 - s));
        }
    }

    fn hurt_unit(&mut self, idx: usize, dmg: f32) {
        let u = &mut self.units[idx];
        if u.dying.is_some() {
            return;
        }
        u.hp -= dmg;
        if u.hp <= 0.0 {
            u.dying = Some(0.0);
        }
    }

    fn explode(&mut self, world: &mut World, at: Vec2, side: usize, radius: f32, dmg: f32, cdmg: f32, ff: bool, burn: Option<(f32, f32, String)>) {
        for i in 0..self.units.len() {
            if self.units[i].dying.is_some() || (!ff && self.units[i].side != side) {
                continue;
            }
            let d = self.units[i].pos.distance(at);
            if d <= radius {
                let fall = 1.0 - 0.6 * (d / radius);
                self.hurt_unit(i, dmg * fall);
            }
        }
        let cp = self.castle_pos(world, side);
        let d = cp.distance(at) - self.side_def(side).castle_radius;
        if d <= radius {
            let fall = 1.0 - 0.6 * (d.max(0.0) / radius);
            self.hit_castle(side, cdmg * fall);
        }
        self.shake = self.shake.max(0.45);
        self.flash(world, at, radius * 0.7, 0.55);
        if let Some((secs, dps, asset)) = burn {
            self.add_cloud(world, at, radius * 0.8, secs, 0.7, dps, false, side, &asset, "fire");
        }
    }

    fn flash(&mut self, world: &mut World, at: Vec2, max: f32, dur: f32) {
        self.fx_flash(world, "boom", at, max, dur);
    }

    /// A short-lived scaled effect from the scene asset named by `key` (skipped when the scene has none).
    fn fx_flash(&mut self, world: &mut World, key: &str, at: Vec2, max: f32, dur: f32) {
        let asset = self.asset(key);
        if !world.models.contains_key(&asset) {
            return;
        }
        if let Some(id) = self.spawn_entity(world, &asset, at, 0.0, 0.1, 0.4, BTreeMap::new(), "fx") {
            self.fx.push(Fx { id, kind: FxKind::Flash { dur, max }, t: 0.0 });
        }
    }

    /// A lingering area on the ground that slows and optionally wears down `side`'s soldiers inside it.
    fn add_cloud(&mut self, world: &mut World, at: Vec2, radius: f32, secs: f32, slow: f32, dps: f32, lethal: bool, side: usize, asset: &str, default_key: &str) {
        let key = if asset.is_empty() { default_key } else { asset };
        let name = self.asset(key);
        let id = if world.models.contains_key(&name) { self.spawn_entity(world, &name, at, 0.0, radius.max(0.5), 0.0, BTreeMap::new(), "cl") } else { None };
        if self.clouds.len() >= 16 {
            let old = self.clouds.remove(0);
            world.remove(&old.id);
        }
        let secs = secs.max(0.5);
        self.clouds.push(Cloud { id: id.unwrap_or_default(), pos: at, radius, left: secs, total: secs, slow: slow.clamp(0.05, 1.0), dps, lethal, side });
    }

    fn step_clouds(&mut self, world: &mut World, dt: f32) {
        let mut done = vec![];
        for (n, c) in self.clouds.iter_mut().enumerate() {
            c.left -= dt;
            if let Some(i) = world.entity_idx(&c.id) {
                let age = c.total - c.left;
                // grows in with a little overshoot, drifts up and down, shrinks away in the last second
                let grow = ease_out_back((age / 0.7).min(1.0));
                let fade = c.left.clamp(0.0, 1.0);
                let e = &mut world.entities[i];
                e.scale = (c.radius * (0.15 + 0.85 * grow) * (0.2 + 0.8 * fade) * (1.0 + 0.04 * (age * 3.0).sin())).max(0.05);
                e.elevation = 0.12 + 0.1 * (age * 1.7).sin();
                if self.cine_eye.is_some_and(|eye| eye.distance(c.pos) < c.radius + 1.0) {
                    e.scale = 0.01;
                }
                e.yaw += 12.0 * dt;
            }
            if c.left <= 0.0 {
                done.push(n);
            }
        }
        for n in done.into_iter().rev() {
            let c = self.clouds.remove(n);
            world.remove(&c.id);
        }
    }

    fn step_pops(&mut self, world: &mut World, dt: f32) {
        for p in self.pops.iter_mut() {
            p.t += dt;
        }
        self.pops.retain(|p| p.t < p.dur && world.entity_idx(&p.id).is_some());
    }

    fn step_bounces(&mut self, world: &mut World, dt: f32) {
        for b in self.bounces.iter_mut() {
            b.t += dt;
            let u = (b.t / b.dur).min(1.0);
            if let Some(i) = world.entity_idx(&b.id) {
                world.entities[i].scale = b.base * (1.0 + b.amp * (-4.5 * u).exp() * (u * std::f32::consts::TAU * 2.0).sin());
            }
        }
        for b in self.bounces.iter().filter(|b| b.t >= b.dur) {
            if let Some(i) = world.entity_idx(&b.id) {
                world.entities[i].scale = b.base;
            }
        }
        self.bounces.retain(|b| b.t < b.dur);
    }

    // ------------------------------------------------------------ biomes

    fn biome_speed(&self) -> f32 {
        self.biome.as_ref().and_then(|k| self.def.biomes.get(k)).map_or(1.0, |b| b.speed.max(0.1))
    }

    pub fn biome_label(&self) -> String {
        match &self.biome {
            Some(k) => self.def.biomes.get(k).map(|b| if b.name.is_empty() { k.clone() } else { b.name.clone() }).unwrap_or_else(|| k.clone()),
            None => String::new(),
        }
    }

    /// Switches terrain colours, sky and scenery to biome `name` (a key of `biomes`, or "random").
    pub fn set_biome(&mut self, world: &mut World, name: &str) -> Result<()> {
        let keys: Vec<String> = self.def.biomes.keys().cloned().collect();
        if keys.is_empty() {
            bail!("this war has no `biomes`");
        }
        let key = if name.eq_ignore_ascii_case("random") { keys[(self.rand() * keys.len() as f32) as usize % keys.len()].clone() } else { name.to_string() };
        let b = self.def.biomes.get(&key).cloned().ok_or_else(|| anyhow!("unknown biome '{name}' (have: {})", keys.join(", ")))?;
        if !b.sky.is_empty() {
            world.set_sky(&b.sky, Some(2.5));
        }
        if !b.terrain.is_empty() {
            world.retheme(&b.terrain)?;
        }
        for p in &b.props {
            let ids: Vec<String> = world.entities.iter().filter(|e| e.id.starts_with(&p.prefix)).map(|e| e.id.clone()).collect();
            for id in ids {
                world.set_asset(&id, &p.asset)?;
            }
        }
        self.biome = Some(key);
        self.hazard_t = 0.0;
        self.biome_t = 0.0;
        Ok(())
    }

    fn next_biome(&mut self, world: &mut World) {
        let Some(rot) = self.def.biome_rotation.clone() else { return };
        let list: Vec<String> = if rot.list.is_empty() { self.def.biomes.keys().cloned().collect() } else { rot.list.clone() };
        if list.is_empty() {
            return;
        }
        let next = if rot.random {
            list[(self.rand() * list.len() as f32) as usize % list.len()].clone()
        } else {
            let cur = self.biome.as_ref().and_then(|k| list.iter().position(|x| x == k));
            list[cur.map_or(0, |i| (i + 1) % list.len())].clone()
        };
        if let Err(e) = self.set_biome(world, &next) {
            self.warn(format!("biome rotation: {e:#}"));
        }
    }

    // ------------------------------------------------------------ autopilot

    /// True while nobody has sent anything for `idle_secs` and the autopilot is playing.
    pub fn autopilot_active(&self) -> bool {
        matches!(self.phase, Phase::Fighting) && self.def.autopilot.as_ref().is_some_and(|a| self.idle >= a.idle_secs)
    }

    fn autopilot_move(&mut self, world: &mut World, ap: &Autopilot) {
        let ctx = Ctx::default();
        if !ap.actions.is_empty() {
            let weights: Vec<f32> = ap.actions.iter().map(|m| self.smart_weight(world, m)).collect();
            let total: f32 = weights.iter().sum();
            if total <= 0.0 {
                return;
            }
            let mut r = self.rand() * total;
            for (m, w) in ap.actions.iter().zip(&weights) {
                r -= w;
                if r <= 0.0 {
                    let side = self.resolve_side(&m.side, &ctx);
                    let label = match &m.action {
                        ActionRef::Name(n) => n.to_uppercase(),
                        ActionRef::Inline(_) => "AKSI".into(),
                    };
                    self.exec(world, &m.action, side, &ctx);
                    let col = self.side_color(side);
                    self.ticker_push(format!("AUTO: {label}"), col);
                    return;
                }
            }
            return;
        }
        let side = usize::from(self.rand() > 0.5);
        let names: Vec<String> = self.def.actions.keys().cloned().collect();
        let col = self.side_color(side);
        if names.is_empty() || self.rand() < 0.6 {
            self.exec(world, &ActionRef::Inline(Box::new(Action::Spawn { unit: String::new(), count: 2, at: String::new() })), side, &ctx);
            self.ticker_push("AUTO: PASUKAN".into(), col);
        } else {
            let n = names[(self.rand() * names.len() as f32) as usize % names.len()].clone();
            self.exec(world, &ActionRef::Name(n.clone()), side, &ctx);
            self.ticker_push(format!("AUTO: {}", n.to_uppercase()), col);
        }
    }

    // ------------------------------------------------------------ targeting

    fn pick_target(&mut self, world: &World, side: usize, mode: &str) -> Vec2 {
        let zone = self.side_def(side).zone;
        let r = self.rand();
        let mode = match mode {
            "castle" | "field" | "units" | "front" => mode,
            _ if r < 0.5 => "units",
            _ if r < 0.8 => "castle",
            _ => "field",
        };
        match mode {
            "castle" => {
                let c = self.castle_pos(world, side);
                c + Vec2::new(self.range(-1.2, 1.2), self.range(-1.2, 1.2))
            }
            "units" | "front" => {
                let mine: Vec<Vec2> = self.units.iter().filter(|u| u.side == side && u.dying.is_none()).map(|u| u.pos).collect();
                if mine.is_empty() {
                    return self.pick_target(world, side, "field");
                }
                let enemy_dir = if side == 0 { 1.0 } else { -1.0 };
                let p = if mode == "front" {
                    *mine.iter().max_by(|a, b| (a.x * enemy_dir).total_cmp(&(b.x * enemy_dir))).unwrap_or(&mine[0])
                } else {
                    let i = (self.rand() * mine.len() as f32) as usize % mine.len();
                    mine[i]
                };
                p + Vec2::new(self.range(-0.6, 0.6), self.range(-0.6, 0.6))
            }
            _ => Vec2::new(self.range(zone[0], zone[2]), self.range(zone[1], zone[3])),
        }
    }

    // ------------------------------------------------------------ actions

    pub fn exec(&mut self, world: &mut World, action: &ActionRef, side: usize, ctx: &Ctx) {
        let a = match action {
            ActionRef::Inline(a) => (**a).clone(),
            ActionRef::Name(n) => match self.def.actions.get(n) {
                Some(a) => a.clone(),
                None => {
                    self.warn(format!("unknown action '{n}'"));
                    return;
                }
            },
        };
        self.react(world, &a, side);
        self.run_action(world, &a, side, ctx);
    }

    /// The camp a skill is for (helpful) or lands on (harmful) answers it: the castle swells and settles like a
    /// spring, and a helpful skill leaves a ring spreading over the ground.
    fn react(&mut self, world: &mut World, a: &Action, side: usize) {
        if !matches!(self.phase, Phase::Fighting) {
            return;
        }
        let (helpful, harmful) = match a {
            Action::Spawn { .. } | Action::Chant { .. } | Action::Shield { .. } | Action::Boost { .. } => (true, false),
            Action::Heal { amount, .. } => (*amount > 0.0, *amount <= 0.0),
            Action::Meteor { .. } | Action::Arrows { .. } | Action::Lightning { .. } | Action::Push { .. } | Action::Cloud { .. } | Action::Convert { .. } | Action::Sabotage { .. } => (false, true),
            _ => (false, false),
        };
        if helpful {
            self.bounce_castle(world, side, 0.06);
            let (cp, r, col) = (self.castle_pos(world, side), self.side_def(side).castle_radius, self.side_color(side));
            self.ring_fx(world, cp, &col, r * 0.4, r * 1.7, 0.9);
        } else if harmful {
            self.bounce_castle(world, side, 0.03);
        }
    }

    fn bounce_castle(&mut self, world: &World, side: usize, amp: f32) {
        let id = self.side_def(side).castle.clone();
        if let Some(b) = self.bounces.iter_mut().find(|b| b.id == id) {
            b.t = 0.0;
            b.amp = amp;
            return;
        }
        if let Some(k) = world.entity_idx(&id) {
            self.bounces.push(Bounce { id, t: 0.0, dur: 0.7, base: world.entities[k].scale, amp });
        }
    }

    /// A glowing ring on the ground (needs a `ring` asset; without one nothing is drawn).
    fn ring_fx(&mut self, world: &mut World, at: Vec2, color: &str, from: f32, to: f32, dur: f32) {
        let name = self.asset("ring");
        if !world.models.contains_key(&name) || self.fx.len() > 90 {
            return;
        }
        let mut pal = BTreeMap::new();
        pal.insert("a".to_string(), color.to_string());
        if let Some(id) = self.spawn_entity(world, &name, at, 0.0, from.max(0.05), 0.06, pal, "fx") {
            self.fx.push(Fx { id, kind: FxKind::Ring { dur, from, to }, t: 0.0 });
        }
    }

    /// How hard a camp is pressed: enemy soldiers at its gate (up to 3) plus one when its castle is below 40%.
    fn base_danger(&self, world: &World, s: usize) -> f32 {
        let c = self.castle_pos(world, s);
        let r = self.side_def(s).castle_radius;
        let n = self.units.iter().filter(|u| u.side == 1 - s && u.dying.is_none() && !u.def.structure && u.pos.distance(c) - r <= DEFEND_RANGE).count() as f32;
        n.min(3.0) + if self.sides[s].hp / self.def.castle_hp < 0.4 { 1.0 } else { 0.0 }
    }

    /// Autopilot weight of one move: a camp whose base is pressed gets more of the moves that help it (heal, reinforce,
    /// cheer) and more of the moves that hit its attackers.
    fn smart_weight(&self, world: &World, m: &AutoMove) -> f32 {
        let base = m.weight.max(0.0);
        let Some(side) = side_idx(&m.side).or_else(|| self.side_lookup(&m.side)) else { return base };
        let act = match &m.action {
            ActionRef::Inline(a) => Some((**a).clone()),
            ActionRef::Name(n) => self.def.actions.get(n).cloned(),
        };
        let Some(act) = act else { return base };
        let helpful = match &act {
            Action::Spawn { .. } | Action::Chant { .. } | Action::Shield { .. } | Action::Boost { .. } => Some(true),
            Action::Heal { amount, .. } => Some(*amount > 0.0),
            Action::Meteor { .. } | Action::Arrows { .. } | Action::Lightning { .. } | Action::Push { .. } | Action::Cloud { .. } | Action::Convert { .. } | Action::Sabotage { .. } => Some(false),
            _ => None,
        };
        match helpful {
            Some(true) => base * (1.0 + 3.0 * self.base_danger(world, side)),
            Some(false) => base * (1.0 + 2.0 * self.base_danger(world, 1 - side)),
            None => base,
        }
    }

    fn schedule(&mut self, delay: f32, side: usize, what: What) {
        self.jobs.push(Job { at: self.time + delay.max(0.0), side, what });
    }

    fn run_action(&mut self, world: &mut World, a: &Action, side: usize, ctx: &Ctx) {
        match a {
            Action::Meteor { count, radius, damage, castle_damage, gap, target, friendly_fire, scale, asset, burn_secs, burn_dps, burn_asset } => {
                let burn = (*burn_secs > 0.0).then(|| (*burn_secs, *burn_dps, burn_asset.clone()));
                for i in 0..(*count).clamp(1, 40) {
                    self.schedule(i as f32 * gap, side, What::Meteor { target: target.clone(), radius: *radius, dmg: *damage, cdmg: *castle_damage, ff: *friendly_fire, scale: *scale, asset: asset.clone(), burn: burn.clone() });
                }
            }
            Action::Arrows { count, radius, damage, castle_damage, target } => {
                for i in 0..(*count).clamp(1, 60) {
                    self.schedule(i as f32 * 0.07, side, What::Arrow { target: target.clone(), radius: *radius, dmg: *damage, cdmg: *castle_damage });
                }
            }
            Action::Lightning { count, damage, castle_damage } => {
                for i in 0..(*count).clamp(1, 12) {
                    self.schedule(i as f32 * 0.25, side, What::Bolt { dmg: *damage, cdmg: *castle_damage });
                }
            }
            Action::Spawn { unit, count, at } => {
                let kind = if unit.is_empty() { self.side_def(side).unit.clone() } else { unit.clone() };
                for i in 0..(*count).clamp(1, 30) {
                    self.schedule(i as f32 * 0.3, side, What::Spawn { unit: kind.clone(), at: at.clone() });
                }
            }
            Action::Heal { amount, asset } => {
                if self.sides[side].destroyed.is_none() {
                    if *amount < 0.0 {
                        self.hit_castle(side, -*amount);
                    } else {
                        self.sides[side].hp = (self.sides[side].hp + amount).min(self.def.castle_hp);
                    }
                    let cp = self.castle_pos(world, side);
                    let key = if asset.is_empty() { "sparkle" } else { asset.as_str() };
                    let name = self.asset(key);
                    if world.models.contains_key(&name) {
                        if let Some(id) = self.spawn_entity(world, &name, cp, 0.0, 1.0, 1.0, BTreeMap::new(), "fx") {
                            self.fx.push(Fx { id, kind: FxKind::Rise { dur: 1.4 }, t: 0.0 });
                        }
                    }
                }
            }
            Action::Chant { texts, count, secs, anim, boost_secs } => {
                let anim = if anim.is_empty() { "cheer" } else { anim.as_str() };
                let mut idx: Vec<usize> = (0..self.units.len()).filter(|&i| self.units[i].side == side && self.units[i].dying.is_none() && !self.units[i].def.structure).collect();
                let n = (*count as usize).min(idx.len());
                for k in 0..n {
                    let j = k + (self.rand() * (idx.len() - k) as f32) as usize % (idx.len() - k);
                    idx.swap(k, j);
                }
                for &i in &idx[..n] {
                    let text = if texts.is_empty() { "AYO!".to_string() } else { texts[(self.rand() * texts.len() as f32) as usize % texts.len()].clone() };
                    let vars = self.ctx_vars(ctx);
                    let text = interpolate(&text, &vars);
                    self.units[i].speech_left = *secs;
                    if let Some(k) = world.entity_idx(&self.units[i].id) {
                        let e = &mut world.entities[k];
                        e.speech = Some(text);
                        if e.clip(anim).is_some() {
                            e.anim.oneshot = Some(anim.to_string());
                            e.anim.oneshot_loop = true;
                            e.anim.t = 0.0;
                        }
                    }
                }
                if *boost_secs > 0.0 {
                    let s = &mut self.sides[side];
                    s.boost = s.boost.max(*boost_secs);
                    s.boost_speed = s.boost_speed.max(1.2);
                    s.boost_dmg = s.boost_dmg.max(1.3);
                }
            }
            Action::Push { strength, radius, damage, target } => {
                let c = self.pick_target(world, side, if target.is_empty() { "front" } else { target });
                for i in 0..self.units.len() {
                    if self.units[i].side != side || self.units[i].dying.is_some() || self.units[i].def.structure {
                        continue;
                    }
                    let d = self.units[i].pos.distance(c);
                    if d <= *radius {
                        let dir = (self.units[i].pos - c).normalize_or_zero();
                        let dir = if dir == Vec2::ZERO { Vec2::X } else { dir };
                        let k = 1.0 - d / radius;
                        self.units[i].push += dir * *strength * (0.4 + 0.6 * k) * 2.0;
                        if *damage > 0.0 {
                            self.hurt_unit(i, damage * k);
                        }
                    }
                }
                self.shake = self.shake.max(0.25);
                self.fx_flash(world, "push", c, radius * 0.45, 0.7);
            }
            Action::Cloud { radius, secs, slow, dps, lethal, asset, target } => {
                let c = self.pick_target(world, side, if target.is_empty() { "front" } else { target });
                self.add_cloud(world, c, *radius, *secs, *slow, *dps, *lethal, side, asset, "cloud");
            }
            Action::Convert { count, radius, target } => {
                let c = self.pick_target(world, side, if target.is_empty() { "front" } else { target });
                let mut cand: Vec<(usize, f32)> = (0..self.units.len())
                    .filter(|&i| self.units[i].side == side && self.units[i].dying.is_none() && !self.units[i].def.structure)
                    .map(|i| (i, self.units[i].pos.distance(c)))
                    .filter(|(_, d)| *d <= *radius)
                    .collect();
                cand.sort_by(|a, b| a.1.total_cmp(&b.1));
                let new = 1 - side;
                let (facing, color) = (self.side_def(new).facing, self.side_color(new));
                for (i, _) in cand.into_iter().take((*count).clamp(1, 20) as usize) {
                    self.units[i].side = new;
                    self.units[i].yaw = facing;
                    self.units[i].push = Vec2::ZERO;
                    let (id, tint, pos) = (self.units[i].id.clone(), self.units[i].def.tint.clone(), self.units[i].pos);
                    if !tint.is_empty() {
                        let _ = world.set_palette(&id, &tint, &color);
                    }
                    self.fx_flash(world, "sparkle", pos, 0.5, 0.8);
                }
            }
            Action::Biome { name } => {
                let n = name.clone();
                if let Err(e) = self.set_biome(world, &n) {
                    self.warn(format!("biome action: {e:#}"));
                }
            }
            Action::Cinematic { pov, secs, look, fov, pitch, bob, shake, unit, tint, vignette, flashes } => {
                if self.cine.is_some() || !matches!(self.phase, Phase::Fighting) {
                    return;
                }
                let pov_side = match pov.to_ascii_lowercase().as_str() {
                    "self" => side,
                    "a" => 0,
                    "b" => 1,
                    _ => 1 - side,
                };
                let c = Cine {
                    t: 0.0,
                    total: secs.clamp(1.5, 20.0),
                    pov: pov_side,
                    look: look.clone(),
                    fov: fov.clamp(30.0, 110.0),
                    pitch: *pitch,
                    bob: *bob,
                    shake: *shake,
                    unit_mode: unit.clone(),
                    unit: None,
                    flashes: *flashes,
                    flash_t: 0.0,
                    home: world.rig.cam,
                };
                self.start_cine(world, c, tint, *vignette);
            }
            Action::Sabotage { slow_secs, slow, structures } => {
                let s = &mut self.sides[side];
                s.shield = 0.0;
                s.boost = 0.0;
                if *slow_secs > 0.0 {
                    s.slow_t = *slow_secs;
                    s.slow_f = slow.clamp(0.05, 1.0);
                }
                if *structures {
                    for i in 0..self.units.len() {
                        if self.units[i].side == side && self.units[i].def.structure {
                            self.hurt_unit(i, 1.0e6);
                        }
                    }
                }
                self.shake = self.shake.max(0.2);
            }
            Action::Shield { secs } => self.sides[side].shield = self.sides[side].shield.max(*secs),
            Action::Boost { secs, speed, damage } => {
                let s = &mut self.sides[side];
                s.boost = s.boost.max(*secs);
                s.boost_speed = *speed;
                s.boost_dmg = *damage;
            }
            Action::Quake { damage, castle_damage } => {
                self.shake = self.shake.max(1.0);
                for i in 0..self.units.len() {
                    if self.units[i].side == side {
                        self.hurt_unit(i, *damage);
                    }
                }
                self.hit_castle(side, *castle_damage);
                let zone = self.side_def(side).zone;
                for k in 0..4 {
                    let p = Vec2::new(self.range(zone[0], zone[2]), self.range(zone[1], zone[3]));
                    self.schedule(k as f32 * 0.12, side, What::Boom { pos: p, max: 1.6 });
                }
            }
            Action::Step { step } => self.world_step(world, step),
            Action::Overlay { add, remove, clear } => {
                let vars = self.ctx_vars(ctx);
                if *clear {
                    self.overlay.clear();
                }
                for id in remove {
                    self.overlay.remove(&interpolate(id, &vars));
                }
                for e in add {
                    self.add_overlay_templated(e, &vars);
                }
            }
            Action::Announce { text, secs, color } => {
                let vars = self.ctx_vars(ctx);
                let color = color.clone().unwrap_or_else(|| self.side_color(side));
                self.announce(interpolate(text, &vars), &color, *secs);
            }
            Action::Seq { actions } => {
                for (i, r) in actions.iter().enumerate() {
                    if i == 0 {
                        self.exec(world, r, side, ctx);
                    } else {
                        self.schedule(0.0, side, What::Exec { action: r.clone(), ctx: ctx.clone() });
                    }
                }
            }
            Action::Macro { name } => match self.def.macros.get(name).cloned() {
                Some(steps) => {
                    let mut c = ctx.clone();
                    c.side = Some(side);
                    self.runners.push(Runner::new(name, steps, c));
                }
                None => self.warn(format!("unknown macro '{name}'")),
            },
        }
    }

    pub fn world_step(&mut self, world: &mut World, step: &Value) {
        if let Err(e) = world.run_json(&step.to_string()) {
            self.warn(format!("world step: {e:#}"));
        }
    }

    fn add_overlay_templated(&mut self, e: &Element, vars: &Vars) {
        if let Err(err) = self.overlay.add_templated(e, vars) {
            self.warn(format!("overlay element: {err}"));
        }
    }

    fn announce(&mut self, text: String, color: &str, secs: f32) {
        self.banner = Some(Banner { text, color: color.to_string(), left: secs, total: secs });
    }

    fn ticker_push(&mut self, text: String, color: String) {
        self.ticker.push_front(Tick { text, color });
        self.ticker.truncate(5);
    }

    // ------------------------------------------------------------ events

    /// A synthetic event from a macro: goes through the triggers but does not count as "not idle".
    pub fn inject(&mut self, world: &mut World, ev: Event) {
        self.process_event(world, ev, false);
    }

    /// Handles one webhook event; the returned JSON lists which triggers fired.
    pub fn handle_event(&mut self, world: &mut World, ev: Event) -> Value {
        self.process_event(world, ev, true)
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

    fn process_event(&mut self, world: &mut World, ev: Event, external: bool) -> Value {
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
        let fighting = matches!(self.phase, Phase::Fighting);
        let mut matched: Vec<String> = vec![];
        let triggers = self.def.triggers.clone();
        for t in &triggers {
            if !t.on.list().iter().any(|p| pattern_matches(p, &ev, &gift_norm)) {
                continue;
            }
            if ev.count < t.min_count || ev.diamonds < t.min_diamonds {
                continue;
            }
            if !fighting {
                matched.push(format!("{} (ignored: {})", t.name, self.phase.name()));
                continue;
            }
            if self.cooldowns.get(&t.name).copied().unwrap_or(0.0) > 0.0 {
                matched.push(format!("{} (cooldown)", t.name));
                continue;
            }
            if t.cooldown > 0.0 {
                self.cooldowns.insert(t.name.clone(), t.cooldown);
            }
            let mut ctx = Ctx { event: Some(ev.clone()), side: None };
            let side = self.resolve_side(&t.side, &ctx);
            ctx.side = Some(side);
            let reps = if t.per_count { ev.count.clamp(1, t.max_repeat.max(1)) } else { 1 };
            if let Some(a) = &t.action {
                for r in 0..reps {
                    if r == 0 {
                        self.exec(world, a, side, &ctx);
                    } else {
                        self.schedule(r as f32 * 0.35, side, What::Exec { action: a.clone(), ctx: ctx.clone() });
                    }
                }
            }
            if let Some(m) = &t.macro_ {
                match self.def.macros.get(m).cloned() {
                    Some(steps) => self.runners.push(Runner::new(m, steps, ctx.clone())),
                    None => self.warn(format!("trigger '{}': unknown macro '{m}'", t.name)),
                }
            }
            let mut vars = self.ctx_vars(&ctx);
            vars.insert("label".into(), t.label.clone().unwrap_or_default());
            vars.insert("icon".into(), t.icon.clone().unwrap_or_default());
            for e in &t.overlay {
                self.add_overlay_templated(e, &vars);
            }
            let line = match (&t.announce, &t.label) {
                (Some(a), _) => interpolate(a, &vars),
                (None, Some(_)) => interpolate("{icon} {user}: {label}", &vars),
                (None, None) => interpolate("{user} {gift} x{count}", &vars),
            };
            let color = self.side_color(side);
            self.ticker_push(line.trim().to_string(), color);
            matched.push(t.name.clone());
            if t.stop {
                break;
            }
        }
        json!({"matched": matched, "phase": self.phase.name()})
    }

    /// Placeholders available to templates: `{user} {gift} {count} {text} {side} {side_key}` + the HUD vars.
    pub fn ctx_vars(&self, ctx: &Ctx) -> Vars {
        let mut v = self.vars();
        if let Some(e) = &ctx.event {
            v.insert("user".into(), e.user.clone());
            v.insert("gift".into(), e.gift.clone());
            v.insert("count".into(), e.count.to_string());
            v.insert("text".into(), e.text.clone());
            v.insert("diamonds".into(), e.diamonds.to_string());
        }
        if let Some(s) = ctx.side {
            v.insert("side".into(), self.side_name(s));
            v.insert("side_key".into(), self.side_keys[s].clone());
            v.insert("side_color".into(), self.side_color(s));
            v.insert("other".into(), self.side_name(1 - s));
        }
        v
    }

    /// Every live value the HUD and overlay text can show.
    pub fn vars(&self) -> Vars {
        let mut v = Vars::new();
        v.insert("title".into(), self.def.title.clone());
        v.insert("round".into(), self.round.to_string());
        let endless = self.def.match_.mode == "endless";
        v.insert("rounds".into(), if endless { "ENDLESS".into() } else { self.def.match_.rounds.to_string() });
        let secs = match self.phase {
            Phase::Fighting => (self.def.match_.round_secs - self.round_time).max(0.0),
            Phase::Lobby(t) => t.max(0.0),
            _ => 0.0,
        };
        v.insert("time".into(), format!("{}:{:02}", secs as u32 / 60, secs as u32 % 60));
        v.insert("phase".into(), self.phase.name().into());
        v.insert("biome".into(), self.biome_label());
        v.insert("biome_key".into(), self.biome.clone().unwrap_or_default());
        for k in ["label_hp", "label_units", "label_wins", "label_losses", "label_round"] {
            v.insert(format!("lbl.{}", &k[6..]), self.text(k, &Vars::new()));
        }
        v.insert("hint".into(), if self.autopilot_active() { self.text("hint", &Vars::new()) } else { String::new() });
        for s in 0..2 {
            let k = &self.side_keys[s];
            v.insert(format!("name.{k}"), self.side_name(s));
            v.insert(format!("color.{k}"), self.side_color(s));
            v.insert(format!("hp.{k}"), (self.sides[s].hp.ceil() as i64).to_string());
            let shown = if self.sides[s].hp_disp > 0.0 { self.sides[s].hp_disp } else { self.sides[s].hp };
            v.insert(format!("hp_frac.{k}"), format!("{:.4}", (shown / self.def.castle_hp).clamp(0.0, 1.0)));
            v.insert(format!("units.{k}"), self.alive(s).to_string());
            v.insert(format!("wins.{k}"), self.sides[s].wins.to_string());
            v.insert(format!("losses.{k}"), self.sides[1 - s].wins.to_string());
            v.insert(format!("shield.{k}"), if self.sides[s].shield > 0.0 { "SHIELD".into() } else { String::new() });
        }
        for i in 0..5 {
            let t = self.ticker.get(i);
            v.insert(format!("ticker.{}", i + 1), t.map(|t| t.text.clone()).unwrap_or_default());
            v.insert(format!("tickercolor.{}", i + 1), t.map(|t| t.color.clone()).unwrap_or_else(|| "#ffffff".into()));
        }
        let mut top: Vec<(&String, &u32)> = self.board.iter().collect();
        top.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
        for i in 0..3 {
            v.insert(format!("top{}", i + 1), top.get(i).map(|(u, p)| format!("{u} {p}")).unwrap_or_default());
        }
        if let Some(b) = &self.banner {
            v.insert("banner".into(), b.text.clone());
        }
        for (k, val) in &self.vars {
            v.insert(format!("var.{k}"), format!("{val}"));
        }
        v
    }

    // ------------------------------------------------------------ rounds

    fn begin_round(&mut self, world: &mut World) {
        self.clear_field(world);
        self.jobs.clear();
        for s in 0..2 {
            let st = &mut self.sides[s];
            st.hp = self.def.castle_hp;
            st.shield = 0.0;
            st.boost = 0.0;
            st.destroyed = None;
            if let Some(i) = world.entity_idx(&self.def.sides[&self.side_keys[s]].castle) {
                world.entities[i].elevation = 0.0;
            }
        }
        self.round_time = 0.0;
        self.auto_t = 0.0;
        if self.round > 1 && self.def.biome_rotation.as_ref().is_some_and(|r| r.every == "round") {
            self.next_biome(world);
        }
        self.phase = Phase::Fighting;
        let vars = self.vars();
        let t = self.text("round", &vars);
        self.announce(t, "#ffffff", 2.2);
        for s in 0..2 {
            let kind = self.side_def(s).unit.clone();
            for _ in 0..self.def.initial_units {
                self.spawn_unit(world, s, &kind, "spawn");
            }
        }
    }

    fn end_round(&mut self, winner: Option<usize>) {
        if !matches!(self.phase, Phase::Fighting) {
            return;
        }
        for s in 0..2 {
            let hp = self.sides[s].hp;
            self.sides[s].hp_sum += hp;
        }
        if let Some(w) = winner {
            self.sides[w].wins += 1;
            self.session.wins[w] += 1;
        } else {
            self.session.draws += 1;
        }
        self.session.rounds += 1;
        self.last_round_winner = Some(winner);
        let key = |i: usize| self.side_keys[i].clone();
        self.log.push(json!({
            "ev": "round",
            "round": self.round,
            "winner": winner.map(key),
            "hp": { self.side_keys[0].clone(): self.sides[0].hp.ceil(), self.side_keys[1].clone(): self.sides[1].hp.ceil() },
            "units": { self.side_keys[0].clone(): self.alive(0), self.side_keys[1].clone(): self.alive(1) },
            "biome": self.biome,
            "round_secs": (self.round_time * 10.0).round() / 10.0,
        }));
        let mut vars = self.vars();
        let (text, color) = match winner {
            Some(w) => {
                vars.insert("name".into(), self.side_name(w));
                (self.text("round_win", &vars), self.side_color(w))
            }
            None => (self.text("draw", &vars), "#ffffff".to_string()),
        };
        self.announce(text, &color, self.def.match_.intermission.max(1.0));
        self.phase = Phase::RoundEnd(self.def.match_.intermission.max(0.5));
    }

    fn match_finished(&self) -> bool {
        if self.def.match_.mode == "endless" {
            return self.stopped;
        }
        let n = self.def.match_.rounds.max(1);
        self.round >= n || self.sides.iter().any(|s| s.wins > n / 2)
    }

    fn finish_match(&mut self) {
        let (a, b) = (&self.sides[0], &self.sides[1]);
        let winner = if a.wins != b.wins {
            Some(usize::from(b.wins > a.wins))
        } else if (a.hp_sum - b.hp_sum).abs() > 1.0 {
            Some(usize::from(b.hp_sum > a.hp_sum))
        } else {
            None
        };
        self.match_winner = Some(winner);
        self.session.matches += 1;
        match winner {
            Some(w) => self.session.match_wins[w] += 1,
            None => self.session.match_draws += 1,
        }
        self.log.push(json!({"ev": "match", "winner": winner.map(|i| self.side_keys[i].clone()), "rounds": self.round}));
        let mut vars = self.vars();
        let (text, color) = match winner {
            Some(w) => {
                vars.insert("name".into(), self.side_name(w));
                (self.text("match_win", &vars), self.side_color(w))
            }
            None => (self.text("match_draw", &vars), "#ffffff".to_string()),
        };
        self.announce(text, &color, f32::INFINITY);
        self.phase = Phase::MatchEnd(self.def.match_.restart_secs.unwrap_or(f32::INFINITY));
    }

    fn reset_match(&mut self, world: &mut World) {
        self.clear_field(world);
        self.jobs.clear();
        for s in self.sides.iter_mut() {
            *s = SideState { hp: self.def.castle_hp, ..Default::default() };
        }
        self.round = 1;
        if let Some(b) = self.def.biome.clone() {
            let _ = self.set_biome(world, &b);
        }
        self.stopped = false;
        self.match_winner = None;
        self.last_round_winner = None;
        self.board.clear();
        self.ticker.clear();
        self.banner = None;
        for s in 0..2 {
            if let Some(i) = world.entity_idx(&self.def.sides[&self.side_keys[s]].castle) {
                world.entities[i].elevation = 0.0;
            }
        }
    }

    /// `start`, `next_round`, `reset`, `stop`, `pause`, `resume`.
    pub fn control(&mut self, world: &mut World, op: &str) -> Result<Value> {
        match op {
            "start" => {
                match self.phase {
                    Phase::MatchEnd(_) => {
                        self.reset_match(world);
                        self.begin_round(world);
                    }
                    Phase::Lobby(_) => self.begin_round(world),
                    _ => {}
                }
                self.paused = false;
            }
            "next_round" | "skip" => {
                if matches!(self.phase, Phase::Fighting) {
                    let w = if (self.sides[0].hp - self.sides[1].hp).abs() < 1.0 { None } else { Some(usize::from(self.sides[1].hp > self.sides[0].hp)) };
                    self.end_round(w);
                }
            }
            "reset" => {
                self.reset_match(world);
                self.phase = Phase::Lobby(self.def.match_.lobby_secs);
            }
            "stop" => self.stopped = true,
            "pause" => self.paused = true,
            "resume" => self.paused = false,
            other if other.starts_with("run:") => {
                // test helper: `run:NAME` or `run:NAME:SIDE` plays an action or a macro now
                let mut parts = other[4..].splitn(2, ':');
                let name = parts.next().unwrap_or_default().to_string();
                let side = match parts.next() {
                    Some(k) => self.side_keys.iter().position(|s| s.eq_ignore_ascii_case(k)).ok_or_else(|| anyhow!("unknown side '{k}'"))?,
                    None => 0,
                };
                if !matches!(self.phase, Phase::Fighting) {
                    bail!("run: needs a round in progress (phase is {})", self.phase.name());
                }
                if self.def.macros.contains_key(&name) {
                    let steps = self.def.macros[&name].clone();
                    let mut ctx = Ctx::default();
                    ctx.side = Some(side);
                    self.runners.push(Runner::new(&name, steps, ctx));
                } else if self.def.actions.contains_key(&name) {
                    self.exec(world, &ActionRef::Name(name), side, &Ctx::default());
                } else {
                    bail!("no action or macro named '{name}'");
                }
            }
            other if other.starts_with("biome:") => self.set_biome(world, &other[6..])?,
            other => bail!("unknown control '{other}' (start, next_round, reset, stop, pause, resume, biome:NAME, run:NAME[:SIDE])"),
        }
        Ok(json!({"phase": self.phase.name(), "round": self.round, "paused": self.paused}))
    }

    // ------------------------------------------------------------ update

    pub fn update(&mut self, dt: f32, world: &mut World) {
        self.time += dt;
        self.overlay.tick(dt);
        self.idle += dt;
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
        self.shake = (self.shake - dt * 1.4).max(0.0);

        match self.phase {
            Phase::Lobby(t) => {
                let t = t - dt;
                self.phase = Phase::Lobby(t);
                if t <= 0.0 && !self.paused {
                    self.begin_round(world);
                }
            }
            Phase::RoundEnd(t) => {
                let t = t - dt;
                self.phase = Phase::RoundEnd(t);
                if t <= 0.0 {
                    if self.match_finished() {
                        self.finish_match();
                    } else {
                        self.round += 1;
                        self.begin_round(world);
                    }
                }
            }
            Phase::MatchEnd(t) => {
                let t = t - dt;
                self.phase = Phase::MatchEnd(t);
                if t <= 0.0 {
                    self.reset_match(world);
                    self.phase = Phase::Lobby(self.def.match_.lobby_secs);
                }
            }
            Phase::Fighting => {}
        }

        if !self.paused {
            if matches!(self.phase, Phase::Fighting) {
                self.round_time += dt;
                self.tick_timers(dt, world);
                self.step_units(world, dt);
                if self.round_time >= self.def.match_.round_secs {
                    let w = if (self.sides[0].hp - self.sides[1].hp).abs() < 1.0 { None } else { Some(usize::from(self.sides[1].hp > self.sides[0].hp)) };
                    self.end_round(w);
                }
            }
            self.run_jobs(world);
            self.step_fx(world, dt);
            self.step_clouds(world, dt);
            self.step_pops(world, dt);
            self.step_bounces(world, dt);
            self.step_dying(world, dt);
        }
        self.run_runners(dt, world);
        self.run_bots(dt, world);
        self.step_cine(world, dt);
        self.sync_world(world, dt);
    }

    fn tick_timers(&mut self, dt: f32, world: &mut World) {
        for s in self.sides.iter_mut() {
            s.shield = (s.shield - dt).max(0.0);
            s.boost = (s.boost - dt).max(0.0);
            s.slow_t = (s.slow_t - dt).max(0.0);
            s.hit_t = (s.hit_t - dt).max(0.0);
            if s.hp_disp <= 0.0 {
                s.hp_disp = s.hp;
            } else {
                s.hp_disp += (s.hp - s.hp_disp) * (1.0 - (-5.0 * dt).exp());
            }
        }
        if let Some(a) = self.def.autospawn.clone() {
            self.auto_t += dt;
            if self.auto_t >= a.every.max(0.5) {
                self.auto_t = 0.0;
                for s in 0..2 {
                    let kind = self.side_def(s).unit.clone();
                    for _ in 0..a.count.min(10) {
                        self.spawn_unit_pending(s, &kind);
                    }
                }
            }
        }
        // the biome acts on its own: eruptions, storms
        let hazard = self.biome.as_ref().and_then(|k| self.def.biomes.get(k)).and_then(|b| b.hazard.clone());
        if let Some(h) = hazard {
            self.hazard_t += dt;
            if self.hazard_t >= h.every.max(1.0) {
                self.hazard_t = 0.0;
                let ctx = Ctx::default();
                let side = self.resolve_side(&h.side, &ctx);
                self.exec(world, &h.action, side, &ctx);
            }
        }
        if let Some(rot) = self.def.biome_rotation.clone() {
            if rot.every == "secs" {
                self.biome_t += dt;
                if self.biome_t >= rot.secs.max(5.0) {
                    self.biome_t = 0.0;
                    self.next_biome(world);
                }
            }
        }
        // nobody is sending anything: play the war by itself
        if let Some(ap) = self.def.autopilot.clone() {
            if self.idle >= ap.idle_secs {
                self.ap_t += dt;
                if self.ap_t >= self.ap_next {
                    self.ap_t = 0.0;
                    self.ap_next = self.range(ap.interval[0].max(0.5), ap.interval[1].max(ap.interval[0].max(0.5)));
                    self.autopilot_move(world, &ap);
                }
            } else {
                self.ap_t = 0.0;
            }
        }
    }

    fn spawn_unit_pending(&mut self, side: usize, kind: &str) {
        self.schedule(0.0, side, What::Spawn { unit: kind.to_string(), at: String::new() });
    }

    fn run_jobs(&mut self, world: &mut World) {
        let now = self.time;
        let mut due: Vec<Job> = vec![];
        let mut keep: Vec<Job> = vec![];
        for j in self.jobs.drain(..) {
            if j.at <= now { due.push(j) } else { keep.push(j) }
        }
        self.jobs = keep;
        for j in due {
            let side = j.side;
            match j.what {
                What::Exec { action, ctx } => self.exec(world, &action, side, &ctx),
                What::Spawn { unit, at } => {
                    if matches!(self.phase, Phase::Fighting) {
                        self.spawn_unit(world, side, &unit, &at);
                    }
                }
                What::Boom { pos, max } => self.flash(world, pos, max, 0.5),
                What::Meteor { target, radius, dmg, cdmg, ff, scale, asset, burn } => {
                    let to = self.pick_target(world, side, &target);
                    let key = if asset.is_empty() { "meteor" } else { asset.as_str() };
                    let asset = self.asset(key);
                    let dur = self.range(0.9, 1.25);
                    let from = Vec3::new(to.x - 5.0, 22.0, to.y - 2.0);
                    let col = self.side_color(side);
                    // telegraph: a ring closes on the spot where it will land
                    self.ring_fx(world, to, &col, radius * scale.max(0.5) * 1.5, radius * scale.max(0.5) * 0.7, dur);
                    if let Some(id) = self.spawn_entity(world, &asset, Vec2::new(from.x, from.z), 0.0, scale, from.y, BTreeMap::new(), "fx") {
                        self.fx.push(Fx { id, kind: FxKind::Fall { from, to, dur, land: Land::Meteor { side, radius: radius * scale.max(0.5), dmg, cdmg, ff, burn } }, t: 0.0 });
                    }
                }
                What::Arrow { target, radius, dmg, cdmg } => {
                    let c = self.pick_target(world, side, &target);
                    let spread = Vec2::new(self.range(-radius, radius), self.range(-radius, radius)) * 0.6;
                    let to = c + spread;
                    let asset = self.asset("arrow");
                    let from = Vec3::new(to.x - 2.0, 11.0, to.y);
                    if let Some(id) = self.spawn_entity(world, &asset, Vec2::new(from.x, from.z), 0.0, 1.0, from.y, BTreeMap::new(), "fx") {
                        self.fx.push(Fx { id, kind: FxKind::Fall { from, to, dur: 0.55, land: Land::Arrow { side, dmg, cdmg } }, t: 0.0 });
                    }
                }
                What::Bolt { dmg, cdmg } => {
                    let at = self.pick_target(world, side, "front");
                    let asset = self.asset("bolt");
                    if let Some(id) = self.spawn_entity(world, &asset, at, 0.0, 1.0, 0.0, BTreeMap::new(), "fx") {
                        self.fx.push(Fx { id, kind: FxKind::Bolt { dur: 0.4 }, t: 0.0 });
                    }
                    for i in 0..self.units.len() {
                        if self.units[i].side == side && self.units[i].dying.is_none() && self.units[i].pos.distance(at) < 1.6 {
                            self.hurt_unit(i, dmg);
                        }
                    }
                    let cp = self.castle_pos(world, side);
                    if cp.distance(at) < self.side_def(side).castle_radius + 1.4 {
                        self.hit_castle(side, cdmg);
                    }
                    self.shake = self.shake.max(0.3);
                    self.flash(world, at, 1.0, 0.35);
                }
            }
        }
    }

    fn step_fx(&mut self, world: &mut World, dt: f32) {
        let mut landed: Vec<(Vec2, Land)> = vec![];
        let mut done: Vec<String> = vec![];
        for f in self.fx.iter_mut() {
            f.t += dt;
            let Some(i) = world.entity_idx(&f.id) else {
                done.push(f.id.clone());
                continue;
            };
            let e = &mut world.entities[i];
            match &f.kind {
                FxKind::Fall { from, to, dur, land } => {
                    let u = (f.t / dur).min(1.0);
                    let k = u * u;
                    e.pos.x = from.x + (to.x - from.x) * u;
                    e.pos.z = from.z + (to.y - from.z) * u;
                    e.elevation = from.y * (1.0 - k);
                    e.yaw += dt * 240.0;
                    if f.t >= *dur {
                        landed.push((*to, land.clone()));
                        done.push(f.id.clone());
                    }
                }
                FxKind::Flash { dur, max } => {
                    let u = (f.t / dur).min(1.0);
                    let s = if u < 0.3 { u / 0.3 } else { 1.0 - (u - 0.3) / 0.7 };
                    e.scale = (max * s).max(0.02);
                    if self.cine_eye.is_some_and(|eye| eye.distance(Vec2::new(e.pos.x, e.pos.z)) < max.max(1.0) * 2.2) {
                        e.scale = 0.01;
                    }
                    if f.t >= *dur {
                        done.push(f.id.clone());
                    }
                }
                FxKind::Bolt { dur } => {
                    let u = (f.t / dur).min(1.0);
                    e.scale = if (f.t * 40.0) as i32 % 2 == 0 { 1.0 } else { 0.7 };
                    e.yaw += 37.0;
                    if u >= 1.0 {
                        done.push(f.id.clone());
                    }
                }
                FxKind::Rise { dur } => {
                    let u = (f.t / dur).min(1.0);
                    e.elevation = 1.0 + u * 3.0;
                    e.scale = 1.0 + 0.4 * (u * std::f32::consts::PI).sin();
                    e.yaw += 90.0 * dt;
                    if u >= 1.0 {
                        done.push(f.id.clone());
                    }
                }
                FxKind::Ring { dur, from, to } => {
                    let u = (f.t / dur).min(1.0);
                    let s = if u < 0.75 {
                        let v = u / 0.75;
                        from + (to - from) * (1.0 - (1.0 - v).powi(3))
                    } else {
                        to * (1.0 - (u - 0.75) / 0.25).max(0.02)
                    };
                    e.scale = s.max(0.02);
                    if u >= 1.0 {
                        done.push(f.id.clone());
                    }
                }
            }
        }
        for id in &done {
            Self::remove_entity(world, id);
        }
        self.fx.retain(|f| !done.contains(&f.id));
        for (at, land) in landed {
            match land {
                Land::Meteor { side, radius, dmg, cdmg, ff, burn } => self.explode(world, at, side, radius, dmg, cdmg, ff, burn),
                Land::Arrow { side, dmg, cdmg } => {
                    for i in 0..self.units.len() {
                        if self.units[i].side == side && self.units[i].dying.is_none() && self.units[i].pos.distance(at) < 0.9 {
                            self.hurt_unit(i, dmg);
                        }
                    }
                    let cp = self.castle_pos(world, side);
                    if cp.distance(at) < self.side_def(side).castle_radius {
                        self.hit_castle(side, cdmg);
                    }
                }
                Land::None => {}
            }
        }
    }

    fn step_dying(&mut self, world: &mut World, dt: f32) {
        let mut gone = vec![];
        for u in self.units.iter_mut() {
            if let Some(t) = &mut u.dying {
                if *t == 0.0 {
                    if let Some(i) = world.entity_idx(&u.id) {
                        let e = &mut world.entities[i];
                        e.anim.oneshot = Some(u.def.defeat_anim.clone());
                        e.anim.oneshot_loop = false;
                        e.anim.t = 0.0;
                        e.moving = false;
                    }
                }
                *t += dt.max(1e-4);
                if *t > 1.3 {
                    gone.push(u.id.clone());
                }
            }
        }
        for id in &gone {
            world.remove(id);
        }
        self.units.retain(|u| !gone.contains(&u.id));
    }

    fn step_units(&mut self, world: &mut World, dt: f32) {
        let n = self.units.len();
        let castles = [self.castle_pos(world, 0), self.castle_pos(world, 1)];
        let radii = [self.side_def(0).castle_radius, self.side_def(1).castle_radius];
        let snapshot: Vec<(usize, Vec2, bool)> = self.units.iter().map(|u| (u.side, u.pos, u.dying.is_none())).collect();
        let is_struct: Vec<bool> = self.units.iter().map(|u| u.def.structure).collect();
        // Home defence comes first: enemy soldiers close to a camp's base are that camp's priority. The
        // soldiers nearest the intruder defend (all of them while the castle is being hit or below 40%),
        // the rest keep marching on the enemy.
        let mut defend: [Option<Vec2>; 2] = [None, None];
        let mut defenders = vec![false; n];
        for s in 0..2 {
            if self.sides[s].destroyed.is_some() {
                continue;
            }
            let mut threats: Vec<(Vec2, f32)> = vec![];
            let mut mine = 0usize;
            for (j, (sj, pj, aj)) in snapshot.iter().enumerate() {
                if !*aj || is_struct[j] {
                    continue;
                }
                if *sj == s {
                    mine += 1;
                } else {
                    let d = pj.distance(castles[s]) - radii[s];
                    if d <= DEFEND_RANGE {
                        threats.push((*pj, d));
                    }
                }
            }
            if threats.is_empty() {
                continue;
            }
            threats.sort_by(|a, b| a.1.total_cmp(&b.1));
            let under_fire = self.sides[s].hit_t > 0.0 || self.sides[s].hp / self.def.castle_hp < 0.4;
            let want = if under_fire { mine } else { (threats.len() * 2 + 1).min(mine) };
            let tp = threats[0].0;
            defend[s] = Some(tp);
            let mut ours: Vec<(usize, f32)> = (0..n).filter(|&j| snapshot[j].0 == s && snapshot[j].2 && !is_struct[j]).map(|j| (j, snapshot[j].1.distance(tp))).collect();
            ours.sort_by(|a, b| a.1.total_cmp(&b.1));
            for (j, _) in ours.into_iter().take(want) {
                defenders[j] = true;
            }
        }
        let mut hits: Vec<(usize, f32)> = vec![];
        let mut castle_hits: Vec<(usize, f32)> = vec![];
        let mut shots: Vec<(Vec2, Vec2)> = vec![];
        let mut dots: Vec<(usize, f32, bool)> = vec![];
        let biome_speed = self.biome_speed();
        let bounds = {
            let (a, b) = (self.side_def(0).zone, self.side_def(1).zone);
            [a[0].min(b[0]), a[1].min(b[1]), a[2].max(b[2]), a[3].max(b[3])]
        };
        for i in 0..n {
            if self.units[i].dying.is_some() {
                continue;
            }
            if self.units[i].speech_left > 0.0 {
                self.units[i].speech_left -= dt;
                if self.units[i].speech_left <= 0.0 {
                    if let Some(k) = world.entity_idx(&self.units[i].id) {
                        let e = &mut world.entities[k];
                        e.speech = None;
                        if e.anim.oneshot_loop {
                            e.anim.oneshot = None;
                            e.anim.oneshot_loop = false;
                        }
                    }
                }
            }
            if self.units[i].def.structure {
                continue;
            }
            // shoved (water cannon, shockwave): carried along, no orders until it settles
            let shove = self.units[i].push;
            if shove.length_squared() > 0.6 {
                let p = self.units[i].pos + shove * dt;
                self.units[i].pos = Vec2::new(p.x.clamp(bounds[0], bounds[2]), p.y.clamp(bounds[1], bounds[3]));
                self.units[i].push = shove * (-4.0 * dt).exp();
                self.units[i].moving = false;
                continue;
            }
            self.units[i].push = Vec2::ZERO;
            let side = self.units[i].side;
            let enemy = 1 - side;
            let pos = self.units[i].pos;
            let def = self.units[i].def.clone();
            let (boost_s, boost_d) = if self.sides[side].boost > 0.0 { (self.sides[side].boost_speed.max(1.0), self.sides[side].boost_dmg.max(1.0)) } else { (1.0, 1.0) };
            let mut slow = 1.0f32;
            if self.sides[side].slow_t > 0.0 {
                slow = slow.min(self.sides[side].slow_f);
            }
            for c in &self.clouds {
                if c.side == side && c.pos.distance(pos) <= c.radius {
                    slow = slow.min(c.slow);
                    if c.dps > 0.0 {
                        dots.push((i, c.dps * dt, c.lethal));
                    }
                }
            }
            let speed_mod = biome_speed * slow;
            let mut near: Option<(usize, f32)> = None;
            for (j, (s, p, alive)) in snapshot.iter().enumerate() {
                if *s == enemy && *alive {
                    let d = p.distance(pos);
                    if near.is_none_or(|b| d < b.1) {
                        near = Some((j, d));
                    }
                }
            }
            let castle_d = pos.distance(castles[enemy]) - radii[enemy];
            let aggro = (def.range * 2.0).max(3.5);
            self.units[i].cd -= dt;
            let mut moving = false;
            let mut face: Option<Vec2> = None;
            let mut attack: Option<(Option<usize>, bool)> = None; // (unit target, castle target)
            if defenders[i] {
                // the intruder closest to the base is hit first, whoever is nearer to this soldier
                let tp = defend[side].unwrap_or(pos);
                let mut tgt: Option<(usize, f32)> = None;
                for (j, (s, p, alive)) in snapshot.iter().enumerate() {
                    if *s == enemy && *alive && p.distance(pos) <= def.range {
                        let dh = p.distance(castles[side]);
                        if tgt.is_none_or(|b| dh < b.1) {
                            tgt = Some((j, dh));
                        }
                    }
                }
                if let Some((j, _)) = tgt {
                    attack = Some((Some(j), false));
                    face = Some(snapshot[j].1);
                } else {
                    let dir = (tp - pos).normalize_or_zero();
                    if dir != Vec2::ZERO {
                        let step = def.speed * boost_s * speed_mod * 1.2 * dt;
                        self.units[i].pos += dir * step.min(pos.distance(tp));
                        moving = true;
                        face = Some(tp);
                    }
                }
            } else if let Some((j, d)) = near.filter(|(_, d)| *d <= def.range) {
                let _ = d;
                attack = Some((Some(j), false));
                face = Some(snapshot[j].1);
            } else if castle_d <= def.range && self.sides[enemy].destroyed.is_none() {
                attack = Some((None, true));
                face = Some(castles[enemy]);
            } else {
                let goal = match near.filter(|(_, d)| *d <= aggro) {
                    Some((j, _)) => snapshot[j].1,
                    None => {
                        let toward = (pos - castles[enemy]).normalize_or_zero();
                        castles[enemy] + toward * (radii[enemy] * 0.9)
                    }
                };
                let dir = (goal - pos).normalize_or_zero();
                if dir != Vec2::ZERO {
                    let step = def.speed * boost_s * speed_mod * dt;
                    self.units[i].pos += dir * step.min(pos.distance(goal));
                    moving = true;
                    face = Some(goal);
                }
            }
            // keep friends from stacking on one spot
            for (j, (s, p, alive)) in snapshot.iter().enumerate() {
                if j != i && *s == side && *alive {
                    let d = p.distance(self.units[i].pos);
                    if d < 0.55 && d > 1e-3 {
                        let push = (self.units[i].pos - *p).normalize_or_zero() * (0.55 - d) * 2.0 * dt;
                        self.units[i].pos += push;
                    }
                }
            }
            if let Some(f) = face {
                let d = f - self.units[i].pos;
                if d.length() > 1e-3 {
                    self.units[i].yaw = d.x.atan2(d.y).to_degrees();
                }
            }
            self.units[i].moving = moving;
            if let Some((tu, tc)) = attack {
                if self.units[i].cd <= 0.0 {
                    self.units[i].cd = def.cooldown;
                    self.units[i].anim_left = 0.45;
                    if def.ranged {
                        let target = if let Some(j) = tu { snapshot[j].1 } else { castles[enemy] };
                        shots.push((self.units[i].pos, target));
                    }
                    if let Some(j) = tu {
                        hits.push((j, def.dmg * boost_d));
                    }
                    if tc {
                        castle_hits.push((enemy, def.dmg * def.castle_mult * boost_d));
                    }
                    if let Some(k) = world.entity_idx(&self.units[i].id) {
                        let e = &mut world.entities[k];
                        let dur = e.clip("attack").map(|c| c.duration);
                        if dur.is_some() {
                            e.anim.oneshot = Some("attack".into());
                            e.anim.oneshot_loop = false;
                            e.anim.t = 0.0;
                            self.units[i].anim_left = dur.unwrap_or(0.45);
                        }
                    }
                }
            }
            if self.units[i].anim_left > 0.0 {
                self.units[i].anim_left -= dt;
                if self.units[i].anim_left <= 0.0 {
                    if let Some(k) = world.entity_idx(&self.units[i].id) {
                        if world.entities[k].anim.oneshot.as_deref() == Some("attack") {
                            world.entities[k].anim.oneshot = None;
                        }
                    }
                }
            }
        }
        for (j, d) in hits {
            self.hurt_unit(j, d);
        }
        for (i, amount, lethal) in dots {
            if self.units[i].dying.is_some() {
                continue;
            }
            if lethal {
                self.hurt_unit(i, amount);
            } else {
                self.units[i].hp = (self.units[i].hp - amount).max(1.0);
            }
        }
        for (s, d) in castle_hits {
            self.sides[s].hit_t = 3.0;
            self.hit_castle(s, d);
        }
        for (from, to) in shots {
            let asset = self.asset("arrow");
            let v = Vec3::new(from.x, 1.0, from.y);
            if let Some(id) = self.spawn_entity(world, &asset, from, 0.0, 0.8, 1.0, BTreeMap::new(), "fx") {
                self.fx.push(Fx { id, kind: FxKind::Fall { from: v, to, dur: 0.3, land: Land::None }, t: 0.0 });
            }
        }
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

    fn run_bots(&mut self, dt: f32, world: &mut World) {
        let _ = world;
        let bots = self.def.bots.clone();
        for b in &bots {
            let t = self.bot_t.entry(b.name.clone()).or_insert(0.0);
            *t += dt;
            let t = *t;
            let due = b.every.is_some_and(|e| t >= e) || b.when_idle.is_some_and(|i| self.idle >= i && t >= i);
            if !due || !matches!(self.phase, Phase::Fighting) {
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

    /// Writes simulation state into world entities: castle sinking, shield bubbles, camera shake.
    fn sync_world(&mut self, world: &mut World, dt: f32) {
        for s in 0..2 {
            let castle = self.side_def(s).castle.clone();
            if let Some(t) = &mut self.sides[s].destroyed {
                *t += dt;
                let t = *t;
                if let Some(i) = world.entity_idx(&castle) {
                    world.entities[i].elevation = -(t / 3.0).min(1.0) * 4.0;
                }
                let jitter = (t * 40.0).sin() * 0.04 * (1.0 - (t / 3.0).min(1.0));
                if let Some(i) = world.entity_idx(&castle) {
                    world.entities[i].yaw = jitter * 57.0;
                }
            }
            let want = self.sides[s].shield > 0.0;
            match (want, self.sides[s].shield_ent.clone()) {
                (true, None) => {
                    let cp = self.castle_pos(world, s);
                    let asset = self.asset("shield");
                    let id = self.spawn_entity(world, &asset, cp, 0.0, 1.0, 0.0, BTreeMap::new(), "sh");
                    self.sides[s].shield_ent = id;
                }
                (true, Some(id)) => {
                    if let Some(i) = world.entity_idx(&id) {
                        let pulse = 1.0 + 0.04 * (self.time * 6.0).sin();
                        world.entities[i].scale = pulse;
                        world.entities[i].yaw += 20.0 * dt;
                    }
                }
                (false, Some(id)) => {
                    world.remove(&id);
                    self.sides[s].shield_ent = None;
                }
                (false, None) => {}
            }
        }
        for u in &self.units {
            if let Some(i) = world.entity_idx(&u.id) {
                let e = &mut world.entities[i];
                e.pos.x = u.pos.x;
                e.pos.z = u.pos.y;
                if u.dying.is_none() {
                    e.yaw = u.yaw;
                    e.moving = u.moving;
                    let boosted = self.sides[u.side].boost > 0.0;
                    let pop = self.pops.iter().find(|p| p.id == u.id).map_or(1.0, |p| ease_out_back(p.t / p.dur).max(0.02));
                    e.scale = u.def.scale * if boosted { 1.18 } else { 1.0 } * pop;
                }
            }
        }
        let k = self.shake;
        world.cam_shake = if k > 0.001 { Vec3::new(((self.time * 53.0).sin()) * k * 0.35, ((self.time * 71.0).cos()) * k * 0.25, ((self.time * 47.0).sin()) * k * 0.35) } else { Vec3::ZERO };
    }

    // ------------------------------------------------------------ introspection

    pub fn banner_info(&self) -> Option<(String, String, f32, f32)> {
        self.banner.as_ref().map(|b| (b.text.clone(), b.color.clone(), b.total - b.left, b.left))
    }

    pub fn side_keys(&self) -> [String; 2] {
        self.side_keys.clone()
    }

    pub fn hud_enabled(&self) -> bool {
        self.def.hud
    }

    /// Round and match results since the last call, for the history file.
    pub fn take_log(&mut self) -> Vec<Value> {
        std::mem::take(&mut self.log)
    }

    /// Wins, losses and draws of this run of the stream.
    pub fn session_json(&self) -> Value {
        let s = &self.session;
        let (ka, kb) = (self.side_keys[0].clone(), self.side_keys[1].clone());
        json!({
            "started_unix": s.started_unix,
            "secs": self.time.round(),
            "rounds": s.rounds,
            "draws": s.draws,
            "matches": s.matches,
            "match_draws": s.match_draws,
            "sides": {
                ka.clone(): {"name": self.side_name(0), "wins": s.wins[0], "losses": s.wins[1], "match_wins": s.match_wins[0]},
                kb.clone(): {"name": self.side_name(1), "wins": s.wins[1], "losses": s.wins[0], "match_wins": s.match_wins[1]},
            },
        })
    }

    pub fn state_json(&self) -> Value {
        let side = |s: usize| {
            json!({
                "key": self.side_keys[s],
                "name": self.side_name(s),
                "hp": self.sides[s].hp.ceil(),
                "max_hp": self.def.castle_hp,
                "units": self.alive(s),
                "round_wins": self.sides[s].wins,
                "shield_secs": (self.sides[s].shield * 10.0).round() / 10.0,
                "boost_secs": (self.sides[s].boost * 10.0).round() / 10.0,
                "destroyed": self.sides[s].destroyed.is_some(),
            })
        };
        let winner = |w: &Option<usize>| match w {
            Some(i) => json!(self.side_keys[*i]),
            None => json!("draw"),
        };
        let mut top: Vec<(&String, &u32)> = self.board.iter().collect();
        top.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
        json!({
            "title": self.def.title,
            "mode": self.def.match_.mode,
            "phase": self.phase.name(),
            "paused": self.paused,
            "round": self.round,
            "rounds": self.def.match_.rounds,
            "round_secs_left": match self.phase { Phase::Fighting => (self.def.match_.round_secs - self.round_time).max(0.0).round(), _ => 0.0 },
            "sides": { self.side_keys[0].clone(): side(0), self.side_keys[1].clone(): side(1) },
            "last_round_winner": self.last_round_winner.as_ref().map(winner),
            "match_winner": self.match_winner.as_ref().map(winner),
            "idle_secs": (self.idle * 10.0).round() / 10.0,
            "events_seen": self.events_seen,
            "biome": self.biome,
            "autopilot": self.autopilot_active(),
            "cinematic": self.cine.is_some(),
            "running_macros": self.runners.iter().map(|r| r.name.clone()).collect::<Vec<_>>(),
            "overlay_elements": self.overlay.list().iter().map(|e| e.id.clone()).collect::<Vec<_>>(),
            "ticker": self.ticker.iter().map(|t| t.text.clone()).collect::<Vec<_>>(),
            "top": top.iter().take(5).map(|(u, p)| json!({"user": u, "points": p})).collect::<Vec<_>>(),
            "session": self.session_json(),
        })
    }

    /// What is published right now: triggers, actions, macros, bots, units.
    pub fn hooks_json(&self) -> Value {
        json!({
            "triggers": self.def.triggers,
            "actions": self.def.actions,
            "macros": self.def.macros,
            "bots": self.def.bots,
            "units": self.def.units,
            "gift_aliases": self.def.gift_aliases,
            "biomes": self.def.biomes.keys().collect::<Vec<_>>(),
        })
    }

    /// Rows of the on-screen gift menu: (icon, label, side the effect lands on, when fixed).
    pub fn legend(&self) -> Vec<(String, String, Option<usize>)> {
        let mut v = vec![];
        for t in &self.def.triggers {
            let show = t.legend.unwrap_or(t.label.is_some());
            if let (true, Some(l)) = (show, &t.label) {
                v.push((t.icon.clone().unwrap_or_default(), l.clone(), side_idx(&t.side)));
            }
        }
        v
    }

    // ------------------------------------------------------------ publish

    /// Merges (or with `replace`, swaps in) triggers, actions, macros, bots and units. Validated as a
    /// whole first: on error nothing changes.
    pub fn publish(&mut self, world: &World, h: Hooks) -> Result<Value> {
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
            if !h.actions.is_empty() {
                def.actions.clear();
            }
            if !h.bots.is_empty() {
                def.bots.clear();
            }
        }
        for name in &h.remove {
            let before = def.triggers.len() + def.actions.len() + def.macros.len() + def.bots.len();
            def.triggers.retain(|t| &trigger_name(t) != name);
            def.actions.remove(name);
            def.macros.remove(name);
            def.bots.retain(|b| &b.name != name);
            if def.triggers.len() + def.actions.len() + def.macros.len() + def.bots.len() < before {
                removed.push(name.clone());
            }
        }
        for (k, v) in h.units {
            def.units.insert(k.clone(), v);
            added.push(format!("unit:{k}"));
        }
        for (k, v) in h.actions {
            def.actions.insert(k.clone(), v);
            added.push(format!("action:{k}"));
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
        Ok(json!({"added": added, "removed": removed, "triggers": self.def.triggers.len(), "macros": self.def.macros.len(), "actions": self.def.actions.len(), "bots": self.def.bots.len()}))
    }

    /// `{"add": [...], "remove": ["id"], "patch": {"id": {...}}, "clear": true}`; a bare element or array adds.
    pub fn overlay_op(&mut self, body: &Value) -> Result<Value> {
        let vars = self.vars();
        self.overlay.apply_op(body, &vars)
    }
}

/// Everything an agent can publish into a running stream.
#[derive(Deserialize, Default, Debug)]
pub struct Hooks {
    #[serde(default)]
    pub triggers: Vec<Trigger>,
    #[serde(default)]
    pub actions: BTreeMap<String, Action>,
    #[serde(default)]
    pub macros: BTreeMap<String, Vec<MStep>>,
    #[serde(default)]
    pub bots: Vec<Bot>,
    #[serde(default)]
    pub units: BTreeMap<String, UnitDef>,
    #[serde(default)]
    pub gift_aliases: BTreeMap<String, String>,
    /// names of triggers, actions, macros or bots to delete
    #[serde(default)]
    pub remove: Vec<String>,
    /// drop the existing triggers / macros / actions / bots of the kinds present here first
    #[serde(default)]
    pub replace: bool,
}

/// The war as the host of macros: it knows sides, so `act` and `announce` land on one.
impl Host for War {
    fn eval(&mut self, c: &Cond, ctx: &Ctx) -> bool {
        War::eval(self, c, ctx)
    }
    fn rand(&mut self) -> f32 {
        War::rand(self)
    }
    fn act(&mut self, world: &mut World, action: &ActionRef, side: Option<&str>, ctx: &Ctx) {
        let s = self.resolve_side(side.unwrap_or("event"), ctx);
        self.exec(world, action, s, ctx);
    }
    fn inject(&mut self, world: &mut World, ev: Event) {
        War::inject(self, world, ev)
    }
    fn announce(&mut self, world: &mut World, text: String, secs: f32, color: Option<String>, ctx: &Ctx) {
        let s = self.resolve_side("event", ctx);
        self.exec(world, &ActionRef::Inline(Box::new(Action::Announce { text, secs, color })), s, ctx);
    }
    fn overlay(&mut self, world: &mut World, add: Vec<Element>, remove: Vec<String>, clear: bool, ctx: &Ctx) {
        let s = self.resolve_side("event", ctx);
        self.exec(world, &ActionRef::Inline(Box::new(Action::Overlay { add, remove, clear })), s, ctx);
    }
    fn world_step(&mut self, world: &mut World, step: &Value, _ctx: &Ctx) {
        War::world_step(self, world, step)
    }
    fn macro_steps(&self, name: &str) -> Option<Vec<MStep>> {
        War::macro_steps(self, name)
    }
    fn get_var(&self, name: &str) -> f64 {
        War::get_var(self, name)
    }
    fn set_var(&mut self, name: &str, v: f64) {
        War::set_var(self, name, v)
    }
    fn warn(&mut self, msg: String) {
        War::warn(self, msg)
    }
}

pub fn trigger_name(t: &Trigger) -> String {
    if !t.name.is_empty() {
        return t.name.clone();
    }
    t.on.list().first().map(|s| s.to_string()).unwrap_or_else(|| "trigger".into())
}

fn steps_refs(steps: &[MStep], acts: &mut Vec<ActionRef>, macros: &mut Vec<String>) {
    for s in steps {
        match s {
            MStep::Act { action, .. } => acts.push(action.clone()),
            MStep::Call { name } => macros.push(name.clone()),
            MStep::Repeat { steps, .. } | MStep::Forever { steps } => steps_refs(steps, acts, macros),
            MStep::Random { choices } => choices.iter().for_each(|c| steps_refs(&c.steps, acts, macros)),
            MStep::If { then, otherwise, .. } => {
                steps_refs(then, acts, macros);
                steps_refs(otherwise, acts, macros);
            }
            _ => {}
        }
    }
}

/// Checks the whole config against the scene: every name resolves, every asset exists.
pub fn validate(def: &WarDef, world: &World) -> Result<()> {
    let mut errs: Vec<String> = vec![];
    if def.sides.len() != 2 {
        errs.push(format!("`sides` needs exactly two kingdoms (e.g. \"A\" and \"B\"), found {}", def.sides.len()));
    }
    for (k, s) in &def.sides {
        if world.entity_idx(&s.castle).is_none() {
            errs.push(format!("side {k}: castle entity '{}' is not in the scene", s.castle));
        }
        if !def.units.contains_key(&s.unit) {
            errs.push(format!("side {k}: unit '{}' is not defined in `units`", s.unit));
        }
        if s.zone[0] >= s.zone[2] || s.zone[1] >= s.zone[3] {
            errs.push(format!("side {k}: zone must be [x0, z0, x1, z1] with x0 < x1 and z0 < z1"));
        }
    }
    for (k, u) in &def.units {
        if !world.models.contains_key(&u.asset) {
            errs.push(format!("unit {k}: asset '{}' is not in the scene's `assets`", u.asset));
        }
    }
    let action_ok = |a: &ActionRef, errs: &mut Vec<String>, ctx: &str| match a {
        ActionRef::Name(n) if !def.actions.contains_key(n) => errs.push(format!("{ctx}: unknown action '{n}'")),
        ActionRef::Inline(i) => check_action(i, def, errs, ctx),
        _ => {}
    };
    for (k, a) in &def.actions {
        check_action(a, def, &mut errs, &format!("action {k}"));
    }
    for t in &def.triggers {
        let name = trigger_name(t);
        if t.on.list().is_empty() || t.on.list().iter().any(|p| p.trim().is_empty()) {
            errs.push(format!("trigger {name}: `on` is empty"));
        }
        if t.action.is_none() && t.macro_.is_none() {
            errs.push(format!("trigger {name}: needs `do` (an action) or `macro`"));
        }
        if let Some(a) = &t.action {
            action_ok(a, &mut errs, &format!("trigger {name}"));
        }
        if let Some(m) = &t.macro_ {
            if !def.macros.contains_key(m) {
                errs.push(format!("trigger {name}: unknown macro '{m}'"));
            }
        }
        let s = t.side.to_ascii_lowercase();
        let by_name = def.sides.values().any(|d| d.name.eq_ignore_ascii_case(&t.side));
        if !SIDE_SPECS.contains(&s.as_str()) && !by_name {
            errs.push(format!("trigger {name}: side '{}' must be A, B, event, random, leader or trailing", t.side));
        }
    }
    for (k, steps) in &def.macros {
        let (mut acts, mut calls) = (vec![], vec![]);
        steps_refs(steps, &mut acts, &mut calls);
        for a in &acts {
            action_ok(a, &mut errs, &format!("macro {k}"));
        }
        for c in calls {
            if !def.macros.contains_key(&c) {
                errs.push(format!("macro {k}: calls unknown macro '{c}'"));
            }
        }
    }
    for b in &def.bots {
        if !def.macros.contains_key(&b.macro_) {
            errs.push(format!("bot {}: unknown macro '{}'", b.name, b.macro_));
        }
        if b.every.is_none() && b.when_idle.is_none() {
            errs.push(format!("bot {}: needs `every` or `when_idle`", b.name));
        }
    }
    for (k, b) in &def.biomes {
        for tile in b.terrain.keys() {
            if !world.terrain_def.legend.contains_key(tile) {
                errs.push(format!("biome {k}: terrain tile '{tile}' is not in the scene's legend"));
            }
        }
        for p in &b.props {
            if !world.models.contains_key(&p.asset) {
                errs.push(format!("biome {k}: prop asset '{}' is not in the scene's assets", p.asset));
            }
        }
        if !b.sky.is_empty() && !["day", "dawn", "sunset", "night", "desert", "snow", "lava"].contains(&b.sky.as_str()) {
            errs.push(format!("biome {k}: unknown sky '{}' (day, dawn, sunset, night, desert, snow, lava)", b.sky));
        }
        if let Some(h) = &b.hazard {
            action_ok(&h.action, &mut errs, &format!("biome {k} hazard"));
        }
    }
    if let Some(b) = &def.biome {
        if !def.biomes.contains_key(b) {
            errs.push(format!("`biome` '{b}' is not a key of `biomes`"));
        }
    }
    if let Some(r) = &def.biome_rotation {
        if def.biomes.is_empty() {
            errs.push("`biome_rotation` needs `biomes`".into());
        }
        if r.every != "round" && r.every != "secs" {
            errs.push(format!("biome_rotation.every must be \"round\" or \"secs\" (got '{}')", r.every));
        }
        for n in &r.list {
            if !def.biomes.contains_key(n) {
                errs.push(format!("biome_rotation: unknown biome '{n}'"));
            }
        }
    }
    if let Some(ap) = &def.autopilot {
        for m in &ap.actions {
            action_ok(&m.action, &mut errs, "autopilot");
            let s = m.side.to_ascii_lowercase();
            if !SIDE_SPECS.contains(&s.as_str()) {
                errs.push(format!("autopilot: side '{}' must be A, B, event, random, leader or trailing", m.side));
            }
        }
    }
    if def.match_.mode != "rounds" && def.match_.mode != "endless" {
        errs.push(format!("match.mode must be \"rounds\" or \"endless\" (got '{}')", def.match_.mode));
    }
    if errs.is_empty() { Ok(()) } else { Err(anyhow!("war config has {} problem(s):\n  - {}", errs.len(), errs.join("\n  - "))) }
}

fn check_action(a: &Action, def: &WarDef, errs: &mut Vec<String>, ctx: &str) {
    match a {
        Action::Spawn { unit, at, .. } => {
            if !unit.is_empty() && !def.units.contains_key(unit) {
                errs.push(format!("{ctx}: unknown unit '{unit}'"));
            }
            if !matches!(at.as_str(), "" | "spawn" | "front" | "center") {
                errs.push(format!("{ctx}: spawn `at` must be spawn, front or center (got '{at}')"));
            }
        }
        Action::Cinematic { pov, look, unit, .. } => {
            if !matches!(pov.to_ascii_lowercase().as_str(), "" | "enemy" | "self" | "a" | "b") {
                errs.push(format!("{ctx}: cinematic pov must be enemy, self, A or B (got '{pov}')"));
            }
            if !matches!(look.as_str(), "" | "enemy" | "sky" | "castle") {
                errs.push(format!("{ctx}: cinematic look must be enemy, sky or castle (got '{look}')"));
            }
            if !matches!(unit.as_str(), "" | "camp" | "castle" | "front" | "random") {
                errs.push(format!("{ctx}: cinematic unit must be camp, front or random (got '{unit}')"));
            }
        }
        Action::Biome { name } if name != "random" && !def.biomes.contains_key(name) => errs.push(format!("{ctx}: unknown biome '{name}'")),
        Action::Macro { name } if !def.macros.contains_key(name) => errs.push(format!("{ctx}: unknown macro '{name}'")),
        Action::Seq { actions } => {
            for r in actions {
                match r {
                    ActionRef::Name(n) if !def.actions.contains_key(n) => errs.push(format!("{ctx}: unknown action '{n}'")),
                    ActionRef::Inline(i) => check_action(i, def, errs, ctx),
                    _ => {}
                }
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn load(tag: &str) -> (World, War) {
        let dir = std::env::temp_dir().join(format!("r3d_war_{tag}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        crate::demo::write(&dir).unwrap();
        War::load(&dir.join("war.json")).unwrap()
    }

    fn sim(war: &mut War, world: &mut World, secs: f32) {
        let dt = 1.0 / 30.0;
        for _ in 0..(secs / dt) as usize {
            war.update(dt, world);
            world.update(dt);
        }
    }

    /// Three soldiers of the enemy of `side` standing at its gate.
    fn intruders(war: &mut War, world: &World, side: usize) {
        let gate = war.castle_pos(world, side);
        let r = war.side_def(side).castle_radius;
        let mut put = 0;
        for u in war.units.iter_mut() {
            if u.side == 1 - side && !u.def.structure && put < 3 {
                u.pos = gate + Vec2::new((r + 1.0) * if side == 0 { 1.0 } else { -1.0 }, put as f32 * 0.7);
                put += 1;
            }
        }
        assert_eq!(put, 3, "the template starts with enough soldiers");
    }

    #[test]
    fn soldiers_drop_the_offensive_to_defend_a_castle_under_attack() {
        let (mut world, mut war) = load("defend");
        war.control(&mut world, "start").unwrap();
        sim(&mut war, &mut world, 3.0);
        let gate = war.castle_pos(&world, 0);
        // A's soldiers are out in the field, three of B's are at A's gate
        for u in war.units.iter_mut() {
            if u.side == 0 && !u.def.structure {
                u.pos = gate + Vec2::new(14.0, (u.pos.y - gate.y).clamp(-3.0, 3.0));
            }
        }
        intruders(&mut war, &world, 0);
        let dist = |war: &War| -> f32 {
            let v: Vec<f32> = war.units.iter().filter(|u| u.side == 0 && !u.def.structure && u.dying.is_none()).map(|u| u.pos.distance(gate)).collect();
            v.iter().sum::<f32>() / v.len() as f32
        };
        let before = dist(&war);
        war.sides[0].hit_t = 3.0; // the castle is being hit: everybody comes home
        sim(&mut war, &mut world, 1.5);
        let after = dist(&war);
        assert!(after < before - 1.5, "A's soldiers should run back to the base: {before} -> {after}");
    }

    #[test]
    fn a_pressed_camp_gets_more_defensive_autopilot_moves() {
        let (mut world, mut war) = load("smartap");
        war.control(&mut world, "start").unwrap();
        sim(&mut war, &mut world, 3.0);
        let ap = war.def.autopilot.clone().unwrap();
        let m = ap.actions.iter().find(|m| matches!(&m.action, ActionRef::Name(n) if n == "nasi_bungkus")).expect("a heal move on A");
        let calm = war.smart_weight(&world, m);
        intruders(&mut war, &world, 0);
        let pressed = war.smart_weight(&world, m);
        assert!(pressed > calm * 3.0, "healing weight {calm} -> {pressed}");
        // and a move that hits A's attackers (it lands on B) also gains
        let hit = ap.actions.iter().find(|m| matches!(&m.action, ActionRef::Name(n) if n == "molotov")).expect("a molotov move");
        assert!(war.smart_weight(&world, hit) > hit.weight);
    }

    #[test]
    fn new_soldiers_grow_in_and_the_hp_bar_eases() {
        let (mut world, mut war) = load("fluent");
        war.control(&mut world, "start").unwrap();
        sim(&mut war, &mut world, 3.0);
        war.control(&mut world, "run:massa:A").unwrap();
        sim(&mut war, &mut world, 0.1);
        let u = war.units.last().unwrap();
        let (id, target) = (u.id.clone(), u.def.scale);
        let s = |world: &World| world.entities[world.entity_idx(&id).unwrap()].scale;
        assert!(s(&world) < target * 0.9, "a new soldier starts small: {} of {target}", s(&world));
        sim(&mut war, &mut world, 0.6);
        assert!((s(&world) - target).abs() < 0.02, "and settles at its size: {} vs {target}", s(&world));

        let real = |war: &War| war.sides[0].hp / war.def.castle_hp;
        let shown = |war: &War| war.vars()["hp_frac.A"].parse::<f32>().unwrap();
        war.control(&mut world, "run:panggung_dikepung:A").unwrap();
        sim(&mut war, &mut world, 0.1);
        assert!(shown(&war) > real(&war) + 0.05, "the bar lags behind a hit: shown {} real {}", shown(&war), real(&war));
        sim(&mut war, &mut world, 2.5);
        assert!((shown(&war) - real(&war)).abs() < 0.02, "and catches up: shown {} real {}", shown(&war), real(&war));
    }
}
