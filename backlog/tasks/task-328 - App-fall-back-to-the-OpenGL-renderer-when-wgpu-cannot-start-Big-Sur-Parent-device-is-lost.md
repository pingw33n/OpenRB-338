---
id: TASK-328
title: >-
  App: fall back to the OpenGL renderer when wgpu cannot start (Big Sur: Parent
  device is lost)
status: Done
assignee:
  - '@agent-complex'
created_date: '2026-11-28T11:45:00Z'
updated_date: '2026-11-28T17:06:00Z'
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
- [x] #1 On this Mac, with the wgpu attempt made to fail on purpose, the app opens its window with the glow renderer and plays a song; without it the app still uses wgpu
- [x] #2 A unit test covers which start errors retry with glow and the error text when both fail
- [x] #3 The panel (both skins), text, icon and --screenshot look the same under glow as under wgpu, or the differences are recorded; startup and frame times of both are recorded
- [x] #4 The renderer override and ORB_GPU_INFO's renderer line are documented in docs/building.md
- [x] #5 cargo test --workspace and clippy pass; the plugin and the web build are unchanged
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Cargo: eframe gets the glow feature in orb-ui's not-wasm32 target table only (the web build and the plugin, which takes orb-ui without the app feature, stay unchanged; egui_glow, glow, glutin are already in Cargo.lock).
2. main.rs: the state the app creator moves into the App (panel, link, controller, settings, start errors, notes, options) goes into one struct held in an Rc<RefCell<Option<_>>>; the creator takes it. A start is a list of renderers: ORB_RENDERER unset = wgpu then glow; wgpu or glow = that one only; anything else = a warning and the default.
3. After run_native fails: retry with the next renderer only when the error is eframe::Error::Wgpu and the app was never created (the state is still in the cell; eframe calls the creator after the painter is up, so a painter error leaves it). eframe reuses its thread-local event loop (run_and_return is the default), so a second run_native in the same process is supported.
4. When every renderer fails: one box with each renderer's error (the WGPU_BACKEND hint stays on the wgpu part, plus a line on ORB_RENDERER).
5. ORB_GPU_INFO=1 also prints "renderer: wgpu|glow" and, under glow, the GL vendor/renderer/version ("gl: ..."); the existing "gpu: " line of appimage-smoke.sh stays.
6. Unit tests: which errors retry (Wgpu before the app vs other errors, after the app), the plan from ORB_RENDERER, the combined text.
7. Check on this Mac: WGPU_BACKEND=vulkan (no Vulkan adapter on macOS) makes wgpu fail -> the window opens under glow; --screenshot of both skins under both renderers compared; ORB_UI_START_STATS and ORB_UI_FRAME_STATS under both; a song plays (--play).
8. Docs: docs/building.md (the renderer, the fallback, ORB_RENDERER in the ORB_* table, ORB_GPU_INFO's new lines).
9. cargo test --workspace, clippy (also the Windows target via cargo-xwin if available), the wasm clippy of orb-web, orb-plugin builds; nightly fmt.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Branch: worktree-agent-a27de0ac0a1ae2f33 (worktree .claude/worktrees/agent-a27de0ac0a1ae2f33), based on main 9115afa.

The reporter's Mac (from the user, 2026-11-28): a "Mac Book Air"; more detail is unlikely. [G] The MacBook Airs whose last macOS is Big Sur are the 2013 and 2014 models (2015 and later run Monterey), so likely a 2013/2014 Air with Haswell Intel HD Graphics 5000 (Metal GPU family Mac1); the owner may also just not have updated a newer Air.

Where the error comes from: "Parent device is lost" is wgpu-core 30.0.2 DeviceError::Lost (src/device/mod.rs:345), here inside WgpuError::RequestDeviceError (egui-wgpu 0.37 lib.rs:57; eframe's Display adds "WGPU error: "), so request_device failed in the painter's set_window, before eframe calls the app creator (eframe 0.37 native/wgpu_integration.rs:272 set_window, :325 app_creator). DeviceError::from_hal (device/mod.rs:367) maps hal DeviceError::Lost AND hal DeviceError::Unexpected to Lost, so any unexpected Metal failure while the device is created shows this text. wgpu-hal 30.0.2's Metal adapter (src/metal/adapter.rs) has no Haswell, HD 5000 or Mac1 workaround (the only Intel case is the timestamp period, adapter.rs:100); the crate ships no changelog, so no known-issue entry was found in ~/.cargo/registry. It does not change the fix.

OpenGL on that Mac (4.1 core): glutin 0.32.3's CGL config (src/api/cgl/config.rs:96-118) asks for NSOpenGLProfileVersion4_1Core, then 3_2Core, then Legacy; eframe 0.37 tries glutin's default attributes, then GLES, then a compatibility profile (native/glow_integration.rs:1862-1885); egui_glow 0.37 reads GL_SHADING_LANGUAGE_VERSION and uses "#version 140" for GLSL 1.40 or newer (shader_version.rs), which a 3.2/4.1 core profile accepts. Apple Silicon runs the same CGL 4.1 core path (Metal-backed), so the run on this Mac exercises the same context request and shaders.

Event loop reuse: eframe keeps a thread-local winit EventLoop (native/run.rs:67 with_event_loop) when NativeOptions::run_and_return is true (the default, epi.rs:477), so a second run_native in the same process is supported; the failed wgpu window is a local dropped with the error (wgpu_integration.rs:482-488).

Implementation (crates/orb-ui/src/main.rs): the app creator closure now sits in an Rc<RefCell<Option<eframe::AppCreator>>>; each attempt's creator takes it. The loop runs run_native per renderer of `renderers(ORB_RENDERER)` (default DEFAULT_RENDERERS = [Wgpu, Glow]); after an error it retries with the next only when `retries(e, app_made)`: an eframe::Error::Wgpu and the cell still full. When all fail, report_start_error(&failed) shows one box with each error (start_error_text). native_options(renderer, size, sized) builds the options per attempt (NativeOptions is not Clone). ORB_GPU_INFO=1 prints "renderer: wgpu|glow" then "gpu: ..." (unchanged, appimage-smoke.sh greps it) or "gl: VENDOR, RENDERER, VERSION". Cargo: eframe features ["glow"] added in orb-ui's cfg(not(wasm32)) table only; Cargo.lock gains one line (eframe -> glow; egui_glow, glutin, glutin-winit were already locked).

Decision (supervisor asked to record it): wgpu stays first on every desktop OS (macOS, Windows, Linux) and glow is the fallback on all of them; no reason found to exclude one: glutin loads its GL libraries at run time (WGL opengl32 on Windows, GLX/EGL via dlopen on Linux), so no new link-time library. On Windows a machine without DX12 now gets an OpenGL window instead of the WGPU_BACKEND box; on Linux "no Vulkan ICD and no libEGL" now tries GLX.

Failed attempt: constructing a glutin error in the test failed (eframe does not re-export glutin); the test uses eframe::Error::NoGlutinConfigs(Default::default(), ...) instead. The device-lost error is built as wgpu::wgc::instance::RequestDeviceError::Device(wgc::device::DeviceError::Lost) and its text is asserted equal to "WGPU error: Parent device is lost", the reported text.

Fallback run on this Mac (M1 Pro, macOS 15): WGPU_BACKEND=vulkan makes wgpu fail with CreateSurfaceError ("Failed to create surface for any enabled backend: {}"), not RequestAdapterError; stderr: "OpenRB-338: the wgpu renderer could not open the window (WGPU error: Failed to create surface for any enabled backend: {}); trying OpenGL (glow)", then "renderer: glow", "gl: Apple, Apple M1 Pro, 4.1 Metal - 89.4". Without it: "renderer: wgpu", "gpu: Metal IntegratedGpu Apple M1 Pro". ORB_RENDERER=metal: warning, then wgpu. ORB_RENDERER=wgpu WGPU_BACKEND=vulkan: exit 1 with the single-error text and the WGPU_BACKEND hint.
Script: scratchpad wt/shots.sh (3 runs x 2 skins x {wgpu, ORB_RENDERER=glow, WGPU_BACKEND=vulkan}, --demo --size 624x510 --ppp 2 --screenshot, ORB_LOGO_T=10, ORB_UI_CONFIG with theme modern/classic, ORB_UI_START_STATS=1) and wt/frames.sh (Calm.rbs --play 16 s, window brought to front with osascript, ORB_UI_FRAME_STATS=1).

Screenshots, glow vs wgpu (1248x1064 = 1,327,872 pixels): modern 60,006 pixels differ, max 2/255 (none > 2), on the antialiased edges of the parts (blending rounding); classic 497 pixels, max 1. The fallback's screenshots equal the forced-glow ones' numbers; two wgpu runs are identical. Text (menu bar), HiDPI (ppp 2) the same. The window icon comes from the ViewportBuilder (winit), not the renderer; on macOS the Dock shows the bundle icon either way.
Start (first complete frame, ms, 3 runs): modern wgpu 315/238/224, glow forced 456/231/255, fallback 282/276/261; classic wgpu 126/124/113, glow forced 138/150/141, fallback 174/147/139. The failed wgpu attempt costs little (the fallback is not slower than forced glow).
Frames while playing Calm.rbs (29.2 frames/s under both; first 2 s window left out): modern wgpu 1.08-1.19 ms CPU/frame, glow 1.37-1.45; classic wgpu 0.76-0.85, glow 0.71-0.79. Under glow without --play the same song idles at 1.0 frames/s, so the 29 frames/s are the playing panel: the song plays under the fallback ("audio on CoreAudio MacBook Pro Speakers (44100 Hz ...)").

Checks: cargo test --workspace 1226 passed, 0 failed (orb-ui bin: 11, among them a_wgpu_error_before_the_app_retries_with_glow, both_errors_are_in_the_text_when_both_fail, orb_renderer_picks_one_or_the_default_order, the_device_lost_text_is_the_reported_one, only_a_no_adapter_error_names_wgpu_backend). cargo clippy --all-targets -- -D warnings clean (orb-plugin checked in it). cargo +nightly fmt --all --check clean. cargo clippy -p orb-web --target wasm32-unknown-unknown --all-targets -- -D warnings clean; on wasm32 eframe's features are the defaults without glow (cargo tree -e features -i eframe), the one glow crate in the web tree is wgpu-hal's WebGL path, as before. cargo tree -p orb-plugin: no eframe, egui_glow or glutin. Windows: CL=-Wno-incompatible-pointer-types VCINSTALLDIR=/xwin cargo xwin clippy -p orb-ui --target x86_64-pc-windows-msvc --all-targets -- -D warnings: clean (without CL the vorbis C build fails under clang-cl, as scripts/build.sh:115 says). Linux not built: the change adds no cfg-gated code, and eframe's default x11/wayland features already turn on glutin's x11, glx and wayland features; glutin loads libGL/libEGL at run time.

2026-11-28, after the merge: the user sent the reporter (r/Rebirth338, MacBook Air on macOS 11 Big Sur) the by-hand x86_64 build OpenRB-338-2026.1128.1223-x86_64-apple-darwin.dmg (release.yml workflow_dispatch run 46144405381 on main at 937711d, SHA-256 82542cef892d7c37e33d9ab33716fd0d5b14412f01a00bad1a4847304a20593b). The reporter says the app now opens: the "WGPU error: Parent device is lost" start error is fixed.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
The standalone app (OpenRB-338) now falls back to eframe's OpenGL renderer (glow) when wgpu cannot start, on every desktop OS, wgpu first. Branch worktree-agent-a27de0ac0a1ae2f33, one commit "ui: fall back to the OpenGL renderer when wgpu cannot start".

- crates/orb-ui/src/main.rs: the app creator is shared between attempts; run_native runs per renderer; a wgpu error before the app was made retries with glow; when both fail one box lists both errors. ORB_RENDERER=wgpu|glow tries one; ORB_GPU_INFO=1 prints "renderer: ..." and "gl: ..." under glow (the "gpu: ..." line kept). crates/orb-ui/Cargo.toml: eframe "glow" feature for non-wasm32 only.
- On this Mac (M1 Pro, macOS 15) with WGPU_BACKEND=vulkan the window opens under glow and Calm.rbs plays (29.2 frames/s playing, 1 frame/s idle); without it wgpu (Metal) draws.
- Screenshots glow vs wgpu: modern skin 60,006 of 1,327,872 pixels differ by at most 2/255 (antialiased edges), classic skin 497 pixels by 1. First frame: modern wgpu 224-315 ms, glow 231-456 ms (fallback 261-282); classic wgpu 113-126, glow 138-174. CPU per frame playing: modern 1.08-1.19 ms wgpu vs 1.37-1.45 glow; classic 0.76-0.85 vs 0.71-0.79.
- The reporter's Mac is a MacBook Air (likely 2013/2014 with HD Graphics 5000 [G]); "Parent device is lost" is wgpu-core DeviceError::Lost from request_device, which also stands for an unexpected Metal error.
- docs/building.md: new section "The window's renderer (task-328)", ORB_RENDERER and ORB_GPU_INFO in the variables table, the Windows and Linux paragraphs updated.
- Tests: cargo test --workspace 1226 pass; clippy clean (macOS all targets; Windows x86_64 orb-ui via cargo-xwin; orb-web wasm32); nightly fmt clean. The plugin links no eframe; the web build has no glow feature.
After the merge: a release or a build for the reporter is the user's call (with ORB_GPU_INFO=1 the reporter's stderr would show which renderer ran).
<!-- SECTION:FINAL_SUMMARY:END -->
