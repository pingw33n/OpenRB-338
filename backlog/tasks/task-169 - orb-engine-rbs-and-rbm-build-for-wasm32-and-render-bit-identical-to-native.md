---
id: TASK-169
title: orb-engine, rbs and rbm build for wasm32 and render bit-identical to native
status: Done
assignee:
  - '@agent-complex'
created_date: 2026-11-03 09:16
updated_date: '2026-11-27T07:58:00Z'
labels:
  - engine
  - tests
milestone: m-6
dependencies: []
references:
  - crates/orb-engine/Cargo.toml
  - crates/rbs
  - crates/rbm
  - tests/refs/manifest.json
  - .github/workflows/ci.yml
  - docs/building.md
priority: high
type: feature
ordinal: 185000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Why: m-6 puts OpenRB-338 in the browser (the user's goal of 2026-11-03: a web version on GitHub Pages that plays songs and mods from github.com/pingw33n/rb-songs and github.com/pingw33n/rb-mods or from local files). Everything above it (the web UI, the web audio output) needs the engine and the file-format crates on wasm32-unknown-unknown first, and the web version must sound like the native one: the same samples, sample for sample.

Context:
- orb-engine depends only on rbs and libm (pinned =0.2.16), so its transcendental math does not come from the platform; f64 arithmetic is IEEE on wasm too, and wasm has no FMA contraction. So bit identity is expected but unproven: check it, don't assume it. Watch for anything that differs on a 32-bit target (usize width, hashing, f32/f64 to int casts, `as` saturation is the same) and for the std float functions that on wasm32 come from compiler-builtins rather than the system libm (e.g. f64::sqrt is a wasm instruction and exact; powf/exp/sin through std are not the same code as on native: every such call must go through libm, as in native).
- rbm uses sha2 and toml; check both build for wasm32-unknown-unknown (no getrandom or C code pulled in).
- No std::fs, threads or time in these three crates' library code on wasm (tests may keep them behind cfg).

What to build:
- `cargo build --target wasm32-unknown-unknown -p orb-engine -p rbs -p rbm` succeeds (add the target to rust-toolchain.toml's targets).
- A check that wasm renders equal native: run the corpus renders (tests/refs/manifest.json; at least the diag and app sets with the built-in kit, and the shipped songs and mod songs when ORB_CD_ISO is set) compiled for wasm32-wasip1 under a wasm runtime (wasmtime as a cargo runner, or a small wasmtime-embedding test harness) and compare each render's SHA-256 with the native render's. Pick the simplest setup that runs in CI on Linux; document it in docs/building.md.
- A CI job (ci.yml) that builds the three crates for wasm32-unknown-unknown and runs the wasm-vs-native comparison on the songs that need no ORB_CD_ISO.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 cargo build --target wasm32-unknown-unknown -p orb-engine -p rbs -p rbm succeeds with the pinned toolchain; the target is listed in rust-toolchain.toml
- [x] #2 Every corpus song of the diag and app sets renders under wasm32 with the same SHA-256 as the native render (the command and the count in the notes); with ORB_CD_ISO set, the shipped and mod songs too
- [x] #3 Any engine change made for wasm is checked against the reference corpus (corpus_golden passes natively)
- [x] #4 ci.yml builds the three crates for wasm32-unknown-unknown and runs the wasm-vs-native comparison of the songs that need no external input
- [x] #5 docs/building.md describes the wasm build and the comparison; any wasm-specific fact about the numerics goes into the engine docs
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Add wasm32-unknown-unknown and wasm32-wasip1 to rust-toolchain.toml's targets; check cargo build --target wasm32-unknown-unknown -p orb-engine -p rbs -p rbm and cargo tree (no getrandom, no C code). Audit the three crates' library code for fs, threads, time and std float functions other than sqrt.
2. A small workspace bin crate crates/orb-wasm-check: reads tests/refs/manifest.json, renders the entries of the chosen sets (diag, app, shipped; mod and modapp with --mod-dir of recrypted mods) as corpus_golden and corpus_mods do, and prints one line per entry with the frames and the WAV's SHA-256. It builds natively and for wasm32-wasip1 (reads files through WASI). Unit tests for its parts.
3. scripts/wasm-check.sh: builds the harness natively and for wasm32-wasip1, fetches a pinned wasmtime release (checked by SHA-256, as validate-plugin.sh does with pluginval) into a temporary folder, runs both, and requires identical output (and every line equal to the manifest's wav_sha256). With ORB_CD_ISO set it recrypts the shipped mods (orb mod recrypt --iso) and adds the mod and modapp sets.
4. ci.yml: a wasm job on ubuntu-latest: build the three crates for wasm32-unknown-unknown and run scripts/wasm-check.sh.
5. docs/building.md: the wasm build and the comparison; any numerics fact into the engine docs.
6. Run it locally with and without ORB_CD_ISO; cargo test, clippy, fmt.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Branch: worktree-agent-adef8a910c60b7245

Research (2026-11-27):
- cargo build --target wasm32-unknown-unknown -p orb-engine -p rbs -p rbm built unchanged, no source change needed (also --release --locked).
- cargo tree --target wasm32-unknown-unknown -e normal,build -p rbm: libm, sha2 (digest, block-buffer, crypto-common, generic-array, typenum, cfg-if, version_check build), toml (serde_core, winnow 0.7/1.0, toml_parser, toml_writer, toml_datetime, serde_spanned). No getrandom, no cc/C code.
- Library code audit (grep, tests excluded): no std::fs, thread or time; the only std float call is f64::sqrt in tables/mod.rs:414 (exact). rbm's HashSet (pack.rs pick_path) and HashMap (recrypt.rs last_of) are only looked up/inserted, never iterated. libm 0.2.16 arch overrides: wasm32 ones need intrinsics_enabled (nightly), so stable uses generic code; x86/aarch64 overrides are sqrt/fma/rint only (correctly rounded).
- Harness: new bin crate crates/orb-wasm-check (renders manifest entries as corpus_golden/corpus_mods; prints SET/NAME FRAMES SHA256 equal|DIFF). scripts/wasm-check.sh builds it natively and for wasm32-wasip1, runs the wasm under a pinned wasmtime 51.0.0 (downloaded into target/wasmtime-v51.0.0, SHA-256 from the GitHub release digests), diffs the outputs.
- Dropped: cargo install wasmtime-cli (long build); the pinned download is simpler for CI and local.

Results:
- scripts/wasm-check.sh (ORB_CD_ISO unset): 164 entries (diag 145, app 11, shipped 8), native 2 s, wasm 9 s, outputs identical, all 164 equal to the manifest.
- ORB_CD_ISO=".../Rebirth 2.01 (2001 version).iso" scripts/wasm-check.sh: 175 entries (+ mod 8, modapp 3), native 3 s, wasm 15 s, identical, all 175 equal.
- actionlint .github/workflows/ci.yml and shellcheck scripts/wasm-check.sh: clean.

Final checks (2026-11-27):
- ORB_CD_ISO set: cargo test --workspace --locked: 1183 passed, 0 failed (corpus_golden diag/app/shipped and corpus_mods included).
- cargo clippy --all-targets -- -D warnings: clean; also clippy -D warnings for --target wasm32-wasip1 -p orb-wasm-check and --target wasm32-unknown-unknown -p orb-engine -p rbs -p rbm.
- cargo +nightly fmt --all --check: clean.
- No engine source change was needed for wasm: orb-engine, rbs and rbm are unchanged.
- The CI job wasm can only be verified by a run after the push.

Review round 1 (merge with small fixes): unwrapped the AGENTS.md crate-list line; shortened the orb-wasm-check Cargo.toml description to under 100 columns; split libm's arch overrides in docs/initial_investigation.md by target (x86_64 sqrt/fma, aarch64 sqrt/fma/rint, wasm32 ceil/floor/trunc/rint/fabs/sqrt only with nightly intrinsics_enabled, so none on stable wasm32); commit subject renamed to tests,tools (no engine source changed). orb-wasm-check tests pass.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
orb-engine, rbs and rbm build for wasm32-unknown-unknown on the pinned 1.99.1 with no source change (pure-Rust deps only: libm, sha2, toml; no getrandom, no C), and the wasm engine renders the whole corpus bit-identical to native.

- rust-toolchain.toml lists wasm32-unknown-unknown and wasm32-wasip1.
- New bin crate crates/orb-wasm-check renders manifest entries as the golden tests do and prints SET/NAME FRAMES SHA256 equal|DIFF; scripts/wasm-check.sh runs it natively and as wasm32-wasip1 under a pinned wasmtime 51.0.0 (downloaded and checked by SHA-256 into target/) and requires identical output, all equal to the manifest. With ORB_CD_ISO it recrypts the shipped mods and adds mod and modapp.
- Result: 164 songs (diag, app, shipped) identical, native 2 s, wasm 9 s; with ORB_CD_ISO 175 songs identical, native 3 s, wasm 15 s.
- ci.yml job wasm (ubuntu-latest): the wasm32-unknown-unknown build and scripts/wasm-check.sh. Not run yet: verify on the first CI run after the push.
- Docs: docs/building.md "The wasm build" (and the CI and scripts lists), docs/initial_investigation.md numerics note [C] task-169, AGENTS.md crate list.
- Tests: cargo test --workspace (ORB_CD_ISO set) 1183 pass; clippy, fmt, actionlint, shellcheck clean.
Branch worktree-agent-adef8a910c60b7245, one commit.
<!-- SECTION:FINAL_SUMMARY:END -->
