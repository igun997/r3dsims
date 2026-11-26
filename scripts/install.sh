#!/bin/sh
# r3dsims installer for Linux and macOS (Apple silicon).
#
# Downloads the latest GitHub release of igun997/r3dsims for this platform, verifies its sha256,
# installs `r3dsims` and `r3dlive` into PREFIX/bin and the agent skills into ~/.claude/skills and
# ~/.agents/skills. Read it before you run it.
#
#   sh install.sh [--prefix DIR] [--no-skills]
#   R3DSIMS_VERSION=v0.1.0 sh install.sh          # a specific release instead of the latest
set -eu

REPO="igun997/r3dsims"
PREFIX="${HOME}/.local"
SKILLS=1
VERSION="${R3DSIMS_VERSION:-latest}"

usage() {
    sed -n '2,12p' "$0" | sed 's/^# \{0,1\}//'
}

while [ $# -gt 0 ]; do
    case "$1" in
        --prefix) PREFIX="$2"; shift ;;
        --no-skills) SKILLS=0 ;;
        -h | --help) usage; exit 0 ;;
        *) echo "unknown option: $1" >&2; usage >&2; exit 2 ;;
    esac
    shift
done

need() {
    command -v "$1" >/dev/null 2>&1 || { echo "missing required tool: $1" >&2; exit 1; }
}
need curl
need tar

case "$(uname -s)-$(uname -m)" in
    Linux-x86_64) target="x86_64-unknown-linux-gnu" ;;
    Darwin-arm64 | Darwin-aarch64) target="aarch64-apple-darwin" ;;
    *)
        echo "no prebuilt release for $(uname -s) $(uname -m)." >&2
        echo "build from source: git clone https://github.com/$REPO && cd r3dsims && cargo build --release --workspace" >&2
        exit 1
        ;;
esac

if [ "$VERSION" = "latest" ]; then
    final="$(curl -fsSLI -o /dev/null -w '%{url_effective}' "https://github.com/$REPO/releases/latest")"
    tag="${final##*/}"
    case "$tag" in v*) ;; *) echo "could not find a release of $REPO (no release published yet?)" >&2; exit 1 ;; esac
else
    tag="$VERSION"
fi

name="r3dsims-${tag}-${target}"
base="https://github.com/$REPO/releases/download/$tag"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

echo "downloading $name"
curl -fsSL "$base/$name.tar.gz" -o "$tmp/$name.tar.gz"
curl -fsSL "$base/SHA256SUMS" -o "$tmp/SHA256SUMS"

(
    cd "$tmp"
    want="$(grep " $name.tar.gz\$" SHA256SUMS | cut -d' ' -f1)"
    [ -n "$want" ] || { echo "no checksum for $name.tar.gz in SHA256SUMS" >&2; exit 1; }
    if command -v sha256sum >/dev/null 2>&1; then
        got="$(sha256sum "$name.tar.gz" | cut -d' ' -f1)"
    else
        got="$(shasum -a 256 "$name.tar.gz" | cut -d' ' -f1)"
    fi
    [ "$want" = "$got" ] || { echo "checksum mismatch for $name.tar.gz" >&2; exit 1; }
)
echo "checksum ok"

tar -xzf "$tmp/$name.tar.gz" -C "$tmp"
src="$tmp/$name"

mkdir -p "$PREFIX/bin"
for bin in r3dsims r3dlive; do
    install -m 755 "$src/bin/$bin" "$PREFIX/bin/$bin"
done
echo "installed r3dsims and r3dlive into $PREFIX/bin"

if [ "$SKILLS" = 1 ] && [ -d "$src/skills" ]; then
    for dest in "$HOME/.claude/skills" "$HOME/.agents/skills"; do
        mkdir -p "$dest"
        for skill in "$src"/skills/*; do
            rm -rf "$dest/$(basename "$skill")"
            cp -R "$skill" "$dest/"
        done
        echo "installed skills into $dest"
    done
fi

case ":$PATH:" in
    *":$PREFIX/bin:"*) ;;
    *) echo "note: add $PREFIX/bin to PATH (export PATH=\"$PREFIX/bin:\$PATH\")" ;;
esac

"$PREFIX/bin/r3dsims" --version
echo "next: r3dsims template pelabuhan -o /tmp/p && r3dsims validate /tmp/p/scene.json"
echo "needs a GPU adapter (Vulkan/Metal) and ffmpeg for mp4 output; see README"
