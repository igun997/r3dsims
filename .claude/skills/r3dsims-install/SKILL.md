---
name: r3dsims-install
description: Use to install, update or build r3dsims (r3dsims CLI, r3dlive GPU app, agent skills) from the igun997/r3dsims GitHub release or source, and to verify the install.
---

# Install r3dsims

Repository: https://github.com/igun997/r3dsims. Two binaries: `r3dsims` (CLI: template, validate,
render, record) and `r3dlive` (Rune GPU UI). Both need a GPU adapter (Vulkan, Metal or DX12).

## From a release (Linux, macOS)

```
curl -fsSL https://raw.githubusercontent.com/igun997/r3dsims/main/scripts/install.sh -o /tmp/r3d-install.sh
less /tmp/r3d-install.sh        # read it first
sh /tmp/r3d-install.sh          # installs to ~/.local/bin and the skills to ~/.claude/skills
```

Options: `R3DSIMS_VERSION=v0.1.0` for a specific release, `--no-skills`, `--prefix DIR`.
Windows: download the `.zip` from the Releases page and put the folder on `PATH`.

## From source

```
git clone https://github.com/igun997/r3dsims && cd r3dsims
cargo build --release --workspace        # target/release/r3dsims and r3dlive
```

Rust stable (edition 2024). Linux build deps: `libxkbcommon-dev libwayland-dev libx11-dev
libxcb1-dev libxcursor-dev libxi-dev libvulkan-dev pkg-config`. If the repo has an `rcargo.toml`,
build with `rcargo` instead of `cargo`.

## Runtime needs

* A GPU adapter. On a GPU-less machine: `apt install mesa-vulkan-drivers` (lavapipe).
* `ffmpeg` for mp4/webm output.
* Linux `r3dlive` also needs the shared libraries from the build deps above at run time.

## Verify

```
r3dsims template pelabuhan -o /tmp/p
r3dsims validate /tmp/p/scene.json                  # prints "story finishes after ..s"
r3dsims render /tmp/p/scene.json -o /tmp/p/shot.png --preset feed --at 40
```

The PNG should show an island with a three-storey building. `r3dsims render` prints `gpu: <adapter>`
on stderr.
