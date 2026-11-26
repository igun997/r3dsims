//! r3dsims: a scriptable voxel-pixel diorama sandbox.
//!
//! * `model`    modular voxel assets (parts + palette/materials + user-defined animation clips)
//! * `vox`      MagicaVoxel `.vox` import (users bring their own pixel models)
//! * `terrain`  ASCII-map terrain with textured tile tops
//! * `building` dollhouse buildings: ASCII floor plans, roofless, cut-away walls, exploded floors
//! * `nav`      multi-layer pathfinding (terrain, floors, stairs, doors)
//! * `story`    story scenes: sequential / parallel steps driving actors, camera, sky, buildings
//! * `camera`   orbit camera + rig with tweens, follow, spin (agent or manual control)
//! * `world`    scene file schema + runtime simulation
//! * `mesh`     voxel faces and tiles to GPU vertices (AO, material uv)
//! * `gpu`      wgpu renderer: shadows, procedural materials, AO, pixel outlines
//! * `record`   mp4 (ffmpeg) / gif / png-sequence recorder with social presets
//! * `templates` Rust-built sample assets and the "kampung" and "rumah" dioramas

#![recursion_limit = "1024"]

pub mod building;
pub mod camera;
pub mod clay;
pub mod client;
pub mod demo;
pub mod emoji;
pub mod hud;
pub mod font;
pub mod gpu;
pub mod harbor;
pub mod history;
pub mod house;
pub mod mesh;
pub mod model;
pub mod nav;
pub mod record;
pub mod story;
pub mod templates;
pub mod live;
pub mod show;
pub mod studio;
pub mod output;
pub mod tiktok;
pub mod overlay;
pub mod perang;
pub mod script;
pub mod war;
pub mod terrain;
pub mod texture;
pub mod vox;
pub mod world;
