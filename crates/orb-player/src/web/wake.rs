//! The screen wake lock while a song plays (task-329, `docs/web.md` section 4.2): when an iPhone
//! locks its screen, Safari suspends the page's audio, so the page holds a Screen Wake Lock
//! (`navigator.wakeLock.request("screen")`) while the song plays and lets it go on Stop.
//!
//! The browser lets the lock go when the page is hidden (the sentinel's `release` event); it is
//! asked for again when the page is visible while the song plays. Without the API (a browser
//! without it, or a page that is not a secure context: http on a LAN address) or when the
//! request is refused, nothing changes but one line in the console.

use std::cell::RefCell;

use js_sys::{Function, Reflect};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::VisibilityState;

#[derive(Default)]
struct Wake {
    /// The song plays: the lock is wanted.
    want: bool,
    sentinel: Option<JsValue>,
    /// A request is under way.
    asking: bool,
    /// The one console line about a missing API or a refusal was written.
    said: bool,
    /// The page's `visibilitychange` listener, made at the first request.
    on_visibility: Option<Closure<dyn FnMut(web_sys::Event)>>,
    on_release: Option<Closure<dyn FnMut(web_sys::Event)>>,
    /// The checks' hidden page (`WebOutput::debug`'s `hide`).
    fake_hidden: bool,
    /// What happened last, and the counts: for the checks (`state`).
    last: &'static str,
    requests: u32,
    grants: u32,
    releases: u32,
}

thread_local! {
    static WAKE: RefCell<Wake> = RefCell::new(Wake {
        last: "none",
        ..Wake::default()
    });
}

fn hidden(w: &Wake) -> bool {
    w.fake_hidden
        || web_sys::window()
            .and_then(|w| w.document())
            .is_some_and(|d| d.visibility_state() == VisibilityState::Hidden)
}

fn say_once(w: &mut Wake, what: &str) {
    if !w.said {
        w.said = true;
        web_sys::console::log_1(&format!("orb: {what}").into());
    }
}

/// The song plays (`true`) or not: the lock is asked for or let go.
pub fn set_playing(on: bool) {
    let changed = WAKE.with_borrow_mut(|w| std::mem::replace(&mut w.want, on) != on);
    if !changed {
        return;
    }
    if on {
        request();
    } else {
        release();
    }
}

/// The page was shown (the lock is asked for again while the song plays) or hidden (it is let
/// go, which the browser has done already).
pub fn page_visibility(visible: bool) {
    WAKE.with_borrow_mut(|w| w.fake_hidden = !visible);
    if visible {
        request();
    } else {
        release();
    }
}

/// What happened last to the lock (none, asking, held, released, unavailable or refused): the
/// `?debug-audio` status line's word.
pub fn last() -> &'static str {
    WAKE.with_borrow(|w| w.last)
}

/// The lock's state as JSON, for the checks: `{"want", "held", "last", "requests", "grants",
/// "releases"}` (`last`: none, asking, held, released, unavailable or refused).
pub fn state() -> String {
    WAKE.with_borrow(|w| {
        format!(
            concat!(
                "{{\"want\":{},\"held\":{},\"last\":\"{}\",\"requests\":{},\"grants\":{},",
                "\"releases\":{}}}"
            ),
            w.want,
            w.sentinel.is_some(),
            w.last,
            w.requests,
            w.grants,
            w.releases
        )
    })
}

fn listen_visibility(w: &mut Wake) {
    if w.on_visibility.is_some() {
        return;
    }
    let Some(d) = web_sys::window().and_then(|w| w.document()) else {
        return;
    };
    let f = Closure::<dyn FnMut(web_sys::Event)>::new(|_e: web_sys::Event| {
        let visible = web_sys::window()
            .and_then(|w| w.document())
            .is_none_or(|d| d.visibility_state() != VisibilityState::Hidden);
        page_visibility(visible);
    });
    let _ = d.add_event_listener_with_callback("visibilitychange", f.as_ref().unchecked_ref());
    w.on_visibility = Some(f);
}

fn request() {
    let promise = WAKE.with_borrow_mut(|w| {
        listen_visibility(w);
        // A sentinel held is taken as the lock held. A lock the browser ends without a `release`
        // event (none is known to) would be asked for again only after a hide or a Stop.
        if !w.want || w.asking || w.sentinel.is_some() || hidden(w) {
            return None;
        }
        let nav = web_sys::window()?.navigator();
        let lock = Reflect::get(&nav, &"wakeLock".into())
            .ok()
            .filter(|l| !l.is_undefined() && !l.is_null());
        let Some(lock) = lock else {
            w.last = "unavailable";
            say_once(
                w,
                concat!(
                    "no Screen Wake Lock API here (it needs a secure context: https or ",
                    "localhost); the screen may lock while a song plays"
                ),
            );
            return None;
        };
        let asked = Reflect::get(&lock, &"request".into())
            .ok()
            .and_then(|f| f.dyn_into::<Function>().ok())
            .and_then(|f| f.call1(&lock, &"screen".into()).ok())
            .and_then(|p| p.dyn_into::<js_sys::Promise>().ok());
        let Some(p) = asked else {
            w.last = "refused";
            say_once(w, "the screen wake lock request failed");
            return None;
        };
        w.asking = true;
        w.last = "asking";
        w.requests += 1;
        Some(p)
    });
    let Some(p) = promise else {
        return;
    };
    wasm_bindgen_futures::spawn_local(async move {
        let got = JsFuture::from(p).await;
        let let_go = WAKE.with_borrow_mut(|w| {
            w.asking = false;
            match got {
                Ok(s) => {
                    w.grants += 1;
                    if !w.want || hidden(w) {
                        // stopped (or hidden) while asking
                        return Some(s);
                    }
                    let f = w.on_release.get_or_insert_with(|| {
                        Closure::<dyn FnMut(web_sys::Event)>::new(|e: web_sys::Event| {
                            WAKE.with_borrow_mut(|w| {
                                w.releases += 1;
                                w.last = "released";
                                // the event of a sentinel let go before may come after a new
                                // one was granted
                                let ours = match (e.target(), &w.sentinel) {
                                    (Some(t), Some(s)) => js_sys::Object::is(&t, s),
                                    _ => false,
                                };
                                if ours {
                                    w.sentinel = None;
                                }
                            });
                        })
                    });
                    if let Some(t) = s.dyn_ref::<web_sys::EventTarget>() {
                        let _ = t.add_event_listener_with_callback(
                            "release",
                            f.as_ref().unchecked_ref(),
                        );
                    }
                    w.sentinel = Some(s);
                    w.last = "held";
                    None
                }
                Err(e) => {
                    w.last = "refused";
                    let why = e
                        .dyn_ref::<js_sys::Error>()
                        .map(|e| String::from(e.message()))
                        .unwrap_or_else(|| format!("{e:?}"));
                    say_once(w, &format!("the screen wake lock was refused: {why}"));
                    None
                }
            }
        });
        if let Some(s) = let_go {
            release_sentinel(&s);
        }
    });
}

fn release_sentinel(s: &JsValue) {
    if let Some(f) = Reflect::get(s, &"release".into())
        .ok()
        .and_then(|f| f.dyn_into::<Function>().ok())
    {
        let _ = f.call0(s);
    }
}

fn release() {
    // the sentinel's `release` event clears it; taken here so that a new request may start
    let s = WAKE.with_borrow_mut(|w| w.sentinel.take());
    if let Some(s) = s {
        release_sentinel(&s);
    }
}
