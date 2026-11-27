---
id: TASK-323
title: 'Web build: wasm SIMD and wasm-opt for faster panel drawing'
status: Done
assignee:
  - '@agent-worker'
created_date: '2026-11-27T14:51:00Z'
updated_date: '2026-11-27T15:14:00Z'
labels:
  - ui
  - tools
milestone: m-6
dependencies: []
references:
  - scripts/build-web.sh
  - docs/building.md
  - docs/web.md
priority: medium
type: enhancement
ordinal: 294700
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Why: on the web page the modern skin's first drawing (resvg rasterizing the SVG panel parts in wasm) takes about 3.2 s in Chrome (task-172 notes, docs/web.md 7.3), behind the "Drawing the panel... N%" screen. scripts/build-web.sh builds with the release profile (opt-level 3, lto fat) but without wasm SIMD and without wasm-opt. tiny-skia (under resvg) has SIMD paths, so the user asked (2026-11-27) for this cheap speed-up first; drawing in the background, caching drawn parts and browser-native SVG are not part of this task.

What to do:
- Build the web page's wasm with `-C target-feature=+simd128` (only for the web build's wasm32 target, e.g. in build-web.sh's RUSTFLAGS or a target-specific config, so native builds and orb-worklet are unaffected unless measured to help there too; say what you chose).
- Run wasm-opt (binaryen, pinned version, installed per user and checked by version like wasm-bindgen-cli) with -O3 or -O4 on the page's wasm; keep the name section if the panic stacks need it (task-172 kept it) or say what changes.
- Measure before and after in Chrome on the same machine: the modern skin's first drawing time (the loading screen's progress from 0 to done), the classic start, and the wasm size raw and gzip. Record a table in the notes and in docs/web.md.
- The engine output is unaffected (orb-worklet is a separate module); confirm the worklet build is unchanged or, if it is changed too, that the wasm-vs-native corpus check (scripts/wasm-check.sh) still passes.
- docs/building.md: the new tool and flags. The Pages workflow (pages.yml) must install the pinned wasm-opt too.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 The page's wasm is built with simd128 and optimized by a pinned wasm-opt, by scripts/build-web.sh and in pages.yml
- [x] #2 Before/after numbers (first modern drawing time, wasm size raw and gzip) are in the notes and docs/web.md
- [x] #3 Native builds and the worklet are unchanged, or scripts/wasm-check.sh passes with the change
- [x] #4 docs/building.md lists the new tool and its pinned version
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Baseline: build scripts/build-web.sh unchanged, copy dist/web to scratchpad (before), measure size raw/gzip and the first Modern drawing in Chrome (3 runs, median, same window).
2. build-web.sh: RUSTFLAGS with +simd128 for the orb-web cargo build only (worklet unchanged; --target-specific flags not used, since both builds are wasm32); download pinned binaryen version_133 into target/ with SHA-256 check (as wasm-check.sh does for wasmtime), run wasm-opt -O3 -g (keep names) on the wasm-bindgen output.
3. pages.yml: install the same pinned binaryen (same script path, a shared helper).
4. Build after, measure again, 3 runs; check panic names kept; docs/web.md table, docs/building.md.
5. Tests: cargo test --workspace, clippy native + wasm32, nightly fmt, actionlint, shellcheck.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Branch: worktree-agent-af463a8cb2af1e22e

Evidence: baseline build (no flags, no wasm-opt) vs after (+simd128 on the orb-web cargo build only, wasm-opt version_133 -O3 -g --enable-simd). Measured in one Chrome 157 tab, same window, localhost servers on ports 8351 (before) and 8352 (after), page started Classic via localStorage theme, then View > Modern; time from the loading screen's first update to its hiding. Before 3334, 3363, 3357 ms (median 3357). After 2167, 2129, 2111 ms (median 2129), plus one 5278 ms first run on the fresh origin, not repeated (median of all four 2148). Start with Modern (navigation to hidden): before 3355, 3417; after 2394, 2473, 2748. wasm raw 19,121,175 -> 16,883,373; gzip -9 9,222,400 -> 8,967,038. Worklet wasm identical (cmp) so wasm-check.sh not rerun. Name section kept by -g: 16105 -> 9178 named functions (inlined ones gone), 2.47 -> 1.33 MB. Not done: SIMD and wasm-opt were not measured apart; classic start not timed. Tests: cargo test --workspace 1211 passed, clippy native and wasm32 orb-web, nightly fmt, actionlint, shellcheck clean. Servers stopped, tab closed.

Review round 1: moved web.md 7.5 after 7.4, rewrapped the build-web.sh comment to 100 columns, replaced the inlined-functions sentence with the measured counts (16,105 unoptimized, 9,178 with -g); subject stays tools:. shellcheck clean.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
The page's wasm is built with simd128 (orb-web cargo build only) and optimized by pinned binaryen version_133 (scripts/binaryen.sh, SHA-256 checked; build-web.sh and a pages.yml step). Modern first drawing 3357 ms -> 2129 ms median in Chrome; wasm 19.12 MB -> 16.88 MB raw, 9.22 -> 8.97 MB gzip. Worklet wasm byte-identical. Names kept with -g (9178 of 16105 functions remain, the rest inlined). Docs: web.md 7.5, building.md. cargo test --workspace 1211 pass.
<!-- SECTION:FINAL_SUMMARY:END -->
