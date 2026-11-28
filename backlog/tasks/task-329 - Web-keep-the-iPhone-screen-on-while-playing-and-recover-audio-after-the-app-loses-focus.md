---
id: TASK-329
title: >-
  Web: keep the iPhone screen on while playing, and recover audio after the app
  loses focus
status: To Do
assignee:
  - '@agent-complex'
created_date: '2026-11-28T12:53:00Z'
labels:
  - player
  - ui
milestone: m-6
dependencies: []
references:
  - crates/orb-player/src/web/mod.rs
  - crates/orb-worklet
  - crates/orb-ui/src/web.rs
  - docs/web.md
priority: high
type: bug
ordinal: 300700
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Why: the user plays the web app on an iPhone (2026-11-28). Two problems:
1. The screen locks during playback, and when an iPhone locks, Safari suspends web audio, so the song stops.
2. When the app loses focus on the iPhone (switching apps, locking, a notification or call), playback stops and does not start again: Play no longer makes sound and only a page reload recovers. The user's words: "currently it stops playing but fails to start again and needs a reload".

Context:
- The web audio path (task-170 path C, task-171): the engine runs in an AudioWorklet as its own wasm instance and talks to the main thread by postMessage; the AudioContext runs at 44100 Hz. Code: crates/orb-player/src/web/mod.rs (the context, the worklet node, the GESTURES listeners that resume the context, which task-326 extended), crates/orb-worklet, crates/orb-ui/src/web.rs. docs/web.md describes the audio path.
- iOS Safari puts an AudioContext into the non-standard state "interrupted" when the page is backgrounded, the phone locks, or another app takes the audio session. It may need resume() from a user gesture afterwards. In some cases (long background, audio session lost) the context or the worklet stays dead even after resume() reports "running"; then the only fix is a new AudioContext and worklet node. The engine's state lives in the worklet's wasm instance, so a new worklet must be given the song, the mod, the settings and the position again.
- Screen Wake Lock: navigator.wakeLock.request("screen"), Safari on iOS 16.4 and later (Home Screen web apps only reliably from iOS 18.4). A secure context is needed (GitHub Pages and localhost are). The browser releases the lock when the page is hidden, so it must be requested again when the page is visible.

What to do:
- Wake lock: hold a screen wake lock while the song plays, release it on Stop. Request it again on visibilitychange to visible while playing, and handle the sentinel's release event. Without the API, or when the request is refused, do nothing visible (log it to the console once). No settings switch.
- Recovery:
  - On visibilitychange, pagehide/pageshow, the context's statechange ("interrupted", "suspended") and focus/blur, record why audio stopped and recover. If the context was playing, resume it on return, from the next user gesture if iOS needs one.
  - If after resume the worklet sends no audio within a short time, or the context state can't reach "running", tear down the context and worklet. Build them again, and restore the song, mod, settings, play state and position, so a reload is never needed.
  - Pressing Play after any interruption must always make sound.
  - Find what the current code does wrong first (e.g. a play flag that thinks it is still playing, a context left "interrupted" which the resume guard misses because it only checks for "suspended", or a dead worklet port). Record it in the notes.
- Testing without an iPhone:
  - Simulate in Brave via the Claude in Chrome extension: ctx.suspend(), a hidden-page emulation, a closed context, a worklet killed by terminating its port.
  - Unit-test the recovery state machine natively if it can be factored out.
  - The real check is the user's, on their iPhone: leave the acceptance criterion for it open and give exact steps.
- Docs: docs/web.md (the wake lock and the recovery) and docs/ui_spec.md if the web section lists browser behavior.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The user, on their iPhone: while a song plays the screen does not lock; after switching to another app (or locking the phone by hand) and back, Play makes sound again without a reload
- [ ] #2 The cause of the failed restart is found and recorded in the notes, with the code that caused it
- [ ] #3 In Brave (via the extension) a suspended, interrupted-like, closed or dead-worklet audio path recovers on the next Play with the song, mod and position kept; the wake lock is requested on Play, released on Stop and requested again on return to the page
- [ ] #4 Without the Wake Lock API the app works as before
- [ ] #5 docs/web.md describes the wake lock and the recovery; cargo test --workspace, clippy (native and wasm32) and nightly fmt pass
<!-- AC:END -->
