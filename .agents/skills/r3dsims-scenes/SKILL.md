---
name: r3dsims-scenes
description: Use when building r3dsims scenes: ASCII terrain, dollhouse buildings with floors and stairs, entities, markers, story scenes (walk_to, play, say, par), camera moves and building steps (show_floor, explode, cut-away).
---

# r3dsims scenes and stories

Schema: `docs/FORMAT.md`. Complete worked scene: `r3dsims template pelabuhan -o /tmp/p`.

## Terrain

`terrain.legend` maps one char to `{h, top, side, jitter, water}`; `terrain.map` is rows back (z = 0)
to front. Colours take materials (`#78b856:grass`). Water tiles block walking, cliffs steeper than 2
levels too. A scene may omit `terrain` when it has a building (a plinth is generated).

## Buildings (dollhouse, no roof)

`scene.buildings[]` with ASCII `floors[].plan`. `#` wall, `W` window, `D` door (opening), `.` wood,
`c` carpet, `t` tile, `<>^v` stairs rising toward -x/+x/-z/+z, `' '` nothing.

* A stair run of N tiles climbs one storey. The next floor needs `' '` over every stair tile and a
  walkable landing tile right after the top. `validate` warns otherwise.
* A door is 2 tiles wide for people. Whole-number `pos` and even plan sizes make doors meet terrain
  tiles. Doors on floor 0 connect to the outside by themselves.
* Furniture: entity with `building`, `floor`, world-space `pos`, and a `footprint` so people walk
  around it. Do not block door tiles, stair bottoms or landings (the walker goes to the nearest
  reachable tile if the target is blocked, but that looks wrong).
* The upper floor hides the lower one from above. In a story, switch `show_floor` to the floor the
  action is on: `{"do":"building","id":"b","show_floor":0}` (-1 = all), `"gap": 3.2, "secs": 1.5`
  explodes floors apart, `"cutaway":"auto|low|none"`.

## Story

`story.scenes[]` = `{name, steps[]}`, run in order, optionally looping (`reset_on_loop` puts actors,
camera, sky and floors back). Steps (`do`): `walk_to` (pathfinds, takes stairs and doors; `to` = entity
id, marker, `[x,z]` or `{"pos":[x,z],"building":"b","floor":1}`; `stop`, `speed`), `face`, `play`
(`anim`, `times`/`secs`), `say`, `wait`, `teleport`, `loop_anim`, `seq`, `par`, `repeat`,
`camera`, `spawn`, `remove`, `play_scene`, `set_sky` (`secs` blends), `building`, `stop_story`.
Blocking steps (`walk_to`, `play`, `say`, `camera` with `secs`, `building` with `secs`) finish before the
next one in a `seq`; put things in `par` to overlap them.

Camera `op`: `set`, `move` (target/yaw/pitch/zoom/secs), `orbit`, `pan`, `zoom`, `look_at`,
`follow`, `unfollow`, `spin`, `reset`. `zoom` = world units across the shorter frame side (pick it
so the subject fits the *portrait* reel; landscape then has margin). Pitch 45-55 looks into a
dollhouse; 30-38 suits outdoors.

## Direct the action so it is visible

* Follow the person with `camera follow` and switch `show_floor` when they change floors.
* Cut to a close `zoom` for dialogue, wide for establishing shots, orbit during beats with no
  movement.
* Use `validate` for duration and warnings, then render stills at several `--at` times.
