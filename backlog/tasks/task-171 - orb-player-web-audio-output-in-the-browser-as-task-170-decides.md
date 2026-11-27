---
id: TASK-171
title: 'orb-player: web audio output in the browser, as task-170 decides'
status: Done
assignee:
  - '@agent-complex'
created_date: 2026-11-03 09:16
updated_date: '2026-11-27T08:53:00Z'
labels:
  - engine
  - ui
milestone: m-6
dependencies:
  - TASK-169
  - TASK-170
references:
  - crates/orb-player/src/lib.rs
  - crates/orb-player/src/output.rs
  - crates/orb-player/tests
priority: high
type: feature
ordinal: 187000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Why: the web version (m-6) plays songs live in the browser. orb-player is the live player under the panel (the engine in cpal's callback, rtrb queues for the UI's commands, a triple buffer of the engine's state back to the UI). task-170 is the user's decision on how this works on wasm32 (audio path A cpal webaudio / B threads + SharedArrayBuffer / C engine in an AudioWorklet with postMessage, and the sample rate); read its final summary first and build what it chose.

What to build:
- orb-player compiles for wasm32-unknown-unknown with the same public API the UI uses natively (Player, its commands, the state snapshot), the web output behind cfg(target_arch = "wasm32"); the native output (output.rs, cpal, rubato) is unchanged.
- The audio starts on the first user gesture (browsers keep an AudioContext suspended until one) and survives the tab being hidden and shown.
- The samples the engine produces reach the AudioContext unchanged at 44.1 kHz (x / 32768 as f32, as natively), unless task-170 decided otherwise.
- A minimal test page or wasm-bindgen-test that plays a corpus song with the built-in kit, used to check the output by ear and in the notes (dropouts under a busy main thread, latency of a knob change).

Not in scope: the UI's web build (its own task) and loading songs from the collections.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 orb-player builds for wasm32-unknown-unknown; the native build, its tests and clippy are unchanged
- [x] #2 The web output follows task-170's decision (the decision named in the commit's Decision: line)
- [x] #3 In Chrome and Firefox a corpus song plays at 44.1 kHz without dropouts while the main thread is idle, and the notes record how it behaves under a busy main thread and the latency of a command
- [x] #4 Playback starts after a user gesture and resumes after the tab is hidden and shown again
- [x] #5 docs/building.md (or a new docs/web.md) describes the web audio path
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Path C of task-170: the engine in an AudioWorklet as its own wasm instance, postMessage only.

1. Protocol, all targets, natively tested (crates/orb-player/src/wire.rs): a small binary encoding of every Command, of Snapshot and of Tracks. A Command that carries a box the other instance cannot share is sent as what rebuilds it: Load as its recipe (the song's load image, form + globals + device blocks + tracks + batches, or a new song) plus the four channel pattern arrays at send time (the UI's template edits); Mod and Preview as a kit id (the SampleSet is sent once as its 46 samples' audio); InitSong/InitLoop as a fresh Pool made in the worklet.
2. orb-engine, behavior unchanged: song::LoadSource (what Engine::load reads) implemented for Song and for the wire's load image, Globals::from_payload, Pool::from_lists (to rebuild Tracks on the UI side). Corpus golden test must stay equal.
3. orb_player::remote::Player: the UI half on the web with the Port methods of the native Player (send, snapshot, tracks, collect_garbage, build, build_new, set_mod, load, startup_sound), over a Transport trait (post/poll of byte messages). orb_player::worklet::Host: the worklet half, owning a native Player + Audio, decoding messages into Commands, rendering planar f32 (x / 32768 as natively) at 44.1 kHz, or through the existing rubato resampler (moved from output.rs into resample.rs unchanged) at a device rate if the browser refuses 44100, and posting snapshots per engine call and tracks when they change.
4. Native test: remote::Player + Host in loopback against a native Player + Audio on corpus songs with loads, play, tempo, pattern edits, mod change, preview, record: identical samples, snapshots and tracks.
5. wasm32: orb-player builds for wasm32-unknown-unknown (cpal, devices, control, output native only). Feature web (wasm32 only): orb_player::web, wasm-bindgen/web-sys glue: AudioContext at 44100 (fallback to the default rate), the processor script (embedded JS, Blob URL), AudioWorkletNode, port messages to the Transport, resume on the first gesture and when the tab is shown again.
6. crates/orb-worklet: a cdylib with a C ABI (no wasm-bindgen, no imports) around worklet::Host, instantiated in the worklet from bytes sent in processorOptions.
7. crates/orb-web-demo + scripts/web-demo.sh: a minimal test page (built-in kit, a shipped corpus song, Play/Stop, tempo, a busy-main-thread button, latency and rate readouts). wasm-bindgen-cli pinned to the lockfile's wasm-bindgen, documented in docs/building.md.
8. Check in Chrome (Claude in Chrome, own tab): 44.1 kHz, playback, gesture start, hide/show, latency of a command, behavior under a busy main thread; Firefox and listening left to the user.
9. docs/web.md on the web audio path; docs/building.md for the build; tests, clippy (native and wasm32), fmt.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Branch: task-171-web-audio (worktree .claude/worktrees/agent-a3b4990565aab7765), based on main 432fbf0.

Design and evidence (2026-11-27):
- Command::Load carries a Box<Engine> that cannot cross wasm instances. Engine::load reads only form, globals, the device blocks of the form, tracks and batches (crates/orb-engine/src/frame/mod.rs, Engine::load), so the wire sends those (wire::LoadImage) and the worklet loads them through the same Engine::load (new trait song::LoadSource, implemented by Song and LoadImage; Engine::load now takes &impl LoadSource, body unchanged). remote::Player keeps the recipe of every engine it built (by address, checked by the per-track event counts) and sends with it the four pattern arrays at send time; the worklet rewrites the patterns that differ with Engine::pattern (the UI's new-song template is exactly such edits, crates/orb-ui/src/template.rs).
- Kits go once as their 46 samples' audio (Sample::from_audio pads as from_aiff does), then by id; Preview by kit+slot when the sample is a kit's (Arc::ptr_eq), else as audio; InitSong/InitLoop with a fresh Pool made in the worklet.
- Tracks come back as lists; orb-engine got Pool::from_lists (Pool has no public builder). Globals::from_payload reads msg 36 back.
- LiveBlock::set now zeroes the bytes past the block: the derived PartialEq compared stale bytes past len, so a snapshot rebuilt from the block alone compared unequal (found by the loopback test, pending_song_switch: live[3] len 0 with old 808 bytes). as_slice is unchanged.
- The rubato Resample moved unchanged from output.rs to resample.rs (output.rs uses it from there) for the worklet's fallback at a rate other than 44100.
- Failed hypothesis: building orb-worklet and orb-web-demo in one cargo invocation would unify orb-player's feature web into the worklet module; scripts/web-demo.sh builds them separately. Result: orb_worklet.wasm 321 KB, WebAssembly.Module.imports = [] (node), exports memory + the 8 orb_* functions.
- wasm-bindgen-cli 0.2.129 (the lockfile's wasm-bindgen) installed per user with cargo install --locked.

Tests: crates/orb-player/tests/web_loopback.rs (7 tests): remote::Player + worklet::Host in a loopback against the native Player, render in 128-frame quanta, bit-equal f32 samples, equal snapshots and tracks after: a shipped song played, tempo, knobs, pattern edit, mixer, a mod change to a new kit, the startup sound, recording with a merge, a new song with edited patterns, InitSong and Undo; every one of the 164 diag/app/shipped corpus songs loaded from its image for 350 quanta; a pending-song switch; unapplied() until the call; a foreign engine panics; wrong lengths refused; resampling at 48000. wire.rs unit tests (round trips, malformed input). orb-worklet unit test of the C ABI.

Chrome 157 (macOS, Claude in Chrome, own tab, scripts/web-demo.sh, http://localhost:8338/):
- Before any gesture: context Suspended, sampleRate 44100, worklet Ready at 44100, frames 0. A click on Play: context running, Fonda Workout plays (frames 243712 and bar 4 after about 5.5 s).
- AudioContext.playbackStats over 55.5 s of playing, through every test below: underrunEvents 0, underrunDuration 0; averageLatency 30.6 ms, outputLatency 32 ms, baseLatency 5.8 ms. Processor: longest gap between process() calls 6 ms.
- Busy main thread: a 3000 ms spin: audio went on (frames 743424 -> 923648, 4.09 s of audio over the spin plus 1.1 s), 0 underruns; 40 ms spins every 50 ms for 5 s: 0 underruns. The UI gets no snapshots during a spin (they queue and arrive after it); commands cannot be sent while the main thread is blocked.
- Latency of a command (tempo, from send until a snapshot shows it applied, polled every 2 ms): 20 changes, min 3.2, mean 13.4, max 25.3 ms. That is the wait for the next 1024-frame engine call (up to 23.2 ms) plus the message ways; the audible change comes about outputLatency (32 ms) later.
- Hide/show: the tools cannot hide the tab (visibilityState stayed visible with another tab in front). Checked the resume path instead: ctx.suspend() then a visibilitychange event: the context ran again and playback went on from where it was (frames 1783808 -> 1872896 in 2 s, playing). Chrome itself does not suspend a hidden tab's context; the real hide/show is left to the user.
- Opening another song while playing (Calm): building it on the UI side took 1.0 ms; generation 2, playing, no errors.
- Firefox and listening: not checked by me; left to the user.

Checks before the commit: cargo test --workspace --no-fail-fast 1197 passed, 0 failed, 6 ignored; cargo clippy --all-targets -D warnings clean; clippy for wasm32-unknown-unknown of orb-player + orb-worklet, and of orb-player --features web + orb-web-demo, clean; cargo +nightly fmt --all --check clean. Corpus: orb compare --manifest tests/refs/manifest.json --set diag,app,shipped: 164 of 164 equal (orb-engine change: Engine::load generic over LoadSource, body unchanged). The no_alloc test's lock scan (crates/orb-player/tests/no_alloc.rs) rejected a Mutex in remote.rs: the recipes are in a RefCell instead (remote::Player stays Send).
Firefox, listening, and a real hide/show of the tab wait for the user: scripts/web-demo.sh --serve, then http://localhost:8338/ (click Play; Busy and Tempo latency x20 buttons log their results on the page).

User check, 2026-11-27 (relayed by the supervisor): the test page "plays fine in brave, firefox, safari" and "plays minimized and with tab inactive". With the agent's Chrome 157 check this covers criterion 3 (Chrome, Firefox, plus Brave and Safari) and criterion 4 (the start on a gesture checked in Chrome; playback goes on with the window minimized and the tab inactive).

Review round 1 (send back), fixed:
1. orb-worklet: the panic message path was dead (with() held STATE.borrow_mut() through the call, so the hook's try_borrow_mut failed and orb_panic_len stayed 0). The message now lives in its own thread-local cell PANIC, which no call holds; orb_panic_len/ptr read it. New test a_panic_in_a_call_keeps_its_message panics inside with() (the state borrowed, as in a trapping call) and reads the message back through orb_panic_len/orb_panic_ptr.
2. docs/web.md section 3, Failures: says what the hook and the script do now.
3. worklet::Host::report had no caller: removed (the script reports the trap itself).
4. Commit subject now player,engine: (the engine API LoadSource, Globals::from_payload, Pool::from_lists, Engine::build_id).
5. remote::Player matched a recipe by engine address + event counts, so an engine not built by the player at a freed address could take a stale recipe. Now each build gets a process-unique id (AtomicU64 counter) stored in the recipe and in the new field Engine::build_id (orb-engine, 0 by default, never read by the engine); send(Load) looks the recipe up by that id, and an engine with id 0 or an unknown id panics (foreign_engine_panics).
Checks: cargo test --workspace --no-fail-fast 1198 passed, 0 failed, 6 ignored (the golden corpus test included); clippy native and wasm32 (orb-player, orb-worklet; orb-player --features web, orb-web-demo) clean; nightly fmt --check clean; scripts/web-demo.sh builds.

Review round 2: docs/web.md section 2 (Load) still said the recipe is keyed by the engine's address; it now says it is keyed by the process-unique build id the engine carries (Engine::build_id). Doc-only, no cargo run.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Web audio output as task-170 decided (path C), on branch task-171-web-audio, one commit. All criteria are met. Criteria 3 and 4 rest on the agent's Chrome 157 check and the user's check of 2026-11-27: the page plays fine in Brave, Firefox and Safari, and keeps playing with the window minimized and the tab inactive.

- orb-player builds for wasm32-unknown-unknown (cpal, output, devices and control native only). New, on all targets: wire (the binary message format), remote::Player (the UI half with the native Player's methods the UI's Port uses, over a Transport), worklet::Host (the worklet half, owning a native Player + Audio). At 44.1 kHz the samples go out unchanged as x / 32768; the native rubato resampler, moved to resample.rs unchanged, is used only at another rate. Feature web (wasm32): orb_player::web::WebOutput, which makes the AudioContext at 44100, loads the processor script, creates the AudioWorkletNode and resumes on gestures and on visibilitychange.
- crates/orb-worklet: the worklet's wasm module, C ABI, no imports (321 KB). crates/orb-web-demo + scripts/web-demo.sh: the test page. wasm-bindgen-cli 0.2.129 pinned (docs/building.md).
- orb-engine, behavior unchanged: song::LoadSource (Engine::load takes it), Globals::from_payload, Pool::from_lists. orb-player: LiveBlock::set zeroes the bytes past the block.
- Tests: web_loopback.rs (7 tests). Against the native player they check bit-equal samples and equal snapshots and tracks through loads, edits, a mod change, the startup sound, recording and a new song. All 164 built-in-kit corpus songs load the same from their load image. Also wire and orb-worklet unit tests. cargo test --workspace: 1197 passed. clippy (native and wasm32) and fmt clean. Corpus: 164 of 164 equal. The CI job wasm builds the new crates.
- Chrome 157: suspended until the first click, then plays at 44100. No underruns over 55 s, including a 3 s busy main thread and 5 s of 40 ms stalls. Command latency to the snapshot: 3.2 to 25.3 ms (mean 13.4); output latency 32 ms.
- Docs: docs/web.md (new), docs/building.md, the AGENTS.md crate list.
- For task-172: use remote::Player::unapplied instead of follow_frames. An engine sent with Load may be changed after its build only in its patterns.

Review round 1: the worklet's panic message now lives in its own cell (tested), Host::report removed, recipes keyed by a unique Engine::build_id instead of the engine's address, docs/web.md Failures updated; 1198 tests pass.
<!-- SECTION:FINAL_SUMMARY:END -->
