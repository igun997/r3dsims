//! Scene file schema + the live simulation (entities, story, camera, sky).

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result, anyhow, bail};
use glam::{Vec2, Vec3};
use serde::{Deserialize, Serialize};

use crate::building::{Building, BuildingDef, Cutaway, layer_id, layer_parts};
use crate::camera::{Camera, CamParams, CameraCmd, CameraRig};
use crate::mesh::Mesh;
use crate::model::{AssetDef, Clip, Model, Pose, Rgb, bake_clip, load_model, parse_hex};
use crate::nav::{Nav, Place};
use crate::story::{Node, StageDef, Step, StoryDef, Target};
use crate::terrain::{Terrain, TerrainDef, TileDef};

fn one() -> f32 {
    1.0
}

// ---------------------------------------------------------------- scene schema

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(untagged)]
pub enum AssetRef {
    /// path to `.json` asset or `.vox` file, relative to the scene file
    Path(String),
    Inline(Box<AssetDef>),
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SceneDef {
    pub name: String,
    #[serde(default)]
    pub assets: BTreeMap<String, AssetRef>,
    /// Ground. Optional: without one, a small plinth is made around the buildings.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terrain: Option<TerrainDef>,
    #[serde(default)]
    pub sky: SkyDef,
    /// Named ground points usable as story targets.
    #[serde(default)]
    pub markers: BTreeMap<String, Target>,
    /// Dollhouse buildings: ASCII floor plans, roofless, with cut-away walls.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub buildings: Vec<BuildingDef>,
    #[serde(default)]
    pub entities: Vec<EntityDef>,
    #[serde(default)]
    pub story: Option<StoryDef>,
    /// Initial camera (all fields optional; default = isometric framing of the whole terrain).
    #[serde(default)]
    pub camera: CamParams,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct EntityDef {
    pub id: String,
    /// key in `assets`, or a path to a `.json`/`.vox` file
    pub asset: String,
    /// world [x, z]; height follows the terrain
    pub pos: [f32; 2],
    /// degrees, 0 faces +Z
    #[serde(default)]
    pub yaw: f32,
    #[serde(default = "one")]
    pub scale: f32,
    /// per-entity palette overrides (e.g. a follower's shirt color)
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub palette: BTreeMap<String, String>,
    /// extra or replaced animation clips for this entity only
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub anims: BTreeMap<String, crate::model::ClipDef>,
    /// blocking rectangle [w, d] in world units for pathfinding (houses, trees)
    #[serde(default, skip_serializing_if = "is_zero2")]
    pub footprint: [f32; 2],
    /// name tag shown above the head
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// looping clip when idle (default "idle")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_anim: Option<String>,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub elevation: f32,
    /// Stand inside this building (its id) on `floor`; `pos` stays in world coordinates.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub building: Option<String>,
    #[serde(default, skip_serializing_if = "is_zero_usize")]
    pub floor: usize,
}

fn is_zero2(v: &[f32; 2]) -> bool {
    v[0] == 0.0 && v[1] == 0.0
}
fn is_zero(v: &f32) -> bool {
    *v == 0.0
}
fn is_zero_usize(v: &usize) -> bool {
    *v == 0
}

// ---------------------------------------------------------------- sky

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct SkyDef {
    /// "day" (default) | "sunset" | "dawn" | "night"
    #[serde(default)]
    pub preset: String,
    pub top: Option<String>,
    pub bottom: Option<String>,
    pub sun_color: Option<[f32; 3]>,
    pub ambient: Option<[f32; 3]>,
    pub sun_yaw: Option<f32>,
    pub sun_elev: Option<f32>,
}

#[derive(Clone, Debug)]
pub struct Sky {
    pub top: Vec3,
    pub bottom: Vec3,
    pub sun_color: Vec3,
    pub ambient: Vec3,
    pub ground: Vec3,
    /// direction light travels (from the sun toward the scene)
    pub light_dir: Vec3,
    /// 0 day .. 1 night: how much `*` palette entries glow
    pub glow: f32,
}

fn hexv(s: &str) -> Option<Vec3> {
    parse_hex(s).map(|(c, _)| Vec3::new(c[0] as f32, c[1] as f32, c[2] as f32) / 255.0)
}

impl Sky {
    pub fn lerp(a: &Sky, b: &Sky, u: f32) -> Sky {
        Sky {
            top: a.top.lerp(b.top, u),
            bottom: a.bottom.lerp(b.bottom, u),
            sun_color: a.sun_color.lerp(b.sun_color, u),
            ambient: a.ambient.lerp(b.ambient, u),
            ground: a.ground.lerp(b.ground, u),
            light_dir: a.light_dir.lerp(b.light_dir, u).normalize_or_zero(),
            glow: a.glow + (b.glow - a.glow) * u,
        }
    }

    pub fn preset(name: &str) -> Sky {
        let sun_dir = |yaw: f32, elev: f32| {
            let (y, e) = (yaw.to_radians(), elev.to_radians());
            -Vec3::new(e.cos() * y.sin(), e.sin(), e.cos() * y.cos())
        };
        match name {
            "sunset" => Sky {
                top: Vec3::new(0.30, 0.25, 0.52),
                bottom: Vec3::new(1.0, 0.66, 0.45),
                sun_color: Vec3::new(1.0, 0.58, 0.30) * 0.95,
                ambient: Vec3::new(0.45, 0.40, 0.62),
                ground: Vec3::new(0.35, 0.25, 0.25),
                light_dir: sun_dir(100.0, 15.0),
                glow: 0.6,
            },
            "dawn" => Sky {
                top: Vec3::new(0.55, 0.65, 0.90),
                bottom: Vec3::new(1.0, 0.82, 0.76),
                sun_color: Vec3::new(1.0, 0.80, 0.65) * 0.9,
                ambient: Vec3::new(0.52, 0.55, 0.72),
                ground: Vec3::new(0.38, 0.32, 0.32),
                light_dir: sun_dir(80.0, 22.0),
                glow: 0.3,
            },
            "night" => Sky {
                top: Vec3::new(0.04, 0.06, 0.17),
                bottom: Vec3::new(0.13, 0.18, 0.38),
                sun_color: Vec3::new(0.40, 0.52, 0.95) * 0.7,
                ambient: Vec3::new(0.27, 0.33, 0.55),
                ground: Vec3::new(0.12, 0.14, 0.25),
                light_dir: sun_dir(20.0, 55.0),
                glow: 1.0,
            },
            _ => Sky {
                top: Vec3::new(0.49, 0.78, 0.94),
                bottom: Vec3::new(0.85, 0.94, 1.0),
                sun_color: Vec3::new(1.0, 0.93, 0.80) * 0.95,
                ambient: Vec3::new(0.55, 0.62, 0.75),
                ground: Vec3::new(0.40, 0.34, 0.30),
                light_dir: sun_dir(70.0, 50.0),
                glow: 0.0,
            },
        }
    }

    pub fn from_def(d: &SkyDef) -> Sky {
        let mut s = Sky::preset(&d.preset);
        if let Some(v) = d.top.as_deref().and_then(hexv) {
            s.top = v;
        }
        if let Some(v) = d.bottom.as_deref().and_then(hexv) {
            s.bottom = v;
        }
        if let Some(v) = d.sun_color {
            s.sun_color = Vec3::from(v);
        }
        if let Some(v) = d.ambient {
            s.ambient = Vec3::from(v);
        }
        if d.sun_yaw.is_some() || d.sun_elev.is_some() {
            let (y, e) = (d.sun_yaw.unwrap_or(70.0).to_radians(), d.sun_elev.unwrap_or(50.0).to_radians());
            s.light_dir = -Vec3::new(e.cos() * y.sin(), e.sin(), e.cos() * y.cos());
        }
        s
    }
}

// ---------------------------------------------------------------- entities

const FADE: f32 = 0.18;

pub struct AnimState {
    pub base: String,
    pub oneshot: Option<String>,
    pub oneshot_loop: bool,
    pub t: f32,
    cur: String,
    fade: f32,
    from: Vec<Pose>,
}

pub struct Entity {
    pub id: String,
    pub model: Arc<Model>,
    pub colors: Vec<Rgb>,
    pub pos: Vec3,
    pub yaw: f32,
    pub scale: f32,
    pub elevation: f32,
    pub label: Option<String>,
    pub footprint: [f32; 2],
    pub speech: Option<String>,
    pub moving: bool,
    pub anim: AnimState,
    pub extra: HashMap<String, Clip>,
    pub pose: Vec<Pose>,
    pub home: (Vec2, f32),
    pub snapped: bool,
    /// Walking layer: 0 = terrain, otherwise a building floor (`building::layer_id`).
    pub layer: u16,
    pub home_layer: u16,
}

fn find_clip<'a>(model: &'a Model, extra: &'a HashMap<String, Clip>, name: &str) -> Option<&'a Clip> {
    extra.get(name).or_else(|| model.clips.get(name))
}

fn id_phase(id: &str) -> f32 {
    let mut h = 2166136261u32;
    for b in id.bytes() {
        h = (h ^ b as u32).wrapping_mul(16777619);
    }
    (h % 1000) as f32 / 1000.0
}

impl Entity {
    pub fn clip(&self, name: &str) -> Option<&Clip> {
        find_clip(&self.model, &self.extra, name)
    }

    pub fn update(&mut self, dt: f32, ground: f32) {
        if self.snapped {
            self.pos.y += (ground - self.pos.y) * (1.0 - (-14.0 * dt).exp());
        } else {
            self.pos.y = ground;
            self.snapped = true;
        }

        let mut desired = match &self.anim.oneshot {
            Some(o) => o.clone(),
            None if self.moving && find_clip(&self.model, &self.extra, "walk").is_some() => "walk".to_string(),
            None => self.anim.base.clone(),
        };
        if find_clip(&self.model, &self.extra, &desired).is_none() {
            desired = if find_clip(&self.model, &self.extra, "idle").is_some() { "idle".into() } else { String::new() };
        }
        if desired != self.anim.cur {
            self.anim.from = self.pose.clone();
            self.anim.fade = 0.0;
            self.anim.cur = desired;
            self.anim.t = 0.0;
        }
        self.anim.t += dt;
        self.anim.fade += dt;

        for p in self.pose.iter_mut() {
            *p = Pose::default();
        }
        if let Some(clip) = find_clip(&self.model, &self.extra, &self.anim.cur) {
            let t = if self.anim.oneshot_loop && self.anim.oneshot.is_some() {
                self.anim.t.rem_euclid(clip.duration)
            } else {
                self.anim.t
            };
            clip.sample(t, &mut self.pose);
        }
        if self.anim.fade < FADE {
            let u = self.anim.fade / FADE;
            let u = u * u * (3.0 - 2.0 * u);
            for (p, f) in self.pose.iter_mut().zip(self.anim.from.iter()) {
                *p = Pose::lerp(*f, *p, u);
            }
        }
    }
}

// ---------------------------------------------------------------- world

pub struct World {
    pub name: String,
    pub terrain: Terrain,
    pub entities: Vec<Entity>,
    pub models: HashMap<String, Arc<Model>>,
    pub markers: HashMap<String, Place>,
    pub buildings: Vec<Building>,
    pub sky: Sky,
    pub rig: CameraRig,
    pub time: f32,
    pub nav: Nav,
    pub stages: Vec<StageDef>,
    pub story_loop: bool,
    pub reset_on_loop: bool,
    pub story: Option<Node>,
    pub stop_story: bool,
    live: Vec<Node>,
    home_sky: Sky,
    /// (from, to, elapsed, duration)
    sky_tween: Option<(Sky, Sky, f32, f32)>,
    story_elapsed: f32,
    warned: HashSet<String>,
    base_dir: PathBuf,
}

impl World {
    pub fn load(path: &Path) -> Result<World> {
        let txt = std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
        let def: SceneDef = serde_json::from_str(&txt).with_context(|| format!("parse {}", path.display()))?;
        let base = path.parent().map(Path::to_path_buf).unwrap_or_default();
        World::from_scene(def, base)
    }

    pub fn from_scene(def: SceneDef, base_dir: PathBuf) -> Result<World> {
        let terrain_def = match &def.terrain {
            Some(t) => t.clone(),
            None => plinth(&def.buildings)?,
        };
        let terrain = Terrain::build(&terrain_def).context("terrain")?;
        let mut models = HashMap::new();
        for (name, r) in &def.assets {
            let model = match r {
                AssetRef::Path(p) => load_model(&base_dir.join(p)).with_context(|| format!("asset '{name}'"))?,
                AssetRef::Inline(d) => Model::bake(d).with_context(|| format!("asset '{name}'"))?,
            };
            models.insert(name.clone(), Arc::new(model));
        }
        let mut buildings = vec![];
        for bd in &def.buildings {
            let base_y = terrain.height_at(bd.pos[0], bd.pos[1]);
            buildings.push(Building::build(bd, base_y).with_context(|| format!("building '{}'", bd.id))?);
        }
        let center = terrain.center();
        let mut cam = Camera { target: center, zoom: terrain.size().max_element() * 1.45, ..Camera::default() };
        let p = &def.camera;
        if let Some(t) = p.target {
            cam.target = Vec3::from(t);
        }
        cam.yaw = p.yaw.unwrap_or(cam.yaw);
        cam.pitch = p.pitch.unwrap_or(cam.pitch);
        cam.dist = p.distance.unwrap_or(cam.dist);
        cam.zoom = p.zoom.unwrap_or(cam.zoom);
        cam.fov = p.fov.unwrap_or(cam.fov);
        cam.ortho = p.ortho.unwrap_or(cam.ortho);

        let nav = Nav::build(&terrain, &buildings, &[]);
        let mut w = World {
            name: def.name.clone(),
            terrain,
            entities: vec![],
            models,
            markers: HashMap::new(),
            buildings,
            sky: Sky::from_def(&def.sky),
            home_sky: Sky::from_def(&def.sky),
            sky_tween: None,
            rig: CameraRig::new(cam),
            time: 0.0,
            nav,
            stages: vec![],
            story_loop: false,
            reset_on_loop: true,
            story: None,
            stop_story: false,
            live: vec![],
            story_elapsed: 0.0,
            warned: HashSet::new(),
            base_dir,
        };
        for (k, t) in &def.markers {
            let place = w.target_place(t).ok_or_else(|| anyhow!("marker '{k}': needs [x, z] or {{pos, building, floor}}"))?;
            w.markers.insert(k.clone(), place);
        }
        let notes: Vec<String> = w.buildings.iter().flat_map(|b| b.warnings.iter().map(|m| format!("building '{}': {m}", b.id))).collect();
        for n in notes {
            w.warn(n);
        }
        for e in &def.entities {
            w.spawn(e).with_context(|| format!("entity '{}'", e.id))?;
        }
        if let Some(story) = def.story {
            w.stages = story.scenes;
            w.story_loop = story.looping;
            w.reset_on_loop = story.reset_on_loop;
            w.restart_story();
        }
        w.update(0.0);
        Ok(w)
    }

    pub fn warn(&mut self, msg: String) {
        if self.warned.insert(msg.clone()) {
            eprintln!("[warn] {msg}");
        }
    }

    pub fn entity_idx(&self, id: &str) -> Option<usize> {
        self.entities.iter().position(|e| e.id == id)
    }

    /// Position on the ground plane of a target (entity, marker, point).
    pub fn resolve(&self, t: &Target) -> Option<Vec2> {
        self.resolve_place(t).map(|p| p.pos)
    }

    /// Position and walking layer of a target.
    pub fn resolve_place(&self, t: &Target) -> Option<Place> {
        match t {
            Target::Name(n) => self
                .entity_idx(n)
                .map(|i| Place { pos: Vec2::new(self.entities[i].pos.x, self.entities[i].pos.z), layer: self.entities[i].layer })
                .or_else(|| self.markers.get(n).copied()),
            other => self.target_place(other),
        }
    }

    fn target_place(&self, t: &Target) -> Option<Place> {
        match t {
            Target::Pos(p) => Some(Place { pos: Vec2::from(*p), layer: 0 }),
            Target::At { pos, building, floor } => {
                let bi = crate::building::find(&self.buildings, building)?;
                (*floor < self.buildings[bi].floors()).then(|| Place { pos: Vec2::from(*pos), layer: layer_id(bi, *floor) })
            }
            Target::Name(_) => None,
        }
    }

    /// Height of the walking surface on `layer` under (x, z), including exploded-view gaps.
    pub fn ground_y(&self, layer: u16, x: f32, z: f32) -> f32 {
        match layer_parts(layer) {
            Some((b, f)) if b < self.buildings.len() => self.buildings[b].ground_y(f, x, z),
            _ => self.terrain.height_at(x, z),
        }
    }

    /// False for entities standing on a floor that is currently hidden.
    pub fn entity_visible(&self, e: &Entity) -> bool {
        match layer_parts(e.layer) {
            Some((b, f)) if b < self.buildings.len() => f < self.buildings[b].show,
            _ => true,
        }
    }

    /// Adds every building's geometry for a camera at `yaw`.
    pub fn push_buildings(&self, yaw: f32, mats: &mut Vec<glam::Mat4>, mesh: &mut Mesh) {
        for (i, b) in self.buildings.iter().enumerate() {
            b.push(10_000 + i as u32, yaw, mats, mesh);
        }
    }

    /// Floors shown / exploded gap / cut-away mode of a building; returns seconds to wait.
    pub fn building_cmd(&mut self, id: &str, show_floor: Option<i32>, gap: Option<f32>, cutaway: Option<&str>, secs: f32) -> f32 {
        let Some(bi) = crate::building::find(&self.buildings, id) else {
            self.warn(format!("building: unknown building '{id}'"));
            return 0.0;
        };
        let mode = match cutaway {
            Some(s) => match Cutaway::parse(s) {
                Some(m) => Some(m),
                None => {
                    self.warn(format!("building: cutaway must be auto, none or low (got '{s}')"));
                    None
                }
            },
            None => None,
        };
        self.buildings[bi].configure(show_floor, gap, mode, secs);
        if show_floor.is_some() {
            self.rebuild_nav();
        }
        if gap.is_some() { secs } else { 0.0 }
    }

    pub fn stage_steps(&self, name: &str) -> Option<Vec<Step>> {
        self.stages.iter().find(|s| s.name == name).map(|s| s.steps.clone())
    }

    pub fn spawn(&mut self, d: &EntityDef) -> Result<()> {
        if self.entity_idx(&d.id).is_some() {
            bail!("duplicate entity id '{}'", d.id);
        }
        let model = match self.models.get(&d.asset) {
            Some(m) => m.clone(),
            None => {
                let m = Arc::new(
                    load_model(&self.base_dir.join(&d.asset))
                        .map_err(|e| anyhow!("unknown asset '{}' (not in `assets`, and not a loadable file: {e:#})", d.asset))?,
                );
                self.models.insert(d.asset.clone(), m.clone());
                m
            }
        };
        let mut colors = model.colors.clone();
        for (k, v) in &d.palette {
            let i = *model.color_names.get(k).ok_or_else(|| anyhow!("asset '{}' has no palette key '{k}'", d.asset))?;
            colors[i as usize] = parse_hex(v).ok_or_else(|| anyhow!("bad color '{v}'"))?.0;
        }
        let mut extra = HashMap::new();
        for (name, cd) in &d.anims {
            extra.insert(name.clone(), bake_clip(cd, &model.part_index).with_context(|| format!("anim '{name}'"))?);
        }
        let (layer, y) = match &d.building {
            Some(bid) => {
                let bi = crate::building::find(&self.buildings, bid).ok_or_else(|| anyhow!("unknown building '{bid}'"))?;
                if d.floor >= self.buildings[bi].floors() {
                    bail!("building '{bid}' has {} floors, entity asks for floor {}", self.buildings[bi].floors(), d.floor);
                }
                (layer_id(bi, d.floor), self.buildings[bi].ground_y(d.floor, d.pos[0], d.pos[1]))
            }
            None => (0, self.terrain.height_at(d.pos[0], d.pos[1])),
        };
        let n = model.parts.len();
        let base = d.base_anim.clone().unwrap_or_else(|| "idle".into());
        self.entities.push(Entity {
            id: d.id.clone(),
            colors,
            pos: Vec3::new(d.pos[0], y, d.pos[1]),
            yaw: d.yaw,
            scale: d.scale,
            elevation: d.elevation,
            label: d.label.clone(),
            footprint: d.footprint,
            speech: None,
            moving: false,
            anim: AnimState {
                cur: base.clone(),
                base,
                oneshot: None,
                oneshot_loop: false,
                t: id_phase(&d.id) * 2.0,
                fade: FADE,
                from: vec![Pose::default(); n],
            },
            extra,
            pose: vec![Pose::default(); n],
            home: (Vec2::from(d.pos), d.yaw),
            snapped: true,
            layer,
            home_layer: layer,
            model,
        });
        self.rebuild_nav();
        Ok(())
    }

    pub fn remove(&mut self, id: &str) -> bool {
        match self.entity_idx(id) {
            Some(i) => {
                self.entities.remove(i);
                self.rebuild_nav();
                true
            }
            None => false,
        }
    }

    fn rebuild_nav(&mut self) {
        self.nav = Nav::build(&self.terrain, &self.buildings, &self.entities);
    }

    /// Switches to a sky preset, blending over `secs` when given.
    pub fn set_sky(&mut self, preset: &str, secs: Option<f32>) {
        let to = Sky::preset(preset);
        match secs {
            Some(s) if s > 0.0 => self.sky_tween = Some((self.sky.clone(), to, 0.0, s)),
            _ => {
                self.sky = to;
                self.sky_tween = None;
            }
        }
    }

    pub fn camera_cmd(&mut self, cmd: &CameraCmd) -> f32 {
        let pos = match cmd {
            CameraCmd::LookAt { actor, .. } => {
                let p = self.entity_idx(actor).map(|i| self.entities[i].pos);
                if p.is_none() {
                    self.warn(format!("camera look_at: unknown actor '{actor}'"));
                }
                p
            }
            CameraCmd::Follow { actor, .. } if self.entity_idx(actor).is_none() => {
                self.warn(format!("camera follow: unknown actor '{actor}'"));
                return 0.0;
            }
            _ => None,
        };
        self.rig.apply(cmd, pos)
    }

    fn restart_story(&mut self) {
        let all: Vec<Step> = self.stages.iter().flat_map(|s| s.steps.clone()).collect();
        let node = Node::from_step(&Step::Seq { steps: all }, self);
        self.story = Some(node);
        self.story_elapsed = 0.0;
    }

    fn reset_state(&mut self) {
        for e in self.entities.iter_mut() {
            e.pos.x = e.home.0.x;
            e.pos.z = e.home.0.y;
            e.yaw = e.home.1;
            e.moving = false;
            e.speech = None;
            e.anim.oneshot = None;
            e.anim.oneshot_loop = false;
            e.snapped = false;
            e.layer = e.home_layer;
        }
        let home = self.rig.home;
        self.rig.cam = home;
        self.rig.follow = None;
        self.rig.spin = 0.0;
        self.sky = self.home_sky.clone();
        self.sky_tween = None;
        for b in self.buildings.iter_mut() {
            b.reset();
        }
    }

    /// Runs one step right now, in parallel with the story (agent / `--commands` / viewer stdin).
    pub fn run(&mut self, step: Step) {
        let n = Node::from_step(&step, self);
        self.live.push(n);
    }

    /// Accepts a `Step` JSON, or a bare camera command (`{"op":"orbit",...}`).
    pub fn run_json(&mut self, line: &str) -> Result<()> {
        let line = line.trim();
        let step = match serde_json::from_str::<Step>(line) {
            Ok(s) => s,
            Err(e) => match serde_json::from_str::<CameraCmd>(line) {
                Ok(c) => Step::Camera(c),
                Err(_) => return Err(anyhow!("not a valid step/camera command: {e}")),
            },
        };
        self.run(step);
        Ok(())
    }

    pub fn update(&mut self, dt: f32) {
        self.time += dt;

        if let Some(mut node) = self.story.take() {
            self.story_elapsed += dt;
            match node.tick(dt, self) {
                None if !self.stop_story => self.story = Some(node),
                None => {}
                Some(_) => {
                    if self.story_loop && self.story_elapsed > 0.0 && !self.stop_story {
                        if self.reset_on_loop {
                            self.reset_state();
                        }
                        self.restart_story();
                    }
                }
            }
        }
        self.stop_story = false;

        let mut live = std::mem::take(&mut self.live);
        let mut keep = Vec::with_capacity(live.len());
        for mut n in live.drain(..) {
            if n.tick(dt, self).is_none() {
                keep.push(n);
            }
        }
        keep.append(&mut self.live);
        self.live = keep;

        let grounds: Vec<f32> = self.entities.iter().map(|e| self.ground_y(e.layer, e.pos.x, e.pos.z)).collect();
        for (e, g) in self.entities.iter_mut().zip(grounds) {
            e.update(dt, g);
        }
        for b in self.buildings.iter_mut() {
            b.update(dt);
        }
        let follow = self
            .rig
            .follow
            .as_ref()
            .and_then(|(n, _)| self.entity_idx(n))
            .map(|i| self.entities[i].pos + Vec3::new(0.0, 1.0, 0.0));
        if let Some((from, to, t, dur)) = &mut self.sky_tween {
            *t += dt;
            let u = (*t / *dur).clamp(0.0, 1.0);
            let u = u * u * (3.0 - 2.0 * u);
            self.sky = Sky::lerp(from, to, u);
            if *t >= *dur {
                self.sky_tween = None;
            }
        }
        self.rig.update(dt, follow);
    }
}

/// A small stone-and-grass base around the buildings, for scenes that give no terrain.
fn plinth(buildings: &[BuildingDef]) -> Result<TerrainDef> {
    if buildings.is_empty() {
        bail!("a scene needs a `terrain` or at least one building");
    }
    let (mut max_x, mut max_z) = (0.0f32, 0.0f32);
    let mut rects = vec![];
    for b in buildings {
        let rows = b.floors.iter().map(|f| f.plan.len()).max().unwrap_or(0) as f32;
        let cols = b.floors.iter().flat_map(|f| f.plan.iter().map(|r| r.chars().count())).max().unwrap_or(0) as f32;
        let cell = b.tile as f32 * b.voxel;
        let (x1, z1) = (b.pos[0] + cols * cell, b.pos[1] + rows * cell);
        max_x = max_x.max(x1);
        max_z = max_z.max(z1);
        rects.push((b.pos[0], b.pos[1], x1, z1));
    }
    let (w, d) = ((max_x.ceil() + 3.0) as usize, (max_z.ceil() + 3.0) as usize);
    let mut map = vec![vec!['g'; w]; d];
    for (z, row) in map.iter_mut().enumerate() {
        for (x, c) in row.iter_mut().enumerate() {
            let (cx, cz) = (x as f32 + 0.5, z as f32 + 0.5);
            if rects.iter().any(|&(x0, z0, x1, z1)| cx > x0 - 1.0 && cx < x1 + 1.0 && cz > z0 - 1.0 && cz < z1 + 1.0) {
                *c = 'p';
            }
        }
    }
    let tile = |top: &str, side: &str| TileDef { h: 2, top: top.into(), side: Some(side.into()), jitter: 0.04, water: false };
    let mut legend = BTreeMap::new();
    legend.insert("g".to_string(), tile("#78b856:grass", "#8a6040:dirt"));
    legend.insert("p".to_string(), tile("#cdbfa5:stone", "#a8987a:stone"));
    Ok(TerrainDef { cell: 1.0, step: 0.5, legend, map: map.iter().map(|r| r.iter().collect()).collect() })
}
