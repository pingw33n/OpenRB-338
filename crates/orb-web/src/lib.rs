//! OpenRB-338 as a web page (task-172, `docs/web.md` section 7): the module `index.html` loads
//! (built by `scripts/build-web.sh` into `dist/web/`). It starts `orb_ui::web::start`: the
//! standalone app's window on a canvas, its audio in an AudioWorklet (`orb_worklet.wasm`).
#![cfg(target_arch = "wasm32")]

use wasm_bindgen::prelude::*;

/// Starts the app on `canvas`, with the worklet's module at `worklet_url` (relative to the
/// page).
#[wasm_bindgen]
pub async fn start(canvas: web_sys::HtmlCanvasElement, worklet_url: String) -> Result<(), JsValue> {
    orb_ui::web::start(canvas, &worklet_url).await
}

/// What the last frame saw, as JSON (the audio context's state and rate, the engine's frames,
/// tick and play state, the commands not shown yet, the song's kit, the status line): for the
/// page's checks from the browser's console (`docs/web.md` section 7).
#[wasm_bindgen]
pub fn state() -> String {
    orb_ui::web::state()
}

/// Suspends the audio context, as the browser does before the first gesture: a check that
/// the next click or key press resumes it.
#[wasm_bindgen]
pub fn suspend_audio() {
    orb_ui::web::suspend_audio()
}

/// The audio checks' stand-ins for what an iPhone does (task-329, docs/web.md section 4.1):
/// `suspend`, `interrupt`, `close`, `stall`, `hide` or `show`.
#[wasm_bindgen]
pub fn debug_audio(what: String) -> Result<(), JsValue> {
    orb_ui::web::debug_audio(&what).map_err(|e| JsValue::from_str(&e))
}

/// The About box's version text (the page shows it while it loads).
#[wasm_bindgen]
pub fn version() -> String {
    orb_ui::about::VERSION.to_string()
}
