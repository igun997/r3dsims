---
name: r3dsims-live
description: Use to run or operate an r3dsims live stream (a war between two kingdoms, or the citizens-vs-government demonstration parody) driven by webhooks: gifts, likes and comments become effects, ultimates get a camp point-of-view camera with their own effects, biomes change the world, an autopilot keeps an empty stream alive. Publish triggers/macros/bots, add overlay text/emoji/SVG, control rounds, read state. The platform connector is a separate app.
---

# r3dsims live

`r3dsims live war.json` runs a diorama in real time. Another app (the platform connector) `POST`s viewer events to
`/webhook` and takes the picture from `/stream.mjpg` or `--out rtmp://...`. You author and change everything with the
CLI while it runs. Full reference: `docs/LIVE.md` in the repo; read it before writing triggers or overlays.

## Start

```
r3dsims template demo -o demo            # or `perang`: kingdoms at war. scene + assets + war.json
r3dsims validate demo/war.json           # every reference checked, a whole match played headless
r3dsims live demo/war.json --token SECRET --log-events          # picture: http://127.0.0.1:8787/view
r3dsims live demo/war.json --out rtmp://HOST/app/KEY            # also push H.264 (needs ffmpeg)
```

All other commands take `--server http://HOST:8787 --token SECRET` (or `R3D_SERVER`, `R3D_TOKEN`).
Look: clay by default (`--style clay`, smooth hand-made figures, soft light, no pixel outlines); `--style voxel` for blocks. Resolution is full by default (`--pixel 1`); `--pixel 2` is chunkier and half the GPU cost. `r3dsims state` shows fps.

## Operate

| goal | command |
| --- | --- |
| simulate what the connector sends | `r3dsims send gift Rose --user Budi --count 3` · `send like 30` · `send comment "rakyat"` · `send --json '{...}'` |
| see the match | `r3dsims state` (phase, round, support/HP, winner, biome, autopilot, cinematic, macros, top supporters) |
| try it without typing | open `http://HOST:8787/test` (buttons for gifts, events, controls, biomes, run any macro, viewer simulator, score + history) |
| wins and losses | `r3dsims history` (every session, from `sessions.jsonl`; `--history FILE`, `--no-history`); `state.session` for the running one |
| rounds | `r3dsims control start\|next_round\|reset\|stop\|pause\|resume\|biome:NAME\|run:NAME[:SIDE]` |
| world theme | `r3dsims biome malam` (any key of `biomes`, or `random`) |
| camera | `r3dsims step '{"op":"orbit","dyaw":90,"secs":2}'` |
| publish webhook functions | `r3dsims hook publish file.json` · `hook list` · `hook remove NAME` |
| on-screen elements | `r3dsims overlay add '{...}'` · `patch ID '{...}'` · `remove ID` · `clear` · `list` · `emoji` |

## Author a gift -> effect (publish into the running stream)

```
r3dsims hook publish - <<'JSON'
{"triggers": [
  {"name": "rose", "on": "gift:rose", "side": "A", "do": {"type": "meteor", "count": 1},
   "label": "ROSE = METEOR MERAH", "icon": ":rose:"},
  {"name": "whale", "on": "gift:whale", "side": "B", "macro": "tsunami_ultimate",
   "label": "PAUS = ULTIMATE", "icon": ":wave:", "cooldown": 15}
],
 "macros": {"tsunami_ultimate": [
  {"do": "announce", "text": "{user} MELEPAS PAUS RAKSASA!", "secs": 2.6},
  {"do": "act", "action": {"type": "cinematic", "pov": "enemy", "secs": 7, "fov": 82, "shake": 0.2, "tint": "#2a7de150", "vignette": 0.55}, "side": "event"},
  {"do": "wait", "secs": 1.6},
  {"do": "act", "action": {"type": "push", "strength": 14, "radius": 40, "damage": 30}, "side": "event"}]}}
JSON
```

* `side` is where the effect **lands** (victim for meteor/arrows/lightning/push/cloud/convert/sabotage/quake,
  beneficiary for spawn/heal/shield/boost/chant). Also `event` (the viewer's own side), `random`, `leader`, `trailing`.
* Effects: `meteor` (any falling `asset`, optional burning patch), `arrows`, `lightning`, `spawn` (`at` front/center),
  `heal` (any `asset`), `shield`, `boost`, `chant` (speech bubbles + morale), `push` (water cannon), `cloud` (gas,
  fire), `convert` (defections, bribes), `sabotage`, `quake`, `biome`, `cinematic`, plus `step`, `overlay`,
  `announce`, `seq`, `macro`. Combine them in a **macro** (`act`, `wait`, `random`, `if`, `repeat`, `call`, ...).
* **Camera effects are for ultimates only**: put one `cinematic` action in the macro; ordinary skills never touch the
  camera. `look` (enemy/sky/castle), `fov`, `shake`, `tint`, `vignette`, `flashes` give each ultimate its own look.
* A **bot** runs a macro when the stream is idle or every N seconds. The **autopilot** (on by default) plays random
  moves when nobody sent an event for 6 s; a real event hands control back. `"autopilot": null` turns it off.
* A bad publication changes nothing and lists every problem (unknown action, macro, unit, biome, side). Fix, resend.
* `label` + `icon` make the entry appear in the on-screen gift menu, so viewers know what each gift does.

## Make a new world (any theme)

Copy `perang` or `demo`: change `sides` (names, colours, objective entity, spawn, zone), `units` (assets; `structure`
for barricades; `defeat_anim` for non-violent), `texts` (all on-screen words), `biomes`, and the triggers. The engine
does not care whether the objective is a castle or a parliament. `r3dsims validate` plays it headless to prove it works.

## Overlay (text, emoji, SVG, PNG, bars)

```
r3dsims overlay add '{"id":"promo","type":"text","text":"TEAM :lion: RAKYAT VS PEMERINTAH","at":[0.5,0.93],"anchor":"bottom","size":0.035,"bg":"#000000aa"}'
r3dsims overlay add '{"type":"emoji","emoji":"🌹","size":0.2,"ttl":2,"anim_in":"pop"}'
r3dsims overlay add '{"type":"svg","src":"logo.svg","h":0.1,"at":[0.92,0.1]}'
r3dsims overlay add '{"type":"rect","full":true,"color":"#ff000030","ttl":1.5,"anim_out":"fade"}'
r3dsims overlay add '{"type":"vignette","strength":0.6,"ttl":3}'
```

Positions are fractions of the frame; lengths and text `size` (the font size) are fractions of the SHORTER side. Text is
smooth (bundled Roboto Bold, upper and lower case, Latin accents; `"font": "pixel"` for the retro A-Z font); keep it small (0.02-0.03), the HUD
is deliberately light so viewers keep the picture. Emoji (`🌹` or `:rose:`) work inline in text and as elements, from a built-in vector set
(`overlay emoji` lists it; `--emoji-dir` adds more). Live values: `{hp.A}`, `{name.B}`, `{round}`, `{time}`, `{biome}`,
`{ticker.1}`, `{top1}`, `{user}` (inside trigger overlays), `{var.NAME}`.

## Checks and trouble

* Always `r3dsims validate war.json` after editing the file; for a running stream read the `hook publish` answer.
* Webhook answered `401`: token. `ignored: lobby`: events only count while a round is fought (the lobby starts by itself).
* Nothing happens for a gift: `r3dsims hook list`, check the `on` pattern and gift name (case/space-insensitive,
  `gift_aliases` maps local names); `--log-events` prints each event with the triggers it fired.
* Choppy picture: `r3dsims state` -> `perf`; use `--pixel 2`, a smaller `--size`, or `--fps 24`.
* Portrait (TikTok) stream: `--preset live` and `r3dsims step '{"op":"set","yaw":90,"zoom":46}'`.
* Needs a GPU adapter on the machine running `live`; `ffmpeg` only for `--out`.
