---
id: TASK-329
title: >-
  Web: keep the iPhone screen on while playing, and recover audio after the app
  loses focus
status: Done
assignee:
  - '@agent-complex'
created_date: '2026-11-28T12:53:00Z'
updated_date: '2026-11-28T17:22:00Z'
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
- [x] #1 The user, on their iPhone: while a song plays the screen does not lock; after switching to another app (or locking the phone by hand) and back, Play makes sound again without a reload
- [x] #2 The cause of the failed restart is found and recorded in the notes, with the code that caused it
- [x] #3 In Brave (via the extension) a suspended, interrupted-like, closed or dead-worklet audio path shows the song stopped at once, with no tap, and plays again on the next Play with the song, mod and position kept; a hidden page keeps playing on desktop; the wake lock is requested on Play, released on Stop and when hidden, and requested again on return while playing
- [x] #4 Without the Wake Lock API the app works as before
- [x] #5 docs/web.md describes the wake lock and the recovery; cargo test --workspace, clippy (native and wasm32) and nightly fmt pass
- [x] #6 The stop is platform-dependent (the user's decision A, 2026-11-28). On iOS/iPadOS WebKit (detected by a tested pure function, forced by ?ios), visibilitychange to hidden, pagehide or freeze stops the song and closes the AudioContext, and it shows stopped at once. Elsewhere hidden or blurred never stops, and only a non-persisted pagehide closes the context. On return a browser-suspended context resumes, or the song shows stopped with no tap and the next tap plays. Checked in Brave via the extension
- [x] #7 Media Session: the metadata names the song (artist OpenRB-338), the play, pause and stop actions drive Play and Stop, playbackState follows the play state; without the API the app works as before
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Cause first (AC 2): read crates/orb-player/src/web/mod.rs and crates/orb-ui/src/web.rs; record in the notes what the code does after an interruption.
2. crates/orb-player/src/recovery.rs (pure Rust, native unit tests): the recovery state machine. Inputs: the context state (running, suspended, interrupted, closed), worklet messages, page visible or hidden, gestures, a 500 ms tick. Outputs: nothing, resume, or rebuild. The rules:
   - a closed context is broken;
   - a context that is running but whose worklet posts nothing for 1.5 s while the page is visible is broken;
   - a gesture's resume that does not reach running in 1.5 s is broken;
   - at a gesture, a broken output, or a context that ran once and is no longer running, is rebuilt;
   - a context that never ran is resumed;
   - on return to the page, a context that ran once is resumed without a gesture (desktop browsers allow it).
3. orb_player::web: WebOutput keeps the worklet's wasm bytes. New listeners: visibilitychange, pagehide/pageshow, focus/blur, the context's statechange, the port's messages as liveness, and a 500 ms interval. A rebuild makes the new AudioContext and calls resume() inside the gesture's handler, then builds the node asynchronously. WebOutput::poll_recovery() swaps in the new context and node and reports the change to its owner. Debug hooks (suspend, interrupt, close, stall the worklet, hide, show) for the Brave checks; the processor gets a debug "stall" message.
4. orb_player::remote::Player::reconnect(transport, kit): posts Init again, resets the kit ids and the message counters, and offsets frames and generation so they stay monotonic for the Link. resume_point() gives the play state and tick from the last snapshot plus the Play/Stop/Locate/Load messages it does not show yet (so the Play pressed on a dead worklet counts). Covered by a loopback test with a fresh Host.
5. orb-ui: Port::reconnect (default None; WebPort implements it). App::web_frame polls the output; on a swap it composes the song as Save does, reconnects, Link::reload (build + queue_song, the panel kept), then Locate(tick) and Play if it was playing. The status line says "Audio restarted". A ?debug-audio query shows the context state and the recovery events in the status line.
6. Wake lock (orb_player::web::wake): request on Play, release on Stop, request again when the page is visible while playing, and handle the sentinel's release event. Missing API or a refusal: one console line.
7. docs/web.md: a new section on the recovery and the wake lock; docs/ui_spec.md if it lists browser behavior.
8. Brave via the extension (localhost, a port other than 8338): suspended, interrupted-like, closed, stalled worklet, hide/show; the wake lock's request, release and request again; the page without the API. A LAN server on 0.0.0.0 for the user's iPhone check (AC 1 stays open).
9. cargo test --workspace, clippy (native, and wasm32 for orb-web and the touched wasm crates), nightly fmt; one commit.

10. (The second report) Hidden means stopped: the recovery's visibility(false) answers Leave. The output closes the context and lets the wake lock go, and on pagehide, visibilitychange to hidden and freeze the UI sends Stop at its next frame. A return to the page resumes nothing, and the next Play rebuilds in its gesture.
11. Media Session (orb_player::web::media, feature-detected): metadata (the song's name, OpenRB-338), play/pause/stop handlers (pause and stop close the context at once, like leaving; play is a gesture), playbackState from the play state.
12. Brave rechecks of every case under the new design; the iPhone steps get the in-app browser case.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Branch: worktree-agent-abd60820e575210ec (worktree .claude/worktrees/agent-abd60820e575210ec), based on a09140c.

The cause of the failed restart (code reading, 2026-11-28):
- The page makes exactly one AudioContext and one AudioWorkletNode, in orb_ui::web::start (crates/orb-ui/src/web.rs, `WebOutput::start(worklet_url)` then `OUTPUT` "open for the page's life"). No code path ever makes another, so a context that iOS closes, or leaves interrupted, or whose audio thread stops calling process(), can only be replaced by a reload.
- The one recovery is the `resume` closure in WebOutput::start (crates/orb-player/src/web/mod.rs, lines 314-323 before this task): on a gesture or visibilitychange it calls `c.resume()` and drops the promise. A rejected resume (a closed context; WebKit refuses while the session is interrupted) and a resume that resolves while the worklet stays silent are both invisible. The next gesture only repeats the same resume().
- No liveness check: the processor's per-second stats (`{stats: {quanta, ...}}`) and the snapshots are read only for diagnostics; nothing notices a worklet that stopped posting.
- The UI then shows a frozen snapshot: the last one before the interruption, still `playing: true`. Pressing Play sends Command::Play into the dead port; nothing answers, so Play makes no sound.
- Not the cause: the guard `c.state() != AudioContextState::Running`. web-sys 0.3.106's AudioContextState has only suspended, running and closed, but wasm-bindgen maps an unknown string ("interrupted") to its hidden invalid variant, which is not Running, so resume() was called for "interrupted" too.
- [G] Which of the two iOS mechanisms happens on the user's iPhone (resume refused while interrupted, or "running" with a silent worklet) cannot be told without the iPhone; the ?debug-audio status line added here shows the context state and the recovery events on the phone.

The second iPhone report (the user, 2026-11-28, relayed by the supervisor): "when opening and playing it in embedded browser on reddit. after that the audio stuck somehow in the background and any link that opens embedded browser in any app now plays the openrb web app audio with no way to stop it".
- [G] (the supervisor's reading): iOS in-app browsers (SFSafariViewController) run in one shared out-of-process service, SafariViewService. Closing the in-app view did not unload or stop the page, so its AudioContext kept playing in that process, which every later in-app browser of any app reuses.
- The decision given with it: a hidden page means a stopped song. On pagehide, visibilitychange to hidden (and freeze), the song stops, the AudioContext is closed and the wake lock let go; on pageshow (persisted) or visible again it stays stopped, and the next Play goes through the recovery (a rebuild in the gesture). This replaces the earlier "resume on return without a gesture" rule of the plan. A consequence on desktop browsers: switching to another tab stops the song too.
- Added with it: the Media Session API (metadata: the song's name, artist OpenRB-338; play, pause and stop handlers wired to Play and Stop; playbackState kept in step), feature-detected like the wake lock, so audio that still escapes shows in Control Center and on the lock screen with a working stop.

Brave checks of the first design (before the hidden-means-stopped change; Brave via the extension, http://localhost:8347/OpenRB-338/web/?debug-audio): stall, close, suspend and interrupt each rebuilt on the next Play and played on from the old tick (2902, 8511, 11475, 12245) with the song kept (tempo 142 and the modified mark survived a close); hide/show suspended, released the wake lock, then resumed without a gesture and re-granted it; Stop released the lock; with navigator.wakeLock removed Play played and logged one console line. These are rechecked below for the new design.

What was built (2026-11-28):
- crates/orb-player/src/recovery.rs: the state machine (pure, native tests). A hidden page answers Leave (close the context, stop the song) and the output stays broken. A closed context, a silent worklet (1.5 s while running and visible) and a gesture's resume that does not take (1.5 s) are broken. At a gesture a broken output, or a context that ran and no longer runs, is rebuilt; a context that never ran is resumed. A return to the page resumes nothing.
- crates/orb-player/src/web/mod.rs: WebOutput keeps the worklet's module. Its listeners: the gestures, visibilitychange, freeze, pagehide, pageshow, focus, blur, the context's statechange, and the port's messages as liveness; a 500 ms interval. The rebuild makes and resumes the new context inside the gesture and then builds the node; poll_recovery swaps it in. Also take_left, audible, recovery_state, and the debug stand-ins (suspend, interrupt, close, stall, hide, show, media-play, media-stop). processor.js got the debug "stall" message.
- crates/orb-player/src/web/wake.rs: the Screen Wake Lock. crates/orb-player/src/web/media.rs: the Media Session. Both are feature-detected and log one console line when missing.
- remote::Player::reconnect: Init again, the kit ids reset, frames and generations offset so they stay monotonic; returns Locate(tick) + Play (if the snapshot played) + the unapplied Play/Stop/Locate (only those after an unapplied Load).
- orb-ui: Port::reconnect (WebPort), Link::reload (load without making the panel new), Link::kit, App::web_audio_frame / web_restore_audio (the song from song_for_save, reconnect, reload, then the commands; status "Audio restarted (why)"), Stop on a left page, the media actions, the wake lock and the playbackState following snapshot.playing && audible, ?debug-audio in the status line. orb-web exports debug_audio(what).

Native tests: recovery 8 unit tests; web_loopback a_new_worklet_goes_on_where_the_old_one_was (a fresh Host after a silent one: the given commands are Locate(old tick), Play, Play; the new host's samples equal a native player's given the same song and commands bit for bit over 320 quanta; tick, frames = old + new, generation = old + new; tracks' generation follows) and a_reconnect_after_an_unapplied_load. A first hypothesis of that test was wrong: with a Stop then Play in the tail the song restarts from the bar start (Stop locates there), so the "tick goes on" assertion holds only for the Play-only tail; the test uses that.

Brave via the extension (http://localhost:8347/OpenRB-338/web/?debug-audio; the numbers are in docs/web.md 4.4):
- Final design: hide while playing at tick 769 closed the context, released the wake lock, playbackState "paused"; show resumed nothing; Play rebuilt ("the page was left") and played from 768 with the lock held again (requests 2). media-stop closed and released; media-play (no click) rebuilt and played, metadata Untitled / OpenRB-338, playbackState "playing". stall, close, suspend, interrupt each rebuilt on the next Play and played on from 6851, 7610, 8358, 9117; generation went 3 to 7.
- Without the APIs (a temporary copy of index.html hiding navigator.mediaSession and navigator.wakeLock before the app starts; removed after): Play played, hide/show/Play rebuilt and played, one console line per API.
- A real tab switch could not be made (a tab the extension opens stays in the background).
- Desktop consequence of the decision: switching tabs stops the song.
- The play light stays lit after a left page until the next Play (the snapshot is the old worklet's last); the wake lock and the Media Session follow what is heard instead.

AC 3 under the decision: "requested again on return to the page" — the wake module still asks again on visibilitychange to visible while the song is wanted, but a hidden page now stops the song, so in practice the lock is asked again at the next Play after the return (seen: requests 1 to 2 at the Play after hide/show).

Servers left running for the user's iPhone check (the supervisor stops them):
- https://192.168.88.223:8348/OpenRB-338/web/?debug-audio : python3 scratchpad/https_serve.py, self-signed certificate (valid 7 days, SAN IP:192.168.88.223), bound to 0.0.0.0, serving the worktree's dist/web. Needed: AudioWorklet and the wake lock are secure-context-only, so http on the LAN cannot play at all.
- http://192.168.88.223:8347/OpenRB-338/web/ : python3 -m http.server bound to 0.0.0.0 (used for the Brave checks on localhost). On the iPhone it will not play (not a secure context).

Checks (2026-11-28): cargo test --workspace 1236 passed, 0 failed, 6 ignored; cargo clippy --all-targets -D warnings clean; wasm32 clippy clean for orb-web, orb-web-demo, orb-worklet, orb-wasm-check; nightly fmt clean. After the last small edit (the status line wake word) native clippy and the orb-player and orb-ui tests (620) were rerun clean.

The user's iPhone steps (AC 1 and AC 8):
1. On the iPhone, on the same Wi-Fi as this Mac, open https://192.168.88.223:8348/OpenRB-338/web/?debug-audio in Safari. The certificate is self-signed: tap Show Details, then visit this website. Plain http on the LAN cannot play at all, because AudioWorklet and the wake lock need a secure context. The alternative is the Pages build after the merge, with ?debug-audio added.
2. Tap Play. The status line at the bottom shows "audio running | wake held | ...". Leave the phone untouched past its auto-lock time: the screen must stay on and the song keep playing (AC 1, first half).
3. Switch to another app (or lock the phone with the side button), wait 10 s or more, and come back. The song is stopped. Tap Play: it plays again from the bar where it stopped, without a reload, and the status line shows "rebuilt" (AC 1).
4. While it plays, open Control Center. The player shows "Untitled" (or the song's name) by OpenRB-338, and its pause stops the sound.
5. In-app browser (AC 8): paste the URL into a Notes note or a Messages message (or the Reddit app) and tap it, so it opens in the in-app browser. Accept the certificate, tap Play, then close the in-app view (Done or swipe). The audio must stop. Open Control Center: no OpenRB-338 player is left playing. Then open any other link in an in-app browser: no OpenRB-338 audio.
6. If something fails, take a screenshot of the status line (and of Control Center): it shows the context's state and the last recovery event.

The iPhone round 1 (the user, 2026-11-28, through the https LAN server, f6d2ebc): "wake hold works. audio recovers after losing focus. though the UI is not updating until a tap (shows as playing). nothings shows up in control center". So the wake lock and the recovery of AC 1 are confirmed on the iPhone.
- The stale panel, [C] from the code: the Stop sent at the first frame after the return went to the closed worklet. The snapshot stayed the old worklet's last one (playing) with the Stop unapplied, and Link::follow takes the engine's values only when nothing is unapplied, so the play light stayed lit until a tap's rebuild. It was not a missing repaint: the output's notify already asked for one on the return. The fix: remote::Player::stop_offline (Port::stop_offline), which posts Stop for the next worklet and shows the snapshot stopped at once (silent meters, nothing unapplied). Loopback test a_stop_while_the_worklet_is_gone: the new worklet then holds the song stopped at the start of its bar, as a native player does. Brave via the extension: after hide and show, with no input, state showed playing false, unapplied 0, and the Play button dim; the next Play rebuilt and played from 768.
- Control Center, from WebKit main's source (read 2026-11-28): MediaSessionManagerCocoa::nowPlayingEligibleSession picks a session by isNowPlayingEligible(). The client default is false (PlatformMediaSessionInterface.h). AudioContext::isNowPlayingEligible (Modules/webaudio/AudioContext.cpp) is true only when the destination is connected, the context was not suspended by script, and navigator.audioSession.type is "playback" or "play-and-record" (hasPlayBackAudioSession, ENABLE(DOM_AUDIO_SESSION)). orb sets no audio session type, so Web Audio alone is never Now Playing. That is [C] for WebKit main and [G] for the user's shipped iOS. So "only HTMLMediaElement" is refuted: an AudioContext with audioSession "playback" can be eligible. Not applied (a product choice: the playback category ignores the silent switch and interrupts other apps' audio). The <audio> route (MediaStreamAudioDestinationNode into srcObject) and its risks are written in docs/web.md 4.3; it is not done. The Media Session code stays for desktop and Android.

The user's decision of 2026-11-28 (relayed by the supervisor): "we don't want it to stop anywhere, only if it's needed for recovery". It replaces "a hidden page means a stopped song". Hidden, blur and freeze no longer stop the song or close the context. On return the path is checked: a running, rendering context does nothing; a suspended or interrupted one gets resume(); if that does not bring audio back, the song shows stopped at once, and the next tap rebuilds. pagehide stops only on a real unload, not a persisted bfcache entry.

Finding before that change (sent to the supervisor first, as asked): closing an SFSafariViewController most likely does not fire pagehide. [G] from the user's second report: the audio went on in SafariViewService after the view closed. A pagehide that is not persisted means the document unloads, and WebKit stops a document's AudioContext when the document is torn down (AudioContext is an ActiveDOMObject, stopped with its document), so the page cannot have been unloaded. A persisted pagehide comes only with a navigation into the back/forward cache, and closing the view is not a navigation. What the page gets is at most visibilitychange to hidden (WKWebView turns the page hidden when its view leaves the window). So with stop-on-pagehide only, the in-app browser leak of the second report comes back.

The user chose option A on 2026-11-28 ("do A", relayed by the supervisor). On iOS and iPadOS WebKit a hidden page (visibilitychange to hidden, pagehide, freeze) stops the song, closes the context and shows it stopped at once. Detection: an iPhone/iPad/iPod user agent, or a Mac user agent with navigator.maxTouchPoints > 1. Everywhere else a hidden or blurred page never stops; only a non-persisted pagehide closes the context. No audioSession change: options B and C are written in docs/web.md 4.1 as the alternatives considered.

What changed for it:
- recovery.rs:
  - Recovery::stop_on_hidden (kept across a rebuild) and Recovery::unload.
  - On return, a context that ran and no longer runs is resumed without a gesture. A context that ran and is not running again within 1.5 s while visible is broken (Broken::NoResume); the time hidden does not count.
  - ios_webkit(user_agent, max_touch_points) is the pure platform decision, tested with iPhone Safari, iPhone Chrome (CriOS), iPadOS desktop-class with and without touch, Mac Chrome and Android Chrome user agents.
  - 13 unit tests.
- web/mod.rs:
  - on_visibility tells an unload (pagehide with persisted false) from a hide.
  - On every new break, Shared::judge closes the context (so a late comeback cannot sound under a stopped panel), lets the wake lock go, sets take_stopped and asks for a repaint.
  - set_stop_on_hidden is set from the page.
  - New stand-ins: hide (no suspend), hide-suspend, hide-dead.
- remote::Player::stop_offline: Stop is posted, the snapshot shows the song stopped at once, and what the old worklet still posts is dropped until reconnect. A first version without the drop failed in Brave in iOS mode: a snapshot already on its way showed the song playing again (playing true, unapplied 1). The loopback test now renders the old worklet once more after stop_offline and checks its snapshot is not taken.
- orb-ui: the page computes ios_webkit and ?ios, logs which applies, and on take_stopped calls Port::stop_offline. The UI matches what is heard.

Brave via the extension (http://localhost:8347/OpenRB-338/web/?debug-audio, &ios for iOS mode):
- Desktop hide while playing: plays on (tick 748 to 1872 while hidden), the wake lock released while hidden and held again at show.
- Desktop hide-suspend, then show: resumed without a gesture and went on (2559 to 3224).
- Desktop hide-dead, then show: 2 s after show, with no input, "broken: the audio context did not resume", stopped at 6144; the tap rebuilt and played from 6144.
- Desktop pagehide persisted true: plays on. pagehide persisted false: "pagehide: unload", closed, stopped.
- Desktop stall, close, interrupt: each shown stopped at once (playing false, unapplied 0) and rebuilt on the tap.
- iOS mode (?ios) hide while playing: closed, stopped at once (playing false, unapplied 0); after show the Play button was dim with no input; the tap rebuilt and played from 768.
- Console: "OpenRB-338: a hidden page stops the song (?ios)".

Criteria 3 and 8 were reworded to the final design (the supervisor's instruction).

Checks after the final design (2026-11-28): cargo test --workspace 1242 passed, 0 failed, 6 ignored; native clippy, wasm32 clippy (orb-web, orb-web-demo, orb-worklet, orb-wasm-check) and nightly fmt clean. The https server on 8348 (PID 44822) serves this build.

The iPhone round 2 (the user, 2026-11-28, https LAN server, b9396c1): "it works, going back into app it's stopped now. can't test embedded browser though because it doesnt open probably because self signed cert. I'll test it from the site". The wake lock works, and after leaving the app and coming back the panel shows the song stopped with no tap; a tap plays again. AC 1 is checked on the user's word (round 1 confirmed the wake lock and the recovery; round 2 the stopped panel on return).

The in-app browser check could not run over the LAN: the in-app browsers do not open the self-signed https server. As the supervisor directed, it was removed from this task (the former AC 8) and is task-330 ("Check the web app in an iPhone in-app browser after deploy", @pingw33n, m-6, depends on task-329), to be done on the deployed site.

Review round 1 (send back, 2026-11-28). Each defect was checked against the code first, and all were real:
1. A failed song conversion left an empty worklet: web_restore_audio called reconnect() before the song was converted and built, so on a failure the new worklet had no song and the restore commands were dropped. Fixed: the song is converted and built first (Link::rebuild_song, which posts nothing). Only then come reconnect and Link::reload(engine), which queues it. On a failure the player stays on the old worklet, whose silence breaks the output again, so the next tap tries again; the error goes to the message box.
2. A system Play on a hidden page was ignored: Recovery::gesture returns None while hidden. Fixed: Recovery::media_play is a gesture whatever the visibility, and the Media Session's play calls it (web/mod.rs activate(.., true)). Test a_system_play_while_hidden.
3. A rebuild finishing after a leave stored a closed context: the spawned future set sh.replacement even when leave() had closed and forgotten the pending context. Fixed: on Ok it keeps the node only if sh.pending is this context (Object::is); otherwise it drops the node, which closes the rest. Not factored into a native test: it is the identity of a JS object across an await.
4. A trapped worklet counted as alive: its stats and its Error message reached heard(). Fixed: only ArrayBuffer messages other than a wire Error (tag 4, ERROR_TAG) count. A host that failed sends no snapshots after its Error (Host::render does not advance the frames, so publish sends nothing), so a trapped or failed worklet is broken after STALL_MS.
5. Note only: wake.rs request() treats a held sentinel as the lock held. A lock the browser ends without a release event would be asked for again only after a hide or a Stop; a comment says so.
6. docs/web.md said round 1 showed the song stopped on return. Corrected: round 1 confirmed the recovery, round 2 the stopped panel.
7. docs/web.md "the song so plays again" typo and the broken line, fixed.
8. The notes' "AC 8" mentions are stale: the step list "The user's iPhone steps (AC 1 and AC 8)" and its step 5 refer to the former AC 8, the in-app browser check, which is now task-330. The notes are not rewritten; this line records it.

The Bluetooth regression (the user, on b9396c1, 2026-11-28): "I tried connecting Bluetooth audio and it didn't work, it went silent. I'll test again with different Bluetooth devices once live. previously (current deployed web app) it worked though".
- The cause, [G] (Brave cannot make an iOS route change): on a route change iOS suspends or interrupts the AudioContext for a moment, or the audio thread pauses process() while the route switches. Before task-329 nothing reacted, and the context came back by itself. b9396c1 judged a context that ran and stayed stopped while visible after 1.5 s (stopped_at, RESUME_MS) and a silent worklet after 1.5 s (STALL_MS). Shared::judge then closed the context, which made the self-recovery impossible, and showed the song stopped.
- The fix:
  - A stopped context while the page is visible gets resume() at every 500 ms check (Recovery::tick now answers Resume) and is judged only after COMEBACK_MS = 5 s.
  - STALL_MS is 4 s.
  - judge no longer closes the context: a late comeback takes the Stop that stop_offline posts and stays silent, never sounding under a stopped panel.
  - Tests: a_route_change_keeps_playing; the suspension and return tests were updated.
- iOS behavior changes, so the supervisor may want another iPhone round:
  - an interruption or route change while visible now waits up to 5 s with resume() retries before showing the song stopped;
  - a break no longer closes the context;
  - a system Play while hidden now rebuilds (moot on iOS, which shows no Now Playing for Web Audio).
  - The hidden-page stop on iOS (option A) is unchanged.
- Brave via the extension (http://localhost:8347/OpenRB-338/web/?debug-audio), set off with the stand-ins:
  - suspend while playing: resumed at the next check 0.2 s later and played on (790 to 1373), with no stop;
  - interrupt (the stand-in stays "interrupted"): shown stopped 5.2 s later (broken: the audio context did not resume), and the tap rebuilt and played;
  - stall: still playing in the panel after 3 s, shown stopped at 4.4 s, and the tap rebuilt and played.
  - A real Bluetooth route change is not verified: Brave cannot make one. The step is in task-330 for the live site.

Bluetooth, correction (2026-11-28): the user's follow-up was "my Bluetooth headphones work but my receiver is weird". Bluetooth headphones work on the iPhone; one receiver misbehaved, not caused by the app. The Bluetooth investigation and its code changes above (COMEBACK_MS, the 4 s stall, the resume() retries, no close on a break) were dropped and are not in the commit; only review defects 1-8 are. The connect/disconnect step stays in task-330 as a check.

Review round 1 checks (2026-11-28): cargo test --workspace 1243 passed, 0 failed, 6 ignored (the new a_system_play_while_hidden test); native clippy, wasm32 clippy (orb-web, orb-web-demo, orb-worklet, orb-wasm-check) and nightly fmt clean. iOS behavior: fixes 1, 3 and 4 change only failure paths (a song that does not load, a leave during a rebuild, a trapped worklet); fix 2 matters only where the Media Session shows (not iOS Web Audio). The normal iOS paths the user checked are unchanged.

Review round 2 (send back, 2026-11-28), defect A, checked against the code and real: the round 1 comment at web_restore_audio was false. By the time rebuild_song fails, poll_recovery has already swapped in the new context and node and dropped the old Inner and its mailbox. So the player's old transport posts and polls into nothing, while the new output looks healthy (running, no break), and no later tap rebuilds.

Fixed:
- On a failed conversion or build, the app calls WebOutput::restore_failed, which runs Recovery::restore_failed (Broken::Restore, "the song could not be given to the new audio worklet") and Shared::judge.
- The song then shows stopped at the next frame, and the next gesture rebuilds and tries again. The player is not reconnected.
- The comment now says that.
- The native test a_failed_restore_rebuilds_again: a rebuilt, running, posting output marked restore_failed stays broken through ticks and messages, and the next gesture answers Rebuild.

Also moved out of docs/web.md, as the review asked: the "review of task-329 (round 1)" list is above in these notes; docs/web.md keeps only the wake lock sentence as a fact, and the failed-restore rule in "The song again".

Review round 2 checks: cargo test --workspace 1244 passed, 0 failed, 6 ignored; native clippy, wasm32 clippy (orb-web, orb-web-demo, orb-worklet, orb-wasm-check) and nightly fmt clean. iOS behavior: only the failed-restore path changes.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
The web app now recovers its audio after an interruption without a reload, and its panel always shows what is heard. It keeps the screen on while a song plays and shows the song in the system's media controls on desktop and Android. A hidden page stops the song only on iOS/iPadOS WebKit (the user's decision A of 2026-11-28); elsewhere it plays on. Branch worktree-agent-abd60820e575210ec, one commit on a09140c.

The cause of the failed restart (AC 2, details in the notes): the page made one AudioContext and worklet for its life, and its only recovery was a fire-and-forget resume() of that context. Nothing ever noticed a context that iOS closed or left interrupted, or a worklet that stopped rendering. The UI kept a frozen "playing" snapshot, and Play went into a dead port.

What changed:
- orb_player::recovery: the state machine and the pure ios_webkit platform test.
- orb_player::web, the new parts:
  - On return, a suspended context resumes without a gesture where the browser allows it.
  - On any break the context is closed and the song shown stopped at once.
  - A gesture rebuilds the context and the worklet.
  - iOS hides and unloads stop the song.
  - wake.rs holds the Screen Wake Lock; media.rs drives the Media Session.
  - Debug stand-ins for the checks.
- remote::Player gets reconnect and stop_offline.
- orb-ui gives a rebuilt worklet the open song as Save writes it, with its kit, position and play state. ?debug-audio and ?ios are new query options.
- docs/web.md 4.1 to 4.4 have the design, the user's decisions, the alternatives considered (B: audioSession "playback"; C: accept the leak), iOS's Now Playing rule from WebKit's source, and the <audio>-element route.

The user's iPhone checks:
- Round 1 (f6d2ebc) confirmed the wake lock and the recovery. Its stale panel is fixed by stop_offline.
- Round 2 (b9396c1): "it works, going back into app it's stopped now". AC 1 is checked.
- Control Center shows nothing on iOS: Web Audio is Now Playing only with audioSession "playback" ([C] WebKit main, [G] shipped iOS), which the user did not take.

Tests: cargo test --workspace 1242 pass (6 ignored); clippy native and wasm32 (orb-web, orb-web-demo, orb-worklet, orb-wasm-check); nightly fmt; Brave via the extension in desktop and ?ios modes.

Follow-up: task-330 (@pingw33n, m-6), the in-app browser check (Gmail, Telegram) on the deployed site. It was this task's former AC 8; the in-app browsers do not open the self-signed LAN server.
<!-- SECTION:FINAL_SUMMARY:END -->
