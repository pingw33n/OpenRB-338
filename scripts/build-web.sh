#!/usr/bin/env bash
# OpenRB-338 as a web page (task-172, docs/web.md section 7, docs/building.md "The web page"):
# builds the AudioWorklet's module (crates/orb-worklet) and the page's module (crates/orb-web,
# orb-ui on eframe's web runner, through wasm-bindgen) for wasm32-unknown-unknown in release,
# and writes the static site to dist/web/:
#
#   index.html  favicon.ico  orb_worklet.wasm  pkg/orb_web.js  pkg/orb_web_bg.wasm
#
# Every URL in it is relative, so it can be served from any path (GitHub Pages serves the
# project site under /OpenRB-338/). With --serve it then serves dist/web/ at
# http://localhost:PORT/OpenRB-338/web/ (default port 8338) until interrupted, through a folder
# of links that puts the site under that sub-path.
#
#   scripts/build-web.sh [--index FILE] [--serve [PORT]]
#
# --index FILE copies FILE (the collection index `orb web-index` writes, docs/web.md section 8)
# into the site as index.json, which the page fetches for File > Open Collection... and the mod
# collection (task-174, docs/web.md section 9). Without it the site has no index and the page
# has no collections.
#
# Needs the pinned toolchain's wasm32-unknown-unknown target (rust-toolchain.toml) and
# wasm-bindgen-cli of the lockfile's wasm-bindgen version (0.2.129):
#   cargo install wasm-bindgen-cli --version 0.2.129 --locked
# wasm-opt of binaryen (the pinned version_133, downloaded into target/ and checked by SHA-256 by
# scripts/binaryen.sh; macOS and Linux). The version and commit of the About box come from
# orb-ui's build script, as natively (SOURCE_DATE_EPOCH and ORB_BUILD_BRANCH apply).
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
target="${CARGO_TARGET_DIR:-$root/target}"
out="$root/dist/web"
want=0.2.129
index=""
serve=""
port=8338
while [[ $# -gt 0 ]]; do
    case "$1" in
        --index) index="${2:?--index needs a file}"; shift 2 ;;
        --serve)
            serve=1
            shift
            if [[ $# -gt 0 && "$1" != --* ]]; then port="$1"; shift; fi
            ;;
        *) echo "usage: scripts/build-web.sh [--index FILE] [--serve [PORT]]" >&2; exit 2 ;;
    esac
done
if [[ -n "$index" ]]; then
    index="$(cd "$(dirname "$index")" && pwd)/$(basename "$index")"
    [[ -f "$index" ]] || { echo "--index $index: no such file" >&2; exit 1; }
fi

if ! have="$(wasm-bindgen --version 2>/dev/null)"; then
    echo "wasm-bindgen not found: cargo install wasm-bindgen-cli --version $want --locked" >&2
    exit 1
fi
if [[ "$have" != "wasm-bindgen $want" ]]; then
    echo "$have, but the lockfile has $want: cargo install wasm-bindgen-cli --version $want --locked" >&2
    exit 1
fi

cd "$root"
# two builds: one with the page's module would put orb-player's feature `web` (wasm-bindgen)
# into the worklet's module too (features are unified within a build)
cargo build --locked --release --target wasm32-unknown-unknown -p orb-worklet
# the page's module only is built with wasm SIMD (tiny-skia under resvg, which rasterizes the
# modern skin's panel, has SIMD paths; task-323); the worklet's module (the engine) is not
RUSTFLAGS="${RUSTFLAGS:-} -C target-feature=+simd128" \
    cargo build --locked --release --target wasm32-unknown-unknown -p orb-web

wasm="$target/wasm32-unknown-unknown/release"
rm -rf "$out"
mkdir -p "$out"
cp "$wasm/orb_worklet.wasm" "$out/"
wasm-bindgen --target web --no-typescript --out-dir "$out/pkg" "$wasm/orb_web.wasm"
# binaryen's wasm-opt on the page's module (pinned, scripts/binaryen.sh). -g keeps the name
# section, so a panic's stack shows function names (task-172); -O3, not -Oz: speed is the aim.
wasm_opt="$(scripts/binaryen.sh)"
"$wasm_opt" -O3 -g --enable-simd "$out/pkg/orb_web_bg.wasm" -o "$out/pkg/orb_web_bg.wasm"
cp crates/orb-web/index.html "$out/"
cp assets/app_icon.ico "$out/favicon.ico"
if [[ -n "$index" ]]; then
    cp "$index" "$out/index.json"
fi
echo "built $out:"
(cd "$out" && find . -type f | sort | while read -r f; do
    printf '  %-24s %8d bytes\n' "${f#./}" "$(wc -c < "$f")"
done)

if [[ -n "$serve" ]]; then
    serve="$target/web-serve"
    rm -rf "$serve"
    mkdir -p "$serve/OpenRB-338"
    ln -s "$out" "$serve/OpenRB-338/web"
    echo "serving http://localhost:$port/OpenRB-338/web/ (Ctrl-C stops)"
    exec python3 -m http.server --bind 127.0.0.1 --directory "$serve" "$port"
fi
