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
| The page's main thread | the UI's module (wasm-bindgen), e.g. `crates/orb-web-demo` now, the UI in task-172 | `orb_player::remote::Player` and `orb_player::web::WebOutput` |
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
`pointerdown`, `keydown` and `touchend` on the document and calls `resume()` while the context
is not running; it does the same on `visibilitychange` when the tab is visible again, in case
the browser suspended the context while the tab was hidden (Chrome does not; mobile browsers
may). The engine is not reset: playback goes on from where it stopped.

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
