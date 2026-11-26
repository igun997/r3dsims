# r3dsims

A scriptable voxel-pixel diorama sandbox in Rust, rendered on the GPU (wgpu). Assets are tiny
modular JSON (or Rust-built) voxel models with procedural materials, behaviour is story scenes, the
camera takes events from a person or an agent, and the result can be watched live (Rune GPU UI),
rendered/recorded headless for Instagram, or **streamed live with webhooks**: gifts, likes and comments from a
platform connector turn into meteors, speeches, water cannons or camp-point-of-view ultimates in a war between two sides.

Sample dioramas, from simple to complex (the last two are for `r3dsims live`):

| template | what |
| --- | --- |
| `kampung` | island village: houses, palms, well, stall, four villagers, day to night |
| `rumah` | roofless two-storey dollhouse: cut-away walls, stairs, furniture, exploded floors |
| `pelabuhan` | **complex**: fishing harbour with dock and boats, lighthouse on a hill, kampung houses, a three-storey guesthouse (two stair runs, lobby, rooms, rooftop terrace), a fisherman's hut, 72 entities (furniture, people, boats, trees), a 58 s story that walks a tourist from the beach to the rooftop |
| `perang` | **live**: two kingdoms side by side, rounds, castles, knights/archers/giants, meteors, lightning, arrow rain, ultimates seen from a camp's point of view, four biomes |
| `demo` | **live**: a parody street demonstration, citizens vs government, endless day and night: speeches raise morale, rice packs heal, barricades, molotov bottles, water cannon, tear gas, foreign aid or corruption, intelligence sabotage, bribes that turn protesters, ultimates |

## Screenshots

All of these come from `r3dsims render` on the bundled templates (720x900, 360x450 pixel render).

<p align="center"><img src="docs/images/pelabuhan-story.gif" alt="pelabuhan: the tourist on the rooftop terrace" width="300"></p>

**`pelabuhan`**: day, rooftop scene, exploded floors, night.

<p align="center">
<img src="docs/images/pelabuhan-day.png" alt="pelabuhan, day" width="24%">
<img src="docs/images/pelabuhan-rooftop.png" alt="pelabuhan, rooftop terrace" width="24%">
<img src="docs/images/pelabuhan-exploded.png" alt="pelabuhan, exploded floors" width="24%">
<img src="docs/images/pelabuhan-night.png" alt="pelabuhan, night" width="24%">
</p>

**`rumah`**: ground floor, upper floor, exploded.

<p align="center">
<img src="docs/images/rumah-ground-floor.png" alt="rumah, ground floor" width="32%">
<img src="docs/images/rumah-upper-floor.png" alt="rumah, upper floor" width="32%">
<img src="docs/images/rumah-exploded.png" alt="rumah, exploded" width="32%">
</p>

**`kampung`**: day and night. **`r3dlive`**: the Rune GPU UI.

<p align="center">
<img src="docs/images/kampung-day.png" alt="kampung, day" width="24%">
<img src="docs/images/kampung-night.png" alt="kampung, night" width="24%">
<img src="docs/images/r3dlive-ui.png" alt="r3dlive window" width="48%">
</p>

Regenerate them: `r3dsims template pelabuhan -o harbor && r3dsims render harbor/scene.json -o shot.png --preset feed --size 720x900 --pixel 2 --at 40`
(`rumah` stills at `--at 4 / 8.5 / 22`, `kampung` at `--at 14 / 39`).

## Install

**One prompt** (paste into Claude Code, Codex or any shell-capable agent): see
[`docs/INSTALL_PROMPT.md`](docs/INSTALL_PROMPT.md). It installs the binaries and the agent skills
and verifies them with a real render.

Or by hand:

```
curl -fsSL https://raw.githubusercontent.com/igun997/r3dsims/main/scripts/install.sh -o install.sh
sh install.sh          # Linux x86_64 or macOS arm64: binaries -> ~/.local/bin, skills -> ~/.claude/skills
```

Windows: take the `.zip` from the [Releases](https://github.com/igun997/r3dsims/releases) page.
From source: `cargo build --release --workspace` (Rust stable; Linux needs `libxkbcommon-dev
libwayland-dev libx11-dev libxcb1-dev libxcursor-dev libxi-dev libvulkan-dev pkg-config`).

Needs a GPU adapter (Vulkan, Metal or DX12; on a GPU-less Linux box install `mesa-vulkan-drivers`)
and `ffmpeg` for mp4 output.

## Use

```
r3dsims template pelabuhan -o harbor            # scene + 27 assets as editable JSON
r3dsims validate harbor/scene.json              # runs the whole story headless, reports problems
r3dsims render harbor/scene.json -o reel.mp4 --preset reel --seconds 58
r3dsims render harbor/scene.json -o shot.png --preset feed --at 40
r3dlive --demo pelabuhan                        # live window (Rune): orbit, floors, scenes, record
r3dlive --file harbor/scene.json
```

## Live streaming (webhook in, picture out)

The platform connector is another application: it `POST`s viewer events to `r3dsims live` and takes the picture from
`/stream.mjpg`, `--out rtmp://...`, `--out srt://...` or `--out -` (MPEG-TS on stdout). You author and change the show
with the CLI while it runs.

```
r3dsims template demo -o demo
r3dsims validate demo/war.json                  # checks every reference, plays a whole match headless
r3dsims live demo/war.json --token SECRET       # http://127.0.0.1:8787/view
r3dsims send gift Rose --user Budi              # what the connector does
r3dsims hook publish my-gifts.json              # publish new triggers / macros / bots without a restart
r3dsims overlay add '{"type":"emoji","emoji":":fire:","size":0.2,"ttl":2}'   # text, emoji, SVG, PNG, bars on screen
r3dsims biome malam                             # day / night / desert / snow / lava ...
r3dsims state                                   # round, support, winner, biome, fps
```

Clay look by default (`--style clay`: smooth hand-made figures; `--style voxel` for blocks). Rounds or endless with a winner, per-gift effects, macro scripts and bots, ultimates with their own camera look
(ordinary skills never move the camera), biomes, and an autopilot that keeps an empty stream alive. Reference:
[`docs/LIVE.md`](docs/LIVE.md); skill: `r3dsims-live`.

`demo` (citizens vs government), day and night, and the two ultimates seen from the point of view of each camp:

<p align="center">
<img src="docs/images/live-demo-day.jpg" alt="demo, day" width="49%">
<img src="docs/images/live-demo-night.jpg" alt="demo, night biome" width="49%">
<img src="docs/images/live-demo-fpv-police.jpg" alt="demo, ultimate seen from the government camp" width="49%">
<img src="docs/images/live-demo-fpv-crowd.jpg" alt="demo, ultimate seen from the citizens' camp" width="49%">
</p>

`perang` (two kingdoms): a battle, the whale ultimate (blue tsunami), the lava biome, the galaxy ultimate (looks up at the meteors):

<p align="center">
<img src="docs/images/live-perang-battle.jpg" alt="perang, battle" width="49%">
<img src="docs/images/live-perang-whale-fpv.jpg" alt="perang, whale ultimate from a camp" width="49%">
<img src="docs/images/live-perang-lava.jpg" alt="perang, lava biome" width="49%">
<img src="docs/images/live-perang-galaxy-fpv.jpg" alt="perang, galaxy ultimate from a camp" width="49%">
</p>

## Agent skills

`.claude/skills/` (mirrored in `.agents/skills/`): `r3dsims` (start here), `r3dsims-assets`,
`r3dsims-scenes`, `r3dsims-record`, `r3dsims-live`, `r3dsims-install`. They are packaged into every release and
installed by `scripts/install.sh`.

## Build and release

This repo's author builds on a remote machine with rcargo (local `.claude/settings.json` and
`rcargo.toml` are git-ignored); everyone else uses plain cargo.

```
cargo build --release --workspace     # r3dsims (CLI) and r3dlive (Rune GPU UI)
cargo test -p r3dsims                 # GPU tests need an adapter, lavapipe is fine
```

CI (`.github/workflows/ci.yml`) tests and smoke-renders on every push. Pushing a tag `vX.Y.Z` runs
`.github/workflows/release.yml`: Linux, macOS (Apple silicon) and Windows archives plus `SHA256SUMS`
are attached to a GitHub release.

## How it renders

`src/gpu.rs` + `src/shader.wgsl`: shadow map (sun, 2048², PCF) -> scene pass at the low "pixel"
resolution (flat shading, procedural materials, per-corner ambient occlusion, night glow, water
shimmer; colour + object id + depth) -> post pass (depth/object outlines, name tags and speech
bubbles). Consumers upscale with nearest-neighbour. In `r3dlive` Rune hands its own wgpu device to
the compositor, so the diorama is drawn straight into the UI with no readback; recording renders a
second frame at the output size only while REC is on.

## Parts

| path | what |
| --- | --- |
| `src/model.rs`, `src/vox.rs` | modular voxel assets, clips, materials, AO bake, MagicaVoxel import |
| `src/terrain.rs` | ASCII-map terrain with textured tops |
| `src/building.rs` | dollhouse buildings from ASCII plans (floors, stairs, doors, cut-away, explode) |
| `src/nav.rs` | multi-layer A* (terrain, floors, stairs, doors) |
| `src/story.rs`, `src/world.rs` | story steps, simulation, scene schema, sky |
| `src/camera.rs` | orbit camera, tweens, follow, spin, manual input |
| `src/mesh.rs`, `src/gpu.rs`, `src/shader.wgsl` | GPU renderer |
| `src/record.rs` | mp4/webm/gif/png/sequence recorder with Instagram presets |
| `src/templates.rs`, `src/house.rs`, `src/harbor.rs` | asset builder helpers and the three templates |
| `crates/live` | `r3dlive`, the Rune GPU UI |

Formats: [`docs/FORMAT.md`](docs/FORMAT.md). Research and decisions: [`docs/RESEARCH.md`](docs/RESEARCH.md).

## Recording presets

`reel`/`story`/`tiktok` 1080x1920, `feed` 1080x1350, `square` 1080x1080, `landscape` 1920x1080.
The renderer draws at 1/3 of that and upscales with nearest-neighbour. `--audio track.mp3` muxes
audio; `--commands cmds.json` adds timed camera/story/building events to a render without editing
the scene.

MIT licensed. The live app uses [Rune](https://github.com/anomalyco/rune) (MIT).
