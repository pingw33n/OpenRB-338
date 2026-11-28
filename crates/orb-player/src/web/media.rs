//! The Media Session (task-329, `docs/web.md` section 4.3): the page tells the system what plays
//! (`navigator.mediaSession.metadata`: the song's name, the artist "OpenRB-338") and whether it
//! plays (`playbackState`), and takes the system's play, pause and stop actions (Control Center,
//! the lock screen, headphones). So a sound the page makes always shows there with a working
//! stop (the user's report of 2026-11-28: an iOS in-app browser kept playing after it closed).
//!
//! Pause and stop close the audio context at once, as an unloaded page does
//! ([`super::WebOutput`]), and the owner shows the song stopped; play is a gesture for the
//! output, and the owner sends the player's Play at its next frame ([`take_action`]). Without the
//! API nothing changes but one line in the console.

use std::cell::RefCell;

use js_sys::{Array, Function, Object, Reflect};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;

/// A system action for the owner's player.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MediaAction {
    Play,
    Stop,
}

/// An action handler, kept alive with the session.
type Handler = Closure<dyn FnMut(JsValue)>;

#[derive(Default)]
struct Media {
    /// The handlers were set (or the API is missing: `None` then, said once).
    handlers: Option<Vec<Handler>>,
    said: bool,
    title: String,
    state: &'static str,
    action: Option<MediaAction>,
    /// The handlers' calls, for the checks.
    calls: u32,
    /// Called when an action arrives (a repaint).
    notify: Option<Box<dyn Fn()>>,
}

thread_local! {
    static MEDIA: RefCell<Media> = RefCell::new(Media::default());
}

/// `navigator.mediaSession`, when the browser has it.
fn session() -> Option<JsValue> {
    let nav = web_sys::window()?.navigator();
    Reflect::get(&nav, &"mediaSession".into())
        .ok()
        .filter(|s| !s.is_undefined() && !s.is_null())
}

/// Called when a system action arrives.
pub fn set_notify(f: Box<dyn Fn()>) {
    MEDIA.with_borrow_mut(|m| m.notify = Some(f));
}

/// What a handler does (also the checks' stand-in, `WebOutput::debug`).
pub(super) fn act(a: MediaAction) {
    match a {
        MediaAction::Play => super::media_gesture(),
        MediaAction::Stop => super::media_stop(),
    }
    let notify = MEDIA.with_borrow_mut(|m| {
        m.action = Some(a);
        m.calls += 1;
        m.notify.take()
    });
    if let Some(f) = notify {
        f();
        MEDIA.with_borrow_mut(|m| m.notify = Some(f));
    }
}

fn install(m: &mut Media, s: &JsValue) {
    let Some(set) = Reflect::get(s, &"setActionHandler".into())
        .ok()
        .and_then(|f| f.dyn_into::<Function>().ok())
    else {
        m.handlers = Some(Vec::new());
        return;
    };
    let mut kept = Vec::new();
    for (name, a) in [
        ("play", MediaAction::Play),
        ("pause", MediaAction::Stop),
        ("stop", MediaAction::Stop),
    ] {
        let f = Closure::<dyn FnMut(JsValue)>::new(move |_d: JsValue| act(a));
        // a browser may refuse an action it does not know ("stop" in older Safari)
        let _ = set.call2(s, &name.into(), f.as_ref().unchecked_ref());
        kept.push(f);
    }
    m.handlers = Some(kept);
}

/// The song's name and whether it plays, each frame: the metadata and `playbackState` change
/// only when these do.
pub fn set(title: &str, playing: bool) {
    let state = if playing { "playing" } else { "paused" };
    MEDIA.with_borrow_mut(|m| {
        if m.title == title && m.state == state && m.handlers.is_some() {
            return;
        }
        let Some(s) = session() else {
            if !m.said {
                m.said = true;
                web_sys::console::log_1(&"orb: no Media Session API here".into());
            }
            m.handlers = Some(Vec::new());
            m.title = title.into();
            m.state = state;
            return;
        };
        if m.handlers.is_none() {
            install(m, &s);
        }
        if m.title != title || m.state.is_empty() {
            let init = Object::new();
            let _ = Reflect::set(&init, &"title".into(), &title.into());
            let _ = Reflect::set(&init, &"artist".into(), &"OpenRB-338".into());
            let meta = web_sys::window()
                .and_then(|w| Reflect::get(&w, &"MediaMetadata".into()).ok())
                .and_then(|c| c.dyn_into::<Function>().ok())
                .and_then(|c| Reflect::construct(&c, &Array::of1(&init)).ok());
            if let Some(meta) = meta {
                let _ = Reflect::set(&s, &"metadata".into(), &meta);
            }
        }
        let _ = Reflect::set(&s, &"playbackState".into(), &state.into());
        m.title = title.into();
        m.state = state;
    });
}

/// The system action since the last call, for the owner's player.
pub fn take_action() -> Option<MediaAction> {
    MEDIA.with_borrow_mut(|m| m.action.take())
}

/// The session as JSON, for the checks: `{"api", "title", "state", "calls"}`.
pub fn state() -> String {
    let api = session().is_some();
    MEDIA.with_borrow(|m| {
        format!(
            "{{\"api\":{api},\"title\":{:?},\"state\":\"{}\",\"calls\":{}}}",
            m.title, m.state, m.calls
        )
    })
}
