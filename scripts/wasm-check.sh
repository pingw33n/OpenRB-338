#!/usr/bin/env bash
# The wasm-vs-native check of the engine (task-169; docs/building.md "The wasm build"): renders
# the reference corpus (tests/refs/manifest.json) with crates/orb-wasm-check built natively and
# built for wasm32-wasip1 and run under wasmtime, and requires the two outputs (one line per song:
# frames, the SHA-256 of the WAV, whether it is the manifest's wav_sha256) to be identical, with
# every song equal to the manifest. Exit 1 on any difference.
#
#   scripts/wasm-check.sh
#
# The sets: diag, app and shipped (the built-in kit; no external input). With ORB_CD_ISO set
# (the ReBirth 2.01 CD image) also mod and modapp: the four shipped mods are recrypted from the
# image (orb mod recrypt --iso) into a temporary folder.
#
# wasmtime is downloaded as a pinned release asset, checked against its SHA-256, into
# target/wasmtime-<version>/ (kept for the next run). Hosts: macOS and Linux, x86_64 or aarch64.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
target=${CARGO_TARGET_DIR:-$root/target}
case $target in /*) ;; *) target=$root/$target ;; esac

# The pinned version: wasmtime 51.0.0. A new version is a deliberate edit of these lines with
# the checksums (the release assets' digests on GitHub).
wt_version=v51.0.0
case "$(uname -s)-$(uname -m)" in
    Darwin-arm64)
        wt_arch=aarch64-macos
        wt_sha=61dd882b44bcf854b301518781f407b9f6db4c6bd157a55f8f44b1f1357f93dc
        ;;
    Darwin-x86_64)
        wt_arch=x86_64-macos
        wt_sha=51767ef35ea3c2653205c09892f174b75e507cf8ffccfcba00ed557832f0e72d
        ;;
    Linux-x86_64)
        wt_arch=x86_64-linux
        wt_sha=a18020674fac4107f329bc9ed720ea9c88e4b30a61e8a527787bcce051123d32
        ;;
    Linux-aarch64)
        wt_arch=aarch64-linux
        wt_sha=a263f34aefad1b24ba4d9ca635ddfea415f4acfe3f3f1bb6d4525c34fc6a7397
        ;;
    *) echo "wasm-check: macOS and Linux only (docs/building.md)" >&2; exit 1 ;;
esac

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

sha256() { if command -v sha256sum >/dev/null; then sha256sum "$1"; else shasum -a 256 "$1"; fi; }

wt_dir=$target/wasmtime-$wt_version
wasmtime=$wt_dir/wasmtime-$wt_version-$wt_arch/wasmtime
if [ ! -x "$wasmtime" ]; then
    asset=wasmtime-$wt_version-$wt_arch.tar.xz
    curl -fsSL --retry 3 -o "$tmp/$asset" \
        "https://github.com/bytecodealliance/wasmtime/releases/download/$wt_version/$asset"
    [ "$(sha256 "$tmp/$asset" | cut -d' ' -f1)" = "$wt_sha" ] \
        || { echo "checksum mismatch for $asset" >&2; exit 1; }
    mkdir -p "$wt_dir"
    tar -xJf "$tmp/$asset" -C "$wt_dir"
fi
"$wasmtime" --version

cd "$root"
cargo build --release --locked -p orb-wasm-check
cargo build --release --locked -p orb-wasm-check --target wasm32-wasip1

sets=diag,app,shipped
native_args=(--root "$root")
wasm_args=(--dir "$root::/root")
guest_args=(--root /root)
if [ -n "${ORB_CD_ISO:-}" ]; then
    cargo run --release --locked -q -p orb-cli -- mod recrypt --iso "$ORB_CD_ISO" -o "$tmp/mods"
    sets=$sets,mod,modapp
    native_args+=(--mod-dir "$tmp/mods")
    wasm_args+=(--dir "$tmp/mods::/mods")
    guest_args+=(--mod-dir /mods)
else
    echo "ORB_CD_ISO is not set: the sets mod and modapp are skipped"
fi

echo "=== native: $sets"
start=$SECONDS
"$target/release/orb-wasm-check" "${native_args[@]}" --set "$sets" > "$tmp/native.txt"
echo "native: $((SECONDS - start)) s"
echo "=== wasm32-wasip1 under wasmtime: $sets"
start=$SECONDS
"$wasmtime" run "${wasm_args[@]}" "$target/wasm32-wasip1/release/orb-wasm-check.wasm" \
    "${guest_args[@]}" --set "$sets" > "$tmp/wasm.txt"
echo "wasm: $((SECONDS - start)) s"

status=0
if ! diff "$tmp/native.txt" "$tmp/wasm.txt"; then
    echo "wasm-check: the wasm renders differ from the native ones (above: < native, > wasm)" >&2
    status=1
fi
if grep ' DIFF$' "$tmp/native.txt"; then
    echo "wasm-check: the songs above differ from the manifest" >&2
    status=1
fi
n=$(wc -l < "$tmp/native.txt" | tr -d ' ')
[ $status -eq 0 ] && echo "wasm-check: $n songs, wasm equal to native and to the manifest"
exit $status
