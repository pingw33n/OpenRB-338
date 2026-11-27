//! The web page (task-172, `docs/web.md` section 7): the standalone app's window as a page,
//! built for wasm32 by `scripts/build-web.sh` (the module `crates/orb-web` calls [`start`]).
//!
//! What the browser gives the page instead of the desktop's:
//! - **Files.** The page sees no folders of the computer. The songs and mods it reads are those the
//!   user picked ([`pick`]: File > Open..., the Mods menu's Choose File... and Choose Mods
//!   Folder...) or dropped onto the page ([`App::web_dropped`]), held in memory for the session
//!   (`orb_cli::fs`: [`SONGS_DIR`] for songs and single mods, [`MODS_DIR`] for the picked mods
//!   folder). The rb-songs and rb-mods collections are files of the memory file system too
//!   ([`crate::collection`]: `/rb-songs/...`, `/rb-mods/...`), listed from the page's `index.json`
//!   and downloaded when used ([`App::web_load_then`], task-174). Save As and Export ask the
//!   browser where to write ([`pick_save`]: its own save picker, `showSaveFilePicker`, where it has
//!   one, else a download named after the song); there is no in-app save dialog on the web.
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

// ---- the address bar and the History API (task-326) ------------------------------------------

/// What the page's links need of the app (`App::web_nav_frame`).
#[derive(Default)]
pub struct WebNav {
    /// The document number the address was last made for.
    pub(crate) seen: u64,
    /// The link of the open document: a collection song (with the collection mod it plays when
    /// that is not the one it resolves to), a collection mod's demo song, or none.
    pub(crate) doc_link: Option<crate::collection::Link>,
    /// The collection mod whose demo song was just opened (`App::open_demo_inner`).
    pub(crate) demo_of: Option<PathBuf>,
    /// The collection mod whose demo song is the open document (taken from `demo_of` when the
    /// document changes).
    pub(crate) demo: Option<PathBuf>,
    /// A link's song and the mod it names (`?song=&mod=`): the mod is used once that song is
    /// the document.
    pub(crate) pending_mod: Option<(PathBuf, PathBuf)>,
    /// The link the page was loaded with, until the index is read and it is opened.
    pub(crate) initial: Option<crate::collection::Link>,
    /// While a Back or Forward is being followed: the history entry it left.
    pub(crate) from: Option<i64>,
}

thread_local! {
    /// The number of the history entry shown: each entry this page pushes has its number in its
    /// state (`{n}`), so a Back or Forward knows its direction and size.
    static HIST_N: std::cell::Cell<i64> = const { std::cell::Cell::new(0) };
}

fn history() -> Option<web_sys::History> {
    window()?.history().ok()
}

/// The page's query string (`?song=...`), or "".
fn location_search() -> String {
    window()
        .and_then(|w| w.location().search().ok())
        .unwrap_or_default()
}

fn state_n(state: &JsValue) -> i64 {
    js_sys::Reflect::get(state, &"n".into())
        .ok()
        .and_then(|n| n.as_f64())
        .unwrap_or(0.0) as i64
}

fn history_n() -> i64 {
    history()
        .and_then(|h| h.state().ok())
        .map_or(0, |s| state_n(&s))
}

/// Makes the address `query` on the page's path: a new history entry (`push`) or the current
/// one replaced.
fn set_url(query: &str, push: bool) {
    let (Some(w), Some(h)) = (window(), history()) else {
        return;
    };
    let Ok(path) = w.location().pathname() else {
        return;
    };
    let n = if push { HIST_N.get() + 1 } else { HIST_N.get() };
    let state = js_sys::Object::new();
    let _ = js_sys::Reflect::set(&state, &"n".into(), &(n as f64).into());
    let url = format!("{path}{query}");
    let r = if push {
        h.push_state_with_url(&state, "", Some(&url))
    } else {
        h.replace_state_with_url(&state, "", Some(&url))
    };
    match r {
        Ok(()) => HIST_N.set(n),
        Err(e) => {
            web_sys::console::warn_1(&format!("OpenRB-338: the address: {}", js_text(e)).into())
        }
    }
}

/// Reports every Back and Forward (`Picked::Popstate`).
fn listen_popstate() {
    let Some(w) = window() else {
        return;
    };
    let f = Closure::<dyn Fn(web_sys::PopStateEvent)>::new(|e: web_sys::PopStateEvent| {
        picked(Picked::Popstate(
            crate::collection::Link::parse(&location_search()),
            state_n(&e.state()),
        ));
    });
    let _ = w.add_event_listener_with_callback("popstate", f.as_ref().unchecked_ref());
    f.forget();
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
    /// Mods > Choose File...: one mod.
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
    /// A song whose unsaved-changes question was answered, once it and its mod are read
    /// (`App::web_fetch_and_open`).
    OpenNow(PathBuf),
    /// The demo song of this collection mod, once the mod is read (a `?mod=` link).
    Demo(PathBuf),
    /// This collection mod is the mod of the song a link opened, once the mod is read.
    LinkMod(PathBuf),
    /// The browser went Back or Forward (the History API): the link the address names now and
    /// the number of the history entry (`WebNav`).
    Popstate(Option<crate::collection::Link>, i64),
    Mod(PathBuf),
    /// The folder and the number of mods (`.rbm` files) in it.
    ModsFolder(PathBuf, usize),
    /// A file dropped onto the page: its name and bytes.
    Dropped(String, Vec<u8>),
    /// A menu command that waited for its mod to be read (`App::web_load_then`).
    Command(crate::menu::Cmd),
    /// What the status line shows while a picked folder is checked.
    Status(String),
    /// The collection index was read (the numbers of songs and mods) or not (task-174).
    Collection(Result<(usize, usize), String>),
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
        "{{{},{},{}}}",
        ctx.unwrap_or_else(|| "\"context\":null".into()),
        collection_state(),
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

/// Where the bytes of a lazy file (`orb_cli::fs::insert_lazy`) come from.
#[derive(Clone, Debug)]
enum Source {
    /// A mod of a picked mods folder: the browser's handle of the file.
    Picked(web_sys::File),
    /// A file of the rb-songs or rb-mods collection: downloaded and checked (task-174).
    Remote(crate::collection::File),
}

thread_local! {
    /// The sources of the files not read yet (`orb_cli::fs::insert_lazy`), by path.
    static LAZY: RefCell<std::collections::HashMap<PathBuf, Source>> =
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
                        LAZY.with_borrow_mut(|l| l.insert(path, Source::Picked(f)));
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
    let mut settings = crate::settings::Settings::load();
    // ?skin=classic or ?skin=modern: this load's skin, not the saved setting (task-326)
    let search = location_search();
    match crate::settings::Theme::from_query(&search) {
        Ok(Some(t)) => {
            settings.set_theme_for_session(t);
        }
        Ok(None) => {}
        Err(v) => web_sys::console::warn_1(
            &format!("OpenRB-338: ?skin={v}: not a skin (classic or modern), ignored").into(),
        ),
    }
    let initial_link = crate::collection::Link::parse(&search);
    HIST_N.set(history_n());
    listen_popstate();
    // the mods source of a page load is the mod collection (the user's decision of
    // 2026-11-27): a picked mods folder is for its session only, and the collection's mods
    // join the folder when the index is read (`load_index`)
    let _ = orb_cli::fs::create_dir_all(Path::new(COLLECTION_MODS));
    settings.mods_dir = Some(PathBuf::from(COLLECTION_MODS));
    load_index();
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
                app.web_nav.initial = initial_link;
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
        set_title(&self.page_title_text());
        self.web_nav_frame();
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

    /// Opens the song at `path` (picked, dropped or of the song collection); a song of the
    /// collection is downloaded first, and when its mod is not read yet (a mod of the picked
    /// mods folder or of the mod collection), the mod is read first (`web_load_then`).
    /// The unsaved-changes prompt comes first, as natively, so a cancel downloads nothing.
    pub fn web_open_song(&mut self, path: &Path) {
        if self.asks_to_save() {
            self.prompt = Some(crate::menu::Then::Open(path.to_path_buf()));
        } else {
            self.web_fetch_and_open(path);
        }
    }

    /// Opens the song at `path` without asking (the prompt was answered, or there are no
    /// unsaved changes): a song of the collection is downloaded first, then its mod when it is
    /// not read yet, each a frame later (`Picked::OpenNow`).
    pub fn web_fetch_and_open(&mut self, path: &Path) {
        if orb_cli::fs::is_lazy(path) {
            self.web_load_then(path.to_path_buf(), Picked::OpenNow(path.to_path_buf()));
            return;
        }
        let lazy = crate::session::open_song(path)
            .ok()
            .filter(|o| o.engine.mod_name != orb_cli::mods::STANDARD)
            .and_then(|o| self.resolve_mod_file(&o.engine.mod_name).ok().flatten())
            .filter(|m| orb_cli::fs::is_lazy(m));
        match lazy {
            Some(m) => self.web_load_then(m, Picked::OpenNow(path.to_path_buf())),
            None => self.open_now(path),
        }
    }

    /// Reads the lazy file `path` into the memory file system, then acts on `then` (a frame
    /// later: the browser reads files asynchronously): a mod of the picked mods folder from the
    /// computer, a file of a collection from its server ([`fetch_into`], with the download
    /// dialog).
    pub fn web_load_then(&mut self, path: PathBuf, then: Picked) {
        let f = match LAZY.with_borrow(|l| l.get(&path).cloned()) {
            Some(Source::Picked(f)) => f,
            Some(Source::Remote(file)) => {
                fetch_into(path, file, then);
                return;
            }
            None => {
                self.fail(format!("{}: the picked file is gone", file_name(&path)));
                return;
            }
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
            Picked::OpenNow(path) => self.web_fetch_and_open(&path),
            Picked::Demo(path) => self.web_demo_of(&path),
            Picked::LinkMod(path) => self.web_link_mod(&path),
            Picked::Popstate(link, n) => self.web_popstate(link, n),
            Picked::Command(cmd) => self.command(cmd),
            Picked::Status(s) => self.status = Some(s),
            Picked::Collection(r) => self.web_collection_read(r),
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

// ---- the collections (task-174) --------------------------------------------------------------

/// The mod collection's folder in memory: the Mods folder of a page load.
pub const COLLECTION_MODS: &str = crate::collection::MODS_FOLDER;
/// The song collection's folder in memory: where File > Open from Collection... starts.
pub const COLLECTION_SONGS: &str = crate::collection::SONGS_FOLDER;
/// The index of the collections, next to the page (`scripts/build-web.sh --index`).
const INDEX_URL: &str = "index.json";
/// The page's download cache (the Cache API), keyed by a file's SHA-256.
const CACHE_NAME: &str = "orb-collection";

/// Where the collection index is.
#[derive(Clone, Debug)]
enum CollectionState {
    Loading,
    /// Read: the numbers of songs and mods entered.
    Ready(usize, usize),
    Failed(String),
}

/// A download of a collection file: what the download dialog shows.
#[derive(Clone, Debug)]
struct Download {
    /// Its number: a result of a download that was cancelled or replaced is dropped.
    serial: u64,
    name: String,
    done: u64,
    total: u64,
    /// Why it failed: the dialog shows it until it is closed.
    error: Option<String>,
    abort: Option<web_sys::AbortController>,
}

thread_local! {
    static COLLECTION: RefCell<CollectionState> = const { RefCell::new(CollectionState::Loading) };
    static DOWNLOAD: RefCell<Option<Download>> = const { RefCell::new(None) };
    /// The collection index, kept for the random song and the links (task-326).
    static INDEX: RefCell<Option<crate::collection::Index>> = const { RefCell::new(None) };
    static SERIAL: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    /// For the checks ([`state`]): files taken from the cache, files downloaded, downloads
    /// cancelled.
    static COUNTS: std::cell::Cell<(u32, u32, u32)> = const { std::cell::Cell::new((0, 0, 0)) };
}

/// The collections' part of [`state`].
fn collection_state() -> String {
    let c = COLLECTION.with_borrow(|c| match c {
        CollectionState::Loading => "\"loading\"".to_string(),
        CollectionState::Ready(s, m) => format!("{{\"songs\":{s},\"mods\":{m}}}"),
        CollectionState::Failed(e) => format!("{{\"error\":{e:?}}}"),
    });
    let d = DOWNLOAD.with_borrow(|d| match d {
        None => "null".to_string(),
        Some(d) => format!(
            "{{\"name\":{:?},\"done\":{},\"total\":{},\"error\":{}}}",
            d.name,
            d.done,
            d.total,
            d.error
                .as_ref()
                .map_or_else(|| "null".into(), |e| format!("{e:?}"))
        ),
    });
    let (hits, fetched, cancelled) = COUNTS.get();
    format!(
        concat!(
            "\"collection\":{},\"download\":{},\"cacheHits\":{},\"downloaded\":{},",
            "\"cancelled\":{}"
        ),
        c, d, hits, fetched, cancelled
    )
}

fn repaint() {
    CTX.with_borrow(|c| {
        if let Some(c) = c {
            c.request_repaint();
        }
    });
}

/// Fetches the collection index next to the page and enters its files into the memory file
/// system as lazy files (each song, each mod with its INFO name), whose bytes are downloaded
/// when used; the result arrives as [`Picked::Collection`].
fn load_index() {
    COLLECTION.with_borrow_mut(|c| *c = CollectionState::Loading);
    wasm_bindgen_futures::spawn_local(async {
        let r = async {
            let w = window().ok_or("no window")?;
            let init = web_sys::RequestInit::new();
            // the index changes with a deploy: asked again, the server answers 304 when it
            // did not
            init.set_cache(web_sys::RequestCache::NoCache);
            let resp =
                wasm_bindgen_futures::JsFuture::from(w.fetch_with_str_and_init(INDEX_URL, &init))
                    .await
                    .map_err(|e| format!("cannot download {INDEX_URL}: {}", js_text(e)))?;
            let resp: web_sys::Response = resp.dyn_into().map_err(|_| "not a response")?;
            if !resp.ok() {
                return Err(format!("{INDEX_URL}: HTTP {}", resp.status()));
            }
            let text = wasm_bindgen_futures::JsFuture::from(resp.text().map_err(js_text)?)
                .await
                .map_err(js_text)?
                .as_string()
                .unwrap_or_default();
            let index =
                crate::collection::Index::parse(&text).map_err(|e| format!("{INDEX_URL}: {e}"))?;
            let n = enter_index(&index);
            INDEX.with_borrow_mut(|i| *i = Some(index));
            Ok::<_, String>(n)
        }
        .await;
        COLLECTION.with_borrow_mut(|c| {
            *c = match &r {
                Ok((s, m)) => CollectionState::Ready(*s, *m),
                Err(e) => CollectionState::Failed(e.clone()),
            }
        });
        if let Err(e) = &r {
            web_sys::console::warn_1(&format!("OpenRB-338: the collections: {e}").into());
        }
        picked(Picked::Collection(r));
    });
}

/// Enters the files of `index` (those it has a hash of) as lazy files; the numbers of songs
/// and mods entered.
fn enter_index(index: &crate::collection::Index) -> (usize, usize) {
    let mut n = (0, 0);
    LAZY.with_borrow_mut(|l| {
        for s in &index.songs {
            if let (Some(size), Some(_)) = (s.file.size, s.file.sha256) {
                let p = s.file.mem_path();
                orb_cli::fs::insert_lazy(&p, size, None);
                l.insert(p, Source::Remote(s.file.clone()));
                n.0 += 1;
            }
        }
        for m in &index.mods {
            if let (Some(size), Some(_)) = (m.file.size, m.file.sha256) {
                let p = m.file.mem_path();
                // the INFO name is what the Mods menu and the lookup need; the rest of the INFO
                // is read with the mod
                let info = m.name.clone().map(|name| rbm::Info {
                    text: String::new(),
                    ftp: String::new(),
                    web: String::new(),
                    name,
                    version: None,
                });
                orb_cli::fs::insert_lazy(&p, size, info);
                l.insert(p, Source::Remote(m.file.clone()));
                n.1 += 1;
            }
        }
    });
    n
}

/// The browser's download cache, if it has one (the Cache API: secure contexts only).
async fn open_cache() -> Option<JsValue> {
    let caches = js_sys::Reflect::get(&window()?.into(), &"caches".into()).ok()?;
    if caches.is_undefined() || caches.is_null() {
        return None;
    }
    call_await(&caches, "open", &js_sys::Array::of1(&CACHE_NAME.into()))
        .await
        .ok()
}

/// The download of `serial` is still the one shown.
fn current(serial: u64) -> bool {
    DOWNLOAD.with_borrow(|d| d.as_ref().is_some_and(|d| d.serial == serial))
}

fn progress(serial: u64, done: u64) {
    DOWNLOAD.with_borrow_mut(|d| {
        if let Some(d) = d.as_mut().filter(|d| d.serial == serial) {
            d.done = done;
        }
    });
    repaint();
}

/// The bytes of the collection file `file`: from the cache when it holds them (checked again),
/// else downloaded from the file's raw URL, checked against the index and put into the cache.
/// The download's progress goes to the dialog; an error says what went wrong.
async fn fetch_file(file: &crate::collection::File, serial: u64) -> Result<Vec<u8>, String> {
    let name = file.name().to_string();
    let key = file
        .key()
        .map(|k| format!("orb-cache/sha256/{k}"))
        .ok_or_else(|| format!("{name}: the collection index has no hash of it"))?;
    let cache = open_cache().await;
    if let Some(c) = &cache
        && let Ok(r) = call_await(c, "match", &js_sys::Array::of1(&key.clone().into())).await
        && let Ok(r) = r.dyn_into::<web_sys::Response>()
        && let Ok(p) = r.array_buffer()
        && let Ok(buf) = wasm_bindgen_futures::JsFuture::from(p).await
    {
        let bytes = js_sys::Uint8Array::new(&buf).to_vec();
        if file.check(&bytes).is_ok() {
            COUNTS.set({
                let (h, f, c) = COUNTS.get();
                (h + 1, f, c)
            });
            // a hit is a use (the least recently used are evicted first)
            let evicted = lru(|l| l.admit(&key, bytes.len() as u64, crate::collection::CACHE_CAP));
            evict(c, evicted.unwrap_or_default()).await;
            return Ok(bytes);
        }
        lru(|l| l.forget(&key));
        let _ = call_await(c, "delete", &js_sys::Array::of1(&key.clone().into())).await;
    }
    let url = file
        .url()
        .ok_or_else(|| format!("{name}: the collection index has no commit to download from"))?;
    let w = window().ok_or("no window")?;
    let ctrl = web_sys::AbortController::new().map_err(js_text)?;
    DOWNLOAD.with_borrow_mut(|d| {
        if let Some(d) = d.as_mut().filter(|d| d.serial == serial) {
            d.abort = Some(ctrl.clone());
        }
    });
    let init = web_sys::RequestInit::new();
    init.set_signal(Some(&ctrl.signal()));
    let failed = |e: JsValue| {
        let offline = !w.navigator().on_line();
        format!(
            "{name}: the download failed{}: {}",
            if offline {
                " (the browser is offline)"
            } else {
                " (no connection to the server?)"
            },
            js_text(e)
        )
    };
    let resp = wasm_bindgen_futures::JsFuture::from(w.fetch_with_str_and_init(&url, &init))
        .await
        .map_err(failed)?;
    let resp: web_sys::Response = resp.dyn_into().map_err(|_| "not a response")?;
    if !resp.ok() {
        return Err(match resp.status() {
            404 => format!(
                "{name}: the server has no such file (HTTP 404). The collection may have changed \
                 since this page was built: reload the page."
            ),
            s => format!(
                "{name}: the server answered HTTP {s} {}",
                resp.status_text()
            ),
        });
    }
    let mut bytes = Vec::with_capacity(file.size.unwrap_or(0) as usize);
    match resp.body() {
        Some(body) => {
            let reader: web_sys::ReadableStreamDefaultReader = body.get_reader().unchecked_into();
            loop {
                let chunk = wasm_bindgen_futures::JsFuture::from(reader.read())
                    .await
                    .map_err(failed)?;
                let done = js_sys::Reflect::get(&chunk, &"done".into())
                    .ok()
                    .is_some_and(|d| d.is_truthy());
                if done {
                    break;
                }
                if let Ok(v) = js_sys::Reflect::get(&chunk, &"value".into()) {
                    bytes.extend(js_sys::Uint8Array::new(&v).to_vec());
                }
                if !current(serial) {
                    return Err(format!("{name}: cancelled"));
                }
                progress(serial, bytes.len() as u64);
            }
        }
        None => {
            let buf = wasm_bindgen_futures::JsFuture::from(resp.array_buffer().map_err(js_text)?)
                .await
                .map_err(failed)?;
            bytes = js_sys::Uint8Array::new(&buf).to_vec();
        }
    }
    file.check(&bytes)?;
    COUNTS.set({
        let (h, f, c) = COUNTS.get();
        (h, f + 1, c)
    });
    // stored within the cap, the least recently used evicted first; a file larger than the cap
    // is not stored. A failed write (the quota) is like any cache failure: the file opens from
    // memory, with no error shown
    if let Some(c) = &cache
        && let Some(evicted) =
            lru(|l| l.admit(&key, bytes.len() as u64, crate::collection::CACHE_CAP))
    {
        evict(c, evicted).await;
        let Ok(r) = web_sys::Response::new_with_opt_u8_array(Some(&mut bytes.clone())) else {
            lru(|l| l.forget(&key));
            return Ok(bytes);
        };
        let args = js_sys::Array::of2(&key.clone().into(), &r);
        if let Err(e) = call_await(c, "put", &args).await {
            web_sys::console::warn_1(&e);
            lru(|l| l.forget(&key));
        }
    }
    Ok(bytes)
}

/// The `localStorage` key of the download cache's record ([`crate::collection::CacheLru`]).
const LRU_KEY: &str = "/OpenRB-338/cache-lru.json";

/// Runs `f` on the download cache's record, read from and written back to `localStorage` (no
/// await between, so two downloads never interleave on it).
fn lru<T>(f: impl FnOnce(&mut crate::collection::CacheLru) -> T) -> T {
    let mut l = crate::collection::CacheLru::parse(&storage_get(LRU_KEY).unwrap_or_default());
    let r = f(&mut l);
    if let Err(e) = storage_set(LRU_KEY, &l.to_text()) {
        web_sys::console::warn_1(&format!("OpenRB-338: the cache record: {e}").into());
    }
    r
}

/// Deletes the `keys` from the cache `c`.
async fn evict(c: &JsValue, keys: Vec<String>) {
    for k in keys {
        let _ = call_await(c, "delete", &js_sys::Array::of1(&k.into())).await;
    }
}

/// Downloads the collection file `file` into the memory file system at `path`, with the
/// download dialog ([`App::web_download_ui`]), then acts on `then`. A download that runs is
/// replaced. A failure stays in the dialog until it is closed; nothing else changes (the song
/// that plays plays on).
fn fetch_into(path: PathBuf, file: crate::collection::File, then: Picked) {
    cancel_download(false);
    let serial = SERIAL.get() + 1;
    SERIAL.set(serial);
    DOWNLOAD.with_borrow_mut(|d| {
        *d = Some(Download {
            serial,
            name: file.name().to_string(),
            done: 0,
            total: file.size.unwrap_or(0),
            error: None,
            abort: None,
        })
    });
    repaint();
    wasm_bindgen_futures::spawn_local(async move {
        let r = fetch_file(&file, serial).await;
        if !current(serial) {
            // cancelled or replaced: the result is dropped
            return;
        }
        match r.and_then(|b| {
            orb_cli::fs::write(&path, &b).map_err(|e| format!("{}: {e}", path.display()))
        }) {
            Ok(()) => {
                LAZY.with_borrow_mut(|l| l.remove(&path));
                DOWNLOAD.with_borrow_mut(|d| *d = None);
                picked(then);
            }
            Err(e) => {
                DOWNLOAD.with_borrow_mut(|d| {
                    if let Some(d) = d.as_mut() {
                        d.error = Some(e);
                        d.abort = None;
                    }
                });
                repaint();
            }
        }
    });
}

/// Stops the download that runs, if any, and closes its dialog; `by_user`: the status line
/// says so.
fn cancel_download(by_user: bool) {
    let Some(d) = DOWNLOAD.with_borrow_mut(|d| d.take()) else {
        return;
    };
    if let Some(a) = &d.abort {
        a.abort();
    }
    if by_user && d.error.is_none() {
        COUNTS.set({
            let (h, f, c) = COUNTS.get();
            (h, f, c + 1)
        });
        picked(Picked::Status(format!(
            "The download of {} was cancelled",
            d.name
        )));
    }
}

/// Whether the download dialog is shown (the panel takes no input meanwhile).
pub fn download_shown() -> bool {
    DOWNLOAD.with_borrow(|d| d.is_some())
}

impl crate::app::App {
    /// The collection index was read (or not): the Mods menu lists the mod collection when it
    /// is the Mods folder.
    fn web_collection_read(&mut self, r: Result<(usize, usize), String>) {
        if r.is_ok() && self.settings.mods_dir.as_deref() == Some(Path::new(COLLECTION_MODS)) {
            self.refresh_mods();
        }
        // the page's link (`?song=`, `?mod=`) opens now that the index is here
        if let Some(link) = self.web_nav.initial.take() {
            self.web_nav.from = Some(HIST_N.get());
            // (the start's own document is not the link's: the address stays until it opens)
            self.web_nav.seen = self.doc_serial;
            match r {
                Ok(_) => self.web_open_link(link),
                Err(e) => self.fail(format!(
                    "The link cannot be opened, the collection is not available: {e}"
                )),
            }
        }
    }

    /// Opens what a link names, through the normal open path (the unsaved-changes prompt first
    /// for a song): a song of the collection (with `mod`, that mod of the collection once the
    /// song is open), or, with no song, the demo song of a mod of the collection (the mod is
    /// downloaded first). A path the index does not list, a file it could not read, or a mod
    /// with no demo song shows a message and changes nothing else.
    pub fn web_open_link(&mut self, link: crate::collection::Link) {
        type Found = Result<(Option<PathBuf>, Option<PathBuf>), String>;
        let found: Found = INDEX.with_borrow(|i| {
            let Some(i) = i else {
                return Err("The collection index is not available".to_string());
            };
            let song = match &link.song {
                None => None,
                Some(p) => match i.song(p) {
                    None => {
                        return Err(format!(
                            "The link names a song that is not in the collection: {p}"
                        ));
                    }
                    Some(s) => match &s.error {
                        Some(e) => {
                            return Err(format!(
                                "{}: the song cannot be opened: {e}",
                                s.file.name()
                            ));
                        }
                        None => Some(s.file.mem_path()),
                    },
                },
            };
            let md = match &link.mod_ {
                None => None,
                Some(p) => match i.mod_at(p) {
                    None => {
                        return Err(format!(
                            "The link names a mod that is not in the collection: {p}"
                        ));
                    }
                    Some(m) => match &m.error {
                        Some(e) => {
                            return Err(format!("{}: the mod cannot be read: {e}", m.file.name()));
                        }
                        None => Some(m.file.mem_path()),
                    },
                },
            };
            Ok((song, md))
        });
        match found {
            Err(e) => self.fail(e),
            Ok((Some(song), md)) => {
                self.web_nav.pending_mod = md.map(|m| (song.clone(), m));
                self.web_open_song(&song);
            }
            Ok((None, Some(path))) => {
                if orb_cli::fs::is_lazy(&path) {
                    self.web_load_then(path.clone(), Picked::Demo(path));
                } else {
                    self.web_demo_of(&path);
                }
            }
            Ok((None, None)) => {}
        }
    }

    /// The mod of a link's song (`?song=&mod=`) is read: it becomes the song's mod, and the
    /// song stays unmodified (the link, not an edit, chose it).
    pub fn web_link_mod(&mut self, path: &Path) {
        use orb_cli::mods::Kit;
        self.use_kit(Kit::Mod(path.to_path_buf()));
        if self.doc.kit == Kit::Mod(path.to_path_buf()) {
            self.doc.modified = false;
        }
    }

    /// The link of the open document (`None` for a local file, New, a drop): a collection song,
    /// with `mod` when its mod is a collection mod other than the one the song resolves to by
    /// itself (the first of the index's `mod_variants`; a mod of the computer, or the built-in
    /// kit where the song resolves to a mod, cannot be named by a link); or a collection mod's
    /// demo song.
    fn web_doc_link(&self) -> Option<crate::collection::Link> {
        use crate::collection::Link;
        if let Some(m) = &self.web_nav.demo {
            return Some(Link {
                song: None,
                mod_: Some(Link::mod_of_mem_path(m)?),
            });
        }
        let song = Link::song_of_mem_path(self.doc.path.as_deref()?)?;
        let mod_ = match &self.doc.kit {
            orb_cli::mods::Kit::Mod(p) => Link::mod_of_mem_path(p).filter(|m| {
                INDEX.with_borrow(|i| {
                    let default = i
                        .as_ref()
                        .and_then(|i| i.song(&song).and_then(|s| i.default_mod(s)));
                    default.map(|d| d.file.path.as_str()) != Some(m.as_str())
                })
            }),
            orb_cli::mods::Kit::Standard => None,
        };
        Some(Link {
            song: Some(song),
            mod_,
        })
    }

    /// Opens the demo song of the mod file `path` (read): the unsaved-changes prompt first,
    /// as Mods > Demo Song does.
    pub fn web_demo_of(&mut self, path: &Path) {
        if !crate::session::mod_has_demo(path) {
            self.fail(format!("{}: this mod has no demo song", file_name(path)));
        } else if self.asks_to_save() {
            self.prompt = Some(crate::menu::Then::Demo(path.to_path_buf()));
        } else {
            self.open_demo_now(path);
        }
    }

    /// File > Open from Collection (Random) (task-326): a song chosen uniformly among those
    /// the index says can open (no read error, a mod that resolves: the built-in kit, one of
    /// the collection, or, with a mods folder of the computer in use, one that folder has),
    /// not the open song, opened as the browser's choice is.
    pub fn web_open_random(&mut self) {
        match COLLECTION.with_borrow(|c| c.clone()) {
            CollectionState::Ready(..) => {}
            CollectionState::Loading => {
                self.status =
                    Some("The collection index is still loading; try again in a moment".into());
                return;
            }
            CollectionState::Failed(e) => {
                self.fail(format!("The collection is not available: {e}"));
                load_index();
                return;
            }
        }
        let current = self
            .doc
            .path
            .as_deref()
            .and_then(crate::collection::Link::song_of_mem_path);
        let local = !crate::collection::is_mods_folder(self.settings.mods_dir.as_deref());
        let mut memo = std::collections::HashMap::<String, bool>::new();
        let mut lookup = |name: &str| {
            *memo
                .entry(name.to_string())
                .or_insert_with(|| self.resolve_mod_file(name).is_ok_and(|m| m.is_some()))
        };
        let r = js_sys::Math::random();
        let path = INDEX.with_borrow(|i| {
            let local: Option<&mut dyn FnMut(&str) -> bool> =
                if local { Some(&mut lookup) } else { None };
            i.as_ref()?
                .random_song(local, current.as_deref(), r)
                .map(|s| s.file.mem_path())
        });
        match path {
            Some(p) => self.web_open_song(&p),
            None => self.status = Some("No song of the collection can be opened".into()),
        }
    }

    /// The browser went Back or Forward to the history entry `n` (task-326): the song its link
    /// names opens through the normal path (an entry with no link is File > New), unless that
    /// song is open already. A song that does not open (the prompt cancelled, a failed
    /// download) sends the browser back to the entry it left ([`App::web_nav_frame`]).
    fn web_popstate(&mut self, link: Option<crate::collection::Link>, n: i64) {
        let from = HIST_N.replace(n);
        if link == self.web_nav.doc_link {
            return;
        }
        self.web_nav.from = Some(from);
        match link {
            Some(l) => self.web_open_link(l),
            None => self.command(crate::menu::Cmd::New),
        }
    }

    /// The address bar follows the page (task-326, `docs/web.md` section 9.4), each frame:
    /// - a new document: a collection song or mod demo is pushed as a history entry (Back and
    ///   Forward go between them); a local file or New removes the link (replaced, no entry); a
    ///   link's `mod` is applied once its song is open;
    /// - a change of the song's mod (Mods menu, a collection mod chosen, a variant): `mod` is set,
    ///   or removed when it is the mod the song resolves to by itself (replaced, no entry);
    /// - `?skin=` names the skin shown, replaced (no entry) when View switches it;
    /// - a Back or Forward whose song did not open (settled: no prompt, download or read left) goes
    ///   back to the entry it left.
    fn web_nav_frame(&mut self) {
        use crate::collection::{Link, with_skin};
        let q = location_search();
        let cur = Link::parse(&q);
        let mut want = cur.clone();
        let mut push = false;
        let mut new_doc = false;
        if self.doc_serial != self.web_nav.seen {
            self.web_nav.seen = self.doc_serial;
            // (the first documents of a load wait for the page's own link to be opened)
            if self.web_nav.initial.is_none() {
                new_doc = true;
                self.web_nav.demo = self.web_nav.demo_of.take();
                let pending = self
                    .web_nav
                    .pending_mod
                    .take()
                    .filter(|(s, _)| self.doc.path.as_deref() == Some(s.as_path()));
                if let Some((_, m)) = pending {
                    // the address keeps the link's mod while it is applied
                    if orb_cli::fs::is_lazy(&m) {
                        self.web_load_then(m.clone(), Picked::LinkMod(m));
                    } else {
                        self.web_link_mod(&m);
                    }
                } else {
                    let link = self.web_doc_link();
                    push = link.is_some() && link != cur;
                    want = link;
                }
            }
        }
        let doc_link = self.web_doc_link();
        let reading = self
            .status
            .as_deref()
            .is_some_and(|s| s.starts_with("Reading "));
        if !new_doc
            && doc_link != cur
            && !self.asking()
            && !reading
            && self.web_nav.pending_mod.is_none()
            && self.web_nav.from.is_none()
            && self.web_nav.initial.is_none()
        {
            // the song's mod changed, or the document is not what the address names
            want = doc_link.clone();
        }
        self.web_nav.doc_link = doc_link;
        let target = with_skin(&Link::with_link(&q, want.as_ref()), self.theme().name());
        if target != q {
            set_url(&target, push);
        }
        if let Some(from) = self.web_nav.from
            && !self.asking()
            && !reading
            && self.web_nav.initial.is_none()
        {
            self.web_nav.from = None;
            self.web_nav.pending_mod = None;
            if Link::parse(&location_search()) != self.web_nav.doc_link {
                let at = HIST_N.get();
                if from != at {
                    if let Some(h) = history() {
                        let _ = h.go_with_delta((from - at) as i32);
                    }
                } else {
                    let q = location_search();
                    let t = Link::with_link(&q, self.web_nav.doc_link.as_ref());
                    set_url(&with_skin(&t, self.theme().name()), false);
                }
            }
        }
    }

    /// File > Open from Collection...: the in-app browser on the song collection, in the folder of
    /// the last song chosen there. While the index loads, or when it could not be read, a
    /// message says so (and a failed index is asked for again).
    pub fn web_open_collection(&mut self) {
        match COLLECTION.with_borrow(|c| c.clone()) {
            CollectionState::Ready(..) => {
                let dir = self
                    .settings
                    .last_song_dir
                    .clone()
                    .filter(|d| d.starts_with(COLLECTION_SONGS) && orb_cli::fs::is_dir(d))
                    .unwrap_or_else(|| PathBuf::from(COLLECTION_SONGS));
                self.open_browser_in(crate::browser::Mode::OpenSong, &dir);
            }
            CollectionState::Loading => {
                self.status =
                    Some("The collection index is still loading; try again in a moment".into())
            }
            CollectionState::Failed(e) => {
                self.fail(format!("The collection is not available: {e}"));
                load_index();
            }
        }
    }

    /// Mods > Use Collection checked again (task-174, the user's review of 2026-11-27): the Mods
    /// folder is the mod collection again; the picked folder is no longer used (its files stay
    /// in memory: the song may play one of its mods). The Mods menu is read again.
    pub fn web_use_collection(&mut self) {
        let _ = orb_cli::fs::create_dir_all(Path::new(COLLECTION_MODS));
        self.settings
            .set_mods_dir(Some(PathBuf::from(COLLECTION_MODS)));
        if let Err(e) = self.settings.save() {
            self.fail(e);
        }
        self.refresh_mods();
        let n = self
            .mods()
            .and_then(|m| m.as_ref().ok())
            .map_or(0, |m| m.len());
        self.status = Some(format!(
            "Mods from the collection: {n} mod{}",
            if n == 1 { "" } else { "s" }
        ));
    }

    /// Draws the download dialog while a collection file downloads (the name, a bar, Cancel)
    /// or after it failed (the error, Close). Esc cancels or closes.
    pub fn web_download_ui(&mut self, ctx: &egui::Context) {
        use crate::palette::{INK, INK_DIM, SLATE_DARK, SLATE_LINE};
        let Some(d) = DOWNLOAD.with_borrow(|d| d.clone()) else {
            return;
        };
        let esc = ctx.input_mut(|i| {
            i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)
                || (d.error.is_some() && i.consume_key(egui::Modifiers::NONE, egui::Key::Enter))
        });
        let frame = egui::Frame::new()
            .fill(SLATE_DARK)
            .stroke(egui::Stroke::new(1.0, SLATE_LINE))
            .inner_margin(egui::Margin::same(12))
            .corner_radius(3.0);
        let mut close = esc;
        egui::Modal::new(egui::Id::unique("orb_download"))
            .frame(frame)
            .backdrop_color(egui::Color32::from_black_alpha(120))
            .show(ctx, |ui| {
                ui.set_width(360.0);
                ui.visuals_mut().override_text_color = Some(INK);
                let title = if d.error.is_some() {
                    "Download failed"
                } else {
                    "Downloading"
                };
                ui.label(egui::RichText::new(title).strong().size(15.0));
                ui.add_space(4.0);
                ui.label(&d.name);
                ui.add_space(4.0);
                match &d.error {
                    Some(e) => {
                        ui.colored_label(egui::Color32::from_rgb(0xe2, 0x41, 0x3b), e);
                    }
                    None => {
                        let f = if d.total > 0 {
                            (d.done as f32 / d.total as f32).min(1.0)
                        } else {
                            0.0
                        };
                        ui.add(egui::ProgressBar::new(f).desired_width(360.0));
                        ui.label(
                            egui::RichText::new(format!(
                                "{} of {}",
                                crate::browser::human_size(d.done),
                                crate::browser::human_size(d.total)
                            ))
                            .color(INK_DIM),
                        );
                    }
                }
                ui.add_space(6.0);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let text = if d.error.is_some() { "Close" } else { "Cancel" };
                    if ui.button(text).clicked() {
                        close = true;
                    }
                });
            });
        if close {
            cancel_download(true);
        }
    }
}

fn file_name(p: &Path) -> String {
    p.file_name().map_or_else(
        || p.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    )
}
