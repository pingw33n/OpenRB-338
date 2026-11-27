//! The web page (task-172, `docs/web.md` section 7): the standalone app's window as a page,
//! built for wasm32 by `scripts/build-web.sh` (the module `crates/orb-web` calls [`start`]).
//!
//! What the browser gives the page instead of the desktop's:
//! - **Files.** The page sees no folders of the computer. The songs and mods it reads are those the
//!   user picked ([`pick`]: File > Open Local..., the Mods menu's Choose file... and Choose Mods
//!   Folder...) or dropped onto the page ([`App::web_dropped`]), held in memory for the session
//!   (`orb_cli::fs`: [`SONGS_DIR`] for songs and single mods, [`MODS_DIR`] for the picked mods
//!   folder). Save As and Export ask the browser where to write ([`pick_save`]: its own save
//!   picker, `showSaveFilePicker`, where it has one, else a download named after the song); there
//!   is no in-app save dialog on the web.
//! - **Settings** in `localStorage`, under the key `/OpenRB-338/OpenRB-338.toml`, the same TOML
//!   text as the native file (`settings.rs`, task-170).
//! - **Audio** in an AudioWorklet ([`orb_player::web::WebOutput`] and
//!   [`orb_player::remote::Player`], docs/web.md sections 1 to 6), at 44100 Hz.
//! - **A panic** logs to the console (eframe's handler, then [`install_panic_hook`]'s line) and
//!   shows the page's error box (the `#orb-error` element of `web/index.html`); the module is dead
//!   after it (wasm32 aborts), so the box offers a reload.

use std::cell::RefCell;
use std::path::{Path, PathBuf};

use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;

/// The folder of the songs and single mods picked or dropped, and of the saves.
pub const SONGS_DIR: &str = "/Songs";
/// The folder under which a picked mods folder is kept (`/Mods/<its name>`).
pub const MODS_DIR: &str = "/Mods";

fn window() -> Option<web_sys::Window> {
    web_sys::window()
}

fn document() -> Option<web_sys::Document> {
    window()?.document()
}

fn js_text(e: JsValue) -> String {
    e.as_string()
        .or_else(|| {
            e.dyn_ref::<js_sys::Error>()
                .map(|e| String::from(e.message()))
        })
        .unwrap_or_else(|| format!("{e:?}"))
}

// ---- localStorage ----------------------------------------------------------------------------

/// The value of `key` in the page's `localStorage` (none: no such key, or no storage).
pub fn storage_get(key: &str) -> Option<String> {
    window()?.local_storage().ok()??.get_item(key).ok()?
}

/// Sets `key` in the page's `localStorage`.
pub fn storage_set(key: &str, value: &str) -> Result<(), String> {
    let s = window()
        .and_then(|w| w.local_storage().ok().flatten())
        .ok_or("this browser gives the page no localStorage")?;
    s.set_item(key, value).map_err(js_text)
}

// ---- downloads -------------------------------------------------------------------------------

/// Has the browser download `bytes` as the file `name` (a Blob URL on a link that is clicked).
pub fn download(name: &str, bytes: &[u8]) -> Result<(), String> {
    let doc = document().ok_or("no document")?;
    let parts = js_sys::Array::of1(&js_sys::Uint8Array::from(bytes));
    let bag = web_sys::BlobPropertyBag::new();
    bag.set_type("application/octet-stream");
    let blob =
        web_sys::Blob::new_with_u8_array_sequence_and_options(&parts, &bag).map_err(js_text)?;
    let url = web_sys::Url::create_object_url_with_blob(&blob).map_err(js_text)?;
    let a: web_sys::HtmlAnchorElement = doc
        .create_element("a")
        .map_err(js_text)?
        .dyn_into()
        .map_err(|_| "not an anchor")?;
    a.set_href(&url);
    a.set_download(name);
    a.style().set_property("display", "none").map_err(js_text)?;
    let body = doc.body().ok_or("no body")?;
    body.append_child(&a).map_err(js_text)?;
    a.click();
    let _ = body.remove_child(&a);
    // the download has started from the URL; it is freed a while later
    let revoke = Closure::once_into_js(move || {
        let _ = web_sys::Url::revoke_object_url(&url);
    });
    if let Some(w) = window() {
        let _ =
            w.set_timeout_with_callback_and_timeout_and_arguments_0(revoke.unchecked_ref(), 60_000);
    }
    Ok(())
}

// ---- the page's title and the unload question ------------------------------------------------

/// Sets the page's title (eframe's web runner does not apply `ViewportCommand::Title`).
pub fn set_title(title: &str) {
    if let Some(d) = document() {
        d.set_title(title);
    }
}

/// Whether the browser asks before the page is closed or reloaded: while the song has unsaved
/// changes (the native window's close prompt; a page can only ask the browser's own question).
pub fn set_unsaved(unsaved: bool) {
    thread_local! {
        static SET: std::cell::Cell<Option<bool>> = const { std::cell::Cell::new(None) };
    }
    if SET.replace(Some(unsaved)) == Some(unsaved) {
        return;
    }
    let Some(w) = window() else {
        return;
    };
    if unsaved {
        let f = Closure::<dyn Fn(web_sys::Event)>::new(|e: web_sys::Event| e.prevent_default());
        w.set_onbeforeunload(Some(f.as_ref().unchecked_ref()));
        f.forget();
    } else {
        w.set_onbeforeunload(None);
    }
}

// ---- the loading screen ----------------------------------------------------------------------

thread_local! {
    /// The loading screen is gone: the app is drawn and takes input.
    static LOADED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Shows `what` on the page's loading screen (`#orb-loading`, which covers the page and takes
/// the input until [`loaded`]), with its bar at `fraction` (none: no bar).
pub fn loading(what: &str, fraction: Option<f64>) {
    let Some(doc) = document() else {
        return;
    };
    if let Some(t) = doc.get_element_by_id("orb-loading-text") {
        let text = match fraction {
            Some(f) => format!("{what}... {:.0}%", f * 100.0),
            None => format!("{what}..."),
        };
        t.set_text_content(Some(&text));
    }
    if let Some(b) = doc
        .get_element_by_id("orb-loading-bar")
        .and_then(|e| e.dyn_into::<web_sys::HtmlElement>().ok())
    {
        let w = fraction.map_or("0%".to_string(), |f| format!("{:.1}%", f * 100.0));
        let _ = b.style().set_property("width", &w);
    }
}

/// Shows or hides the loading screen (`#orb-loading`).
fn cover(shown: bool) {
    if let Some(e) = document()
        .and_then(|d| d.get_element_by_id("orb-loading"))
        .and_then(|e| e.dyn_into::<web_sys::HtmlElement>().ok())
    {
        let _ = e
            .style()
            .set_property("display", if shown { "flex" } else { "none" });
    }
}

thread_local! {
    /// The parts of a redraw rasterized so far, while the loading screen covers it.
    static REDRAWN: std::cell::Cell<Option<usize>> = const { std::cell::Cell::new(None) };
}

/// After the start: a frame's slice of the part textures rasterized `done` parts that had no
/// texture of any size, and `left` such parts wait (a skin drawn for the first time, View >
/// Modern after a start in Classic). While any waits, the loading screen covers the panel with
/// the drawing's progress, as at the start: the page never shows a half-drawn panel. A few
/// such parts, done in one slice (the frame shows them whole), show nothing.
pub fn redraw_progress(done: usize, left: usize) {
    if !LOADED.get() {
        return;
    }
    let before = REDRAWN.get();
    if left == 0 {
        if before.is_some() {
            REDRAWN.set(None);
            cover(false);
            focus_canvas();
        }
        return;
    }
    let done = before.unwrap_or(0) + done;
    REDRAWN.set(Some(done));
    if before.is_none() {
        cover(true);
    }
    loading(
        "Drawing the panel",
        Some(done as f64 / (done + left) as f64),
    );
}

fn focus_canvas() {
    if let Some(c) = document()
        .and_then(|d| d.get_element_by_id("orb-canvas"))
        .and_then(|e| e.dyn_into::<web_sys::HtmlElement>().ok())
    {
        let _ = c.focus();
    }
}

/// Hides the loading screen (kept for a redraw, [`redraw_progress`]) and gives the canvas the
/// keyboard.
fn loaded() {
    LOADED.set(true);
    cover(false);
    focus_canvas();
}

// ---- the file pickers ------------------------------------------------------------------------

/// What a picker is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pick {
    /// File > Open: one song.
    Song,
    /// Mods > Choose file...: one mod.
    Mod,
    /// Mods > Choose Mods Folder...: a folder, whose mods become the Mods folder.
    ModsFolder,
}

impl Pick {
    /// The id of its `<input type=file>` (kept in the page, so a test can give it files).
    pub fn input_id(self) -> &'static str {
        match self {
            Pick::Song => "orb-pick-song",
            Pick::Mod => "orb-pick-mod",
            Pick::ModsFolder => "orb-pick-mods-folder",
        }
    }
}

/// What a save picker is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SaveFor {
    /// File > Save As (and Save of a song with no place yet): the song.
    Song,
    /// File > Export audio: the loop as FLAC.
    Export,
}

/// Where the browser writes a saved file: the file handle the user chose in the browser's save
/// picker, or none (a download named `name`).
#[derive(Clone, Debug)]
pub struct SaveTarget {
    name: String,
    handle: Option<JsValue>,
}

impl SaveTarget {
    /// A download named `name`.
    pub fn download(name: &str) -> Self {
        SaveTarget {
            name: safe_name(name),
            handle: None,
        }
    }

    /// The file's name, usable as one component of a memory path.
    pub fn file_name(&self) -> String {
        safe_name(&self.name)
    }
}

/// What the user picked, read into the memory file system.
#[derive(Clone, Debug)]
pub enum Picked {
    Song(PathBuf),
    Mod(PathBuf),
    /// The folder and the number of mods (`.rbm` files) in it.
    ModsFolder(PathBuf, usize),
    /// A file dropped onto the page: its name and bytes.
    Dropped(String, Vec<u8>),
    /// A menu command that waited for its mod to be read (`App::web_load_then`).
    Command(crate::menu::Cmd),
    /// What the status line shows while a picked folder is checked.
    Status(String),
    /// Where the browser's save picker (or the download fallback) says to write.
    SaveTo(SaveFor, SaveTarget),
    /// The save picker was closed without a choice.
    SaveCancelled(SaveFor),
    /// A save or an export was written: the file's name, what the status line adds, and the
    /// result.
    Written(SaveFor, String, &'static str, Result<(), String>),
    Error(String),
}

/// Suspends the audio context, as a browser does before the first gesture (checks of the
/// resume, which a context running already would hide).
#[cfg(feature = "app")]
pub fn suspend_audio() {
    OUTPUT.with_borrow(|o| {
        if let Some(o) = o {
            let _ = o.context().suspend();
        }
    });
}

/// The state the last frame saw, as JSON ([`state`]).
#[cfg(feature = "app")]
pub fn state() -> String {
    let ctx = OUTPUT.with_borrow(|o| {
        o.as_ref().map(|o| {
            format!(
                concat!(
                    "\"context\":\"{:?}\",\"rate\":{},\"worklet\":{},\"playback\":{},",
                    "\"loaded\":{},\"wasmMemory\":{}"
                ),
                o.context().state(),
                o.context().sample_rate(),
                o.worklet_stats()
                    .and_then(|v| js_sys::JSON::stringify(&v).ok())
                    .and_then(|s| s.as_string())
                    .unwrap_or_else(|| "null".into()),
                // Chrome's `AudioContext.playbackStats` (underruns), where there is one
                js_sys::Reflect::get(o.context(), &"playbackStats".into())
                    .ok()
                    .filter(|v| v.is_object())
                    .map(|v| {
                        let n = |k: &str| {
                            js_sys::Reflect::get(&v, &k.into())
                                .ok()
                                .and_then(|x| x.as_f64())
                                .map_or("null".into(), |x| x.to_string())
                        };
                        format!(
                            "{{\"underrunEvents\":{},\"underrunDuration\":{}}}",
                            n("underrunEvents"),
                            n("underrunDuration")
                        )
                    })
                    .unwrap_or_else(|| "null".into()),
                LOADED.get(),
                wasm_bindgen::memory()
                    .unchecked_into::<js_sys::WebAssembly::Memory>()
                    .buffer()
                    .unchecked_into::<js_sys::ArrayBuffer>()
                    .byte_length()
            )
        })
    });
    let frame = STATE.with_borrow(|s| s.clone());
    format!(
        "{{{},{}}}",
        ctx.unwrap_or_else(|| "\"context\":null".into()),
        if frame.is_empty() {
            "\"frames\":null".into()
        } else {
            frame
        }
    )
}

thread_local! {
    /// The last frame's state ([`state`]): what the page's checks read (task-172).
    static STATE: RefCell<String> = const { RefCell::new(String::new()) };
    static PICKED: RefCell<Vec<Picked>> = const { RefCell::new(Vec::new()) };
    static CTX: RefCell<Option<egui::Context>> = const { RefCell::new(None) };
}

/// Notes `p` for the next frame and has one drawn.
fn picked(p: Picked) {
    PICKED.with_borrow_mut(|v| v.push(p));
    CTX.with_borrow(|c| {
        if let Some(c) = c {
            c.request_repaint();
        }
    });
}

/// What was picked since the last call.
pub fn take_picked() -> Vec<Picked> {
    PICKED.with_borrow_mut(std::mem::take)
}

/// A file name usable as one component of a memory path.
fn safe_name(name: &str) -> String {
    let n: String = name
        .chars()
        .map(|c| if c == '/' || c == '\\' { '_' } else { c })
        .collect();
    match n.as_str() {
        "" | "." | ".." => "unnamed".into(),
        _ => n,
    }
}

/// The `<input type=file>` of `kind`, made once and kept (hidden) in the page.
fn input(kind: Pick) -> Result<web_sys::HtmlInputElement, String> {
    let doc = document().ok_or("no document")?;
    if let Some(e) = doc.get_element_by_id(kind.input_id()) {
        return e.dyn_into().map_err(|_| "not an input".into());
    }
    let input: web_sys::HtmlInputElement = doc
        .create_element("input")
        .map_err(js_text)?
        .dyn_into()
        .map_err(|_| "not an input")?;
    input.set_type("file");
    input.set_id(kind.input_id());
    input
        .style()
        .set_property("display", "none")
        .map_err(js_text)?;
    match kind {
        Pick::Song => input.set_accept(".rbs"),
        Pick::Mod => input.set_accept(".rbm"),
        Pick::ModsFolder => {
            input.set_multiple(true);
            input.set_webkitdirectory(true);
        }
    }
    let target = input.clone();
    let on_change = Closure::<dyn Fn()>::new(move || {
        let Some(files) = target.files() else {
            return;
        };
        let files: Vec<web_sys::File> = (0..files.length()).filter_map(|i| files.get(i)).collect();
        // the same file picked again must fire `change` again
        target.set_value("");
        if files.is_empty() {
            return;
        }
        wasm_bindgen_futures::spawn_local(async move {
            picked(read_picked(kind, files).await.unwrap_or_else(Picked::Error));
        });
    });
    input.set_onchange(Some(on_change.as_ref().unchecked_ref()));
    on_change.forget();
    doc.body()
        .ok_or("no body")?
        .append_child(&input)
        .map_err(js_text)?;
    Ok(input)
}

/// The bytes of a picked file.
async fn bytes_of(f: &web_sys::File) -> Result<Vec<u8>, String> {
    let buf = wasm_bindgen_futures::JsFuture::from(f.array_buffer())
        .await
        .map_err(|e| format!("{}: cannot read: {}", f.name(), js_text(e)))?;
    Ok(js_sys::Uint8Array::new(&buf).to_vec())
}

thread_local! {
    /// The browser's handles of the mods not read yet (`orb_cli::fs::insert_lazy`), by path.
    static LAZY: RefCell<std::collections::HashMap<PathBuf, web_sys::File>> =
        RefCell::new(std::collections::HashMap::new());
}

/// Bytes `start..end` of a picked file (the browser reads that part only).
async fn slice_of(f: &web_sys::File, start: f64, end: f64) -> Option<Vec<u8>> {
    let blob = f.slice_with_f64_and_f64(start, end).ok()?;
    let buf = wasm_bindgen_futures::JsFuture::from(blob.array_buffer())
        .await
        .ok()?;
    Some(js_sys::Uint8Array::new(&buf).to_vec())
}

/// The INFO of a mod file, with the checks of `rbm::Rbm::parse` and without reading the
/// samples: [`rbm::Walk`] (the parse's own walk) is given the slices it asks for, the CAT
/// header, every chunk header, the start of every `EMBF` up to its name's NUL and the INFO body.
/// `None` for a file the parse refuses or that has no 1280-byte INFO, as natively.
/// Each read takes at least [`READ_AHEAD`] bytes (to the file's end), so a chunk header and the
/// `EMBF` name after it are one read: a browser's read of a slice costs about a millisecond
/// whatever its size.
async fn mod_info_of(f: &web_sys::File) -> Option<rbm::Info> {
    let size = f.size() as usize;
    let mut w = rbm::Walk::new(size);
    let (mut start, mut buf) = (0usize, Vec::new());
    while let rbm::Want::Bytes { at, len } = w.want() {
        if at < start || at + len > start + buf.len() {
            let end = (at + len.max(READ_AHEAD)).min(size);
            buf = slice_of(f, at as f64, end as f64).await?;
            start = at;
        }
        w.feed(buf.get(at - start..at - start + len)?).ok()?;
    }
    w.finish().ok()?.info
}

/// The least a lazy read of a mod takes (`mod_info_of`).
const READ_AHEAD: usize = 1024;

/// How many mods of a picked folder are checked at once: the browser's reads overlap (326 mods:
/// 81 s one by one, 4.5 s 32 at a time, 3.5 s 128 at a time).
const PARALLEL_CHECKS: usize = 128;

/// Reads the picked `files` into the memory file system.
async fn read_picked(kind: Pick, files: Vec<web_sys::File>) -> Result<Picked, String> {
    match kind {
        Pick::Song | Pick::Mod => {
            let f = &files[0];
            let path = Path::new(SONGS_DIR).join(safe_name(&f.name()));
            orb_cli::fs::write(&path, &bytes_of(f).await?).map_err(|e| e.to_string())?;
            Ok(if kind == Pick::Song {
                Picked::Song(path)
            } else {
                Picked::Mod(path)
            })
        }
        Pick::ModsFolder => {
            // `webkitRelativePath` is `<folder>/<path in it>`; a file given without one (a
            // test's) counts as in the folder "Picked"
            let rel = |f: &web_sys::File| {
                js_sys::Reflect::get(f, &"webkitRelativePath".into())
                    .ok()
                    .and_then(|v| v.as_string())
                    .unwrap_or_default()
            };
            let top = files
                .iter()
                .find_map(|f| {
                    let r = rel(f);
                    r.split_once('/').map(|(t, _)| t.to_string())
                })
                .unwrap_or_else(|| "Picked".into());
            let dir = Path::new(MODS_DIR).join(safe_name(&top));
            orb_cli::fs::remove_dir_all(&dir);
            orb_cli::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            LAZY.with_borrow_mut(|l| l.retain(|p, _| !p.starts_with(&dir)));
            let mut mods = Vec::new();
            for f in &files {
                let r = rel(f);
                let inner = r
                    .split_once('/')
                    .map_or_else(|| f.name(), |(_, i)| i.to_string());
                // only the mods at the top are kept: the Mods folder's rules read its top
                // level only (`orb_cli::mods::scan`)
                let is_mod = Path::new(&inner)
                    .extension()
                    .is_some_and(|x| x.eq_ignore_ascii_case("rbm"));
                if !is_mod || inner.contains('/') {
                    continue;
                }
                // A mods folder can be large (a collection of 326 mods is 1 GB): a mod is not
                // read now, only its INFO (the Mods menu's name), and the browser's file handle
                // is kept; its bytes are read when a song or the Mods menu uses it
                // (`App::web_load_then`).
                mods.push((dir.join(safe_name(&inner)), f.clone()));
            }
            let n = mods.len();
            // the mods are checked [`PARALLEL_CHECKS`] at a time, the status line counting
            let queue = std::rc::Rc::new(RefCell::new(std::collections::VecDeque::from(mods)));
            let done = std::rc::Rc::new(std::cell::Cell::new(0usize));
            let workers = js_sys::Array::new();
            for _ in 0..PARALLEL_CHECKS.min(n) {
                let (queue, done, top) = (queue.clone(), done.clone(), top.clone());
                workers.push(&wasm_bindgen_futures::future_to_promise(async move {
                    loop {
                        let Some((path, f)) = queue.borrow_mut().pop_front() else {
                            return Ok(JsValue::UNDEFINED);
                        };
                        let info = mod_info_of(&f).await;
                        orb_cli::fs::insert_lazy(&path, f.size() as u64, info);
                        LAZY.with_borrow_mut(|l| l.insert(path, f));
                        done.set(done.get() + 1);
                        if done.get() % 16 == 0 {
                            picked(Picked::Status(format!(
                                "Reading the mods folder {top} ... {} of {n}",
                                done.get()
                            )));
                        }
                    }
                }));
            }
            wasm_bindgen_futures::JsFuture::from(js_sys::Promise::all(&workers))
                .await
                .map_err(js_text)?;
            Ok(Picked::ModsFolder(dir, n))
        }
    }
}

/// Opens the browser's file picker for `kind`; what is picked arrives in [`take_picked`]
/// (`App::web_frame` acts on it). The browser opens a picker only shortly after a click or a
/// key press, which a menu item is.
pub fn pick(kind: Pick) {
    match input(kind) {
        Ok(i) => i.click(),
        Err(e) => picked(Picked::Error(format!("cannot open the file picker: {e}"))),
    }
}

// ---- the save picker ------------------------------------------------------------------------

/// The browser's `showSaveFilePicker` (Chrome, Edge, Opera, Brave; not Firefox or Safari), if it
/// has one.
fn save_picker() -> Option<(web_sys::Window, js_sys::Function)> {
    let w = window()?;
    let f = js_sys::Reflect::get(&w, &"showSaveFilePicker".into()).ok()?;
    let f = f.dyn_into::<js_sys::Function>().ok()?;
    Some((w, f))
}

/// Asks the browser where to save `suggested` for `what`: its save picker, where it has one;
/// else (or when the picker cannot open) a download named `suggested`. The answer arrives as
/// [`Picked::SaveTo`] or [`Picked::SaveCancelled`]. As [`pick`], the picker opens only shortly
/// after a click or a key press.
pub fn pick_save(what: SaveFor, suggested: &str) {
    let fallback = SaveTarget::download(suggested);
    let Some((w, f)) = save_picker() else {
        picked(Picked::SaveTo(what, fallback));
        return;
    };
    let (description, mime, ext) = match what {
        SaveFor::Song => ("ReBirth song", "application/octet-stream", ".rbs"),
        SaveFor::Export => ("FLAC audio", "audio/flac", ".flac"),
    };
    let accept = js_sys::Object::new();
    let set = |o: &JsValue, k: &str, v: &JsValue| {
        let _ = js_sys::Reflect::set(o, &k.into(), v);
    };
    set(&accept, mime, &js_sys::Array::of1(&ext.into()));
    let kind = js_sys::Object::new();
    set(&kind, "description", &description.into());
    set(&kind, "accept", &accept);
    let opts = js_sys::Object::new();
    set(&opts, "suggestedName", &fallback.name.clone().into());
    set(&opts, "types", &js_sys::Array::of1(&kind));
    let promise = match f.call1(&w, &opts) {
        Ok(p) => js_sys::Promise::resolve(&p),
        Err(e) => {
            web_sys::console::warn_1(&e);
            picked(Picked::SaveTo(what, fallback));
            return;
        }
    };
    wasm_bindgen_futures::spawn_local(async move {
        match wasm_bindgen_futures::JsFuture::from(promise).await {
            Ok(handle) => {
                let name = js_sys::Reflect::get(&handle, &"name".into())
                    .ok()
                    .and_then(|n| n.as_string())
                    .unwrap_or_else(|| fallback.name.clone());
                picked(Picked::SaveTo(
                    what,
                    SaveTarget {
                        name,
                        handle: Some(handle),
                    },
                ));
            }
            Err(e) => {
                let name = js_sys::Reflect::get(&e, &"name".into())
                    .ok()
                    .and_then(|n| n.as_string());
                if name.as_deref() == Some("AbortError") {
                    picked(Picked::SaveCancelled(what));
                } else {
                    // no user gesture any more, a blocked picker: the download instead
                    web_sys::console::warn_1(&e);
                    picked(Picked::SaveTo(what, fallback));
                }
            }
        }
    });
}

/// Calls `obj.method(...args)` and waits for the promise it returns.
async fn call_await(obj: &JsValue, method: &str, args: &js_sys::Array) -> Result<JsValue, JsValue> {
    let f: js_sys::Function = js_sys::Reflect::get(obj, &method.into())?.dyn_into()?;
    let p = f.apply(obj, args)?;
    wasm_bindgen_futures::JsFuture::from(js_sys::Promise::resolve(&p)).await
}

/// Writes `bytes` to `target` (its file handle, or a download); the result arrives as
/// [`Picked::Written`].
pub fn write_target(what: SaveFor, target: &SaveTarget, bytes: Vec<u8>) {
    let name = target.name.clone();
    let Some(handle) = target.handle.clone() else {
        let r = download(&name, &bytes).map_err(|e| format!("the download failed: {e}"));
        picked(Picked::Written(what, name, " (downloaded)", r));
        return;
    };
    wasm_bindgen_futures::spawn_local(async move {
        let r = async {
            let w = call_await(&handle, "createWritable", &js_sys::Array::new()).await?;
            let data = js_sys::Uint8Array::from(bytes.as_slice());
            call_await(&w, "write", &js_sys::Array::of1(&data)).await?;
            call_await(&w, "close", &js_sys::Array::new()).await?;
            Ok::<(), JsValue>(())
        }
        .await
        .map_err(js_text);
        picked(Picked::Written(what, name, "", r));
    });
}

// ---- the panic hook --------------------------------------------------------------------------

/// The text of the page's error box after a panic (as the native box's, `crash::box_text`,
/// without the crash log: the console has the message and the stack).
pub fn box_text(message: &str, location: Option<&str>) -> String {
    let at = location.map(|l| format!(" ({l})")).unwrap_or_default();
    format!(
        "OpenRB-338 stopped because of an internal error. The browser's console has the \
         details.\n\n{message}{at}\n\nUnsaved changes were lost. Reload the page to start again."
    )
}

/// Shows `text` in the page's error box (the `#orb-error` element with its `#orb-error-text`).
fn show_error_box(text: &str) {
    let Some(doc) = document() else {
        return;
    };
    if let Some(t) = doc.get_element_by_id("orb-error-text") {
        t.set_text_content(Some(text));
    }
    if let Some(b) = doc.get_element_by_id("orb-error") {
        let _ = b.class_list().add_1("shown");
    }
}

/// Chains a hook to the one installed before (eframe's, which logs the message and the stack to
/// the console): it logs the version and shows the page's error box. `version` is the About
/// box's.
pub fn install_panic_hook(version: &'static str) {
    let before = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        before(info);
        let p = info.payload();
        let message = p
            .downcast_ref::<&str>()
            .map(|s| s.to_string())
            .or_else(|| p.downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "(no message)".into());
        let location = info.location().map(|l| l.to_string());
        web_sys::console::error_1(
            &format!(
                "OpenRB-338 {version}: panic: {message} ({})",
                location.as_deref().unwrap_or("?")
            )
            .into(),
        );
        show_error_box(&box_text(&message, location.as_deref()));
    }));
}

// ---- the player ------------------------------------------------------------------------------

/// The link's port on the web: the UI half of the worklet's player (docs/web.md section 2).
struct WebPort(orb_player::remote::Player);

impl crate::link::Port for WebPort {
    fn send(&mut self, c: orb_player::Command) -> Result<(), orb_player::Command> {
        self.0.send(c)
    }

    fn snapshot(&mut self) -> &orb_player::Snapshot {
        self.0.snapshot()
    }

    fn tracks(&mut self) -> &orb_player::Tracks {
        self.0.tracks()
    }

    fn collect_garbage(&mut self) {
        self.0.collect_garbage();
    }

    fn build(
        &self,
        song: &orb_engine::song::Song,
        samples: Option<std::sync::Arc<orb_engine::samples::SampleSet>>,
    ) -> Result<Box<orb_engine::frame::Engine>, orb_engine::frame::LoadRefused> {
        self.0.build(song, samples)
    }

    fn build_new(
        &self,
        samples: Option<std::sync::Arc<orb_engine::samples::SampleSet>>,
    ) -> Box<orb_engine::frame::Engine> {
        self.0.build_new(samples)
    }

    fn set_mod(
        &mut self,
        samples: std::sync::Arc<orb_engine::samples::SampleSet>,
    ) -> Result<(), orb_player::Command> {
        self.0.set_mod(samples)
    }

    /// The snapshot that shows the commands is told by [`WebPort::unapplied`], not by frames.
    fn follow_frames(&self) -> u64 {
        0
    }

    fn unapplied(&self) -> u64 {
        self.0.unapplied()
    }

    fn take_errors(&mut self) -> Vec<String> {
        self.0.take_errors()
    }
}

#[cfg(feature = "app")]
thread_local! {
    /// The audio output, open for the page's life.
    static OUTPUT: RefCell<Option<orb_player::web::WebOutput>> = const { RefCell::new(None) };
    static RUNNER: RefCell<Option<eframe::WebRunner>> = const { RefCell::new(None) };
}

/// Starts the page: the panel on `canvas`, the audio with the worklet module at `worklet_url`
/// (relative to the page, so any sub-path works). The page's query `?debug-panic` panics in the
/// first frame on purpose (the error box's test, as the native `--debug-panic-window`).
#[cfg(feature = "app")]
pub async fn start(canvas: web_sys::HtmlCanvasElement, worklet_url: &str) -> Result<(), JsValue> {
    use std::sync::Arc;

    let runner = eframe::WebRunner::new();
    install_panic_hook(crate::about::VERSION);
    for d in [SONGS_DIR, MODS_DIR] {
        let _ = orb_cli::fs::create_dir_all(Path::new(d));
    }
    // the pickers' inputs are in the page from the start (a check can give them files)
    for kind in [Pick::Song, Pick::Mod, Pick::ModsFolder] {
        input(kind).map_err(|e| JsValue::from_str(&e))?;
    }
    let debug_panic = window()
        .and_then(|w| w.location().search().ok())
        .is_some_and(|q| q.split(['?', '&']).any(|p| p == "debug-panic"));
    let settings = crate::settings::Settings::load();
    loading("Starting the audio", None);
    let out = orb_player::web::WebOutput::start(worklet_url).await?;
    loading("Drawing the panel", None);
    let tables = Arc::new(orb_engine::tables::Tables::new());
    let samples = Some(rbm::standard::builtin_samples());
    let player =
        orb_player::remote::Player::new(tables.clone(), samples.clone(), Box::new(out.transport()));
    OUTPUT.with_borrow_mut(|o| *o = Some(out));
    let link = crate::link::Link::with_port(Box::new(WebPort(player)), tables, samples.clone());
    let options = eframe::WebOptions::default();
    runner
        .start(
            canvas,
            options,
            Box::new(move |cc| {
                // always dark (task-127), as natively
                cc.egui_ctx.set_theme(egui::ThemePreference::Dark);
                // the app's fonts only, as natively: eframe's fallback draws a glyph the fonts
                // lack on a 2D canvas and reads it back (getImageData), which Chrome warns about
                cc.egui_ctx.set_glyph_rasterizers(Vec::new());
                CTX.with_borrow_mut(|c| *c = Some(cc.egui_ctx.clone()));
                let mut app = crate::app::App::with_link(crate::model::Panel::new(), link, None);
                app.samples = samples;
                app.menu = true;
                app.enable_logo_anim();
                app.settings = settings;
                app.web_debug_panic = debug_panic;
                app.action(|app| app.start(None));
                Ok(Box::new(app))
            }),
        )
        .await?;
    RUNNER.with_borrow_mut(|r| *r = Some(runner));
    Ok(())
}

// ---- the app's side --------------------------------------------------------------------------

impl crate::app::App {
    /// The web's part of a frame: what the pickers gave and what was dropped onto the page, the
    /// title and the unload question.
    pub fn web_frame(&mut self, ctx: &egui::Context) {
        if std::mem::take(&mut self.web_debug_panic) {
            panic!("?debug-panic: a deliberate panic");
        }
        if !LOADED.get() {
            // the loading screen covers the page until every part of the first complete
            // frame is rasterized (the first frame draws none, `App::track_resize`)
            let pending = crate::app::pending_parts(ctx);
            let done = crate::app::RASTERIZED.with(|n| n.get());
            if self.shown >= 2 && pending == 0 && done > 0 {
                loaded();
            } else {
                let all = done + pending;
                loading(
                    "Drawing the panel",
                    (all > 0).then(|| done as f64 / all as f64),
                );
            }
        }
        for p in take_picked() {
            self.web_picked(p);
        }
        // a browser reads a dropped file only asynchronously: it arrives as `Picked::Dropped`
        for f in ctx.input(|i| i.raw.dropped_files.clone()) {
            wasm_bindgen_futures::spawn_local(async move {
                let name = f
                    .path()
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                picked(match f.bytes_async().await {
                    Ok(b) => Picked::Dropped(name, b),
                    Err(e) => Picked::Error(format!("{name}: cannot read: {e}")),
                });
            });
        }
        set_title(&self.title_text());
        set_unsaved(self.doc.modified);
        if let Some(link) = &mut self.link {
            let p = link.player();
            let unapplied = p.unapplied();
            let s = *p.snapshot();
            let text = format!(
                concat!(
                    "\"frames\":{},\"tick\":{},\"playing\":{},\"generation\":{},",
                    "\"unapplied\":{},\"modified\":{},\"kit\":{:?},\"status\":{:?},",
                    "\"errors\":{}"
                ),
                s.frames,
                s.tick,
                s.playing,
                s.generation,
                unapplied,
                self.doc.modified,
                format!("{:?}", self.doc.kit),
                self.status.clone().unwrap_or_default(),
                self.errors.len()
            );
            STATE.with_borrow_mut(|t| *t = text);
        }
    }

    /// Opens the song at `path` (picked or dropped); when its mod is in the picked mods folder
    /// and not read yet, the mod is read first (`web_load_then`).
    pub fn web_open_song(&mut self, path: &Path) {
        let lazy = crate::session::open_song(path)
            .ok()
            .filter(|o| o.engine.mod_name != orb_cli::mods::STANDARD)
            .and_then(|o| self.resolve_mod_file(&o.engine.mod_name).ok().flatten())
            .filter(|m| orb_cli::fs::is_lazy(m));
        match lazy {
            Some(m) => self.web_load_then(m, Picked::Song(path.to_path_buf())),
            None => self.open_song(path),
        }
    }

    /// Reads the mod `path` of the picked mods folder from the computer into the memory file
    /// system, then acts on `then` (a frame later: the browser reads files asynchronously).
    pub fn web_load_then(&mut self, path: PathBuf, then: Picked) {
        let Some(f) = LAZY.with_borrow(|l| l.get(&path).cloned()) else {
            self.fail(format!("{}: the picked file is gone", file_name(&path)));
            return;
        };
        self.status = Some(format!("Reading {} ...", file_name(&path)));
        wasm_bindgen_futures::spawn_local(async move {
            let read = bytes_of(&f).await.and_then(|b| {
                orb_cli::fs::write(&path, &b).map_err(|e| format!("{}: {e}", path.display()))
            });
            match read {
                Ok(()) => {
                    LAZY.with_borrow_mut(|l| l.remove(&path));
                    picked(then);
                }
                Err(e) => picked(Picked::Error(e)),
            }
        });
    }

    /// Acts on a picker's files.
    pub fn web_picked(&mut self, p: Picked) {
        // the "Reading NAME ..." of `web_load_then` is over
        if self
            .status
            .as_deref()
            .is_some_and(|s| s.starts_with("Reading "))
        {
            self.status = None;
        }
        match p {
            Picked::Song(path) => self.web_open_song(&path),
            Picked::Command(cmd) => self.command(cmd),
            Picked::Status(s) => self.status = Some(s),
            Picked::Mod(path) => match crate::browser::ModStatus::of(&path) {
                crate::browser::ModStatus::Ready => self.use_kit(orb_cli::mods::Kit::Mod(path)),
                crate::browser::ModStatus::NeedsRecrypt => self.fail(format!(
                    "{}: not recrypted. Run `orb mod recrypt --rb20ful RB20FUL.DAT FILE -o \
                     OUT.rbm` once and choose OUT.rbm.",
                    file_name(&path)
                )),
                crate::browser::ModStatus::Invalid(e) => {
                    self.fail(format!("{}: {e}", file_name(&path)))
                }
            },
            Picked::ModsFolder(dir, n) => {
                self.use_mods_folder(&dir);
                self.status = Some(format!(
                    "Mods folder {}: {n} mod file{} (for this session)",
                    file_name(&dir),
                    if n == 1 { "" } else { "s" }
                ));
            }
            Picked::Dropped(name, bytes) => self.web_dropped(&name, &bytes),
            Picked::SaveTo(SaveFor::Song, t) => self.web_save_to(t),
            Picked::SaveTo(SaveFor::Export, t) => self.web_export_to(t),
            Picked::SaveCancelled(what) => self.web_save_cancelled(what),
            Picked::Written(what, name, how, r) => self.web_written(what, &name, how, r),
            Picked::Error(e) => self.fail(e),
        }
    }

    /// A file dropped onto the page: a song opens; a mod joins the Mods folder (the picked one,
    /// else `/Mods/Dropped`), which is then read again.
    pub fn web_dropped(&mut self, name: &str, bytes: &[u8]) {
        let ext = Path::new(name)
            .extension()
            .map(|x| x.to_string_lossy().to_ascii_lowercase());
        match ext.as_deref() {
            Some("rbs") => {
                let path = Path::new(SONGS_DIR).join(safe_name(name));
                match orb_cli::fs::write(&path, bytes) {
                    Ok(()) => self.web_open_song(&path),
                    Err(e) => self.fail(format!("{name}: {e}")),
                }
            }
            Some("rbm") => {
                let dir = self
                    .settings
                    .mods_dir
                    .clone()
                    .filter(|d| d.starts_with(MODS_DIR) && orb_cli::fs::is_dir(d))
                    .unwrap_or_else(|| Path::new(MODS_DIR).join("Dropped"));
                if let Err(e) = orb_cli::fs::write(&dir.join(safe_name(name)), bytes) {
                    self.fail(format!("{name}: {e}"));
                    return;
                }
                if self.settings.mods_dir.as_deref() != Some(dir.as_path()) {
                    self.settings.set_mods_dir(Some(dir.clone()));
                    if let Err(e) = self.settings.save() {
                        self.fail(e);
                    }
                }
                self.refresh_mods();
                self.status = Some(format!(
                    "{name} added to the Mods folder {} (for this session)",
                    file_name(&dir)
                ));
            }
            _ => self.fail(format!(
                "{name}: only songs (.rbs) and mods (.rbm) can be dropped here"
            )),
        }
    }
}

fn file_name(p: &Path) -> String {
    p.file_name().map_or_else(
        || p.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    )
}
