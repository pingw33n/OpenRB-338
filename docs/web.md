# The web player's audio path

The web version (milestone m-6) plays songs live in the browser. This document describes how
the engine's audio reaches the browser (task-171), as the user decided in task-170 on
2026-11-27: path C, the engine in an AudioWorklet as its own wasm instance, the UI talking to
it with `postMessage`, no shared memory, the stable toolchain, no cross-origin isolation
headers. The native player (`orb-player` with cpal, `docs/building.md`) is unchanged.

The build (crates, scripts, the pinned wasm-bindgen-cli) is in `docs/building.md`, "The web
player".

## 1. Two instances

| Where | Module | What runs |
|---|---|---|
| The page's main thread | the page's module (wasm-bindgen): `crates/orb-web`, the UI (task-172, section 7), or the test page `crates/orb-web-demo` | `orb_player::remote::Player` and `orb_player::web::WebOutput` |
| The AudioWorklet (the browser's audio thread) | `crates/orb-worklet`: a plain wasm module with a C ABI, no imports | `orb_player::worklet::Host`, which owns a native `orb_player::Player` and its `Audio` |

The two share no memory: every message is an `ArrayBuffer` in the format of `orb_player::wire`
(a tag byte, then little-endian fields), transferred through the `AudioWorkletNode`'s
`MessagePort`.

The worklet's module is fetched by the page (`WebOutput::start(url)`) and passed to the node in
`processorOptions.wasm` as bytes; the processor script (`crates/orb-player/src/web/processor.js`,
embedded in orb-player and loaded from a Blob URL) compiles it with `WebAssembly.Module`
(falling back to the asynchronous `WebAssembly.instantiate`) and an empty import object. It
needs no wasm-bindgen glue in the worklet, where `TextDecoder` and other APIs are missing.

## 2. The UI half: `remote::Player`

It has the methods of the native `Player` that the UI's `Port` uses, with the same signatures:
`send`, `snapshot`, `tracks`, `collect_garbage`, `build`, `build_new`, `load`, `set_mod`,
`startup_sound`, `process`. It is `Send`: the transport (`remote::Transport`) of the web output
holds only an id, and the `MessagePort` lives in a thread-local of `orb_player::web` (wasm32
without atomics has one thread). The differences from the native player:

- **Load.** A `Command::Load(Box<Engine>)` cannot cross instances. `remote::Player::build` builds
  the engine on the UI side as natively (so the load's refusal, the tracks and the patterns are
  the UI's to read) and keeps its recipe, keyed by the process-unique build id the engine
  carries (`Engine::build_id`): the song's load image
  (`wire::LoadImage`: the form, the msg 36 globals, the msg 41 data blocks of the form, the
  tracks and the msg 37 batches, everything `Engine::load` reads, `orb_engine::song::LoadSource`)
  or "a new empty song", and the kit. Sending the engine sends the recipe plus the engine's four
  pattern arrays; the worklet builds the song with `Engine::load` and writes each pattern that
  differs with `Engine::pattern`. So an engine may be changed after it is built only in its
  patterns, which is what the UI does (the new song's template, `crates/orb-ui/src/template.rs`).
  Sending an engine the player did not build (or built more than `remote::RECIPES` builds
  before) panics, as the native player's process-globals assertion does.
- **Kits.** A `SampleSet` goes once as its 46 samples' audio (`wire::ToWorklet::Kit`), then by
  id; the player remembers the last `remote::KITS` it sent. `Command::Preview` of a kit's sample
  goes as kit and slot, any other sample as its audio. `InitSong` and `InitLoop` take a fresh
  `Pool` made in the worklet.
- **The queue** never refuses; `send` fails only for a data block or a pattern of the wrong
  length, as natively. `collect_garbage` returns 0: the worklet drops what its player lets go of.
- **The snapshot** is the last one the worklet sent; `snapshot()` and `tracks()` first take the
  messages that arrived. Before the worklet's first one, the snapshot is the one the native
  player starts with. `unapplied()` counts the messages the latest snapshot does not show yet
  (each snapshot carries the count of messages the worklet had taken before the engine call it
  shows). This is what the UI's "a snapshot newer than the commands" rule (`Link::sync`,
  `Port::follow_frames`) needs on the web, where the delay is not one engine call.

## 3. The worklet half: `worklet::Host`

Messages and `process()` run on the worklet's one thread, so the commands of the messages taken
before a render quantum are in the player's queue when it starts, and they apply at the start of
the next 1024-frame engine call, as natively. `process()` asks for 128-frame quanta; every eighth
one runs an engine call.

- **Samples.** At 44.1 kHz the engine's int16 samples go out as `x / 32768` in f32
  (`Audio::render_f32`), split into the node's two channels, unchanged.
- **Rate.** `WebOutput` makes the `AudioContext` with `sampleRate: 44100`. If a browser refuses
  that rate it makes one at the device rate, and the host resamples with the native output's
  resampler (rubato's sinc resampler, `crates/orb-player/src/resample.rs`, moved there from
  `output.rs` unchanged). Chrome 157 takes 44100.
- **Back to the UI.** After every engine call a snapshot (about 1 KB), and the tracks when their
  generation or revision changed. Once a second the processor script posts its own statistics
  (`{stats: {quanta, maxGapMs, dead}}`, not a wire message; `WebOutput::worklet_stats`).
- **Failures.** A malformed message makes the host send a wire `Error`
  (`remote::Player::take_errors`) and output silence from then on. A panic of the module is a
  trap (wasm32 aborts, nothing unwinds): the module's panic hook first keeps the message in a cell
  of its own (`orb_panic_len`, `orb_panic_ptr`, apart from the host's state, which the panicking
  call still holds), and the processor script reads it, sends it as a wire `Error` with the
  JavaScript error, and calls the module no more; the node outputs silence.
- **A song load** runs on the audio thread (the engine is built from its image in the
  worklet's message handler). The UI side's build of a shipped song took 1.0 ms in Chrome; the
  worklet's is the same work.

## 4. Starting and resuming

Browsers keep an `AudioContext` suspended until a user gesture. `WebOutput` listens to
`pointerdown`, `keydown` and `touchend` on the document, in the capture phase, and calls
`resume()` while the context is not running (in the capture phase because a page's own handlers
may stop the events: eframe stops its canvas's pointer and key events, so a bubbling listener never
heard a click on the panel, and Play stayed silent where the browser had not let the context
run already, task-172's review); it does the same on `visibilitychange` when the tab is visible again, in case
the browser suspended the context while the tab was hidden (Chrome does not; mobile browsers
may). The engine is not reset: playback goes on from where it stopped.

**The startup sound waits for the first gesture** (the user's report of 2026-11-27: in Firefox and
Safari the mod's startup sound played only after a mouse click). The sound is the engine's,
queued when the mod opens, so the suspended context plays it at the first resume: before the
first click or key press there can be no sound, in any browser, and the page adds no overlay.
`GESTURES` (`crates/orb-player/src/web/mod.rs`) now lists `pointerdown`, `pointerup`, `mousedown`,
`mouseup`, `click`, `keydown`, `keyup` and `touchend` (it had `pointerdown`, `keydown`, `touchend`),
all in the capture phase, so whichever event a browser counts as its user activation resumes the
context (Firefox counts `keydown` except Escape and `mousedown`/`pointerdown` of a mouse, Safari
`click`, `touchend` and `keydown`; a modifier key alone is no activation in either, and Escape none
in Firefox). `keydown` was listened to before, so a key press is expected to have started the audio
already; which of the events each browser accepted could not be checked by an agent (Firefox and
Safari are the user's).

## 5. Measured (Chrome 157, macOS, 2026-11-27)

With the test page (`scripts/web-demo.sh --serve`, `http://localhost:8338/`):

- Before a gesture the context is suspended; the first click plays. The context runs at
  44100 Hz.
- `AudioContext.playbackStats` over 55 s of playing, including the busy-thread tests: 0 underrun
  events. Average output latency 30.6 ms (`outputLatency` 32 ms, `baseLatency` 5.8 ms).
- A busy main thread (a 3 s spin; 40 ms spins every 50 ms for 5 s) does not interrupt the
  audio. The UI gets no snapshots while it is blocked (they arrive after) and cannot send.
- A command (tempo) shows in a snapshot 3.2 to 25.3 ms after it is sent (mean 13.4 ms over 20):
  the wait for the next engine call (up to 23.2 ms) plus the two messages. It is heard about
  the output latency later.

Not measured by the agent: Firefox, listening, and a real hide and show of the tab (the
automation could not hide the tab; the resume path was checked by suspending the context and
dispatching `visibilitychange`).

## 6. For the UI's web build (task-172)

- `Port` requires `Send`; `remote::Player` is `Send`. `Port::build` takes `&self`, as
  `remote::Player::build` does.
- The settings in localStorage and the WAV/FLAC export on the main thread (task-170) need
  nothing of this path: the export renders with its own engine (`orb_engine::frame::export`).
- `follow_frames` is not enough on the web (the message delay varies); use `unapplied()`.

## 7. The page (task-172)

`crates/orb-web` is the page's module; `index.html` (in that crate) loads it and calls its
`start(canvas, worklet_url)`, which is `orb_ui::web::start` (`crates/orb-ui/src/web.rs`). The
build is `scripts/build-web.sh` (`docs/building.md`, "The web page"); what the user sees differ
from the desktop app is in `docs/ui_spec.md` section 26.

- **Start.** eframe's `WebRunner` (its panic handler first), then orb-ui's panic hook, the
  settings from `localStorage`, `WebOutput::start` with the worklet's module URL (relative to
  the page), a `remote::Player` over its transport with the built-in kit, and a `Link` over it
  (`Link::with_port`, the port `WebPort`). Then the standalone app's `App` with the menu, as
  `main.rs` makes it, on a new empty song.
- **The port.** `WebPort` passes `remote::Player`'s methods through. `Port::follow_frames` is 0
  and `Port::unapplied` is the player's: the link takes the engine's values into the panel only
  from a snapshot that shows every command sent (`unapplied() == 0`), and the panel repaints
  while one does not (`Link::moving`). Natively `unapplied` is 0 and nothing changes.
  `Port::take_errors` gives the worklet's errors, which `App::check_audio` shows once.
- **Files.** `orb_cli::fs` is the file system of the library and the UI: `std::fs` natively, a
  memory file system on wasm32. The page puts there what the pickers give (`/Songs/<name>` for a
  song or a single mod, `/Mods/<folder>/...` for a picked mods folder, only its `.rbm` files)
  and what is dropped onto the canvas (eframe reads dropped files; a `.rbs` opens, a `.rbm` joins
  the Mods folder, `/Mods/Dropped` when none was picked). The pickers are `<input type=file>`
  elements kept in the page (`orb-pick-song`, `orb-pick-mod`, `orb-pick-mods-folder` with
  `webkitdirectory`), so a check can give them files: set the input's `files` from a
  `DataTransfer` and dispatch `change`.
- **Save and Export ask the browser** (the user's decision of 2026-11-27; no in-app save
  dialog on the web). `web::pick_save` calls `window.showSaveFilePicker` with the suggested
  name and the type (`.rbs`, or `.flac` as `audio/flac`) where the browser has it; the chosen
  `FileSystemFileHandle` comes back as `Picked::SaveTo` (an `AbortError`, the picker closed,
  as `Picked::SaveCancelled`: nothing is saved, a pending "save, then go on" is dropped; any
  other error falls back to the download). Without the picker the target is a download named
  after the song. `App::web_save_to` makes `/Songs/<name>` the song's path and keeps the target
  with the document's number (`App::web_target`, `App::doc_serial`: a new number whenever a
  song replaces the document, by Open, New, a drop, the start or a plugin's song load, and not
  on a save), so Save writes there again only for that document at that path; a song opened
  later, even of the same name, goes through Save As;
  `web::write_target` writes the bytes (`createWritable`, `write`, `close`) or downloads them
  (a Blob URL on a clicked link), and `Picked::Written` ends the save (the status line, then a
  pending New or Open goes on; a failure marks the song modified again and shows the error).
  Like the file pickers, `showSaveFilePicker` needs the user activation of the click or key
  press that ran the command.
- **Export** (task-170) runs on the page's thread after the target is chosen: `menu::WebExport`
  steps `orb_engine::frame::export::Export` (the export driver's calls, the same order on the
  same buffer as `export::export`, which is now built on it) for 12 ms per frame, then encodes
  FLAC by `orb_cli::encode::FlacSteps` (flacenc's single-threaded encoder, frame by frame; the
  same file as `encode`), 12 ms per frame. The web exports FLAC only (the user's decision of
  2026-11-27, narrowing task-170's WAV and FLAC). The status line shows the step and its
  percent.
- **Settings** are the TOML text of the native file under the `localStorage` key
  `/OpenRB-338/OpenRB-338.toml` (`settings.rs`: `read_text`, `write_atomic`).
- **A panic** goes to eframe's handler (`console.error` with the message and the stack), then to
  orb-ui's hook, which logs a line with the version and shows the page's error box
  (`#orb-error`) with the message and a Reload button. The module is dead after it (wasm32
  aborts). `?debug-panic` in the page's URL panics in the first frame (the box's check).
- **For checks.** `window.orbState()` (from the module's `state()`) returns what the last frame
  saw: the context's state and rate, the engine's frames, tick, play state and generation, the
  commands not shown yet, the song's kit, the status line, the number of errors, the worklet's
  statistics (`quanta`, `maxGapMs`, `dead`), Chrome's `playbackStats` (underruns) and whether
  the loading screen is gone. `window.orbSuspendAudio()` suspends the context, as a browser does
  before the first gesture, so a check can see the next click or key press resume it.
- **Loading.** `index.html` covers the page with an opaque loading screen (`#orb-loading`) that
  takes every click: it shows the download of the page's module with a bar (a streamed `fetch`
  of `orb_web_bg.wasm`, given to wasm-bindgen's `init`), then "Starting", then the app's steps
  ("Starting the audio", "Drawing the panel" with the share of the panel's parts rasterized).
  On the web the parts are rasterized 30 ms per frame (`app::flush_slice`; natively on all
  cores at once, unchanged). `App::web_frame` hides the screen and gives the canvas the
  keyboard once a complete frame has no part left to rasterize: no half-drawn panel and no menu
  before then.
- **Redrawing.** After the start, a part whose new size waits for its slice is drawn from the
  last size rasterized (`PartTextures::ready`), so a resize shows the old pixels scaled for a
  moment, never a gap. A part with no rasterized texture of any size (a skin never drawn: View >
  Modern after a start in Classic; or one whose textures were dropped after 120 unused frames)
  is "fresh": when a slice leaves fresh parts waiting, `web::redraw_progress` shows the loading
  screen again with "Drawing the panel... N%" until none waits. Fresh parts done within one
  slice show nothing, as the frame that asked for them is drawn whole. The classic skin's
  bitmaps are not rasterized and need no screen.
- **Text.** eframe adds a glyph rasterizer on the web that draws a glyph the fonts lack on a
  2D canvas and reads it back with `getImageData` (`eframe::web::canvas_glyphs`), which Chrome
  warns about ("Multiple readback operations using getImageData are faster with the
  willReadFrequently attribute set to true"). The page removes it
  (`egui::Context::set_glyph_rasterizers(Vec::new())`): the text uses the app's fonts only, as
  natively, and nothing on the page reads pixels back.
- **Open** (the user's decision of 2026-11-27, task-172's review). File > **Open...**
  and Ctrl/Cmd+O open the browser's file picker directly (`web::pick`, no in-app browser on the
  web for local files), as do Mods > Choose File... and Mods > Choose Mods Folder... (the folder
  picker). Ctrl/Cmd+N, O and S run their menu items' commands (`Cmd::New`, `Cmd::Open`,
  `Cmd::Save`), so a shortcut does what its item does; before the review Ctrl/Cmd+O opened the
  in-app browser while the menu item opened the picker. Chromium and Firefox count the keydown
  as the user activation a picker needs; the picker opens in the frame after it, within the
  activation's time (checked: `navigator.userActivation.isActive` was true at the input's
  click). The collections are File > Open from Collection... and Mods > Use Collection (section 9).
- **The mods folder is read lazily.** A picked mods folder can be large (the user's is 326 mods,
  1 GB). For each `.rbm` at its top the page runs `rbm::Walk`, the walk `Rbm::parse` itself
  runs, over `File.slice`s of the file: the CAT header, every chunk header (an overrun fails),
  the start of every `EMBF` up to its name's NUL, every 1280-byte INFO body, and the trailing
  bytes check, but not the samples. So a mod gets the same result as natively: a file the
  parse refuses has no name and matches no song (`tests/walk.rs` in `rbm` compares the walk,
  fed only what it asks for, with the former parse on damaged mods). Each read takes at least
  1024 bytes, so a chunk header and the `EMBF` name after it are one read, and 128 mods are
  walked at once (`PARALLEL_CHECKS`), with "Reading the mods folder NAME ... K of N" in the
  status line. The page keeps the browser's `File` handle and enters the file in `orb_cli::fs`
  as lazy (`insert_lazy`: listed, its size, its INFO for `mods::scan` and `session::mod_info`;
  `read` fails until the bytes are there). A mod's bytes are read when it is used: a song
  opened whose mod it is (`App::web_open_song`), a Mods menu choice of it (`App::command` defers
  `UseKit` and `ChooseVariant`), each through `App::web_load_then` ("Reading NAME ..." in the
  status line), and then kept.

### 7.1 Checked (Chrome 157, macOS, 2026-11-27)

Served from a local static server under `/OpenRB-338/web/`, with the shipped songs and the four
recrypted shipped mods as fixtures from the same server, given through drops and the pickers'
inputs:

- The panel shows and scales to the window (two window sizes); before a gesture the context
  is suspended, then it runs at 44100 Hz.
- Calm (built-in kit), dropped: Play plays (frames and tick advance, the bar display counts).
  Fonda Workout opened through the song picker after the four mods were given to the folder
  picker: its kit is `/Mods/Picked/MSM2.rbm` (found by its INFO name), the panel shows the MSM
  splash, Play plays.
- Save of Fonda Workout after Options > Vintage: the downloaded file (20342 bytes, SHA-256
  `75438b1b...`) is the native app's save of the same edit byte for byte (the headless app,
  `App::save_to`); dropped back and saved again, it equals the native re-save (`abbc1256...`).
- Export of Calm (5 min 18 s): WAV and FLAC download, equal to `orb render`'s files
  (`54560491...`, 56013828 bytes; `01991887...`, 19594661 bytes). FLAC export took 7.2 s; the
  longest frame during it was 70 ms (before the encoding was stepped: one 5.8 s frame).
- Settings (theme, Mods folder) were there after a reload. `?debug-panic`: the error box, the
  console has the message, the stack and the version line.

### 7.2 The user's review (Brave, 2026-11-27) and its fixes, checked in Chrome 157

- **Play did nothing** (no sound, the steps stood): the context stayed suspended. `WebOutput`
  listened for the gestures on the document in the bubbling phase, and eframe stops the canvas's
  pointer and key events, so no click on the panel and no key reached it; the agent's first
  checks passed only because Chrome had let that origin's context run already. Reproduced
  (context `Suspended` after a click on Play), fixed by listening in the capture phase (section
  4). After the fix, with the context suspended first (`orbSuspendAudio()`): a click on Play,
  and Space, each resumed it and played (the startup song, a dropped song, a song through the
  song picker with its mod from the folder picker); the worklet's quanta advanced, its longest
  gap 6 ms, `playbackStats` 0 underruns, also with the main thread busy 40 ms of every 50 ms.
- **Loading**: the loading screen and the sliced rasterizing of section 7 (before, `index.html`
  removed its text when `start` returned, and the half-drawn panel and the menu showed).
- **Open**: File > Open... and Ctrl/Cmd+O both open the browser's picker (Ctrl/Cmd+O
  opened the in-app browser before).
- **A 1 GB mods folder** (326 mods, `~/rb-mods/mods`, given to the folder picker's input): the
  INFOs were read in 214 ms, the wasm memory stayed at 27 MB; the Mods menu listed them; a song
  whose mod is Orbit 2.0 read that mod (5 MB) on opening and played (wasm memory 44 MB, with the
  classic skin's bitmaps of the mod); a Mods menu choice of another mod read it then (48 MB).
- **Separators**: the Options menu's separator before Program Synth... is back on the web, and
  every menu's separators are the native menu's.

### 7.3 The user's second review (Brave, 2026-11-27), checked in Chrome 157

- **A half-drawn panel after View > Modern** in a page started in Classic: the modern parts
  were rasterized 30 ms a frame with no cover. Now the loading screen shows "Drawing the
  panel... N%" (1% to 98% over 3.2 s, 14 updates), then the panel is whole. View > Classic
  needs no screen (bitmaps). Resizes draw the last size meanwhile ("Redrawing" in section 7).
- **Save and Export without an in-app dialog**: checked with `showSaveFilePicker` replaced by a
  stub that returns a handle recording what is written (the real picker is a browser dialog a
  check cannot answer): Save As of a new song asked for `Untitled.rbs` with the `.rbs` type,
  wrote 16840 bytes and closed the file, and the title became the chosen name; Cmd+S then wrote
  the same handle again without asking; Export asked for `Test.flac` (`audio/flac`) and wrote
  94814 bytes starting with `fLaC`. A stub that rejects with `AbortError` saved nothing and the
  song stayed modified. With no `showSaveFilePicker` (as Firefox and Safari) Save As and
  Export downloaded `Test.rbs` and `Test.flac`.
- **The user's check of 95ab9f1, round 3** (Brave on macOS, 2026-11-27): Brave has
  `showSaveFilePicker`; the real save picker opened for Save As and the saved file kept its
  extension (the macOS panel showed the name without it). The skin switch's drawing progress,
  Save and Export, and the clean console were accepted: "all looks good".
- **The getImageData warning** came from eframe's glyph fallback ("Text" in section 7). After
  its removal the console was empty through a load, a switch to Modern and back to Classic,
  the menus, Save and Export.

### 7.4 The code review of task-172 (2026-11-27)

- **A kept save target outlived its document**: after Save As to a file, a song of the same
  name opened later saved into that file's handle without asking. The target is now kept with
  the document's number (section 7, "Save and Export ask the browser"; the plugin's song load
  takes a new number too); `tests/menu.rs` in orb-ui checks that the number changes on Open
  (also of the same name or the same file) and New, and not on Save As or Save.
- **The lazy INFO skipped the parse's checks** (a tail shortcut and a header walk without the
  overrun, `EMBF` name and trailing checks): now `rbm::Walk`, shared with `Rbm::parse`.
  `tests/walk.rs` checks it on 18 synthetic mods built in the test (11 of them damaged), not
  on real mod files. On the user's 326 mods (1 GB), from `~/rb-mods/mods`, read only, in
  Chrome 157: 30356 reads of
  31 MB in all, 3.5 s (81 s one by one; 4.5 s 32 at a time; 3.5 s 128 at a time), the wasm
  memory unchanged (31 MB); the song `orion.rbs` then read and played its mod `Orbit2.rbm`.
  Before, with the shortcut, the scan took 0.2 s.

### 7.5 wasm SIMD and wasm-opt (task-323, Chrome 157, macOS, 2026-11-27)

The page's module is built with `-C target-feature=+simd128` and optimized by binaryen
`wasm-opt -O3 -g --enable-simd` (version_133, `scripts/binaryen.sh`; flags in `docs/building.md`
"The web page"). The worklet's module is built without the flag and is byte for byte the same.
Measured in one Chrome tab at one window size on localhost, the page started in Classic
(`theme = "classic"` in localStorage), then View > Modern; the time is the loading screen of
"Drawing the panel" from its first update to its hiding (a MutationObserver on `#orb-loading`):

| | before | after |
|---|---|---|
| Modern first drawing, runs | 3334, 3363, 3357 ms | 2167, 2129, 2111 ms (a first run after the new page's first load: 5278 ms, not repeated) |
| median | 3357 ms | 2129 ms (median of all four: 2148 ms) |
| start with Modern (navigation to the screen hidden) | 3355, 3417 ms | 2394, 2473, 2748 ms |
| `orb_web_bg.wasm` raw | 19,121,175 bytes | 16,883,373 bytes |
| gzip -9 | 9,222,400 bytes | 8,967,038 bytes |
| name section / named functions | 2.47 MB / 16105 | 1.33 MB / 9178 |
| `orb_worklet.wasm` | 320,382 | 320,382 (identical) |

The name section is kept by `-g`: 16,105 named functions unoptimized, 9,178 with wasm-opt.
The two changes were not measured apart. The classic start (no drawing) was not timed.

### 7.6 The visitor count (task-325)

`index.html` carries the GoatCounter script (`data-goatcounter` =
`https://openrb338-web.goatcounter.com/count`, `async src="https://gc.zgo.at/count.js"`): page
views only, cookieless, in the web app's own dashboard https://openrb338-web.goatcounter.com
(the landing page counts to https://openrb338.goatcounter.com, docs/building.md). It is the
page's only third-party request besides raw.githubusercontent.com (the collections). It loads
async in the `<head>`, so it does not hold up the loading screen; if it fails (offline,
blocked, localhost) nothing else changes.

## 8. The collection index: `orb web-index` (task-173)

The page cannot list a GitHub repository (60 unauthenticated API requests an hour) or read the
mods to find one by its INFO name (326 mods, about 1 GB). The deploy runs, once per release,

    orb web-index --songs rb-songs/songs --mods rb-mods/mods \
        --songs-commit SHA --mods-commit SHA -o index.json

and the page fetches `index.json`, then only the song and the mod it needs, from `raw_base` +
`path` (raw.githubusercontent.com sends `Access-Control-Allow-Origin: *`; a path is the file's
name as git stores it, so the page percent-encodes it). The subcommand is a developer tool,
hidden from `orb --help` (task-137). Code: `crates/orb-cli/src/webindex.rs`; test:
`crates/orb-cli/tests/web_index.rs`.

**The output is deterministic**: compact JSON on one line, tables sorted by the bytes of the
path, no timestamp, one trailing newline; the same files and commits give the same bytes. A path
is relative to the repository root: it starts with the name of the folder given (`songs/...`,
`mods/...`), `.rbs` and `.rbm` in any letter case, `/` separated, UTF-8 (a name that is not fails
the run). Songs are found in the folder and all the folders below it; mods in the top level of
the folder only, as `orb_cli::mods::scan` and the web page read a mod folder (a mod in a
subfolder is not indexed and never matches). A file that cannot be read does not stop the run:
it is listed with `size` and `sha256` null and the read error.

### 8.1 Format, version 1

The page downloads the file, so it is compact: each table has its `columns` once and its rows as
arrays (no key names per entry), folders and mods are numbers, a hash is base64url. Top level:

| Key | Value |
|---|---|
| `format` | `"orb-web-index"` |
| `version` | `1`; a change of a key's meaning, or a removed key or column, raises it; a column added at the end does not |
| `songs_repo`, `mods_repo` | `{name, commit, raw_base}`: `name` is `"rb-songs"` / `"rb-mods"`, `commit` the `--songs-commit` / `--mods-commit` text or null, `raw_base` `https://raw.githubusercontent.com/pingw33n/<name>/<commit>/` (pinned to the commit, so a later push never mismatches the index) or null without the commit |
| `mods_dir` | the mods' folder in `rb-mods` (`"mods"`) |
| `song_dirs` | the songs' folders, sorted, each relative to the repository root (`"songs"`, `"songs/Some/Folder"`) |
| `song_columns`, `songs` | the columns of a song row, and the rows, sorted by folder path then file name |
| `mod_columns`, `mods` | the columns of a mod row, and the rows, sorted by file name |

A **song row** has the columns `dir`, `file`, `size`, `sha256`, `error`, `format`, `version`,
`title`, `mod_name`, `tempo_bpm`, `last_event_bar`, `mod_variants`. The song's path is
`song_dirs[dir] + "/" + file`.
- `size` (bytes) and `sha256` (the file's SHA-256, base64url without padding, 43 characters; the
  alphabet is `A-Z a-z 0-9 - _`).
- `error`: null for a song orb reads; else why it cannot (the text `orb render` gives, or the
  read error), and every column after it is null.
- The facts are those of `orb info --json` (`SongFacts`, so the page and the CLI agree), only the
  ones the browser shows: `format` (`"rbs"` or `"rb1x"`), `version`, `title`, `mod_name`,
  `tempo_bpm`, `last_event_bar` (1-based bar of the latest track event, null for a song with no
  events). The rest of `orb info` (text, web page, copyright, loop, shuffle and so on) is read
  from the song when it is opened.
- `mod_variants`: the indexes in `mods` of every mod whose INFO name is `mod_name`, in orb's
  order; the first is the mod `orb render` loads. The built-in kit is `mod_name` equal to
  `"Standard ReBirth"` exactly (the song then has no variants); any other song with no variants
  names a mod that is not in the collection.

A **mod row** has the columns `file`, `size`, `sha256`, `name`, `recrypted`, `error`. The mod's
path is `mods_dir + "/" + file`.
- `size`, `sha256`: as for a song.
- `name`: its INFO menu name, null when the file does not parse or has no INFO, which is when
  `Rbm::parse` and `info()` give none, `orb_cli::mods::scan`'s rule and the web page's.
- `recrypted`: 1 when the embedded key bytes cover every sample slot (the test of orb-ui's
  `ModStatus`, docs/rbm.md 12.4), else 0 (an unrecrypted mod, a file that does not parse).
- `error`: null, or why the file does not parse or cannot be read.

### 8.2 The mod lookup rule

Unchanged from task-145, task-150 and task-151: a song's mod is found by its INFO name only,
compared ignoring ASCII case (`mods::names_match`), never by file name. The mods that match are
`mod_variants`, in the byte order of their file names (`mods::byte_order`, `AXIOM.rbm` before
`axiom(b3).rbm`), and the first is the default. A mod that is not recrypted takes part like any
other: it is found, and fails when its samples are loaded; the page marks it with `recrypted`.
`tests/web_index.rs` compares `mod_variants` with `mods::scan` and `mods::find_in_dir` over the
fixture's mod folder.

### 8.3 Measured (2026-11-27, macOS, release build)

On `~/rb-songs/songs` (commit `6e0b92e`) and `~/rb-mods/mods` (commit `bfbd8c9`): 5685 songs in
249 folders, 326 mods, 4.4 to 5.3 s (one thread; the cost is reading and hashing 1.2 GB and
parsing 326 mods and 5685 songs). 0 unreadable songs, 0 unrecrypted mods, every mod has a name;
2526 songs use the built-in kit, 3055 resolve to a mod file (33 of them to one of several
variants) and 104 name a mod that is not in rb-mods.

The size of the file, which the page downloads (GitHub Pages serves gzip):

| Format | Raw | gzip -9 | brotli |
|---|---|---|---|
| first version: objects per entry, hex hashes, every `orb info --json` fact, paths in full | 4,754,036 | 936,065 | 656,626 |
| this format (compact JSON, base64url) | 910,189 | 400,938 | 351,897 |
| this format with hex hashes | 1,042,739 | 440,065 | 353,460 |
| the same tables as tab-separated text | 782,256 | 392,874 | 348,292 |

Where the first version's bytes went (gzip, 5685 songs): the song text 278 KB, the hashes 217 KB,
the paths 61 KB, the title 42 KB, the web page 25 KB. The text and the other facts the browser
does not list are gone (the page gets them from the song it opens), and the hashes are the floor:
32 random bytes a file, 5685 + 326 files, about 190 KB however they are written. Tab-separated
text saves 2 % over the compact JSON and needs its own escaping and a hand parser (a title may
hold tabs or newlines, a list of mods has no cell), so the index stays JSON, which the page
already parses with serde.

## 9. The collections on the page (task-174)

The page offers the songs of rb-songs and the mods of rb-mods (the user's design of
2026-11-27: two entry points, File > Open... for the computer's files and File > Open
Collection... for the song collection; the mod collection is the Mods folder of a page load,
and Mods > Use Collection tells and switches it).
What the user sees is in `docs/ui_spec.md` section 26. Code: `crates/orb-ui/src/collection.rs`
(the index, the checks and the cache record; it builds on every target and its tests run
natively) and the section "the collections" of `crates/orb-ui/src/web.rs`.

- **The index.** At the start the page fetches `index.json` next to it (a relative URL, with the
  request's cache mode `no-cache`, so a deploy's new index is seen; `scripts/build-web.sh --index
  FILE` puts it there) and reads it with `collection::Index::parse` (format 1, the columns looked
  up by name). Every song and mod with a hash becomes a **lazy file** of the memory file system
  (`orb_cli::fs::insert_lazy`, section 7, "The mods folder is read lazily"): `/rb-songs/<path>`
  and `/rb-mods/<path>`, with the size, and for a mod an INFO holding its index name. So the
  in-app browser lists the folders of `/rb-songs/songs`, and `orb_cli::mods::scan` (the Mods
  menu, `find_preferring`, the remembered variant preference of task-151) works on
  `/rb-mods/mods` as on any mods folder, without a byte of a mod. The page does not read the
  index's `mod_variants`: the lookup is orb's own over the same names, which `mod_variants`
  equal (section 8.2, task-173's test). A failed index (none deployed, offline) is logged to the
  console; File > Open from Collection... then shows the error and asks for the index again.
- **The mods source.** At the start `settings.mods_dir` is `/rb-mods/mods` for the session,
  whatever `localStorage` holds; Mods > Choose Mods Folder... replaces it with a picked folder
  (section 7) for the session. Mods > Use Collection is checked while `mods_dir` is
  `/rb-mods/mods` (`collection::is_mods_folder`, `App::uses_collection`); `Cmd::UseCollection`
  then opens the folder picker (`web::pick(Pick::ModsFolder)`, so a cancel changes nothing),
  and when unchecked it sets `mods_dir` back (`App::web_use_collection`) and reads the Mods
  menu again. The picked folder's files stay in memory (the song may play one of them).
- **The in-app browser on the web** is reached only by File > Open from Collection... (Ctrl/Cmd+Shift+O,
  task-324; browser check in 9.3); its sidebar
  has the one place "Song collection" (`Browser::sidebar_body` on wasm32), no other places, no
  recent folders and no "Use as ... folder" buttons (the user's review of 2026-11-27).
- **Opening a song** (`App::web_open_song`: a song of the song collection, picked or dropped)
  asks about unsaved changes first, as natively, so a cancel downloads nothing; after Save or
  Don't Save, `App::web_fetch_and_open` downloads the song and its mod, then opens the song
  without asking again (`Picked::OpenNow`; the prompt's `Then::Open` goes there on the web).
- **A lazy file's source** is a picked `File` (a mods folder) or a collection file. When a song
  or the Mods menu needs a collection file (`App::web_load_then`: a song opened from the
  browser, its mod, a Mods menu choice), it is fetched (`fetch_into`), written
  into the memory file system, and then the waiting action runs (a song: download the song,
  then its mod, then open it).
- **The download.** `fetch_file` takes the file from the page's cache when it holds it and the
  bytes pass the check, else fetches its raw URL (`raw_base` + the path percent-encoded except
  the unreserved characters and `/`) with an `AbortController` (Cancel), reads the body as a
  stream for the dialog's progress, and checks the HTTP status (404: the collection changed since
  the index), a network failure (`navigator.onLine` tells "offline"), then the size and the
  SHA-256 against the index (`collection::File::check`). A file that fails is not used: the
  error stays in the download dialog, and nothing else changes (the song that plays plays on).
  A cancelled download or one replaced by the next is dropped when it ends.
- **The cache** (the Cache API, cache `orb-collection`; secure contexts only, so `localhost`, a
  `127.0.0.1` server or GitHub Pages; without it every use downloads). The key is a page-relative
  URL `orb-cache/sha256/<the index's base64url hash>`, so a file is found by its content whatever
  its path. **The cap** (the user's addition of 2026-11-27): `collection::CACHE_CAP`, 256 MB
  (256 x 2^20 bytes). The Cache API keeps no sizes or use times, so the page keeps a record in
  `localStorage` (`/OpenRB-338/cache-lru.json`: per key the size and the tick of the last use;
  `collection::CacheLru`). A hit and a store are uses (`CacheLru::admit`): before a store the least
  recently used files are deleted until the new file fits under the cap, and a file larger than
  the cap is not stored. A failed store (the browser's quota) is like any cache failure: the key
  leaves the record, the file opens from memory, no error is shown. The page requests no
  persistent storage (the user's choice), so the browser may evict the whole cache; the record
  then names files that are gone, which cost only their share of the cap until they are evicted
  from the record.
- **For checks.** `orbState()` adds `collection` (`"loading"`, `{songs, mods}` or `{error}`),
  `download` (name, done, total, error, or null), `cacheHits`, `downloaded` and `cancelled`.

### 9.1 Checked (Chrome, macOS, 2026-11-27)

Built with `scripts/build-web.sh --index` (rb-songs `6e0b92e`, rb-mods `bfbd8c9`, 910,189
bytes), served under `/OpenRB-338/web/` on a local port, the cache and its record cleared
first:

- The index: 5685 songs, 326 mods. Open from Collection... lists `/rb-songs/songs`; typing "orion"
  selects `orion.rbs`; Enter shows "Downloading orion.rbs", then the mod; the kit is
  `/rb-mods/mods/Orbit2.rbm` (5,064,616 bytes) and Play plays (0 underruns). The record:
  `orion.rbs` 28,092 bytes at tick 1, `Orbit2.rbm` at tick 2.
- After a reload, the same song: 2 cache hits, 0 downloads, and no request to
  raw.githubusercontent.com in the network panel.
- A mod chosen from the collection (then by Mods > Mod Collection..., replaced in the review
  below; the Mods menu downloads the same way): AXIOM.rbm, one request,
  `https://raw.githubusercontent.com/pingw33n/rb-mods/bfbd8c9.../mods/AXIOM.rbm`, 200.
- With the raw URLs answered by a stub while `orion.rbs` played: a 404, a rejected fetch with
  the browser offline, a short file, a file of the right size with another hash, and a slow
  stream cancelled each left the song playing; the first four showed their message in the
  dialog, the cancel "The download of orbitlife.rbs was cancelled" in the status line.
- A mods folder picked after the start (one `Orbit2.rbm`) takes precedence: `orion.rbs` then
  plays `/Mods/Picked/Orbit2.rbm`.

### 9.2 The user's review of dbf6037 (2026-11-27), checked in Chrome

- Mods > Mod Collection... is gone; **Use Collection** is a check item before Choose Mods
  Folder... Checked on a page load. Clicked checked: the folder picker's input was clicked (the
  check recorded it) and, with no choice, the item stayed checked. A folder given to the input
  (`CaveyRageRemix.rbm`): unchecked, the Mods menu lists that one mod. Clicked again: checked,
  "Mods from the collection: 317 mods" (326 files, 317 INFO names) and the collection's list.
- File > **Open...**; Ctrl/Cmd+O clicks the song picker's input.
- File > Open from Collection...: the sidebar shows "Song collection" only.
- The user's last changes (2026-11-27, not reviewed in a browser by the user, who approved the
  round-1 build): Use Collection moved before Choose File..., so the web's order is Use
  Collection, Choose File..., Choose Mods Folder...; "Choose file..." is "Choose File..." (Title
  Case) natively too.

### 9.3 Ctrl/Cmd+Shift+O (task-324)

File > Open from Collection... has the shortcut Cmd+Shift+O (Ctrl+Shift+O elsewhere). egui's
logical shortcut matching ignores an extra Shift, so Cmd+O (`shortcuts::OPEN`) used to match
Cmd+Shift+O as well (the user saw the file picker in Firefox); `shortcuts::consume` now refuses
a shortcut without Shift while Shift is held, and the app checks `OPEN_COLLECTION` first.

Checked in Chrome 2026-11-27 (macOS, a local build of the page without an index, so Open
Collection... shows its "index.json: HTTP 404" error box, which proves the command ran):
Cmd+Shift+O raised that box and clicked no file input; Cmd+O clicked the song input (`.rbs`)
and no box. The keys were sent by the extension's key action, so the check covers what the page
receives, not the browser's own menu bindings (Chrome has Cmd+Shift+O as Bookmark Manager; the
page's `preventDefault` is what must stop it, which a synthetic key cannot show). Firefox and
Safari: not checked by an agent. The user then checked the page served from the branch and a
macOS .app built from it and approved (2026-11-27: "324 is good"); the per-browser detail was
not reported, so no Firefox or Safari result is recorded here.

## 9.4 Random song, links, title, icons and ?skin= (task-326)

The user's requests of 2026-11-27 for the page. Code: `crates/orb-ui/src/web.rs` (the sections
"the address bar and the History API" and "the collections"), `collection.rs` (`Song::can_open`,
`Index::random_song`, `Link`, `with_skin`), `settings.rs` (`Theme::from_query`).

- **Title.** The page's title is `<song> - OpenRB-338`, `<song>* - OpenRB-338` when modified
  (`App::page_title_text`); the native window's stays `OpenRB-338 - <song>` (`App::title_text`).
- **Icons.** `index.html` has the landing page's three tags (`site/index.html`): `img/favicon.svg`,
  `img/favicon-32.png` (32x32) and `img/apple-touch-icon.png`, relative to the page, so any
  sub-path works. `scripts/build-web.sh` copies the three files from `site/img/` into
  `dist/web/img/`; `favicon.ico` is gone.
- **File > Open from Collection (Random)** (`Cmd::OpenRandom`, `App::web_open_random`; the former
  "Open Collection..." is now "Open from Collection..."). It picks uniformly among the songs
  `Song::can_open` accepts by what the index says, with no download: no read error, the hash
  and size present, and the mod resolves: the built-in kit (`mod_name` "Standard ReBirth"), or a
  non-empty `mod_variants`; with a mods folder of the computer in use (the Mods folder is not
  `/rb-mods/mods`), by that folder's lookup (`App::resolve_mod_file`, once per mod name). It
  avoids the song open (unless it is the only one) and then opens it as the browser's choice
  does (`web_open_song`: the unsaved prompt first, downloads, errors in the dialog).
  In the 2026-11-27 index 104 of 5830 songs name a mod the collection lacks and are never picked.
- **The shortcut is Cmd+Shift+Y (Ctrl+Shift+Y elsewhere)**, not the requested Cmd/Ctrl+Option/Alt+O.
  Reasons: on macOS Option+O types a character (the key is "o with stroke"; eframe
  falls back to the physical key, `eframe/src/web/events.rs`, so egui would still see O, but a
  browser or layout may act on the typed character), and Ctrl+Alt is AltGr on Windows and Linux
  layouts (a Polish AltGr+O types "o acute", a plain character that would open a random song).
  A shortcut without Alt cannot have either problem. Y: no binding in Chrome or Safari; Firefox
  (Downloads) and Edge (Collections) bind Ctrl+Shift+Y, and the page asks them to leave it with
  `preventDefault` (`index.html`, only with Shift held, so a plain Ctrl+Y stays the browser's),
  as it does Ctrl+Shift+O (a browser's reserved keys,
  such as Ctrl/Cmd+Shift+R, N, T, W and Q, were avoided). Natively the key is taken and does
  nothing, as Cmd+Shift+O. `shortcuts::consume` is now exact about Alt too (a shortcut without
  Alt is not taken while Alt is held), so Cmd+Alt+O is never Cmd+O. The item shows in the menu and
  in Help > Keyboard Shortcuts on the web only.
- **Links.** `?song=<path in rb-songs, as the index lists it>` (`songs/Folder/Song.rbs`,
  percent-encoded, `/` kept) and `?mod=<path in rb-mods>` (`mods/Mod.rbm`), by themselves or
  together (`collection::Link`; the user's change of 2026-11-27 replaced `?mod-demo=`, which never
  shipped):
  - `?song=S` opens the song with the mod it resolves to;
  - `?song=S&mod=M` opens the song with the mod M of the collection instead (the song opens, M is
    read, then `use_kit`; the song stays unmodified, as the link chose the mod);
  - `?mod=M` alone opens the demo song of M, what Mods > Demo Song opens (`App::demo_song`).

  The index's paths, not raw URLs, so a link survives new commits of the collections.
  - **The address follows the document** (`App::web_nav_frame`, every frame, on the History
    API): when a collection song or a collection mod's demo becomes the document, the link is
    **pushed** (`pushState`, state `{n}` = the entry's number), so Back and Forward go between the
    songs; a local file, a drop, New: the link is removed with `replaceState`, so the bar never
    names a song that is not open. The link is set when the song is open, not when it is asked
    for.
  - **Back and Forward** (`popstate`, `Picked::Popstate`) open the entry's song through the
    normal path (`web_open_link`: the unsaved prompt first for a song); an entry with no link
    is File > New; the entry whose song is open already does nothing. When the song does not open
    (the prompt cancelled, a download failed, a message shown) the browser is sent back to the
    entry it left with `history.go(delta)` (the numbers `n` give the direction), so the bar stays
    true. A `?mod=` link (a demo) downloads the mod first and asks the unsaved question after
    (`Then::Demo`), unlike a song link, which asks first.
  - **The link follows a mod change.** When the mod of an open collection song changes (a Mods
    menu choice, a collection mod chosen, a variant), `&mod=<path>` is set with `replaceState`
    (no history entry), so the link reproduces the song with that mod; when the mod is again the
    one the song resolves to by itself (the first of the index's `mod_variants`, what a link to
    the song opens), `&mod=` is removed. A **mod of the computer** (Choose File... from disk, a
    picked mods folder) has no collection path, so such a link cannot carry it: `mod=` is
    dropped and the link opens the song with its own mod; likewise for the built-in kit chosen
    for a song that resolves to a mod. A demo song's link stays `?mod=M` whatever the kit does.
  - **Loading a link** opens the song once the index is read (`App::web_collection_read`). A
    path the index does not list, a song or mod it could not read, a mod with no demo song, or
    an index that failed to load shows a message in the error box, the link is removed and the
    page is usable.
- **`?skin=classic` / `?skin=modern`.** The page starts in that skin as if View had chosen it, for
  that load only: `Settings::set_theme_for_session` sets the theme and its base, so no save
  writes it; the saved `theme` changes only when the user switches in View. A value that is
  neither is ignored with a console warning. The address bar always names the skin shown: on
  load `?skin=` is set (replaceState) to the actual skin, a View switch replaces it (no history
  entry), and every `?song=` / `?mod=` entry carries it. Parsed by `Theme::from_query`,
  written by `collection::with_skin` (other parameters, as `?debug-panic`, are kept).
- **Tests.** `collection.rs` (`random_picks_only_songs_that_can_open_and_not_the_open_one`,
  `links_parse_and_write`, `the_index_finds_a_song_and_a_mod_by_path`), `settings.rs`
  (`the_skin_parameter_is_read`), `shortcuts.rs` (`an_extra_alt_or_shift_is_not_the_plain_shortcut`,
  `open_random_is_shift_y`), `tests/menu.rs` (`alt_is_exact_and_the_page_title_has_the_song_first`).

**Known limitations** (task-326):
- A Save chosen at the unsaved-changes prompt that then fails does not restore the address bar
  after a Back or Forward.
- A `?mod=` link (a mod's demo song) asks the unsaved-changes question after the mod download,
  not before (`Then::Demo` needs the mod read); a song link asks first.
- A mod of the computer, and the built-in kit chosen for a song that resolves to a mod, cannot
  be named by a link (`&mod=` is dropped).

### 9.4.1 Checked (Chrome 157, macOS, 2026-11-27)

A local build with an index of rb-songs and rb-mods (5830 songs, 326 mods), served under
`/OpenRB-338/web/` on a local port, in an own tab, no dialogs:
- Title: `Untitled - OpenRB-338`; after loading `?song=songs/BCBG.rbs` `BCBG.rbs - OpenRB-338`;
  after an edit `meho-ilovetechno2.rbs* - OpenRB-338`.
- A `?song=` link opened the song with its mod (`/rb-mods/mods/redtop2.rbm`). Cmd+Shift+Y opened a
  random song (pushed an entry); Back opened the first song again, Forward the second.
  With the second song modified, Back showed the unsaved prompt; Cancel left the song open and
  the address back on its link.
- `?song=songs%2Fnope.rbs&skin=bogus`: the error box "The link names a song that is not in the
  collection: songs/nope.rbs", the link removed, `?skin=modern` set, the console warning for
  `bogus`.
- `?mod-demo=mods%2F030microdot.rbm` (round 0, now `?mod=`) opened its Demo.rbs (kit `030microdot.rbm`, Untitled).
  View > Classic changed the bar to `skin=classic` with no new entry, and the saved settings then
  hold `theme`.
- The icons: the page's tags are the three above (the extension's tab group replaces the favicon
  with its own marker, so the tab itself could not be compared; the files are served and are the
  landing page's).
- Not checked by an agent: Firefox and Safari (the user's, see the task).

### 9.4.2 Round 1: `?mod=` (Chrome 157, macOS, 2026-11-27)

`?mod-demo=` is replaced by `?mod=` (see Links above). Checked on a local build:
- `?song=songs/BCBG.rbs&mod=mods/030microdot.rbm`: the song opened with `030microdot.rbm`
  (BCBG resolves to `redtop2.rbm`), unmodified, no history entry added.
- Mods menu: "707 Mod" set `&mod=mods/Futuregrapher%20707%20Mod.rbm` (replaceState, entry number
  unchanged); "Standard ReBirth" (a kit with no collection path for a song that resolves to a
  mod) removed `&mod=`.
- `?mod=mods/030microdot.rbm` alone opened its Demo.rbs (Untitled, that kit).
- `?song=songs/BCBG.rbs&mod=mods/nope.rbm`: a mod the index does not list showed "The link names a mod
  that is not in the collection: mods/nope.rbm" and the link was removed.
- Not checked: a mod of the computer (the rule above follows from `Link::mod_of_mem_path`, tested
  natively), Firefox and Safari.
