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
//! every `pointerdown`, `keydown` and `touchend` of the document while it is not running, and
//! when the tab is shown again (`visibilitychange`), in case the browser suspended it while the
//! tab was hidden.

use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};

use js_sys::{Array, ArrayBuffer, Object, Reflect, Uint8Array};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    AudioContext, AudioContextOptions, AudioContextState, AudioWorkletNode,
    AudioWorkletNodeOptions, Blob, BlobPropertyBag, MessageEvent, Response, Url, VisibilityState,
};

use crate::RATE;
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

/// The running output: the AudioContext and the node. Dropping it closes the context.
pub struct WebOutput {
    id: u32,
    ctx: AudioContext,
    node: AudioWorkletNode,
    /// The listeners, kept alive with the output (and removed when it is dropped).
    _on_message: Closure<dyn FnMut(MessageEvent)>,
    resume: Closure<dyn FnMut(web_sys::Event)>,
}

const GESTURES: [&str; 3] = ["pointerdown", "keydown", "touchend"];

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

        let parts = Array::of1(&JsValue::from_str(PROCESSOR_JS));
        let bag = BlobPropertyBag::new();
        bag.set_type("text/javascript");
        let blob = Blob::new_with_str_sequence_and_options(&parts, &bag)?;
        let url = Url::create_object_url_with_blob(&blob)?;
        let added = ctx.audio_worklet()?.add_module(&url);
        let added = JsFuture::from(added?).await;
        Url::revoke_object_url(&url)?;
        added?;

        let resp: Response = JsFuture::from(window.fetch_with_str(worklet_wasm_url))
            .await?
            .dyn_into()?;
        if !resp.ok() {
            return Err(format!("{worklet_wasm_url}: HTTP {}", resp.status()).into());
        }
        let wasm: ArrayBuffer = JsFuture::from(resp.array_buffer()?).await?.dyn_into()?;

        let po = Object::new();
        Reflect::set(&po, &"wasm".into(), &wasm)?;
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
        let on_message = Closure::<dyn FnMut(MessageEvent)>::new(move |e: MessageEvent| {
            let data = e.data();
            MAILBOXES.with(|m| {
                let mut m = m.borrow_mut();
                let Some(b) = m.get_mut(&id) else { return };
                if let Some(buf) = data.dyn_ref::<ArrayBuffer>() {
                    b.inbox.push_back(Uint8Array::new(buf).to_vec());
                } else if let Ok(s) = Reflect::get(&data, &"stats".into())
                    && !s.is_undefined()
                {
                    b.stats = Some(s);
                }
            });
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

        let c = ctx.clone();
        let resume = Closure::<dyn FnMut(web_sys::Event)>::new(move |_e: web_sys::Event| {
            let hidden = web_sys::window()
                .and_then(|w| w.document())
                .is_some_and(|d| d.visibility_state() == VisibilityState::Hidden);
            if !hidden && c.state() != AudioContextState::Running {
                // a refused resume (no gesture yet) is retried at the next event
                let _ = c.resume();
            }
        });
        // In the capture phase: a page's own handlers may stop the events (eframe stops the
        // canvas's pointer and key events), and a listener of the bubbling phase on the document
        // would then never hear the gesture (task-172: Play did nothing in Brave)
        for ev in GESTURES.into_iter().chain(["visibilitychange"]) {
            document.add_event_listener_with_callback_and_bool(
                ev,
                resume.as_ref().unchecked_ref(),
                true,
            )?;
        }
        Ok(WebOutput {
            id,
            ctx,
            node,
            _on_message: on_message,
            resume,
        })
    }

    /// The transport of a player to this output's worklet. One player per output: the
    /// messages from the worklet go to whichever transport polls first.
    pub fn transport(&self) -> WebTransport {
        WebTransport { id: self.id }
    }

    /// The AudioContext (its state, rate and latencies).
    pub fn context(&self) -> &AudioContext {
        &self.ctx
    }

    /// The last statistics the processor posted (`{quanta, maxGapMs, dead}`), for diagnostics.
    pub fn worklet_stats(&self) -> Option<JsValue> {
        MAILBOXES.with(|m| m.borrow().get(&self.id)?.stats.clone())
    }

    /// Resumes the context now (call it from a user gesture's handler).
    pub fn resume(&self) -> Result<js_sys::Promise, JsValue> {
        self.ctx.resume()
    }
}

impl Drop for WebOutput {
    fn drop(&mut self) {
        if let Some(d) = web_sys::window().and_then(|w| w.document()) {
            for ev in GESTURES.into_iter().chain(["visibilitychange"]) {
                let _ = d.remove_event_listener_with_callback_and_bool(
                    ev,
                    self.resume.as_ref().unchecked_ref(),
                    true,
                );
            }
        }
        let _ = self.node.disconnect();
        if let Ok(port) = self.node.port() {
            port.set_onmessage(None);
            port.close();
        }
        let _ = self.ctx.close();
        MAILBOXES.with(|m| m.borrow_mut().remove(&self.id));
    }
}
