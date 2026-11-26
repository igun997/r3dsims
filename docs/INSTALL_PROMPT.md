# One-prompt install

Paste this whole block into Claude Code, Codex, or any coding agent that can run shell commands.
It installs the release binaries and the skills, then proves the install with a real render.

````text
Install r3dsims (https://github.com/igun997/r3dsims) on this machine and prove it works. Do it yourself end to end; stop and ask me only if a step needs sudo that you cannot get.

1. Detect my OS and CPU architecture.
2. Download https://raw.githubusercontent.com/igun997/r3dsims/main/scripts/install.sh, read it, then run it with `sh`. It installs the latest release of `r3dsims` (CLI) and `r3dlive` (GPU live app) into ~/.local/bin, and the agent skills (r3dsims, r3dsims-assets, r3dsims-scenes, r3dsims-record, r3dsims-live, r3dsims-install) into ~/.claude/skills and ~/.agents/skills.
3. If the script says there is no release for my platform, build from source instead: install Rust stable plus the build packages for my OS (Linux: libxkbcommon-dev libwayland-dev libx11-dev libxcb1-dev libxcursor-dev libxi-dev libxrandr-dev libvulkan-dev pkg-config), `git clone https://github.com/igun997/r3dsims`, run `cargo build --release --workspace`, and copy target/release/r3dsims and target/release/r3dlive into ~/.local/bin. On Windows, download the .zip from https://github.com/igun997/r3dsims/releases and put its bin folder on PATH.
4. Make sure ~/.local/bin is on PATH for new shells (add it to my shell profile if it is not).
5. Make sure `ffmpeg` is installed (needed for mp4 output) and that a GPU adapter is available (on a Linux machine without a GPU install mesa-vulkan-drivers, which gives the lavapipe software driver).
6. Verify with real commands:
   r3dsims template pelabuhan -o /tmp/r3d-test
   r3dsims validate /tmp/r3d-test/scene.json
   r3dsims render /tmp/r3d-test/scene.json -o /tmp/r3d-test/shot.png --preset feed --at 40
   r3dsims render /tmp/r3d-test/scene.json -o /tmp/r3d-test/clip.mp4 --preset reel --seconds 3
   Look at shot.png and describe what it shows (it should be an island with a lighthouse, a dock with boats and a three-storey building seen without a roof).
7. Finish with a short report: installed versions and paths, which skills were installed, the GPU adapter name that `render` prints (`gpu: ...`), the story length `validate` printed, and any step that failed with its exact error message. Do not change anything outside ~/.local, ~/.claude, ~/.agents and /tmp.
````

After it finishes, ask the agent: *"Use the r3dsims skills to make a 20 second Instagram reel of
the harbour scene where the tourist climbs to the rooftop, and show me three frames."*
