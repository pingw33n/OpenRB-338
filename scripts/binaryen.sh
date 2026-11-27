#!/usr/bin/env bash
# Prints the path of the pinned wasm-opt (binaryen, task-323; docs/building.md "The web page"),
# downloading it first when it is not there: a release asset, checked against its SHA-256, into
# target/binaryen-<version>/ (kept for the next run, as scripts/wasm-check.sh keeps wasmtime).
# scripts/build-web.sh runs it; the Pages workflow runs it to install the tool in its own step.
# Hosts: macOS and Linux, x86_64 or aarch64.
#
#   scripts/binaryen.sh
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
target=${CARGO_TARGET_DIR:-$root/target}
case $target in /*) ;; *) target=$root/$target ;; esac

# The pinned version: binaryen version_133. A new version is a deliberate edit of these lines
# with the checksums (the release assets' digests on GitHub).
version=version_133
case "$(uname -s)-$(uname -m)" in
    Darwin-arm64)
        arch=arm64-macos
        sha=ad66da82ac13f163e424b1643f16c6dfcccc98b5966296b43e52d3cab04f84a8
        ;;
    Darwin-x86_64)
        arch=x86_64-macos
        sha=13a9b90be775c6389ce3d1f879cb8627bea56708ba8c122983941d53a8199b95
        ;;
    Linux-x86_64)
        arch=x86_64-linux
        sha=2dc9c7813f5375db93d96ead4b78222fcc3e2677bbb832297af4797782a37489
        ;;
    Linux-aarch64)
        arch=aarch64-linux
        sha=89c07ea56faf38d0fbecf36ca8ec0721756716185f265b568e133d427f299bf8
        ;;
    *) echo "binaryen: macOS and Linux only (docs/building.md)" >&2; exit 1 ;;
esac

dir=$target/binaryen-$version
wasm_opt=$dir/binaryen-$version/bin/wasm-opt
if [ ! -x "$wasm_opt" ]; then
    tmp=$(mktemp -d)
    trap 'rm -rf "$tmp"' EXIT
    asset=binaryen-$version-$arch.tar.gz
    curl -fsSL --retry 3 -o "$tmp/$asset" \
        "https://github.com/WebAssembly/binaryen/releases/download/$version/$asset"
    if command -v sha256sum >/dev/null; then
        got=$(sha256sum "$tmp/$asset" | cut -d' ' -f1)
    else
        got=$(shasum -a 256 "$tmp/$asset" | cut -d' ' -f1)
    fi
    [ "$got" = "$sha" ] || { echo "checksum mismatch for $asset" >&2; exit 1; }
    mkdir -p "$dir"
    tar -xzf "$tmp/$asset" -C "$dir"
fi
echo "$wasm_opt"
