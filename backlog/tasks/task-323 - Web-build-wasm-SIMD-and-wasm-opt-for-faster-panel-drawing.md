---
id: TASK-323
title: 'Web build: wasm SIMD and wasm-opt for faster panel drawing'
status: To Do
assignee:
  - '@agent-worker'
created_date: '2026-11-27T14:51:00Z'
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
- [ ] #1 The page's wasm is built with simd128 and optimized by a pinned wasm-opt, by scripts/build-web.sh and in pages.yml
- [ ] #2 Before/after numbers (first modern drawing time, wasm size raw and gzip) are in the notes and docs/web.md
- [ ] #3 Native builds and the worklet are unchanged, or scripts/wasm-check.sh passes with the change
- [ ] #4 docs/building.md lists the new tool and its pinned version
<!-- AC:END -->
