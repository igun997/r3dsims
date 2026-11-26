# Live streaming: any show (a game or not) driven by webhooks

`r3dsims live` runs a diorama in real time. What the diorama does with a viewer event is a **show**: a game (`war.json`:
two camps, rounds, a winner) or a **studio** (`studio.json`: no game at all, a village where chat becomes speech bubbles,
a harbour where gifts raise fireworks, your own scene with your own rules). The server around the show (picture, overlay,
TikTok, RTMP, dashboard, webhooks) does not know which one it runs, and does two things only:

1. **takes webhooks** (a viewer sent a gift, liked, commented) and hands them to the show, which turns them into effects
   that exist in the diorama (a war: meteors, lightning, reinforcements; a studio: whatever its macros say);
2. **delivers the picture** (MJPEG over HTTP, and/or ffmpeg to `rtmp://`, `srt://`, a file, stdout).

The platform connector (TikTok, YouTube, ...) is **another application**. It only has to `POST` JSON to
`/webhook` and point its video input at `/stream.mjpg` or at the ffmpeg output. Everything else, how
the war is built, which gift does what, what the screen shows, is data you author and change with the CLI.

```
 connector app  --POST /webhook-->  r3dsims live  --/stream.mjpg | rtmp | srt-->  OBS / platform
 agent + CLI    --hook/overlay/control-->  (same process, hot, no restart)
```

## Quick start

Two ready-made worlds, same engine: `perang` (two kingdoms at war, rounds) and `demo` (citizens vs
government: a parody street demonstration, endless, day and night).

```
r3dsims template demo -o demo           # or: perang. scene + assets + war.json
r3dsims validate demo/war.json          # checks every reference, plays a whole match headless
r3dsims live demo/war.json --log-events # http://127.0.0.1:8787/view shows the stream
r3dsims send gift Rose --user Budi      # what the connector does; in `demo`, ROSE = a speech (morale up)
r3dsims state                           # round, support, winner, biome, macros, top supporters
r3dsims history                         # wins and losses of every session
```

No typing needed to try it: open **`http://127.0.0.1:8787/dashboard`** (older links `/test` and `/view?panel=1` open the same page). The
picture on the left; on the right three tabs. **Siaran**: the TikTok connection, the video output, the score of this session, the
match controls and the history table. **Uji coba**: buttons for every gift of the war, like / follow / share / join / comment, a
"run any action or macro" box, biome buttons and a random-viewer simulator. **Stream lain**: the state of other `r3dsims live`
servers (other ports or machines) next to this one. If `live` runs with `--token`, open `/dashboard?token=SECRET`. `/view` stays the
bare picture, so it is safe as a browser source.

Picture out:

| how | use |
| --- | --- |
| `http://HOST:8787/view` | a page for an OBS *Browser Source* or any browser |
| `http://HOST:8787/stream.mjpg` | multipart JPEG (`<img src>`, ffmpeg `-f mjpeg`, VLC) |
| `http://HOST:8787/frame.jpg` | latest frame |
| `--out rtmp://live.example/app/KEY` | ffmpeg pushes H.264 FLV (needs `ffmpeg`) |
| `--out srt://...` / `udp://...` | MPEG-TS over SRT / UDP |
| `--out -` | MPEG-TS on stdout, pipe it to your connector: `r3dsims live war.json --out - \| other-app` |
| `--out match.mp4` | record the whole stream to a file |

Size: `--preset live-wide` (1280x720, default), `live` (720x1280), any `render` preset, or `--size WxH`.
`--pixel 1` (default) renders at the output size. `--pixel 2` renders at half the size and upscales with
nearest-neighbour (chunky, half the GPU cost). `r3dsims state` shows the measured fps and where the frame time
goes (`perf`).

Look: `--style clay` (default for `live`) draws every model and the ground as smooth hand-made clay: the voxel
assets are smoothed into one soft surface (rounded edges, matte light, soft shadows, a slightly bumpy finish, no pixel
outlines), so units, castles, trees and your own assets need no redrawing. The island rim and cliffs are rounded too.
`--style voxel` brings back the blocky cube look. `render` takes the same flag (default `voxel`).
For a portrait (TikTok) stream send a camera command once: `r3dsims step '{"op":"set","yaw":90,"zoom":46}'`.

## TikTok: the built-in connector, going live from Linux, the dashboard

`r3dsims live` can read a TikTok LIVE room by itself, so no separate connector app is needed:

```
r3dsims live demo/war.json --tiktok @username      # or R3D_TIKTOK=@username
r3dsims tiktok status                              # state, viewers, likes, events per minute, last events
r3dsims tiktok connect @other | disconnect         # change the account while running
```

Give it an account once. It waits until the account is live (checks every `--tiktok-poll` seconds, default 30), joins the room,
and on its own rejoins when the connection drops, the stream ends or the account goes live again. The dashboard shows the state
(`menunggu akun live`, `tersambung`, `menyambung ulang`, `galat`), the number of viewers, the total likes, the events per minute
and the latest chat and gifts. Accounts can be written as `@name`, `name` or a TikTok URL.

| TikTok | becomes the webhook event |
| --- | --- |
| chat message | `comment` (`text`, `user` = the viewer's nickname) |
| gift | `gift` with `gift` = TikTok's English name (`Rose`, `Galaxy`, ...), `count` = the final streak length, `diamonds` = per gift. A combo is reported once, when the streak ends |
| likes | `like`, one event per viewer every 250 ms with the summed `count` |
| viewer joins | `join` (at most 6 a second; the rest are counted as dropped) |
| follow, share | `follow`, `share` |

Chat is capped at 25 a second. Every message is delivered once (TikTok's message id), and `events_seen`, the supporters board and the
triggers treat these events exactly like webhook ones. Use `gift_aliases` in `war.json` for other names. Language is fixed to English
so gift names stay the same whatever the machine's locale.

Read this before relying on it:

* It uses TikTok's **private web protocol**, not an official API. TikTok can change it at any time (it did while this was built: the
  anonymous `ttwid` cookie moved from the home page to `/live`, which broke the upstream library; `crates/tiktok-live` is that library,
  patched, see its `NOTICE.md`). Check TikTok's terms for your use.
* It only **reads**. It cannot send chat or act on the room, and there is no way to resume: events are missed while reconnecting.
* TikTok may refuse **datacenter IPs**. The status then shows `galat` with a hint. Use `--tiktok-proxy socks5://user:pass@host:1080`
  (`http://` and `https://` work too) or run from a home network. Room lookups from a VPS worked in testing; the WebSocket from a VPS
  was not tried.
* `--tiktok-cookies "sessionid=...; sid_tt=..."` is only needed for 18+ rooms.

### Going live to TikTok from Linux

TikTok LIVE Studio exists only for Windows and macOS. On Linux the picture goes out as plain RTMP, which `r3dsims` already can do.
TikTok gives a **new Server URL and Stream key for every LIVE session**, so the output can be started, replaced and stopped while the
war keeps running, no restart needed:

1. In a browser, open `livecenter.tiktok.com`, start the LIVE and copy the Server URL and the Stream key (only accounts that TikTok
   granted LIVE access and the stream key option see them).
2. Paste both into the dashboard card **Siaran ke TikTok** and press **Mulai siaran**, or run
   `r3dsims output start --url rtmp://... --key KEY` (`R3D_STREAM_KEY` also works). Stop with **Stop** / `output stop`.
3. The key is never shown again (the card shows `rtmp://host/app/…1234`) and is not stored.

If ffmpeg dies (network drop) it is restarted after 3 seconds, at most 6 times a minute, then the card says the key may be used up.
The dashboard only accepts `rtmp://`, `rtmps://`, `srt://`, `udp://`, `tcp://` and `rtp://` targets, because a file path would let a remote
client overwrite files; files and stdout stay available through `--out` at start. Use `--preset live` (720x1280) for the vertical picture and
`--bitrate 2500..4000` (kbit/s). `r3dsims output status` and `/output` report the state, uptime and restarts.

### Instagram Live (and Facebook, YouTube, Twitch)

The same output feeds any RTMP(S) ingest; the address tells which service it is, so the dashboard shows a short guide and the status
carries `platform` and `warnings`. For **Instagram Live**:

1. Open the Live Producer of your account (Instagram on the web, or the app's option for streaming software) and copy the address it
   shows. It looks like `rtmps://edgetee-upload-...fbcdn.net:443/rtmp/<number>?s_bl=1&...&s_vt=ig&a=<token>`. Use **Practice** (latihan)
   first: it connects without publishing.
2. Paste the **whole address** into the first field of the card (leave Stream key empty), or split it into Server URL and key. Spaces
   from copying are ignored. CLI: `r3dsims output start --url 'rtmps://.../rtmp/<number>?...'`.
3. Run with `--preset live` (720x1280). Instagram wants 9:16, H.264 + AAC (a silent AAC track is added), 30 fps, a keyframe every 2 s,
   about 3500 kbit/s: all of that is what the output sends. The status warns when the picture is not 9:16 or the address is `rtmp://`
   (Instagram and Facebook accept only `rtmps://`).

The token in the query string is as secret as the key: the card and `/output` show only `rtmps://host:443/rtmp/…9664`, and the dashboard
never keeps a pasted address in the browser. Only the **video out** is covered. Reading Instagram comments needs Meta's Graph API
(a business account and an approved app), which is not built in; until then viewer actions come from the **Uji coba** tab or from your
own connector posting to `/webhook`.

### Running it on a Linux server

```
# needs: ffmpeg, and Vulkan. Without a GPU install mesa's software driver (apt: mesa-vulkan-drivers) - it works but is slow
r3dsims live demo/war.json --preset live --pixel 2 --listen 0.0.0.0:8787 --token CHANGE-ME \
  --tiktok @username --tiktok-proxy socks5://user:pass@host:1080
```

* Listening beyond this machine **requires `--token`** (the dashboard can start a broadcast and change the stream); `--allow-no-token`
  accepts the risk. There is no TLS: put it behind a reverse proxy (Caddy, nginx) or an SSH tunnel for anything public.
* A systemd service: `ExecStart=/usr/local/bin/r3dsims live /srv/war/war.json --preset live --listen 127.0.0.1:8787 --token ...`,
  `Restart=always`, `Environment=R3D_TIKTOK=@username`; open `/dashboard?token=...` through the tunnel.
* Several streams: run one `r3dsims live` per stream on its own port (`--listen`), each with its own `war.json` / history file, and watch
  them all in the **Stream lain** tab of any dashboard.
* **Speed without a GPU is the real limit.** Measured on a 14-vCPU AMD EPYC VPS rendering with mesa's `llvmpipe`, the 720x1280 vertical
  picture ran at roughly 6 to 13 fps (it varied a lot between runs, voxel was not clearly faster than clay, and neither `--pixel 3`/`4`
  nor a smaller shadow map helped reliably), against 24 to 30 fps on a laptop's integrated GPU. A machine with any Vulkan GPU, or a
  home PC, is the safe place to broadcast from; a GPU-less server is fine for testing, the dashboard, the connector and the history.
  `r3dsims state` shows the real fps (`perf`), and the dashboard shows it in its header.

## Webhook contract (what the connector sends)

`POST /webhook`, body = one event, an array, or `{"events": [...]}`. Auth when `--token SECRET` is set:
`Authorization: Bearer SECRET`, or `X-Token: SECRET`, or `?token=SECRET`. The answer lists which triggers fired.

```json
{"type": "gift", "user": "Budi", "gift": "Rose", "count": 3, "diamonds": 1, "id": "msg-123"}
{"type": "like", "user": "Ani", "count": 25}
{"type": "comment", "user": "Cici", "text": "merah!"}
{"type": "follow" | "share" | "join" | "subscribe", "user": "Dodi"}
{"type": "custom", "gift": "my-event", "user": "x"}
```

* `side` (`"A"`/`"B"`) says which kingdom the viewer supports, when the connector knows it.
* `id` makes retries safe: the same id is ignored the second time.
* TikTok-style names are accepted as they are: `uniqueId`/`nickname` (user), `giftName`, `repeatCount`
  (count), `diamondCount`, `comment`, `msgId`.
* Gift names ignore case, spaces, `_` and `-` (`Finger Heart` = `finger_heart`). `gift_aliases` maps
  local names (`"mawar": "rose"`).

Other endpoints: `GET /state` (also carries `tiktok`, `output` and `session`), `GET /hooks`, `GET /overlay`, `GET /history`, `GET /health`,
`GET /dashboard` (`/test` and `/view?panel=1` are the same page), `GET /tiktok` (connector status and the latest events), `GET /output`
(video output status, key masked), `POST /hooks`, `POST /overlay`, `POST /control`, `POST /command` (any story/camera step),
`POST /tiktok` (`{"op":"connect","username":"@name"}` / `{"op":"disconnect"}`), `POST /output` (`{"op":"start","server":"rtmp://...",
"key":"..."}` / `{"op":"stop"}`). `r3dsims send|hook|overlay|control|state|history|tiktok|output|step` wrap them (`--server
http://host:8787`, `--token`, or `R3D_SERVER` / `R3D_TOKEN`).

### Wins and losses per session

A session is one run of `r3dsims live`. Every finished round (and match) is appended to a JSON-lines file, by default
`sessions.jsonl` next to `war.json` (`--history FILE` to move it, `--no-history` for none), so the record survives
restarts and crashes (a heartbeat every 30 s keeps the duration of a killed session). Per side: rounds won and rounds
lost (the other side's wins); draws are counted apart; the session goes to the side with more round wins.
`r3dsims history` prints the table, `GET /history` returns `{sessions, totals}` (totals per side name across all sessions:
rounds won/lost, sessions won/lost). `GET /state` has `session` for the running one. The HUD shows `MENANG` and `KALAH` per
side (labels `label_wins` / `label_losses` in `texts`).

## Shows: a war, or a studio (not a game)

`r3dsims live FILE` reads `FILE` and decides by what is in it: with `sides` it is a war, without it is a studio.
Both name a `scene` and both are made of the same parts: **triggers** (what a viewer event matches), **macros** (what runs:
`wait`, `repeat`, `random`, `if`, `overlay`, `step` for any story or camera command, `set`, `add`, `call`, `announce`),
**bots** (what runs when nobody writes) and an **overlay**. What a war adds is `sides`, units, actions, rounds and the history of
wins and losses; a studio adds nothing, so nothing in the way.

```
r3dsims template studio -o studio      # the kampung diorama as a live show
r3dsims validate studio/studio.json    # checks macros, steps and actors, fires every trigger, idles 45 s
r3dsims live studio/studio.json --preset live     # then http://127.0.0.1:8787/dashboard
r3dsims send comment "halo kampung" --user Sari
r3dsims send gift rose --user Budi
r3dsims send like 20
```

```json
{
  "title": "KAMPUNG LIVE",
  "scene": "scene.json",
  "vars": {"likes": 0},
  "goal": {"var": "likes", "target": 200, "label": "TARGET LIKE", "on_reach": "target_tercapai"},
  "triggers": [
    {"on": "comment", "macro": "balon", "cooldown": 1.2},
    {"on": "like", "macro": "hitung_like"},
    {"on": "gift:rose", "macro": "tari_mawar", "label": "ROSE = TARI MAWAR", "icon": ":rose:", "stop": true}
  ],
  "macros": {
    "balon": [{"do": "step", "step": {"do": "say", "actor": "ani", "text": "{user}: {text}", "secs": 4}}],
    "hitung_like": [{"do": "add", "var": "likes", "from": "count"}]
  },
  "bots": [{"name": "sapa", "macro": "balon", "when_idle": 15}]
}
```

* In macro text, overlay elements and `step`s, `{user} {text} {gift} {count} {diamonds}` name the viewer event that started
  it and `{var.NAME}` a counter. Chat text is cut at 70 characters.
* `add` takes `"from": "count"` (a like of count 20 adds 20) or `"diamonds"`; `set`/`add` change a counter, `goal` shows one
  as a bar. A repeating goal asks for another `target` each time it is reached.
* A studio refuses at load what needs a game: trigger `do`, the `act` step, conditions about sides or rounds. An unknown
  macro, a `step` that is not a story or camera command, or an `actor` that is not in the scene is an error that names it.
* The on-screen part: title, goal bar, the leaderboard (top 3 by points; a gift counts its diamonds), the ticker, the
  banner and the gift menu (triggers with a `label`). `"hud": false` turns it off.
* Control: `pause`, `resume`, `reset`, `run:MACRO`. `GET /state` has `kind: "studio"`, `vars`, `goal`; there is no
  win/loss history without camps. The dashboard hides what only a game has.
* `r3dsims hook publish` works the same way (triggers, macros, bots, gift aliases); a studio refuses `actions` and `units`.

## war.json

```json
{
  "title": "PERANG KERAJAAN",
  "scene": "scene.json",
  "castle_hp": 1000,
  "match": {"mode": "rounds", "rounds": 3, "round_secs": 90, "lobby_secs": 5, "intermission": 7, "restart_secs": 20},
  "sides":  {"A": {"name": "MERAH", "color": "#e0453a", "castle": "castle_a", "spawn": [12.5, 12], "unit": "knight",
                   "facing": 90, "castle_radius": 3.0, "zone": [4, 4, 25, 20]},
             "B": { ... }},
  "units":  {"knight": {"asset": "knight", "hp": 45, "speed": 2.0, "dmg": 7, "range": 1.2, "cooldown": 0.9}, ...},
  "actions":  { "meteor": {"type": "meteor", "count": 1, "radius": 3.2, "damage": 150, "castle_damage": 110}, ... },
  "triggers": [ {"name": "rose", "on": "gift:rose", "side": "A", "do": "meteor", "label": "ROSE = METEOR MERAH", "icon": ":rose:"} ],
  "macros": { ... }, "bots": [ ... ], "gift_aliases": { ... }, "overlay": [ ... ]
}
```

### Rounds or endless, and the winner

* `"mode": "rounds"`: best of `rounds` (first to a majority ends it). A round ends when a castle falls
  (the other side wins) or at `round_secs` (the castle with more HP wins, equal = draw). After the last
  round the **match winner** is the side with more round wins, then more castle HP summed over the
  rounds, otherwise a draw. `restart_secs` (default 15) starts a new match by itself; `null` waits for
  `r3dsims control start`.
* `"mode": "endless"`: a winner after every round, round after round, until `r3dsims control stop`
  (then the match winner is decided like above).
* `control`: `start`, `next_round` (end this round now), `reset`, `stop`, `pause`, `resume`, `biome:NAME`, and `run:NAME[:SIDE]`
  (play any action or macro right now, e.g. `run:aksi_massa:B`, for testing; needs a round in progress).

### Nobody is watching: the autopilot

The stream never sits still. The lobby starts the first round by itself, rounds end by themselves, a new
match or round follows by itself, and when no webhook event arrived for `autopilot.idle_secs` (default 6) an
**autopilot** plays moves at random intervals: reinforcements, speeches, meteors, whatever you list. The HUD shows
a hint line while it plays, and the first real event hands control straight back.

```json
"autopilot": {"idle_secs": 6, "interval": [2.5, 6.0], "actions": [
  {"weight": 4, "do": "reinforce", "side": "random"},
  {"weight": 1, "do": "meteor", "side": "leader"}]}
```

Without `actions` it reinforces a random side, or fires any named action on a random side. `"autopilot": null`
turns it off. Macro bots (below) are for behaviour you script yourself.

### Soldiers

Each side's soldiers march toward the enemy objective (`castle`; `base` and `objective` are accepted names), fight the
nearest enemy in range, attack the objective when they reach it. Clips used from the unit's asset: `idle`, `walk`,
`attack`, `cheer` (speeches) and the `defeat_anim` (default `die`; the demo uses `faint`, nobody is hurt in a
parody). `ranged` units shoot (cosmetic arrows). `structure: true` units (barricades) never move or attack and are
not counted as soldiers: the other side tears them down. Autospawn (`autospawn.every`) keeps a trickle of
reinforcements; `max_units` caps a side.

### Actions: what a trigger can do

`side` is where the effect lands: the victim for meteor/arrows/lightning/quake, the beneficiary for the rest.

| type | fields |
| --- | --- |
| `meteor` | `count`, `radius`, `damage`, `castle_damage`, `gap`, `target` (`auto` `castle` `field` `units` `front`), `friendly_fire`, `scale`, `asset` (what falls, e.g. a bottle), `burn_secs` / `burn_dps` / `burn_asset` (leaves a burning patch) |
| `arrows` | `count`, `radius`, `damage`, `castle_damage`, `target` |
| `lightning` | `count`, `damage`, `castle_damage` |
| `spawn` | `unit` (empty = the side's default), `count`, `at` (`spawn`, `front`, `center`) |
| `heal` | `amount` (castle HP; negative costs HP), `asset` (the little effect that floats up: `nasi`, `uang`...) |
| `shield` | `secs` (castle takes no damage) |
| `boost` | `secs`, `speed`, `damage` (soldiers of the side) |
| `chant` | speech bubbles and a cheer: `texts`, `count`, `secs`, `anim`, `boost_secs` (rally) |
| `push` | shove the side's soldiers away from a point (water cannon): `strength`, `radius`, `damage`, `target` |
| `cloud` | lingering gas / smoke / fire: `radius`, `secs`, `slow` (speed multiplier), `dps`, `lethal`, `asset`, `target` |
| `convert` | soldiers near a point change sides (bribes, defections): `count`, `radius`, `target` |
| `sabotage` | strips shield and boost, slows (`slow_secs`, `slow`), tears down structures |
| `quake` | `damage` (every soldier of the side), `castle_damage` |
| `biome` | `name`: a key of `biomes`, or `random` |
| `cinematic` | first-person camera for an ultimate (see below) |
| `step` | `step`: any story/camera step, e.g. `{"op":"orbit","dyaw":90,"secs":2}` |
| `overlay` | `add`, `remove`, `clear` (see Overlay) |
| `announce` | `text`, `secs`, `color` (big banner) |
| `seq` | `actions`: a list of names or inline actions |
| `macro` | `name` |

Name them under `actions` and use the name, or write the action inline in the trigger's `do`.

### Camera effects: ultimates only

Ordinary skills never move the camera. An **ultimate** is a macro that includes a `cinematic` action: the camera swoops
from the broadcast view into the **point of view of a camp**: raised above the gate of that side's objective, looking
out over the field and slowly sweeping (`pov`: `enemy`, the camp opposite the action's side, by default, so the attacker's
camp sees the damage land; `self`, `A`, `B`), stays for `secs`, and swoops back. The viewpoint is the camp, not a person.
What the viewer sees depends on the parameters you give each ultimate:

| field | effect |
| --- | --- |
| `look` | `enemy` straight ahead (default), `sky` looking up (meteors, lightning), `castle` at the enemy objective; `pitch` overrides |
| `fov` | wide or tight lens |
| `shake` | camera shake (an earthquake feel) |
| `tint`, `vignette` | a colour wash (`#rrggbbaa`) and darkened edges: blue for a tsunami, orange for fire, yellow for a crowd |
| `flashes` | white lightning flashes per second |
| `unit` | `camp` (default). `front` / `random` ride one soldier of that camp instead, with `bob` (head bob) |

The `perang` template has three: `galaxy` (meteor apocalypse, looks up, orange), `whale` (a tsunami pushing the whole
enemy line back, blue, wide lens), `universe` (a lightning storm, flashes). The `demo` template has `galaxy` (the
crackdown seen from the government camp) and `whale` (a thousand people marching, seen from the citizens' camp).

### Soldier AI and skill animations

* **Home defence first.** When enemy soldiers get within 7 units of a castle's edge, that camp's nearest soldiers turn
  back and fight them, the intruder closest to the base first (a few soldiers while the base is only approached; all
  of them while the castle is being hit or below 40% HP). The rest keep marching on the enemy. No setting needed.
* **Smart autopilot.** While a camp's base is pressed, the autopilot picks more of the moves that help that camp (heal,
  reinforce, cheer) and more of the moves that hit its attackers.
* **Animations.** Soldiers grow in with a little overshoot when they appear. A helpful skill makes its castle swell and
  settle like a spring and leaves a ring of the camp colour spreading over the ground; a harmful skill makes the target
  castle shake once. A meteor shows a ring closing on the spot where it will land. The HP bar eases to its new value
  instead of jumping. Gas clouds grow in with overshoot, drift and shrink away. The ring comes from an asset named `ring`
  (`fx_assets.ring` to change it; without one the rings are simply not drawn).

### Biomes

A biome re-skins the world while the war goes on: it recolours terrain tiles (by legend key), sets the sky, swaps scenery
by entity-id prefix, scales soldier speed and can run a hazard by itself.

```json
"biome": "siang",
"biome_rotation": {"every": "round", "list": ["siang", "malam"]},
"biomes": {
  "malam": {"name": "MALAM", "sky": "night", "speed": 0.95,
            "terrain": {"p": {"top": "#8e8aa6:stone", "side": "#625f7a:stone"}},
            "props": [{"prefix": "tree_", "asset": "pine"}],
            "hazard": {"every": 16, "do": {"type": "meteor", "count": 2, "target": "field"}, "side": "random"}}
}
```

Skies: `day`, `dawn`, `sunset`, `night`, `desert`, `snow`, `lava`. Switch by hand with `r3dsims biome NAME`, by gift with
the `biome` action, or let `biome_rotation` do it every round (`every: "round"`) or every N seconds (`every: "secs"`,
`secs`; `random: true` shuffles). `perang` has grass, desert, snow and lava (eruptions); `demo` has day, dusk and night
(street lamps and windows glow).

### The two templates

| | `perang` | `demo` |
| --- | --- | --- |
| sides | MERAH / BIRU kingdoms, castles | RAKYAT (citizens, stage) / PEMERINTAH (police, government building) |
| mode | 3 rounds, best of | endless, day and night rotation |
| units | knight, archer, giant | warga, mahasiswa, polisi, barikade (structure); nobody dies, they faint |
| gifts | meteor, lightning, arrows, reinforcements, shield, ultimates | orasi (morale), nasi bungkus (heal), molotov, barricade, foreign aid or corruption (government heal), water cannon, tear gas, intelligence (sabotage), rice-pack bribes (defection), ultimates |

Everything in both is data: copy a template and change names, assets, gifts and numbers; the engine does not care
whether the objective is a castle or a parliament.

### Triggers

`{"name", "on", "side", "do" | "macro", ...}`. `on` is one pattern or a list: `gift:rose`, `gift:*`,
`like`, `comment:~word` (contains), `comment:*`, `follow`, `share`, `join`, `subscribe`, `custom:NAME`.
`side`: `A`, `B`, `event` (the event's own `side`, else random), `random`, `leader`, `trailing` (the
side with less castle HP, to help the underdog). More fields: `min_count`, `min_diamonds`, `per_count`
(repeat once per gift count, capped by `max_repeat`), `cooldown`, `announce` / `label` / `icon` (ticker
line and the on-screen gift menu), `legend` (hide from the menu), `overlay` (elements popped on trigger,
`{user}` `{gift}` `{count}` `{side}` are replaced), `stop`.

Events are only acted on while a round is being fought; during the lobby and between rounds they are
reported as `ignored`.

### Macros and bots (scripts)

A macro is a list of steps: `act` (action + side), `event` (feed a synthetic event through the
triggers), `wait`, `repeat`, `forever`, `random` (weighted `choices`), `if` / `until` (conditions),
`announce`, `overlay`, `step` (camera), `call` (another macro), `set` / `add` (numeric variables), `stop`.
Conditions: `hp_pct_lt` / `hp_pct_gt` (`{"side","value"}`), `units_lt` / `units_gt`, `round`, `round_gte`,
`phase`, `leading`, `idle_secs_gt`, `round_secs_gt`, `var` (`name` + `gte`/`lte`/`eq`), `chance`, `not`,
`all`, `any`.

A trigger runs a macro with the event as context (`{user}` works inside it). A **bot** runs a macro by
itself: `{"name": "autoplay", "macro": "fill", "when_idle": 8}` (nobody sent anything for 8 s) or
`"every": 14`, optionally with `"when": <condition>`. The template's bots keep the field alive and
shield a castle that is about to fall.

### Publish webhook functions while live

An agent changes behaviour without a restart:

```
r3dsims hook publish my-hooks.json
r3dsims hook list
r3dsims hook remove galaxy
```

```json
{"actions":  {"mega": {"type": "meteor", "count": 12, "target": "front"}},
 "triggers": [{"name": "universe", "on": "gift:universe", "side": "A", "do": "mega",
               "label": "UNIVERSE = KIAMAT MERAH", "icon": ":boom:", "cooldown": 10}],
 "macros":   {"fireworks": [{"do": "repeat", "times": 5, "steps": [{"do": "act", "action": "arrow_rain", "side": "random"}, {"do": "wait", "secs": 0.5}]}]},
 "remove":   ["rose"], "replace": false}
```

Entries with an existing name are replaced, new ones added. The whole publication is validated first
(unknown action, macro, unit, bad side...): on error **nothing** changes and every problem is listed.
`"replace": true` first drops the triggers/actions/macros/bots of the kinds you send.

## Overlay: text, emoji, SVG, PNG, bars

Anything on screen besides the diorama is an overlay element. The built-in war HUD is made of the same
elements, and `"hud": false` turns it off so you can draw your own.

```
r3dsims overlay add '{"id":"promo","type":"text","text":"FOLLOW FOR :fire: BATTLES","at":[0.5,0.95],"anchor":"bottom","size":0.035,"bg":"#000000aa"}'
r3dsims overlay add '{"type":"emoji","emoji":"🌹","size":0.18,"at":[0.5,0.45],"ttl":2,"anim_in":"pop"}'
r3dsims overlay add logo.json            # a file: SVG elements, bars, anything
r3dsims overlay patch promo '{"text":"ROUND {round}: {name.A} VS {name.B}"}'
r3dsims overlay remove promo
r3dsims overlay list
r3dsims overlay emoji                    # the built-in emoji and their :short: names
```

Units: `at` is a fraction of the frame; every length (`size`, `w`, `h`, `pad`, `offset`) is a fraction of the
**shorter** frame side, so a layout survives a change from 1280x720 to 720x1280.

| `type` | fields |
| --- | --- |
| `text` | `text`, `size` (font size), `color`, `shadow`, `outline`, `bg`, `pad`, `align`, `max_width`, `font` (`ui` default, `pixel`) |
| `emoji` | `emoji` (a character or `:name:`), `size` |
| `svg` | `src` (file) or `svg` (inline markup), `w`, `h` |
| `image` | `src` (PNG, or an SVG by extension), `w`, `h` |
| `rect` | `w`, `h`, `color`, `radius`, `border`, `border_w`; `full: true` covers the whole frame (tints, flashes) |
| `vignette` | `color`, `strength` (darkened edges over the whole frame) |
| `bar` | `w`, `h`, `value` or `bind` (a live variable such as `hp_frac.A`), `color`, `bg`, `border`, `from_right` |

Common: `id`, `at`, `anchor` (`center`, `top-left`, `top`, `bottom`...), `offset`, `opacity`, `z`, `ttl`,
`anim_in` (`pop` `fade` `slide_up` `slide_down` `slide_left` `slide_right`), `anim_out`, `anim_secs`,
`from` / `until` (seconds on the overlay clock), `visible`.

* **Text** uses a bundled smooth font (Roboto Bold): upper and lower case, digits, Latin accents and punctuation. Characters it lacks (CJK, Arabic) are skipped as a gap. `"font": "pixel"` selects the retro 3x5 font (A-Z, 0-9 and `. , ! ? : - @ ' / + % ( ) = > < * # _ & | [ ]`). Keep `size` small (0.02 to 0.03): the built-in HUD is thin on purpose so it does not cover the picture.
  Emoji, real characters or `:short:` names, are drawn inline.
* **Emoji** come from a built-in set of about 55 vector emoji, so they work on a server without any
  emoji font (`r3dsims overlay emoji`). For the full set put Twemoji/Noto SVGs named by code point
  (`1f339.svg`, or `.png`) in a folder and pass `--emoji-dir DIR`. An unknown emoji shows a `?` tile.
* **SVG** is rasterised at the exact size needed (resvg); `<text>` in an SVG needs a system font.
* Live values for text (`{title}` `{round}` `{rounds}` `{time}` `{phase}` `{name.A}` `{hp.A}` `{hp_frac.A}`
  `{units.A}` `{wins.A}` `{color.A}` `{ticker.1}`..`{ticker.5}` `{top1}`..`{top3}` `{banner}` `{var.NAME}`)
  are replaced every frame, the colour fields too. Replace `A` by the side key you chose.
* `r3dsims render scene.json -o out.mp4 --overlay overlay.json` uses the same elements on recorded videos.

## Safety and limits

* `--listen 127.0.0.1:8787` by default. To take webhooks from another machine listen on `0.0.0.0:PORT`
  **with `--token`**, or keep it local and let the connector run next to it. There is no TLS: use a
  reverse proxy for anything public.
* Bodies over 1 MB are cut; one simulation thread owns the state, so requests are applied in order.
* `max_units` caps soldiers per side; macros are capped (400 steps per tick, depth 24, 64 at a time).
* The picture is rendered at the configured size on the server's GPU (Vulkan, Metal, DX12). In real time the world follows the wall
  clock, so a machine that cannot keep up drops frames instead of playing in slow motion. `r3dsims state` shows the measured fps.
