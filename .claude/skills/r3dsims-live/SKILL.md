---
name: r3dsims-live
description: Use to run or operate an r3dsims live stream, a game (war between two kingdoms, citizens-vs-government parody) or NOT a game (a studio: any scene where chat becomes speech bubbles, gifts become effects, likes fill a goal), driven by TikTok LIVE or webhooks: the built-in TikTok connector, the dashboard, going live over RTMP from Linux, gifts, likes and comments become effects, ultimates get a camp point-of-view camera, biomes, an autopilot for an empty stream. Publish triggers/macros/bots, add overlay text/emoji/SVG, control the show, read state and wins/losses.
---

# r3dsims live

`r3dsims live FILE` runs a diorama in real time; FILE is a **show**: `war.json` (a game, has `sides`) or `studio.json` (no game: triggers, macros, bots and a goal over any scene). Another app (the platform connector) `POST`s viewer events to
`/webhook` and takes the picture from `/stream.mjpg` or `--out rtmp://...`. You author and change everything with the
CLI while it runs. Full reference: `docs/LIVE.md` in the repo; read it before writing triggers or overlays.

## Start

```
r3dsims template demo -o demo            # or `perang`: kingdoms at war. scene + assets + war.json
r3dsims validate demo/war.json           # every reference checked, a whole match played headless
r3dsims live demo/war.json --token SECRET --log-events          # picture: http://127.0.0.1:8787/view
r3dsims live demo/war.json --out rtmp://HOST/app/KEY            # also push H.264 (needs ffmpeg)
r3dsims live demo/war.json --token SECRET --tiktok @username     # follow a TikTok LIVE room; dashboard at /dashboard
```

All other commands take `--server http://HOST:8787 --token SECRET` (or `R3D_SERVER`, `R3D_TOKEN`).
Look: clay by default (`--style clay`, smooth hand-made figures, soft light, no pixel outlines); `--style voxel` for blocks. Resolution is full by default (`--pixel 1`); `--pixel 2` is chunkier and half the GPU cost. `r3dsims state` shows fps.

## Operate

| goal | command |
| --- | --- |
| simulate what the connector sends | `r3dsims send gift Rose --user Budi --count 3` · `send like 30` · `send comment "rakyat"` · `send --json '{...}'` |
| see the match | `r3dsims state` (phase, round, support/HP, winner, biome, autopilot, cinematic, macros, top supporters) |
| dashboard | open `http://HOST:8787/dashboard` (`?token=SECRET`): tabs Siaran (TikTok, video output, score, match, history), Uji coba (gift buttons, events, run any macro, viewer simulator), Stream lain (other servers) |
| TikTok connector | `live --tiktok @name [--tiktok-proxy socks5://...] [--tiktok-poll 30]` · `r3dsims tiktok status\|connect @name\|disconnect` · waits until live, rejoins by itself; private protocol, may refuse datacenter IPs (use a proxy) |
| go live from Linux | get Server URL + Stream key at livecenter.tiktok.com (new every session) · `r3dsims output start --url rtmp://... --key KEY [--bitrate 3500]` · `output stop\|status` · key is never echoed back; restarts ffmpeg itself |
| Instagram / Facebook / YouTube / Twitch | same `output start`; paste the whole `rtmps://...` address from the Live Producer as `--url` (no `--key`), practice mode first, run `live --preset live` (9:16); `/output` shows `platform` and `warnings`; only video out, Instagram comments need Meta's Graph API |
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

## A show that is not a game (studio)

`r3dsims template studio -o studio` (the kampung village), `r3dsims validate studio/studio.json`, `r3dsims live studio/studio.json --preset live`.
`studio.json` = `{title, scene, vars, goal{var,target,label,on_reach,repeat}, triggers, macros, bots, gift_aliases, overlay, play, hud}`. Same triggers/macros/bots as a war, minus what needs a game: no `sides`, `units`, `actions`, trigger `do`, `act` step, or side/round conditions (refused at load, named). A macro `step` is any story or camera step (`say`, `play`, `set_sky`, `{"op":"orbit",...}`); `{user} {text} {gift} {count} {diamonds}` name the viewer event, `{var.NAME}` a counter; `{"do":"add","var":"likes","from":"count"}` adds a like event's count. Control: `pause`, `resume`, `reset`, `run:MACRO`. `state` has `kind: "studio"`, `vars`, `goal`, no win/loss history. Studio beats war when the user wants a chat-driven village, harbour, room, product showcase: copy the template, change `scene`, actors in macros, triggers.

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

Notes for a Linux server: listening beyond localhost needs `--token`; without a GPU rendering is software (llvmpipe) and measured only 6-13 fps at 720x1280, so broadcast from a machine with a Vulkan GPU. Details in `docs/LIVE.md`.

* Always `r3dsims validate war.json` after editing the file; for a running stream read the `hook publish` answer.
* Webhook answered `401`: token. `ignored: lobby`: events only count while a round is fought (the lobby starts by itself).
* Nothing happens for a gift: `r3dsims hook list`, check the `on` pattern and gift name (case/space-insensitive,
  `gift_aliases` maps local names); `--log-events` prints each event with the triggers it fired.
* Choppy picture: `r3dsims state` -> `perf`; use `--pixel 2`, a smaller `--size`, or `--fps 24`.
* Portrait (TikTok) stream: `--preset live` and `r3dsims step '{"op":"set","yaw":90,"zoom":46}'`.
* Needs a GPU adapter on the machine running `live`; `ffmpeg` only for `--out`.
