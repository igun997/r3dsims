---
name: r3dsims-record
description: Use when rendering or recording r3dsims output for social media: stills, mp4/webm/gif, PNG sequences, Instagram reel/feed/square presets, timed camera commands, audio, and the live r3dlive recorder.
---

# Rendering and recording

```
r3dsims render scene.json -o shot.png --preset feed --at 14          # one still at t=14s
r3dsims render scene.json -o reel.mp4 --preset reel --seconds 30     # H.264, faststart
r3dsims render scene.json -o loop.gif --preset square --seconds 8
r3dsims render scene.json -o frames/ --preset reel --seconds 5       # PNG sequence
r3dsims render scene.json -o reel.mp4 --audio music.mp3 --commands cmds.json
```

* Presets: `reel`/`story`/`tiktok`/`shorts` 1080x1920, `feed` 1080x1350, `square` 1080x1080,
  `landscape`/`youtube` 1920x1080, or `--size WxH`. Rendering happens at 1/`--pixel` of that (default
  3) and is upscaled with nearest-neighbour; `--pixel 2` = finer pixels, `--pixel 4` = chunkier.
* `--start S` simulates S seconds first, `--play NAME` plays one scene only, `--fps` (30).
* Video needs `ffmpeg` on the machine that renders. `.gif`, `.png` and sequences do not.
* `--commands cmds.json` adds timed steps without editing the scene:
  `[{"at":2,"do":"camera","op":"orbit","dyaw":90,"secs":3},{"at":6,"do":"building","id":"b","gap":3}]`.
  Any step works (camera, say, play, building, set_sky, spawn).
* Renders are deterministic for one GPU adapter; a render of 40 s at 1080x1920 takes under a minute
  on a laptop GPU (the H.264 encode dominates).

## Instagram checklist

Reel/story 9:16 `reel`, feed post 4:5 `feed`, 15-30 s, first second already moving. Keep the subject
inside the central 80 % (UI overlays the edges). Check `ffprobe`: h264, yuv420p, 30 fps.
Verify by extracting frames: `ffmpeg -ss 12 -i reel.mp4 -frames:v 1 f.png`.

## Live recording

`r3dlive --demo pelabuhan --preset reel --out recordings`: pick the format, press **Record mp4**
(simulation steps at a fixed 1/30 s so the video is deterministic), orbit/zoom with the mouse, send
JSON steps from the command box, press stop. **Screenshot** saves a PNG at output size.

## Troubleshooting

* "no GPU adapter": install a Vulkan driver (`mesa-vulkan-drivers` gives lavapipe, slow but works).
* "ffmpeg not found": install ffmpeg or render to `.gif` / a PNG directory.
* Story ends early or an actor stands still: run `validate` and read the `[warn]` lines.
