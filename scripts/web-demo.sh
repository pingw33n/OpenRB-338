#!/usr/bin/env bash
# The web player's test page (task-171, docs/web.md): builds the worklet's module
# (crates/orb-worklet) and the page's module (crates/orb-web-demo, through wasm-bindgen) for
# wasm32-unknown-unknown into target/web-demo/, and with --serve serves it at
# http://localhost:PORT/ (default 8338) until interrupted.
#
#   scripts/web-demo.sh [--serve [PORT]]
#
# Needs wasm-bindgen-cli of the lockfile's wasm-bindgen version (0.2.129):
#   cargo install wasm-bindgen-cli --version 0.2.129 --locked
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
target="${CARGO_TARGET_DIR:-$root/target}"
out="$target/web-demo"
want=0.2.129

if ! have="$(wasm-bindgen --version 2>/dev/null)"; then
    echo "wasm-bindgen not found: cargo install wasm-bindgen-cli --version $want --locked" >&2
    exit 1
fi
if [[ "$have" != "wasm-bindgen $want" ]]; then
    echo "$have, but the lockfile has $want: cargo install wasm-bindgen-cli --version $want --locked" >&2
    exit 1
fi

cd "$root"
# two builds: one with the feature `web` of orb-player would put wasm-bindgen into the
# worklet's module too (features are unified within a build)
cargo build --locked --release --target wasm32-unknown-unknown -p orb-worklet
cargo build --locked --release --target wasm32-unknown-unknown -p orb-web-demo

wasm="$target/wasm32-unknown-unknown/release"
rm -rf "$out"
mkdir -p "$out"
cp "$wasm/orb_worklet.wasm" "$out/"
wasm-bindgen --target web --no-typescript --out-dir "$out/pkg" "$wasm/orb_web_demo.wasm"
cp crates/orb-web-demo/index.html "$out/"
echo "built $out"

if [[ "${1:-}" == "--serve" ]]; then
    port="${2:-8338}"
    echo "serving http://localhost:$port/ (Ctrl-C stops)"
    exec python3 -m http.server --bind 127.0.0.1 --directory "$out" "$port"
fi
