# Research notes and decisions

Goal: a "3D pixel" diorama sandbox. Users add their own modular voxel models, program behaviour
with story scenes, an agent or a person moves the camera, and the result can be watched live and
recorded for Instagram. Rust for rendering and asset creation, no GLB.

## Look: how "3D pixel art" is made

Sources: David Holland, *3D Pixel Art Rendering* (davidhol.land); ProPixelizer docs; the T3SSEL8R /
Starbi-inspired Unity pipelines (logancho.com, otaviopeixoto.github.io).

* Render at a low resolution and scale up with nearest-neighbour. Everything else is polish.
* Outlines come from edge detection on depth and object id, not from geometry.
* Directional light with shadow map; flat per-face shading keeps voxels readable.
* Known issue we did not solve yet: sub-pixel camera movement makes pixels "swim". The usual fix
  is snapping the camera to the pixel grid in the camera's own axes (roadmap).

Decision: a wgpu renderer (`src/gpu.rs`, `src/shader.wgsl`): quads per exposed voxel face, 2048²
sun shadow map with 3x3 PCF, depth/object outlines, flat lighting with per-corner ambient
occlusion, procedural materials (wood planks, brick bond, tile grout, grass speckle...), water
shimmer, night glow. The first version was a CPU rasteriser; it was replaced because the app is
native and the GPU gives smooth AO, texturing and shadows for free. Headless use (CLI recording,
tests) creates its own device; on a GPU-less VPS wgpu falls back to lavapipe (software Vulkan), so
CI and `r3dsims render` still run there. Output is the same frame on every machine for one
adapter; across adapters pixels can differ slightly.

## Voxel rendering approaches (voxel.wiki, Aalto thesis, Mileff & Dudra 2019)

Two families: mesh the exposed faces and rasterise, or ray-march a grid/octree. For small animated
voxel sets, meshing exposed faces is simplest and fast enough. Skeletal animation of voxels works
by rigid parts (each part is a voxel bag with a pivot and a parent), which is also how MagicaVoxel
scene graphs animate. Decision: models are part trees (`src/model.rs`); only surface faces are
kept at bake time (interior voxels are dropped).

## Asset format

GLB is a general mesh container and heavy for this. A villager is 5.9 KB of JSON here. Assets are
JSON (palette + parts + clips) or built from Rust with `templates.rs` helpers. MagicaVoxel `.vox`
(ephtracy spec; `dot_vox` is the reference Rust parser) is imported by a small built-in parser
(`src/vox.rs`) so users can model in MagicaVoxel and drop the file in. Only the first model is
imported, as one part named `body`.

## Rust ecosystem

`bevy_voxel_world`, `block-mesh-rs`, `bevy_vox_scene` exist, but they tie the project to Bevy or to
GPU meshing and do not give a deterministic headless pipeline. Not adopted; revisit when a GPU
path is needed.

## Live view

Rune (github.com/anomalyco/rune) is a GPU-rendered Rust UI framework with a headless mode
(`--snap`, `--tree`, scripted input), so an agent can render and inspect the UI. Its `Compositor`
hook hands the app Rune's own `wgpu::Device`, and `El::effect(mark)` slots tell it where to draw, so
`crates/live` (`r3dlive`) renders the diorama on Rune's device and blits it (nearest filter) into the
viewport slot: no readback while watching. Controls, JSON command box and recording sit around it. Rune also has a web target (wasm + WebGPU/WebGL2); porting `r3dlive` there is
possible because `r3dsims` has no native-only requirement except the ffmpeg recorder.

## Market

Twitch-first "world around your stream" products exist: IdleView, StreamWorlds, microscape,
PixelTown, PlayVille. None found that is Instagram-first, user-authored (assets + scripted story)
or agent-driven. [INFERENCE] from the first page of search results only.

## Instagram constraint to verify before building the "followers become villagers" feature

[INFERENCE, verify in Meta docs] The Instagram Graph API does not list a business account's
followers. Opt-in is the realistic path: a follower comments a keyword or DMs the account, a
webhook fires, and the app sends `{"do":"spawn","entity":{...}}` with their handle as `label`
and chosen colours in `palette`. The spawn step already exists; the webhook side does not.

## Not done yet

* Pixel-snapped camera, greedy meshing, A* is per-tile only (no dynamic obstacles).
* Dollhouse: one building per nav test, no balconies/railings, stairs are straight runs only, no
  roof toggle (always roofless), furniture cannot be moved at runtime except through `teleport`.
* Audio, particles, per-entity shadows blob for tiny props.
* A scripting layer (Rune-lang / Luau) on top of the JSON steps. The step format is the stable
  wire protocol; a script would compile down to it.
* Web build of `r3dlive`.
