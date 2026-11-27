---
id: TASK-172
title: 'OpenRB-338 web build: orb-ui on wasm32 with eframe, served as a static page'
status: Done
assignee:
  - '@agent-complex'
created_date: 2026-11-03 09:16
updated_date: '2026-11-27T11:42:00Z'
labels:
  - ui
  - tools
milestone: m-6
dependencies:
  - TASK-169
  - TASK-170
  - TASK-171
references:
  - crates/orb-ui
  - crates/orb-cli/Cargo.toml
  - crates/orb-ui/src/browser.rs
  - crates/orb-ui/src/settings.rs
  - crates/orb-ui/src/crash.rs
  - crates/orb-ui/src/menu.rs
  - crates/orb-ui/src/session.rs
  - docs/ui_spec.md
  - docs/building.md
priority: high
type: feature
ordinal: 188000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Why: m-6's goal (the user, 2026-11-03) is OpenRB-338 as a web page hosted on GitHub Pages. This task makes the desktop UI (orb-ui, eframe 0.37) run in the browser with the built-in kit (assets/Standard.rbm is embedded) and songs opened from the local filesystem. Loading from the GitHub collections is a later task; the deploy is another.

What is native-only today and needs a web path (cfg(target_arch = "wasm32")), per task-170's decision:
- orb-ui depends on orb-cli for info, mods, render (export) and encode; orb-cli pulls clap, cab and vorbis_rs (libvorbis, C, does not build for wasm32-unknown-unknown). Make the parts the UI uses build for wasm (cargo features on orb-cli, e.g. a default `native` feature for clap/vorbis/cab, or moving the shared code into a library crate); Ogg export is hidden on the web if task-170 says so.
- std::fs everywhere: session.rs (song and mod reads, the atomic save), browser.rs (the in-app file browser lists real folders), settings.rs (the settings file), crash.rs (the crash log). On the web: open a song or a mod with the browser's file picker (an <input type=file>, rfd's wasm AsyncFileDialog, or drag and drop onto the page); a mods folder through a directory picker (webkitdirectory) held in memory for the session so a song's mod is found by its INFO name as natively (orb_cli::mods::scan's rules); save and export become downloads; settings in localStorage; a panic shows the error box and logs to the console.
- Threads: the export thread in menu.rs; on the web it runs in slices per frame (or as task-170 says).
- rfd (native message box) and the window icon, the native menu shortcuts that collide with the browser's (Ctrl+O, Ctrl+S, Ctrl+W, Ctrl+Q): keep what the browser lets the page take, document the rest.
- The panel scales to the browser window as it does to the native window.

Build: a script `scripts/build-web.sh` (trunk, or wasm-bindgen-cli + wasm-opt, pinned versions; pick one and document it in docs/building.md) that writes a static site to dist/web/ (index.html, the .wasm, the JS glue, any worklet), loadable from any sub-path (GitHub Pages serves a project site at /openrb-338/), with the version and commit shown in the About box as natively.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 scripts/build-web.sh builds dist/web/ with the pinned toolchain and the pinned web tools; the native build, tests and clippy are unchanged
- [x] #2 Served from a local static server under a sub-path (e.g. /openrb-338/), the page shows the panel, plays a song opened from a local file with the built-in kit, and plays a song whose mod is in a local mods folder the user picked
- [x] #3 Save and export (the formats task-170 allows) download the file; a saved song reopens with the same content as the native save of the same edit
- [x] #4 Settings persist across reloads in localStorage; a panic shows the error box
- [x] #5 docs/building.md has the web build; docs/ui_spec.md lists what differs on the web
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. orb-cli: optional deps behind a default feature cli (clap, ctrlc, cab, claxon for the binary) and ogg (vorbis_rs); the lib modules the UI uses (info, mods, render, encode) build for wasm32; Format::Ogg stays and fails to encode without ogg; flacenc single-threaded on wasm32. orb-ui depends on orb-cli with ogg natively, without it on wasm32.
2. orb_cli::fs: a file-system facade. Native: the std::fs / Path calls unchanged. wasm32: an in-memory file system for the session (picked files, the picked mods folder, saves and exports). The call sites in orb-cli (mods, render) and orb-ui (session, browser, skin/bitmaps, menu) go through it.
3. orb-ui on wasm32: cfg out what is native (cpal Controller, Audio Output window, rfd, arboard, threads, Instant via web-time, portable mode, crash log files). Settings in localStorage (same TOML text). Panic: an HTML error box plus console.error. Export: sliced per frame (render chunks per frame, then encode) with the progress in the status line, then a download. Save: written into the memory file system and downloaded.
4. orb_ui::web (wasm32): the entry (eframe WebRunner on a canvas, WebOutput + remote::Player behind a Port), file pickers (input type=file, webkitdirectory for the mods folder), drop of .rbs/.rbm onto the page, downloads. Port gets unapplied() (0 natively) so Link takes engine values only from a snapshot that shows every command sent (task-171's advice).
5. Web menu: File > Open and Mods > Choose File use the browser's picker; Mods > Choose Mods Folder (web); Ogg hidden in Export; Quit absent; shortcuts the browser keeps documented.
6. crates/orb-web (cdylib) + web/index.html; scripts/build-web.sh writes dist/web/ (index.html, wasm, JS glue, worklet), relative URLs so any sub-path works; version and commit in About as natively.
7. Check in Chrome under /OpenRB-338/web/ from a local server: panel, a shipped song from a file (via drop fixture), a mod song with a picked mods folder, save and export downloads, settings across reload, a debug panic. Native: cargo test, clippy, fmt unchanged; wasm clippy.
8. Docs: docs/building.md (web build), docs/ui_spec.md (web differences), docs/web.md (the UI side).
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Branch: worktree-agent-a1feb1e606ca7b402 (worktree .claude/worktrees/agent-a1feb1e606ca7b402), rebased onto local main 6a3de20 (task-169, task-171).

Work notes (2026-11-27):
- orb-cli: features cli (clap, claxon, ctrlc; the orb binary has required-features = cli) and ogg (vorbis_rs), both default. The library builds for wasm32 without them (cargo build --target wasm32-unknown-unknown -p orb-cli --lib --no-default-features: clean, no other change needed). encode::ogg without ogg returns an error. FLAC on wasm32: multithread off (flacenc's par spawns threads).
- orb_cli::fs: the file facade. Native: each function is the std::fs / Path call. wasm32: a memory file system (thread_local BTreeMap of files + set of folders). Call sites moved: orb-cli mods.rs (mod_dir, scan, resolve), render.rs (read_song, read_mod, render_to_file); orb-ui session.rs, browser.rs (read_entries, accept, finish, new, with_dir), skin/bitmaps.rs, menu.rs (mod_opts). The browser's listing used DirEntry::file_name; now path.file_name() of the same entries.
- Failed/changed hypothesis: eframe's DroppedFile is a trait in egui 0.37 (DroppedFileHandle, bytes_async on wasm), not a struct with bytes: dropped files are read with spawn_local and arrive as Picked::Dropped next frame. web-sys has no File::webkit_relative_path: read through js_sys::Reflect.
- Export on the web: orb_engine::frame::export::Export (a stepper; export() and export_song() now loop over it, same calls, same buffer) + export_song_steps. First Chrome run encoded the FLAC in one go: a 5758 ms frame for Calm (5:18). Added orb_cli::encode::FlacSteps (flacenc's non-par encode_with_fixed_block_size, frame by frame); then the longest frame was 70 ms, the whole FLAC export 7.2 s.
- Port got unapplied() (0 natively) and take_errors(); Link::follow, Link::moving and the deferred pattern use unapplied() (task-171's advice); WebPort has follow_frames 0.
- wasm32 build: eframe default features compile for wasm32 (wgpu with webgpu + webgl from egui-wgpu's defaults). rfd and arboard moved to native-only deps. clippy::drop_non_drop fires on wasm32 for crash.rs's drop(file) (std File has no Drop there): allowed on wasm32 only.
- Sizes: orb_web_bg.wasm 19.0 MB (9.2 MB gzip; without the name section 16.6 MB / 8.8 MB gzip; kept for readable panic stacks), orb_web.js 167 KB, orb_worklet.wasm 320 KB.

Chrome 157 checks (own tab, Claude in Chrome; local server python3 -m http.server on a scratch folder with OpenRB-338/web -> dist/web and fixtures/ = Calm.rbs, Fonda Workout.rbs and the four recrypted shipped mods from orb mod recrypt --iso):
- Panel shows at /OpenRB-338/web/; context Suspended before a gesture, Running at 44100 after.
- Calm dropped (DragEvent with a DataTransfer on the canvas): title Calm.rbs; Play: frames 720896 -> 811008, tick 67 -> 917 in 2 s, bar display 4.
- Mods folder input (orb-pick-mods-folder, files set from a DataTransfer, change dispatched) with MSM2/PBE2/Metallicon/Orbit2: status "Mods folder Picked: 4 mod files"; Fonda Workout through orb-pick-song: kit Mod("/Mods/Picked/MSM2.rbm"), MSM splash on the panel, plays (tick 83 -> 1435 in 3 s).
- Save after Options > Vintage (menu click; Cmd+S also): download intercepted (HTMLAnchorElement.click patched to fetch the Blob URL): Fonda Workout.rbs 20342 bytes sha256 75438b1bbde4c42e4ffa1069e39a6971...; a scratch native test (headless App with Link::new, open, Cmd::ToggleVintage, save_to; not committed) gave 75438b1bbde4c42e4ffa1069e39a697172ec0ff5a1a388000868393fc69f514f: equal. Dropped back as Reopened.rbs: Vintage checked; saved again abbc125615d69f805923172d5e82f5bc..., the native re-save of the native save abbc125615d69f805923172d5e82f5bc573b43e64ff01af87e81395118088532: equal (the re-save reorders same-tick track events in both).
- Export of Calm through the in-app Export browser (wav/flac only): calm.wav 56013828 bytes 54560491ecb1c8c601e688933a282b1e..., calm.flac 19594661 bytes 01991887bad92c5fa7c831b8877c4837...; orb render of Calm.rbs: 54560491ecb1c8c601e688933a282b1ee1d9598e5a37e15fb740bfe88ab67d2c and 01991887bad92c5fa7c831b8877c483728dc12d06b42a9b2371dd178623bf208. Status "Exporting calm.flac ... 94%", "Encoding calm.flac ... 37%", "Exported calm.flac (downloaded)".
- Settings: localStorage /OpenRB-338/OpenRB-338.toml held mods_dir, theme = "classic", pattern_editor_303_1_pos after a reload in a new tab, and the classic skin showed.
- ?debug-panic: the page's error box with the message and location; console: eframe's message + stack, then "OpenRB-338 2026.1127.0931 (dev-6a3de20-dirty): panic: ...".

Final checks (2026-11-27, rebased on main 6a3de20, which is still main):
- cargo test --workspace --no-fail-fast: 1200 passed, 0 failed, 6 ignored (new: orb-engine corpus_frame the_export_in_steps_equals_the_whole_export, Calm stepped 1 and 7 calls at a time equals export_song and its reference sha256; orb-cli encode the_flac_in_steps_is_the_whole_flac). A first run used Fonda Workout under "shipped", which is not there (it needs a mod): fixed to Calm.
- Corpus: orb compare --manifest tests/refs/manifest.json --mod-dir RECRYPTED (all sets): 175 of 175 equal (orb-engine change: export::export and export_song now run through export::Export).
- cargo clippy --all-targets -D warnings clean; cargo clippy --target wasm32-unknown-unknown -p orb-web --all-targets -D warnings clean, and -p orb-web -p orb-ui -p orb-cli --no-default-features --lib clean; cargo xwin clippy --target aarch64-pc-windows-msvc --all-targets -D warnings clean (x86_64-pc-windows-msvc stops in asio-sys's build script on this Mac: its LLVM/bindgen setup, unrelated to this change); cargo +nightly fmt --all --check clean; scripts/build-web.sh builds dist/web.
- Chrome after the last build: the Mods menu shows Choose file..., Choose Mods Folder..., Refresh List; Options has no Audio Output; Help > About shows "Version 2026.1127.0931 (dev-6a3de20-dirty)" as natively.
- CI: the wasm job now also builds -p orb-web and runs its wasm32 clippy.

AC evidence:
- AC 1: scripts/build-web.sh (pinned toolchain + wasm-bindgen-cli 0.2.129, version checked) writes dist/web/; native tests 1200 pass, native clippy clean, native features kept (orb-ui takes orb-cli with ogg natively; orb binary with cli).
- AC 2 (left for the user): checked by the agent through the page's state (engine frames/tick advancing, kit MSM2 from the picked folder) with fixture files given by drop and by setting the pickers' inputs; the real file and folder dialogs and listening need the user.
- AC 3: save download equals the native save of the same edit (75438b1b...), reopens with Vintage on, its re-save equals the native re-save (abbc1256...); WAV and FLAC exports download and equal orb render (54560491..., 01991887...).
- AC 4: localStorage settings (theme classic, mods_dir, editor position) back after a reload in a new tab; ?debug-panic shows the error box and logs to the console.
- AC 5: docs/building.md "The web page" + scripts row + CI; docs/ui_spec.md section 26; docs/web.md section 7.

User review round 1 (2026-11-27, Brave on macOS, relayed by the supervisor): "it should do better while it's still loading. play doesn't work, no sound and steps are not running. cmd+o opens built in file dialog while menu opens native." Then the user's decisions of the same day: File > Open Local... and Cmd/Ctrl+O both open the browser's picker directly (no in-app dialog for local files on the web; an intermediate "This computer..." place in the in-app browser was built and removed again when the user changed their mind); "Open Collection" is task-174's, no stub; Mods > Choose Mods Folder stays the webkitdirectory picker; and a new fix: read a picked mods folder lazily (the user's is 1 GB).

Fixes:
1. Loading: index.html has an opaque loading screen (title, bar, text) above the canvas that takes every click: the page's wasm is fetched as a stream with its progress ("Downloading... N%"), then "Starting", then the app's "Starting the audio" and "Drawing the panel... N%". On the web the part textures are rasterized 30 ms per frame (app::flush_slice; natively unchanged, all cores at once) and App::web_frame removes the screen and focuses the canvas once a complete frame has no part pending. Before, index.html removed its text as soon as start() returned, so the half-drawn panel and live menus showed (worse at the user's 20x CPU throttle). Checked under a busy main thread (90 ms of every 100 ms) in an iframe: the stages appeared in order and the screen went away when drawn.
2. Play did nothing: cause found. orb_player::web::WebOutput resumed the context from pointerdown/keydown/touchend listeners on the document in the bubbling phase; eframe's canvas handlers call stopPropagation, so a click on the panel or a key never reached them and the context stayed Suspended. My earlier checks passed only because Chrome already let that origin's context run (engagement). Reproduced: after load, context Suspended, a click on the panel's Play left it Suspended, frames 0. Fix: the listeners are registered in the capture phase (add_event_listener_with_callback_and_bool(.., true), removed the same way). Checks with the context suspended first (new orbSuspendAudio()): click on Play -> Running, playing, tick 83 -> 967 in 2 s; Space -> Running, playing, worklet quanta advancing, maxGapMs 6, playbackStats underrunEvents 0; a dropped song (Calm) and a song through the song picker input with its mod from the folder picker input (Fonda Workout, MSM2) both play, 0 underruns, also with a 40 ms of 50 ms busy main thread (frames +176128 in 4 s). DevTools CPU throttling cannot be set from the extension; the busy-thread runs stand in for it.
3. Shortcuts: Ctrl/Cmd+N, O, S now run Cmd::New / Cmd::Open / Cmd::Save (app.rs), so a shortcut does what its menu item does; on the web Cmd::Open is the picker. The menu item is "Open Local..." on the web (menu::OPEN_ITEM; the key help says so too). Checked: Cmd+O clicked the input orb-pick-song with navigator.userActivation.isActive true; no in-app browser opened.
4. Lazy mods folder: a picked folder's top-level .rbm files are not read; for each the page reads the 12-byte container header and the INFO chunk by File.slice (the last 1288 bytes, else the chunk headers one by one), keeps the File handle, and enters the file as lazy in orb_cli::fs (insert_lazy, is_lazy, lazy_info; read fails with WouldBlock until loaded). mods::scan and session::mod_info take the INFO from there. A mod is read whole when used: App::web_open_song (a picked or dropped song whose mod is lazy) and App::command (UseKit / ChooseVariant of a lazy mod) go through App::web_load_then, which reads it ("Reading NAME ...") and then acts. rbm::Info::from_body and rbm::INFO_SIZE are factored out of Rbm::parse (same fields). Nested folders are no longer kept (scan reads only the top level). Measured with ~/rb-mods/mods (326 files, 1086 MB, served read-only through a symlink in the scratch folder, given to the folder input): scan 214 ms, wasm memory 27 MB before and after; Mods menu lists them; orion.rbs (Orbit 2.0) opened: Orbit2.rbm read on demand, plays, 0 underruns, wasm memory 44 MB; a Mods menu choice of "707 Mod" read that file then, 48 MB.

Also: window.orbState() has the worklet's stats, Chrome's playbackStats, the loaded flag and the wasm memory size; docs/web.md 4, 7, 7.2 and docs/ui_spec.md 26 updated.

5. (same round) The web Options menu lacked the separator before Program Synth: the separator was dropped on wasm32 together with Audio Output... (my cfg). It is unconditional again, so the web menu's separators match the native menu's. Other menus checked in menu.rs: the only other web difference is the extra item Choose Mods Folder... in the Mods menu's group of Choose file... and Refresh List, no separator change; File, Edit, View and Help have no wasm32 cfg.

Checks after review round 1: cargo test --workspace 1200 passed, 0 failed, 6 ignored; cargo clippy --all-targets -D warnings clean; wasm32 clippy of orb-web (all targets) and of orb-player --features web clean; cargo xwin clippy aarch64-pc-windows-msvc clean; nightly fmt --check clean; scripts/build-web.sh builds. Criterion 2 stays unchecked for the user's re-check.

Review round 2 (user, Brave, on b4c36fe), 2026-11-27:
1. Skin switch half-drawn (View > Modern after a start in Classic): the modern parts were rasterized 30 ms per frame with nothing covering the panel. Fix in app.rs and web.rs. PendingTexture carries its key and `fresh` (the part has no rasterized texture of any size). flush_slice calls web::redraw_progress(fresh done, fresh left), which shows #orb-loading again ("Drawing the panel... N%") while fresh parts wait, then hides it. loaded() now hides the screen instead of removing it. A part whose new size is still waiting is drawn from its last rasterized size (PartTextures::ready), so a resize never shows gaps. Checked: overlay at 1% at 28896 ms, gone at 98% at 32148 ms (14 updates), then the panel is whole; View > Classic needs no overlay (bitmaps).
2. Save: no in-app dialog on the web. web::pick_save uses showSaveFilePicker where present (Chromium; Brave turns it off by default), else a download named after the song. App::web_target keeps the handle, so Save rewrites it. Cancel (AbortError) saves nothing.
3. Export: no in-app dialog. The picker suggests SONG.flac, else a download. FLAC only on the web: WebExport has no format field, and ExportFormat::OFFERED on wasm is [Flac]. Supervisor: task-170 records the narrowing (main bc0965c).
4. getImageData warning: the only caller is eframe 0.37's web glyph fallback (eframe/src/web/canvas_glyphs.rs:189, added in app_runner.rs:50); usvg's image.rs is not on this path. Fix: start() calls set_glyph_rasterizers(Vec::new()), so text uses the app's fonts only, as natively. Checked: console empty through load, Modern, Classic, menus, Save, Export.
Browser checks (Chrome 157, a stubbed showSaveFilePicker because the real one is a browser dialog):
- Save As asked for Untitled.rbs and wrote 16840 bytes; the title became Test.rbs.
- Cmd+S rewrote the same handle without asking.
- Export asked for Test.flac (audio/flac) and wrote 94814 bytes starting with fLaC.
- An AbortError stub saved nothing; the song stayed modified.
- With no picker: Save As downloaded Test.rbs; Export downloaded Test.flac, status "Exported Test.flac (downloaded)".
Docs: ui_spec.md section 26 (Loading, Save and Export with the per-browser behavior); web.md section 7 (Save and Export, Export, Redrawing, Text) and 7.3.
Checks: cargo test --workspace 1200 passed, 0 failed, 6 ignored. Clippy clean natively, on wasm32 (orb-web all targets, orb-player --features web) and on Windows aarch64 (xwin). Nightly fmt clean. Server stopped, tab closed. Criterion 2 stays unchecked for the user.

Round 3 (the user's check of 95ab9f1 in Brave on macOS, 2026-11-27):
- Brave has showSaveFilePicker, and the real save picker opened for Save As. Its macOS panel showed the name without the extension ("Untitled"), but the saved file kept .rbs. The coordinator first asked for a change to show the extension, then cancelled it: the user said the file keeps its extension, so nothing is needed. No code was changed for it.
- Chromium source read while it was open, for the record: content/browser/file_system_access/file_system_chooser.cc (ConvertAcceptsToFileTypeInfo sets keep_extension_visible) and components/remote_cocoa/app_shim/select_file_dialog_bridge.mm (extensionHidden = NO when keep_extension_visible). A test page on port 8341 opened one real picker in my own tab; I closed the tab, stopped the server and deleted the screen capture without reading it.
- The user accepted the skin switch's drawing progress, Save/Export through the browser, and the clean console: "all looks good". Criterion 2 is checked on that verdict, together with the earlier checks: Calm dropped plays with the built-in kit; Fonda Workout from the song picker plays with its mod from the picked folder; the page is served under /OpenRB-338/web/.
- Docs corrected: Brave gets the save picker, not downloads (docs/ui_spec.md 26, docs/web.md 7 and 7.3, the web.rs comment). Only a comment changed in code; fmt and wasm32 clippy are clean, and the round 2 cargo results stand (1200 passed).

Code review round (reviewer: send back, on 1a17822), 2026-11-27:
1. Stale save target. App::web_target outlived its document: after Save As to a.rbs, a song of the same name opened later saved into the earlier file's handle. Fix: App::doc_serial is a new number whenever settle() replaces the document (Open, New, a drop, the start), and not on a save. web_target is (serial, path, target), and save() and save_to_inner match both. Test: crates/orb-ui/tests/menu.rs the_document_number_changes_with_the_song_and_not_with_a_save (Open, Open of the same name in another folder, Open of the same file, New; Save As and Save keep it).
2. The lazy INFO bypassed the parse's checks. Fix: rbm::Walk, a sans-IO walk that asks for the bytes it needs (want/feed/finish) and does every check of the former Rbm::parse: the CAT header and size, each chunk header with the overrun check, the EMBF NUL-terminated name read in 256-byte steps, the 1280-byte INFO (the last one counts), and trailing bytes. Rbm::parse runs it over the whole file (Walk::run); web::mod_info_of runs it over File.slices. Test: crates/rbm/tests/walk.rs.
   - The former parse is kept there as the reference. 18 cases (11 damaged: empty, short, not a CAT, CAT size off by one, trailing bytes, trailing chunk header, chunk overrun, huge chunk size, EMBF without a NUL, empty EMBF, odd chunk without its pad) give the same chunks, INFO and warnings, or the same error, from Rbm::parse and from the walk fed only what it asks for.
   - On a valid 103 KB mod it reads 1836 bytes.
   The deviation sentence in docs/web.md is gone.
   Measured on ~/rb-mods/mods (326 mods, 1085550232 bytes; read only, served from a scratch folder on port 8342) in Chrome 157:
   - Mod by mod, the full walk took 81289 ms for 61164 slices (8.3 MB); each browser slice read costs about 1.3 ms.
   - Each read now takes at least 1024 bytes (READ_AHEAD), so a chunk header and its EMBF name are one read: 30356 slices, 31 MB.
   - Several mods are walked at once (PARALLEL_CHECKS): 32 at a time took 4483 ms, 128 at a time 3456 ms. 128 is kept.
   - The status line counts "Reading the mods folder NAME ... K of N"; the folder status came at 3.8 s.
   - The wasm memory was unchanged (30.7 to 32.5 MB). orion.rbs then read Orbit2.rbm lazily and loaded it (kit /Mods/Picked/Orbit2.rbm, 0 errors, 42.5 MB).
   - Before this fix, the tail-INFO shortcut took 214 ms without the checks.
3. Formatting: the long state() format string is split with concat!, and the stray line break before the "," after LOADED.get() is fixed.
4. The commit subject names player: ui,player,cli,engine,tools.
Minor:
- docs/ui_spec.md 26 no longer says "no memory": it gives the 3.5 s and about 30 MB, and that later Saves go to the picked file only for that song.
- The final summary now dates "all looks good" to round 3, the user's check of 95ab9f1.
Checks:
- cargo test --workspace: 1203 passed, 0 failed, 6 ignored (+2 walk, +1 menu).
- cargo clippy --all-targets clean; wasm32 clippy clean (orb-web all targets, orb-player --features web); xwin aarch64-pc-windows-msvc clippy clean.
- nightly fmt clean.
- The server on 8342 is stopped, the tab is closed, and the scratch symlinks are removed; ~/rb-mods was only read.

Corpus after the rbm::Walk change: orb compare --manifest tests/refs/manifest.json --mod-dir RECRYPTED (the four shipped mods), all sets: 175 of 175 EQUAL.

Re-review round (reviewer accepted all but one gap), 2026-11-27: the plugin's song load (crates/orb-ui/src/embed.rs, the self.doc replacement) did not bump doc_serial, against the comments in app.rs; it now does (self.doc_serial += 1), and the comments stand. The 18 mods of crates/rbm/tests/walk.rs are synthetic fixtures built in the test (11 damaged), not real mod files; said so in docs/web.md 7.4. Checks: cargo test -p orb-ui 497 passed, 0 failed; cargo clippy --all-targets clean; wasm32 clippy (orb-web) clean; nightly fmt clean.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
OpenRB-338 runs as a static web page: orb-ui on eframe's web runner, its audio in task-171's AudioWorklet. scripts/build-web.sh builds it into dist/web/ with relative URLs, so it works from any sub-path. Branch worktree-agent-a1feb1e606ca7b402, one commit. All five criteria are checked; criterion 2 on the user's verdict in round 3, the check of 95ab9f1 ("all looks good", Brave on macOS, 2026-11-27). A code review round followed (below).

Main changes:
- orb-cli: default features `cli` (clap, claxon, ctrlc; the orb binary requires it) and `ogg` (vorbis_rs). The library builds for wasm32 without them.
- orb_cli::fs: std::fs natively; on wasm32 a memory file system with lazy files (a picked mod's bytes are read only when the mod is used). encode::FlacSteps writes the same FLAC file, frame by frame.
- orb-engine: export::Export runs the export driver in steps, and export() / export_song() run through it. Corpus: 175 of 175 equal.
- rbm: Walk, the container walk of Rbm::parse as a machine that asks for the bytes it needs; the web checks a picked mods folder with it, so a mod gets the native result without its samples being read.
- orb-player: WebOutput listens for gestures in the capture phase (the fix for "Play does nothing").
- orb_ui::web:
  - startup, and a loading screen that also covers a skin's first drawing;
  - WebPort, with unapplied() used by Link;
  - pickers: File > Open Local... and Ctrl/Cmd+O open the browser's file picker, as do Mods > Choose file... and Choose Mods Folder... (the folder picker; its mods are checked lazily, 128 at a time, 326 mods in 3.5 s);
  - Save As and Export through the browser's save picker (showSaveFilePicker: Chromium browsers including Brave) or a download (Firefox, Safari), with no in-app dialog; the target is kept for its document only (App::doc_serial);
  - drops, settings in localStorage, the panic box with a console log, beforeunload while unsaved, the tab title, orbState() and orbSuspendAudio() for checks;
  - eframe's getImageData glyph fallback removed.
- Native-only: the Audio Output window, the cpal controller, rfd and arboard. Native behavior is unchanged.
- Web export: FLAC only (narrows task-170); renders and encodes 12 ms per frame and shows its progress.
- Shortcuts run their menu items' commands.

Reviews by the user in Brave:
- Round 1: loading screen; Play (the capture phase); Cmd+O as the menu; a lazy mods folder (326 mods, 1 GB, scanned in 214 ms at 27 MB); the Options separator.
- Round 2: a skin's first drawing behind the progress screen; Save and Export through the browser; FLAC only; a clean console.
- Round 3, the check of 95ab9f1: accepted ("all looks good"); the docs now say Brave has the save picker.

Code review round (send back), fixed: the save target is kept for its document only (tests/menu.rs); the lazy mod check is rbm::Walk, shared with Rbm::parse (tests/walk.rs, damaged mods); the formatting; the commit subject names player.

Checked in Chrome 157 under /OpenRB-338/web/:
- songs play from the startup state, by drop and by picker, with their mods; 0 underruns, also with a busy main thread;
- a save equals the native save byte for byte;
- exports equal orb render;
- the save picker path and its download fallback (stubbed);
- settings persist; ?debug-panic shows the error box.

Tests: cargo test --workspace 1203 passed. Clippy clean natively, on wasm32 and on Windows aarch64; nightly fmt clean. The CI wasm job builds and lints orb-web.

Docs: docs/building.md "The web page", docs/ui_spec.md section 26, docs/web.md sections 4, 7, 7.1 to 7.4, the AGENTS.md crate list.

To open it: scripts/build-web.sh --serve, then http://localhost:8338/OpenRB-338/web/.
<!-- SECTION:FINAL_SUMMARY:END -->
