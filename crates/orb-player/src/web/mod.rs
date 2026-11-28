//! The web output (task-171, task-170's path C; `docs/web.md`): the browser glue of the UI's
//! wasm instance, wasm32 with the feature `web` only.
//!
//! [`WebOutput::start`] makes the AudioContext at 44.1 kHz (the device rate if the browser
//! refuses that), loads the processor script ([`PROCESSOR_JS`]) into its worklet, fetches the
//! worklet's wasm module (`crates/orb-worklet`) and makes the `AudioWorkletNode` that runs it,
//! connected to the speakers. [`WebOutput::transport`] is the [`Transport`] of a
//! [`crate::remote::Player`] to it.
//!
//! The browser keeps an AudioContext suspended until a user gesture: the output resumes it on
//! every `pointerdown`, `pointerup`, `mousedown`, `mouseup`, `click`, `keydown`, `keyup` and
//! `touchend` of the document while it never ran.
//!
//! A hidden page plays on (task-329, `docs/web.md` section 4.1; the user's decision of
//! 2026-11-28); back on the page a context the browser suspended is resumed. After an
//! interruption the context may also stay interrupted or closed, or run a worklet that renders
//! nothing: the owner then shows the song stopped at once ([`WebOutput::take_stopped`]), and an
//! unloaded page stops it. [`crate::recovery::Recovery`] decides what to do; with
//! [`WebOutput::enable_rebuild`] a gesture then makes a new context and worklet, which
//! [`WebOutput::poll_recovery`] swaps in for the owner to give the song again
//! ([`crate::remote::Player::reconnect`]). [`wake`] keeps the screen on while a song plays, and
//! [`media`] shows it to the system (the Media Session).

pub mod media;
pub mod wake;

use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::rc::{Rc, Weak};

use js_sys::{Array, ArrayBuffer, Object, Reflect, Uint8Array};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    AudioContext, AudioContextOptions, AudioWorkletNode, AudioWorkletNodeOptions, Blob,
    BlobPropertyBag, MessageEvent, Response, Url, VisibilityState,
};

use crate::RATE;
use crate::recovery::{Action, CtxState, Recovery, TICK_MS};
use crate::remote::Transport;

/// The AudioWorkletProcessor script, registered as `orb-engine`.
pub const PROCESSOR_JS: &str = include_str!("processor.js");

/// What the port of one output received, by output.
#[derive(Default)]
struct Mailbox {
    port: Option<web_sys::MessagePort>,
    inbox: VecDeque<Vec<u8>>,
    stats: Option<JsValue>,
}

thread_local! {
    // wasm32 without atomics has one thread: the ports live here so that the transports,
    // which hold only an id, are Send as the native player is.
    static MAILBOXES: RefCell<HashMap<u32, Mailbox>> = RefCell::new(HashMap::new());
    static NEXT: RefCell<u32> = const { RefCell::new(1) };
    /// The latest output's shared state, for the Media Session's actions ([`media`]).
    static CURRENT: RefCell<Weak<RefCell<Shared>>> = const { RefCell::new(Weak::new()) };
}

/// The transport to one output's worklet: [`crate::remote::Player::new`] takes it.
pub struct WebTransport {
    id: u32,
}

impl Transport for WebTransport {
    fn post(&mut self, msg: Vec<u8>) {
        MAILBOXES.with(|m| {
            if let Some(port) = m.borrow().get(&self.id).and_then(|b| b.port.as_ref()) {
                let a = Uint8Array::from(&msg[..]);
                let buf = a.buffer();
                let transfer = Array::of1(&buf);
                // a closed port drops the message; the player's errors say nothing then
                let _ = port.post_message_with_transferable(&buf, &transfer);
            }
        });
    }

    fn poll(&mut self) -> Option<Vec<u8>> {
        MAILBOXES.with(|m| m.borrow_mut().get_mut(&self.id)?.inbox.pop_front())
    }
}

/// One context and its node (dropping it closes the context).
struct Inner {
    id: u32,
    ctx: AudioContext,
    node: AudioWorkletNode,
    _on_message: Closure<dyn FnMut(MessageEvent)>,
    _on_state: Closure<dyn FnMut(web_sys::Event)>,
}

impl Drop for Inner {
    fn drop(&mut self) {
        let _ = self.node.disconnect();
        if let Ok(port) = self.node.port() {
            port.set_onmessage(None);
            port.close();
        }
        self.ctx.set_onstatechange(None);
        let _ = self.ctx.close();
        MAILBOXES.with(|m| m.borrow_mut().remove(&self.id));
    }
}

/// What the listeners share with the output.
struct Shared {
    machine: Recovery,
    /// The current context and its mailbox.
    id: u32,
    ctx: AudioContext,
    /// The worklet's module, for a rebuild.
    wasm: ArrayBuffer,
    rebuild: bool,
    /// A rebuild's new context, until [`WebOutput::poll_recovery`] swaps it in.
    pending: Option<AudioContext>,
    replacement: Option<Inner>,
    /// Why the output was rebuilt, until the owner takes it.
    why: Option<&'static str>,
    /// The song cannot be heard any more (the output broke, the page was unloaded, the system's
    /// pause or stop came): the owner shows it stopped at its next frame
    /// ([`WebOutput::take_stopped`]).
    stopped: bool,
    /// The machine's break was told to the owner (`Shared::judge`).
    told: bool,
    /// The debug hooks' stand-ins for what Brave cannot do (`WebOutput::debug`).
    fake_state: Option<CtxState>,
    fake_hidden: bool,
    /// The recent events, newest last, with the seconds since the start.
    events: VecDeque<String>,
    start: f64,
    notify: Option<Box<dyn Fn()>>,
}

/// The first byte of a wire `FromWorklet::Error` (`crate::wire`; `processor.js` writes it too).
const ERROR_TAG: u8 = 4;

/// The events [`WebOutput::recovery_state`] keeps.
const EVENTS: usize = 12;

fn now() -> f64 {
    js_sys::Date::now()
}

/// `AudioContext.state` as a string: web-sys knows only three of them, not iOS's
/// "interrupted" (which its enum turns into a hidden invalid variant).
fn state_of(ctx: &AudioContext) -> CtxState {
    Reflect::get(ctx, &"state".into())
        .ok()
        .and_then(|s| s.as_string())
        .map_or(CtxState::Suspended, |s| CtxState::parse(&s))
}

fn page_visible() -> bool {
    web_sys::window()
        .and_then(|w| w.document())
        .is_none_or(|d| d.visibility_state() != VisibilityState::Hidden)
}

impl Shared {
    fn log(&mut self, what: String) {
        let t = (now() - self.start) / 1000.0;
        web_sys::console::log_1(&format!("orb audio: {what} ({t:.1} s)").into());
        self.events.push_back(format!("{t:.1}s {what}"));
        if self.events.len() > EVENTS {
            self.events.pop_front();
        }
    }

    fn state(&self) -> CtxState {
        self.fake_state.unwrap_or_else(|| state_of(&self.ctx))
    }

    /// Takes the context's state now, saying when it changed.
    fn refresh(&mut self) {
        let s = self.state();
        if s != self.machine.state() {
            self.log(format!("context {}", s.as_str()));
        }
        self.machine.context(s, now());
    }

    fn visible(&self) -> bool {
        !self.fake_hidden && page_visible()
    }

    fn notify(&self) {
        if let Some(f) = &self.notify {
            f();
        }
    }

    /// After the machine took an input: on a new break the context is closed and the owner told,
    /// which shows the song stopped at once (nothing is heard: the UI matches it), with a
    /// repaint.
    fn judge(&mut self) {
        if let Some(b) = self.machine.broken()
            && !self.told
        {
            self.told = true;
            self.stopped = true;
            // a context that comes back late must not sound under a panel that shows the song
            // stopped: the next gesture rebuilds anyway
            let _ = self.ctx.close();
            self.log(format!("broken: {}", b.as_str()));
            wake::set_playing(false);
            self.notify();
        }
    }
}

type SharedRc = Rc<RefCell<Shared>>;

impl Inner {
    /// The node on `ctx` with the worklet's module `wasm`, its port and its listeners.
    async fn build(
        ctx: AudioContext,
        wasm: &ArrayBuffer,
        shared: &SharedRc,
    ) -> Result<Inner, JsValue> {
        let parts = Array::of1(&JsValue::from_str(PROCESSOR_JS));
        let bag = BlobPropertyBag::new();
        bag.set_type("text/javascript");
        let blob = Blob::new_with_str_sequence_and_options(&parts, &bag)?;
        let url = Url::create_object_url_with_blob(&blob)?;
        let added = ctx.audio_worklet()?.add_module(&url);
        let added = JsFuture::from(added?).await;
        Url::revoke_object_url(&url)?;
        added?;

        let po = Object::new();
        Reflect::set(&po, &"wasm".into(), wasm)?;
        Reflect::set(&po, &"rate".into(), &JsValue::from(ctx.sample_rate()))?;
        let opts = AudioWorkletNodeOptions::new();
        opts.set_number_of_inputs(0);
        opts.set_number_of_outputs(1);
        opts.set_output_channel_count(&Array::of1(&JsValue::from(2)));
        opts.set_processor_options(Some(&po));
        let node = AudioWorkletNode::new_with_options(&ctx, "orb-engine", &opts)?;
        node.connect_with_audio_node(&ctx.destination())?;

        let id = NEXT.with(|n| {
            let mut n = n.borrow_mut();
            let id = *n;
            *n += 1;
            id
        });
        let port = node.port()?;
        let sh = shared.clone();
        let on_message = Closure::<dyn FnMut(MessageEvent)>::new(move |e: MessageEvent| {
            let data = e.data();
            let mut rendered = false;
            MAILBOXES.with(|m| {
                let mut m = m.borrow_mut();
                let Some(b) = m.get_mut(&id) else { return };
                if let Some(buf) = data.dyn_ref::<ArrayBuffer>() {
                    let m = Uint8Array::new(buf).to_vec();
                    rendered = m.first() != Some(&ERROR_TAG);
                    b.inbox.push_back(m);
                } else if let Ok(s) = Reflect::get(&data, &"stats".into())
                    && !s.is_undefined()
                {
                    b.stats = Some(s);
                }
            });
            // the worklet is alive: every engine call posts a snapshot. Its statistics and its
            // Error do not count: a trapped or failed worklet still posts them, and renders
            // nothing (the review of task-329)
            if rendered
                && let Ok(mut sh) = sh.try_borrow_mut()
                && sh.id == id
            {
                sh.machine.heard(now());
            }
        });
        port.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
        MAILBOXES.with(|m| {
            m.borrow_mut().insert(
                id,
                Mailbox {
                    port: Some(port),
                    ..Mailbox::default()
                },
            )
        });
        let sh = shared.clone();
        let on_state = Closure::<dyn FnMut(web_sys::Event)>::new(move |_e: web_sys::Event| {
            if let Ok(mut sh) = sh.try_borrow_mut()
                && sh.id == id
            {
                sh.refresh();
                sh.judge();
                sh.notify();
            }
        });
        ctx.set_onstatechange(Some(on_state.as_ref().unchecked_ref()));
        Ok(Inner {
            id,
            ctx,
            node,
            _on_message: on_message,
            _on_state: on_state,
        })
    }
}

/// Starts a rebuild: the new context is made and resumed now, in the gesture (iOS lets a
/// context start only there), its node once the worklet's module is added.
fn rebuild(shared: &SharedRc, sh: &mut Shared) {
    let why = sh
        .machine
        .broken()
        .map_or("the audio context stopped", |b| b.as_str());
    sh.log(format!("rebuilding: {why}"));
    let ctx = match context() {
        Ok(c) => c,
        Err(e) => {
            sh.log(format!("no new audio context: {e:?}"));
            return;
        }
    };
    let _ = ctx.resume();
    sh.pending = Some(ctx.clone());
    sh.why = Some(why);
    let wasm = sh.wasm.clone();
    let shared = shared.clone();
    wasm_bindgen_futures::spawn_local(async move {
        let built = Inner::build(ctx.clone(), &wasm, &shared).await;
        let mut sh = shared.borrow_mut();
        // a leave while it was built closed this context and forgot it (the review of
        // task-329): the node is dropped, which closes the rest, instead of being swapped in
        let current = sh
            .pending
            .as_ref()
            .is_some_and(|p| js_sys::Object::is(p, &ctx));
        match built {
            Ok(inner) if !current => {
                sh.log("a rebuild finished after the page was left: dropped".into());
                drop(inner);
                return;
            }
            Ok(inner) => sh.replacement = Some(inner),
            Err(e) => {
                let _ = ctx.close();
                sh.pending = None;
                sh.log(format!("the rebuild failed: {e:?}"));
            }
        }
        sh.notify();
    });
}

/// Leaves: the context is closed at once (an unloaded page, the system's stop, or a hidden page
/// where [`WebOutput::set_stop_on_hidden`] is set) and the wake lock let go; the owner shows the
/// song stopped at its next frame.
fn leave(sh: &mut Shared) {
    let _ = sh.ctx.close();
    if let Some(p) = sh.pending.take() {
        let _ = p.close();
    }
    sh.replacement = None;
    sh.judge();
}

/// The Media Session's play: a gesture for the context, whatever the page's visibility
/// ([`Recovery::media_play`]; [`media`]).
fn media_gesture() {
    if let Some(sh) = CURRENT.with_borrow(Weak::upgrade) {
        activate(&sh, true);
    }
}

/// The Media Session's pause or stop: the context is closed at once, as when the page is left,
/// and the next Play rebuilds it ([`media`]).
fn media_stop() {
    if let Some(sh) = CURRENT.with_borrow(Weak::upgrade)
        && let Ok(mut sh) = sh.try_borrow_mut()
    {
        sh.log("media stop".into());
        sh.machine.unload();
        let visible = sh.visible();
        sh.machine.visibility(visible, now());
        leave(&mut sh);
    }
}

/// A user gesture: resume, or rebuild ([`Recovery::gesture`]).
fn on_gesture(shared: &SharedRc) {
    activate(shared, false);
}

/// A gesture, or the system's Play (`media`: whatever the page's visibility).
fn activate(shared: &SharedRc, media: bool) {
    let Ok(mut sh) = shared.try_borrow_mut() else {
        return;
    };
    if let Some(p) = &sh.pending {
        // the rebuild's context: whichever event of the gesture the browser counts
        if state_of(p) != CtxState::Running {
            let _ = p.resume();
        }
        return;
    }
    sh.refresh();
    let action = if media {
        sh.machine.media_play(now())
    } else {
        sh.machine.gesture(now())
    };
    match action {
        Action::None | Action::Leave => {}
        Action::Rebuild if sh.rebuild => rebuild(shared, &mut sh),
        Action::Resume | Action::Rebuild => {
            let _ = sh.ctx.resume();
        }
    }
}

/// The page shown (`visibilitychange` to visible, `pageshow`, `focus`), hidden
/// (`visibilitychange` to hidden, `freeze`, a persisted `pagehide`) or unloaded (a `pagehide`
/// that is not persisted: the only stop of a page that is not set to stop when hidden), or
/// blurred (only logged).
fn on_visibility(shared: &SharedRc, what: &str, e: Option<&web_sys::Event>) {
    let Ok(mut sh) = shared.try_borrow_mut() else {
        return;
    };
    // a pagehide into the back/forward cache keeps the page: hidden, not unloaded
    let persisted = e
        .and_then(|e| Reflect::get(e, &"persisted".into()).ok())
        .is_some_and(|p| p.is_truthy());
    let action = match what {
        "blur" => {
            sh.log("blur".into());
            return;
        }
        "pagehide" if !persisted => {
            sh.log("pagehide: unload".into());
            sh.machine.unload()
        }
        _ => {
            let visible = what != "pagehide" && what != "freeze" && sh.visible();
            sh.log(format!(
                "{what}{}: {}",
                if persisted { " (persisted)" } else { "" },
                if visible { "visible" } else { "hidden" }
            ));
            sh.refresh();
            sh.machine.visibility(visible, now())
        }
    };
    match action {
        Action::Leave => leave(&mut sh),
        Action::Resume => {
            let _ = sh.ctx.resume();
        }
        Action::None | Action::Rebuild => {}
    }
    sh.judge();
    sh.notify();
}

/// The periodic check ([`Recovery::tick`]).
fn on_tick(shared: &SharedRc) {
    let Ok(mut sh) = shared.try_borrow_mut() else {
        return;
    };
    sh.refresh();
    sh.machine.tick(now());
    sh.judge();
}

/// A listener: the target, the event, the capture phase and the closure.
type Listener = (
    web_sys::EventTarget,
    &'static str,
    bool,
    Rc<Closure<dyn FnMut(web_sys::Event)>>,
);

/// The running output: the AudioContext and the node, replaced when it is rebuilt. Dropping
/// it closes the context.
pub struct WebOutput {
    inner: Inner,
    shared: SharedRc,
    /// The listeners, kept alive with the output (and removed when it is dropped).
    listeners: Vec<Listener>,
    interval: i32,
    _tick: Closure<dyn FnMut()>,
}

const GESTURES: [&str; 8] = [
    "pointerdown",
    "pointerup",
    "mousedown",
    "mouseup",
    "click",
    "keydown",
    "keyup",
    "touchend",
];

/// The context at 44.1 kHz (`latencyHint` "interactive"), or at the device's rate when the
/// browser refuses 44.1 kHz.
fn context() -> Result<AudioContext, JsValue> {
    let o = AudioContextOptions::new();
    o.set_sample_rate(RATE as f32);
    match AudioContext::new_with_context_options(&o) {
        Ok(c) => Ok(c),
        Err(e) => {
            web_sys::console::warn_2(
                &"orb: the browser refused a 44100 Hz AudioContext; resampling to the device rate"
                    .into(),
                &e,
            );
            AudioContext::new()
        }
    }
}

impl WebOutput {
    /// Starts the output with the worklet's wasm module at `worklet_wasm_url` (fetched). The
    /// context may stay suspended until the first user gesture.
    pub async fn start(worklet_wasm_url: &str) -> Result<WebOutput, JsValue> {
        let window = web_sys::window().ok_or("no window")?;
        let document = window.document().ok_or("no document")?;
        let ctx = context()?;

        let resp: Response = JsFuture::from(window.fetch_with_str(worklet_wasm_url))
            .await?
            .dyn_into()?;
        if !resp.ok() {
            return Err(format!("{worklet_wasm_url}: HTTP {}", resp.status()).into());
        }
        let wasm: ArrayBuffer = JsFuture::from(resp.array_buffer()?).await?.dyn_into()?;

        let t = now();
        let shared = Rc::new(RefCell::new(Shared {
            machine: Recovery::new(state_of(&ctx), page_visible(), t),
            id: 0,
            ctx: ctx.clone(),
            wasm: wasm.clone(),
            rebuild: false,
            pending: None,
            replacement: None,
            why: None,
            stopped: false,
            told: false,
            fake_state: None,
            fake_hidden: false,
            events: VecDeque::new(),
            start: t,
            notify: None,
        }));
        let inner = Inner::build(ctx, &wasm, &shared).await?;
        shared.borrow_mut().id = inner.id;
        CURRENT.with_borrow_mut(|c| *c = Rc::downgrade(&shared));

        let mut listeners: Vec<Listener> = Vec::new();
        let sh = shared.clone();
        let gesture = Rc::new(Closure::<dyn FnMut(web_sys::Event)>::new(
            move |_e: web_sys::Event| on_gesture(&sh),
        ));
        // In the capture phase: a page's own handlers may stop the events (eframe stops the
        // canvas's pointer and key events), and a listener of the bubbling phase on the document
        // would then never hear the gesture (task-172: Play did nothing in Brave)
        let doc: &web_sys::EventTarget = document.as_ref();
        for ev in GESTURES {
            listeners.push((doc.clone(), ev, true, gesture.clone()));
        }
        let win: &web_sys::EventTarget = window.as_ref();
        for (target, ev) in [
            (doc, "visibilitychange"),
            (doc, "freeze"),
            (win, "pagehide"),
            (win, "pageshow"),
            (win, "focus"),
            (win, "blur"),
        ] {
            let sh = shared.clone();
            let f = Rc::new(Closure::<dyn FnMut(web_sys::Event)>::new(
                move |e: web_sys::Event| on_visibility(&sh, ev, Some(&e)),
            ));
            listeners.push((target.clone(), ev, true, f));
        }
        for (target, ev, capture, f) in &listeners {
            target.add_event_listener_with_callback_and_bool(
                ev,
                f.as_ref().as_ref().unchecked_ref(),
                *capture,
            )?;
        }
        let sh = shared.clone();
        let tick = Closure::<dyn FnMut()>::new(move || on_tick(&sh));
        let interval = window.set_interval_with_callback_and_timeout_and_arguments_0(
            tick.as_ref().unchecked_ref(),
            TICK_MS as i32,
        )?;
        Ok(WebOutput {
            inner,
            shared,
            listeners,
            interval,
            _tick: tick,
        })
    }

    /// Lets a gesture rebuild the context and the worklet when they cannot play
    /// ([`crate::recovery`]); the owner then takes the new ones with
    /// [`WebOutput::poll_recovery`]. Without it a gesture only resumes the context.
    pub fn enable_rebuild(&self) {
        self.shared.borrow_mut().rebuild = true;
    }

    /// Called when the owner should look at the output again (a repaint): a rebuild is ready,
    /// the context's state or the page's visibility changed.
    pub fn set_notify(&self, f: Box<dyn Fn()>) {
        self.shared.borrow_mut().notify = Some(f);
    }

    /// Swaps in a rebuild's new context and worklet when they are ready: then the old ones are
    /// closed, [`WebOutput::transport`] is the new worklet's, and the answer is why the output
    /// was rebuilt. The owner gives the new worklet the song again
    /// ([`crate::remote::Player::reconnect`]).
    pub fn poll_recovery(&mut self) -> Option<&'static str> {
        let (inner, why) = {
            let mut sh = self.shared.borrow_mut();
            let inner = sh.replacement.take()?;
            sh.id = inner.id;
            sh.ctx = inner.ctx.clone();
            sh.pending = None;
            sh.fake_state = None;
            let s = state_of(&inner.ctx);
            sh.machine.rebuilt(s, now());
            sh.told = false;
            sh.stopped = false;
            let why = sh.why.take().unwrap_or("the audio context stopped");
            sh.log(format!("rebuilt, context {}", s.as_str()));
            (inner, why)
        };
        // the old context closes here, outside the borrow
        drop(std::mem::replace(&mut self.inner, inner));
        Some(why)
    }

    /// The owner could not give the swapped-in worklet the song ([`Recovery::restore_failed`]):
    /// the output is broken, the song shows stopped at the next frame, and the next gesture
    /// rebuilds again.
    pub fn restore_failed(&self) {
        let mut sh = self.shared.borrow_mut();
        sh.machine.restore_failed();
        sh.judge();
    }

    /// Whether the song stopped being heard since the last call (the output broke, the page was
    /// unloaded, the system's pause or stop came): the owner then shows it stopped at once
    /// (`remote::Player::stop_offline`), so the UI matches what is heard (task-329).
    pub fn take_stopped(&self) -> bool {
        std::mem::take(&mut self.shared.borrow_mut().stopped)
    }

    /// Whether a hidden page stops the song (closes the context) instead of playing on
    /// ([`Recovery::stop_on_hidden`]).
    pub fn set_stop_on_hidden(&self, on: bool) {
        self.shared.borrow_mut().machine.stop_on_hidden = on;
    }

    /// The context runs and nothing is broken: the song can be heard (the wake lock and the
    /// Media Session's state follow it with the play state).
    pub fn audible(&self) -> bool {
        let sh = self.shared.borrow();
        sh.machine.broken().is_none() && sh.state() == CtxState::Running
    }

    /// The transport of a player to this output's worklet. One player per output: the
    /// messages from the worklet go to whichever transport polls first.
    pub fn transport(&self) -> WebTransport {
        WebTransport { id: self.inner.id }
    }

    /// The AudioContext (its state, rate and latencies).
    pub fn context(&self) -> &AudioContext {
        &self.inner.ctx
    }

    /// The last statistics the processor posted (`{quanta, maxGapMs, dead}`), for diagnostics.
    pub fn worklet_stats(&self) -> Option<JsValue> {
        MAILBOXES.with(|m| m.borrow().get(&self.inner.id)?.stats.clone())
    }

    /// Resumes the context now (call it from a user gesture's handler).
    pub fn resume(&self) -> Result<js_sys::Promise, JsValue> {
        self.inner.ctx.resume()
    }

    /// The context's state as the recovery sees it, whether a rebuild is under way, and the
    /// recent events, newest last.
    pub fn recovery_state(&self) -> (CtxState, bool, Vec<String>) {
        let sh = self.shared.borrow();
        (
            sh.state(),
            sh.pending.is_some(),
            sh.events.iter().cloned().collect(),
        )
    }

    /// The checks' stand-ins for what an iPhone does (task-329, `docs/web.md` section 4.1):
    ///
    /// - `suspend`: the context is suspended, as a browser may do;
    /// - `interrupt`: the same, but reported as iOS's "interrupted" state;
    /// - `close`: the context is closed;
    /// - `stall`: the worklet stops rendering and posting, its context still running;
    /// - `hide`, `show`: the page counts as hidden (as `visibilitychange` to hidden), then shown
    ///   again;
    /// - `hide-suspend`: as `hide`, with the context suspended, as a browser may do;
    /// - `hide-dead`: as `hide-suspend`, but the context stays "interrupted" whatever resume() does
    ///   (iOS refusing a resume without a gesture);
    /// - `media-play`, `media-stop`: what the Media Session's play and pause or stop handlers do
    ///   ([`media`]).
    pub fn debug(&self, what: &str) -> Result<(), String> {
        let mut sh = self.shared.borrow_mut();
        sh.log(format!("debug: {what}"));
        match what {
            "suspend" => {
                let _ = sh.ctx.suspend();
            }
            "interrupt" => {
                let _ = sh.ctx.suspend();
                sh.fake_state = Some(CtxState::Interrupted);
                sh.refresh();
            }
            "close" => {
                let _ = sh.ctx.close();
            }
            "stall" => {
                let msg = Object::new();
                Reflect::set(&msg, &"debug".into(), &"stall".into())
                    .map_err(|e| format!("{e:?}"))?;
                let port = self.inner.node.port().map_err(|e| format!("{e:?}"))?;
                port.post_message(&msg).map_err(|e| format!("{e:?}"))?;
            }
            "hide" | "hide-suspend" | "hide-dead" | "show" => {
                // `hide-suspend` as iOS may do with a hidden page: the context is suspended (and,
                // for `hide-dead`, stays "interrupted" whatever resume() does)
                sh.fake_hidden = what != "show";
                if what == "hide-suspend" || what == "hide-dead" {
                    let _ = sh.ctx.suspend();
                }
                if what == "hide-dead" {
                    sh.fake_state = Some(CtxState::Interrupted);
                }
                drop(sh);
                on_visibility(&self.shared, "visibilitychange", None);
                // the browser's own visibilitychange tells the wake lock; the stand-in does
                wake::page_visibility(what == "show");
            }
            "media-play" | "media-stop" => {
                drop(sh);
                media::act(if what == "media-play" {
                    media::MediaAction::Play
                } else {
                    media::MediaAction::Stop
                });
            }
            _ => {
                return Err(format!(
                    "{what}: not one of suspend, interrupt, close, stall, hide, hide-suspend, \
                     hide-dead, show, media-play, media-stop"
                ));
            }
        }
        Ok(())
    }
}

impl Drop for WebOutput {
    fn drop(&mut self) {
        for (target, ev, capture, f) in &self.listeners {
            let _ = target.remove_event_listener_with_callback_and_bool(
                ev,
                f.as_ref().as_ref().unchecked_ref(),
                *capture,
            );
        }
        if let Some(w) = web_sys::window() {
            w.clear_interval_with_handle(self.interval);
        }
        let mut sh = self.shared.borrow_mut();
        sh.notify = None;
        sh.replacement = None;
        if let Some(p) = sh.pending.take() {
            let _ = p.close();
        }
    }
}
