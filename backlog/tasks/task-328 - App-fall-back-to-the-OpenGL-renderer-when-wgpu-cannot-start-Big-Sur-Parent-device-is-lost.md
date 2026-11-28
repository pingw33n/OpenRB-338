---
id: TASK-328
title: >-
  App: fall back to the OpenGL renderer when wgpu cannot start (Big Sur: Parent
  device is lost)
status: To Do
assignee:
  - '@agent-complex'
created_date: '2026-11-28T11:45:00Z'
labels:
  - ui
milestone: m-5
dependencies: []
references:
  - crates/orb-ui/src/main.rs
  - crates/orb-ui/Cargo.toml
  - docs/building.md
priority: high
type: bug
ordinal: 299700
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Why: a user on r/Rebirth338 (shown to the supervisor by the user on 2026-11-28) tried release v2026.1126 on a MacBook with macOS 11 Big Sur. The app shows only the start error box, "OpenRB-338 could not open its window: WGPU error: Parent device is lost", and quits. Big Sur is the oldest macOS the app supports (scripts/build.sh:130, MACOSX_DEPLOYMENT_TARGET=11.0), so this Mac should work. The Mac model and GPU are not known yet; the Macs that run Big Sur go back to 2013 (Intel HD/Iris and older AMD/NVIDIA GPUs).

Context:
- The window is eframe 0.37 with egui-wgpu (wgpu 30); crates/orb-ui/src/main.rs: `wgpu_setup()` / `wgpu_backends()` (WGPU_BACKEND, DX12 only on Windows) and `run_native` (about line 308). On a start error, `start_error_text` / `report_start_error` show the box and end the process.
- The WGPU_BACKEND hint is added only for RequestAdapterError and CreateSurfaceError, not for this error. On macOS wgpu has no usable GL backend without ANGLE, so WGPU_BACKEND=gl is no workaround there.
- egui_glow is already in Cargo.lock under eframe. eframe has a `glow` feature and `NativeOptions::renderer = Renderer::Glow` (OpenGL through glutin), which runs on old Macs where wgpu's Metal path fails.

What to do:
- Enable eframe's `glow` renderer alongside wgpu for the standalone app (not the plugin, not the web build, unless it is free there).
- When run_native fails with a wgpu error at startup (any eframe::Error::Wgpu, including the device-lost case), run it again with Renderer::Glow before showing the error box. Show the box only when both fail, with both errors in the text.
- An override to force a renderer for testing and support (e.g. ORB_RENDERER=glow|wgpu; record it in docs/building.md's table of ORB_* variables), and ORB_GPU_INFO=1 also prints which renderer is used.
- Check that everything the app draws works the same under glow: the panel art and its textures (the modern skin's one-time drawing, the classic skin), the --screenshot path, text, the window icon, and HiDPI. Note any differences.
- Make the fallback testable without old hardware: e.g. a way to make the wgpu attempt fail on purpose (a test-only env var or WGPU_BACKEND set to a backend the machine lacks) so the glow path can be run on this Mac, and a unit test of the decision (which errors retry, the combined error text).
- Measure startup and frame time under glow against wgpu on this Mac and record them.
- Docs: docs/building.md (the renderer, the fallback, the variable) and the user-facing troubleshooting text if there is one.
- After the merge: a CI release or a by-hand build for the reporter is the user's call. The user may ask the reporter for the Mac model and GPU; record the answer in the notes if it comes.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 On this Mac, with the wgpu attempt made to fail on purpose, the app opens its window with the glow renderer and plays a song; without it the app still uses wgpu
- [ ] #2 A unit test covers which start errors retry with glow and the error text when both fail
- [ ] #3 The panel (both skins), text, icon and --screenshot look the same under glow as under wgpu, or the differences are recorded; startup and frame times of both are recorded
- [ ] #4 The renderer override and ORB_GPU_INFO's renderer line are documented in docs/building.md
- [ ] #5 cargo test --workspace and clippy pass; the plugin and the web build are unchanged
<!-- AC:END -->
