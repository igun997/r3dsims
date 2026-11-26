---
name: r3dsims
description: Use for r3dsims voxel-pixel 3D dioramas: what it is, the CLI workflow (template, validate, render), the three bundled templates and where the detailed skills live. Start here.
---

# r3dsims

Scriptable voxel-pixel diorama, rendered on the GPU (wgpu). Everything is JSON: assets (voxel
parts + palette/materials + animation clips), scenes (terrain, buildings, entities, story) and live
commands. `docs/FORMAT.md` in the repo is the format reference: read it before writing JSON.

| need | skill |
| --- | --- |
| install or build the tools, fix "no GPU adapter" | `r3dsims-install` |
| make or edit a voxel model, animation, materials | `r3dsims-assets` |
| run a webhook-driven live war stream: gifts to effects, hooks, overlay emoji/SVG | `r3dsims-live` |
| terrain, dollhouse buildings, story steps, camera moves | `r3dsims-scenes` |
| render a still / mp4 / gif for Instagram, TikTok, YouTube | `r3dsims-record` |

## Loop

1. `r3dsims template pelabuhan -o /tmp/p` (also `kampung` island village, `rumah` two-storey
   dollhouse). `pelabuhan` is the complex example: harbour, lighthouse, boats, 3-storey guesthouse.
   `perang` and `demo` are for `r3dsims live` (see `r3dsims-live`): kingdoms at war, and a parody
   street demonstration, driven by webhooks.
2. Edit the JSON (`assets/*.json`, `scene.json`).
3. `r3dsims validate scene.json`: loads everything, runs the story headless once, prints voxel
   counts, clip names, story duration and `[warn]` lines (unknown actor/anim/target, stairs without
   a hole, no path).
4. `r3dsims render scene.json -o /tmp/a.png --preset reel --at 12`, then look at the PNG.
5. Video: `-o out.mp4 --preset reel --seconds 30`.

## Rules that save time

* Camera, story and live commands share one JSON step format. A bare camera command works live:
  `{"op":"orbit","dyaw":90,"secs":2}`.
* Never compile locally in a repo that has `rcargo` set up: build with `rcargo`.
* Live window: `r3dlive --demo pelabuhan` or `r3dlive --file scene.json`; its command box takes the
  same step JSON, so what an agent sends equals what a person clicks.
