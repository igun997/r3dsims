---
name: r3dsims-assets
description: Use when creating or editing r3dsims voxel assets: parts and pivots, palette keys with materials (wood, brick, tile...), glow, animation clips (idle, walk, wave), MagicaVoxel .vox import, per-entity recolours.
---

# r3dsims assets

An asset is a few KB of JSON: `{name, voxel, palette, parts[], anims{}}`. Full schema in
`docs/FORMAT.md`; working examples are written by `r3dsims template kampung|rumah|pelabuhan`
(`assets/*.json`) and built from Rust in `src/templates.rs`, `src/house.rs`, `src/harbor.rs`.

## Geometry

* Integer voxel coordinates in the rest pose: feet at y = 0, x/z centred on 0, the model faces +Z.
  `voxel` is the world size of one voxel (people 0.1, houses 0.25, props 0.07-0.2). A terrain tile is
  1.0 wide.
* Ops, applied in order (later overwrite earlier): `box`, `sphere`, `line`, `clear`, `grid`
  (ASCII layers: layers bottom to top, rows back to front, `.` empty).
* One part per rigid piece. `pivot` is where it rotates; children follow their parent. Parts need
  parents listed or reachable (cycles are an error). Interior voxels are culled automatically.
* Character's right side is -X when facing +Z. Arms rotate about Z (raise: right arm negative, left
  positive), legs about X.

## Colour and material

`"#b9824f:wood"`: hex, then `:material` (`wood brick tile stone grass dirt fabric metal plaster
sand`), then optional glow: `!` always glowing, `*` glows only at night (`"#ffd27a*"` for windows
and lamps). Patterns are per voxel, so plank joints and brick bond line up across neighbours. Use
a material on big surfaces (roofs, floors, walls, leaves as `grass`) and plain colour on skin/hair.
Corner ambient occlusion is automatic.

## Animation

`anims.<name> = {duration, loop, ease: "smooth"|"linear", tracks: {part: {rot: [[t,[x,y,z]]], pos: ...}}}`.
`rot` is euler degrees about the pivot, `pos` a voxel offset. A looping clip interpolates from its
last key back to its first across `duration`. Name clips `idle` and `walk` (played automatically
when standing / walking); anything else (`wave`, `cheer`, `talk`, `open`) is triggered by the
`play` step. Rotate a loop 360 degrees with a final key at time = duration and `ease: "linear"`.

## Your own models

* Put a MagicaVoxel `.vox` path where an asset path is accepted. First model only, one part
  `body`; animate it with entity-level `anims` on `body`.
* Per entity: `palette` overrides asset colours by key (a follower's shirt), `anims` adds or
  replaces clips, `label` shows a name tag, `footprint [w,d]` blocks pathfinding.

## Check

`r3dsims validate scene.json` lists each asset's voxel count and clips. Render a close still
(`--at`, `--preset square`) and look at it: wrong pivots show as pieces swinging from the wrong
joint.
