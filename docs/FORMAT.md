# File formats

All formats are JSON. `r3dsims template kampung -o DIR` writes a complete example; every snippet
below is taken from it.

## Conventions

* Y is up. Ground is y = 0. A model faces +Z at yaw 0. Entity `yaw` is degrees around Y.
* Voxel coordinates in assets are integers in the rest pose, feet at y = 0, x/z centred on 0.
* `voxel` in an asset is the world size of one voxel (villager 0.1, house 0.25). One terrain tile
  is 1.0 wide, one terrain level is 0.5 high.
* Colours: `#rgb` or `#rrggbb`, optionally `:material` and a glow suffix, e.g. `#b9824f:wood`,
  `#ffd27a*`. Suffix `!` = always glowing (unlit), `*` = glows only at night.
  Materials (procedural texture, evaluated on the GPU): `wood` `brick` `tile` `stone` `grass`
  `dirt` `fabric` `metal` `plaster` `sand`. On voxels one texel = one voxel, so plank joints,
  brick bond and tile grout line up across neighbours; on terrain tops there are 4x4 texels per
  tile. Every voxel face also gets per-corner ambient occlusion automatically.
* "Character's right" is -X when facing +Z.

## Asset

```json
{
  "name": "villager",
  "voxel": 0.1,
  "palette": {"s": "#f1c27d", "b": "#e0584a"},
  "parts": [
    {"name": "body", "pivot": [0, 5, 0], "shape": [
      {"op": "box", "at": [-4, 5, -2], "size": [8, 6, 4], "c": "b"}]},
    {"name": "arm_r", "parent": "body", "pivot": [-5, 10, 0], "shape": [
      {"op": "box", "at": [-6, 8, -1], "size": [2, 3, 2], "c": "b"}]}
  ],
  "anims": {
    "wave": {"duration": 1.6, "loop": false, "ease": "smooth", "tracks": {
      "arm_r": {"rot": [[0, [0, 0, 0]], [0.25, [0, 0, -95]], [1.6, [0, 0, 0]]]}}}
  }
}
```

Shape ops (applied in order, later ops overwrite): `box {at,size,c}`, `sphere {at,r,c}` (at in
voxel units, may be fractional), `line {from,to,c}`, `clear {at,size}`, and `grid`:

```json
{"op": "grid", "at": [0, 0, 0], "layers": [
  ["aa", "ab"],      // layer y=0, rows are z back -> front, chars are x
  ["a.", ".."]       // layer y=1, '.' = empty
]}
```

`c` is a palette key or a literal `#rrggbb`. The grid uses one-char palette keys.

Keyframes are `[time, [x, y, z]]`: `rot` = euler degrees about the part's `pivot`, `pos` = voxel
offset. A looping clip interpolates from its last key back to its first across `duration`. Part
parents must exist; children inherit the parent's pose. Clip names are free: `idle` and `walk` are
played automatically when an actor stands or walks; anything else is triggered by `play`.

A `.vox` file can be used wherever an asset path is accepted.

## Scene

```json
{
  "name": "Kampung Bahagia",
  "assets": {"villager": "assets/villager.json", "mine": "assets/mine.vox"},
  "terrain": {"cell": 1.0, "step": 0.5,
    "legend": {"g": {"h": 2, "top": "#78b856", "side": "#8a6040"},
               "w": {"h": 1, "top": "#3d95dc", "water": true}},
    "map": ["wwww", "wggw", "wwww"]},
  "sky": {"preset": "day"},
  "markers": {"plaza": [13, 13]},
  "entities": [
    {"id": "ani", "asset": "villager", "pos": [7.5, 10.2], "yaw": 0, "label": "@ani",
     "palette": {"b": "#e0584a"}, "footprint": [0, 0], "scale": 1.0,
     "anims": {"dance": {"duration": 1, "tracks": {}}}}
  ],
  "story": {"loop": true, "reset_on_loop": true, "scenes": [{"name": "pagi", "steps": []}]},
  "camera": {"target": [13, 1, 13], "yaw": 45, "pitch": 30}
}
```

* `terrain.map`: one char per tile, row 0 = z 0. `h` is the height in levels; `water` tiles
  shimmer and block walking. Cliffs steeper than 2 levels block walking.
* `entities[].palette` overrides asset palette keys for this entity only (per-follower colours).
* `entities[].anims` adds or replaces clips for this entity only (per-user animation).
* `entities[].footprint` [w, d] blocks pathfinding (houses, trees). `label` is a name tag.
* `sky.preset`: `day`, `dawn`, `sunset`, `night`; optional `top`, `bottom`, `sun_color`, `ambient`,
  `sun_yaw`, `sun_elev` override it.

## Steps (story scenes, `--commands`, stdin, the r3dlive command box)

`{"do": ...}`. Steps in `steps` run in order; `par` runs children together.

| do | fields |
| --- | --- |
| `walk_to` | `actor`, `to` (entity id, marker or `[x,z]`), `speed` (2.0), `stop` (1.2 for actors), `via` |
| `face` | `actor`, `to` |
| `play` | `actor`, `anim`, `times` (1) or `secs` |
| `loop_anim` | `actor`, `anim` (replaces `idle`, does not block) |
| `say` | `actor`, `text`, `secs` |
| `wait` | `secs` |
| `teleport` | `actor`, `to` |
| `seq` / `par` | `steps` |
| `repeat` | `times`, `steps` |
| `camera` | any camera command below, flattened: `{"do":"camera","op":"orbit",...}` |
| `spawn` | `entity` (same shape as an entity) |
| `remove` | `actor` |
| `play_scene` | `name` |
| `set_sky` | `preset`, `secs` (blend) |
| `building` | `id`, `show_floor` (hide every floor above N, -1 = all), `gap` (explode floors apart, world units), `cutaway` (`auto` / `none` / `low`), `secs` (blend + wait) |
| `stop_story` | |

Camera commands (`op`): `set` / `move` (`target`, `yaw`, `pitch`, `distance`, `zoom`, `fov`,
`ortho`, `secs`, `ease`), `orbit` (`dyaw`, `dpitch`, `secs`), `pan` (`dx`, `dz`, `secs`), `zoom`
(`factor`, `secs`), `look_at` (`actor`, `secs`, `height`), `follow` (`actor`, `smooth`),
`unfollow`, `spin` (`dps`), `reset`. `zoom` is world units across the shorter frame side
(orthographic); `ease` is `smooth` (default), `linear`, `in`, `out`.

A bare camera command (`{"op":"orbit","dyaw":90,"secs":2}`) is accepted live without the `do`.

## Buildings (dollhouse, no roof)

`scene.buildings[]`: a floor plan per floor as ASCII, built into voxels with one part per floor.
The camera looks in from above; walls that face the camera are lowered automatically
(`cutaway: "auto"`), interior walls are always low, floors above a chosen one can be hidden and
floors can be pulled apart (exploded view). `terrain` is optional when a scene has buildings (a small
plinth is made).

```json
{"id": "rumah", "pos": [3, 3], "voxel": 0.25, "tile": 2, "floor_height": 12, "slab": 2,
 "cut_height": 3, "cutaway": "auto",
 "palette": {"wall": "#ece3d3:plaster", "floor": "#b98550:wood"},
 "floors": [
   {"name": "Lantai 1", "plan": ["#####D#####", "#.........#", "#.>>>>>...#", "###########"]},
   {"name": "Lantai 2", "plan": ["###########", "#.........#", "#.     ...#", "###########"]}]}
```

Plan characters: `#` wall, `W` window, `D` door (opening), `.` wood floor, `c` carpet, `t` tiles,
`<` `>` `^` `v` stairs rising toward -x / +x / -z / +z (a run of N tiles climbs one storey; leave a
hole `' '` above it on the next floor and a walkable landing tile beyond the top), `' '` nothing.
Palette keys: `wall trim floor carpet tile slab glass stair stair_top` (any colour spec above).
One plan char is `tile` voxels (default 2 x 0.25 = 0.5 world units); use whole-number `pos` and
plans whose size is a multiple of 2 chars so doors line up with the terrain tiles outside.

Entities live on a floor with `"building": "rumah", "floor": 1` (their `pos` stays in world
coordinates). Walking works across floors: stairs, doors and the terrain are linked, so
`{"do":"walk_to","actor":"anak","to":"meja_makan"}` takes the stairs by itself. Targets on a floor
are `{"pos":[x,z],"building":"rumah","floor":1}` (also usable as `markers`). `validate` warns about
stairs without a hole or landing.

## Timed commands for headless renders

`r3dsims render scene.json --commands cmds.json`, where `cmds.json` is an array and every item has
`at` (seconds) plus any step:

```json
[
  {"at": 2.0, "do": "camera", "op": "orbit", "dyaw": 90, "secs": 3},
  {"at": 5.0, "do": "say", "actor": "ani", "text": "HALO", "secs": 2}
]
```
