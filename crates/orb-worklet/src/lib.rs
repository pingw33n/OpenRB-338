//! The web player's AudioWorklet instance (task-171, `docs/web.md`): [`Host`] behind a C ABI
//! that the processor script (`crates/orb-player/src/web/processor.js`) calls. Built for
//! `wasm32-unknown-unknown` as a plain module that imports nothing (no wasm-bindgen), so the
//! worklet instantiates it from its bytes with `WebAssembly.Instance` and an empty import
//! object.
//!
//! The calls, all on the worklet's one thread:
//!
//! - `orb_init(rate)`: the host for an AudioContext of `rate` Hz (again: a new host).
//! - `orb_buffer(len) -> ptr`: room for a message of `len` bytes in the module's memory; the script
//!   copies the message there, then `orb_receive(len)` takes it.
//! - `orb_render(frames) -> ptr`: renders a quantum; `frames` f32 of the left channel at `ptr`,
//!   then `frames` of the right.
//! - `orb_outgoing() -> len`: the next message for the UI is at `orb_outgoing_ptr()` (0: none).
//! - `orb_panic_len()`, `orb_panic_ptr()`: the message of a panic, after a call trapped. A panic
//!   hook keeps it in a cell of its own, which no call holds, so it is there even when the call
//!   that panicked held the host's state (a wasm32 panic aborts: nothing unwinds and that state
//!   stays borrowed; the script calls nothing else of a module that trapped).
//!
//! A pointer stays valid until the next call (memory may grow: the script makes its views
//! again after each call).

use std::cell::RefCell;

use orb_player::worklet::Host;

struct State {
    host: Option<Host>,
    input: Vec<u8>,
    output: Vec<u8>,
}

thread_local! {
    static STATE: RefCell<State> = const {
        RefCell::new(State {
            host: None,
            input: Vec::new(),
            output: Vec::new(),
        })
    };
    /// The last panic's message: apart from `STATE`, which the panicking call may hold.
    static PANIC: RefCell<String> = const { RefCell::new(String::new()) };
}

fn with<R>(f: impl FnOnce(&mut State) -> R) -> R {
    STATE.with(|s| f(&mut s.borrow_mut()))
}

/// The host for an AudioContext of `rate` Hz.
#[unsafe(no_mangle)]
pub extern "C" fn orb_init(rate: u32) {
    std::panic::set_hook(Box::new(|info| {
        let msg = info.to_string();
        // only the panic readers borrow this cell, and never across a call that can panic
        PANIC.with(|p| {
            if let Ok(mut p) = p.try_borrow_mut() {
                *p = msg;
            }
        });
    }));
    with(|s| s.host = Some(Host::new(rate)));
}

/// Room for a message of `len` bytes.
#[unsafe(no_mangle)]
pub extern "C" fn orb_buffer(len: u32) -> *mut u8 {
    with(|s| {
        s.input.resize(len as usize, 0);
        s.input.as_mut_ptr()
    })
}

/// Takes the message of `len` bytes in the buffer.
#[unsafe(no_mangle)]
pub extern "C" fn orb_receive(len: u32) {
    with(|s| {
        let State { host, input, .. } = s;
        if let Some(h) = host {
            h.receive(&input[..(len as usize).min(input.len())]);
        }
    })
}

/// Renders `frames` frames, planar.
#[unsafe(no_mangle)]
pub extern "C" fn orb_render(frames: u32) -> *const f32 {
    with(|s| match &mut s.host {
        Some(h) => h.render(frames as usize).as_ptr(),
        None => std::ptr::null(),
    })
}

/// The length of the next message for the UI, now at [`orb_outgoing_ptr`]; 0 when none.
#[unsafe(no_mangle)]
pub extern "C" fn orb_outgoing() -> u32 {
    with(|s| {
        let m = s.host.as_mut().and_then(|h| h.take_outgoing());
        s.output = m.unwrap_or_default();
        s.output.len() as u32
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn orb_outgoing_ptr() -> *const u8 {
    with(|s| s.output.as_ptr())
}

/// The length of the last panic's message.
#[unsafe(no_mangle)]
pub extern "C" fn orb_panic_len() -> u32 {
    PANIC.with(|p| p.try_borrow().map_or(0, |p| p.len() as u32))
}

#[unsafe(no_mangle)]
pub extern "C" fn orb_panic_ptr() -> *const u8 {
    PANIC.with(|p| p.try_borrow().map_or(std::ptr::null(), |p| p.as_ptr()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The calls in the order the processor script makes them: silence before the start, a
    /// message through the buffer, a quantum, the messages back.
    #[test]
    fn the_calls() {
        orb_init(44_100);
        let p = orb_render(128);
        assert!(!p.is_null());
        let n = orb_outgoing();
        assert!(n > 0, "Ready first");
        let ready = unsafe { std::slice::from_raw_parts(orb_outgoing_ptr(), n as usize) };
        assert_eq!(
            orb_player::wire::FromWorklet::decode(ready),
            Ok(orb_player::wire::FromWorklet::Ready { rate: 44_100 })
        );
        let m = orb_player::wire::ToWorklet::Init {
            kit: None,
            call_frames: 1024,
        }
        .encode();
        let b = orb_buffer(m.len() as u32);
        unsafe { std::ptr::copy_nonoverlapping(m.as_ptr(), b, m.len()) };
        orb_receive(m.len() as u32);
        for _ in 0..8 {
            orb_render(128);
        }
        let mut snapshots = 0;
        loop {
            let n = orb_outgoing();
            if n == 0 {
                break;
            }
            let b = unsafe { std::slice::from_raw_parts(orb_outgoing_ptr(), n as usize) };
            if let Ok(orb_player::wire::FromWorklet::Snapshot { received, .. }) =
                orb_player::wire::FromWorklet::decode(b)
            {
                assert_eq!(received, 1);
                snapshots += 1;
            }
        }
        assert_eq!(snapshots, 1);
        assert_eq!(orb_panic_len(), 0);
    }

    /// A panic inside a call, while the call holds the host's state, leaves its message for
    /// the script: the hook does not need that state.
    #[test]
    fn a_panic_in_a_call_keeps_its_message() {
        orb_init(44_100);
        let r = std::panic::catch_unwind(|| {
            with(|s| {
                assert!(s.host.is_some());
                panic!("the host failed: test 171");
            })
        });
        assert!(r.is_err());
        let n = orb_panic_len();
        assert!(n > 0, "no panic message");
        let b = unsafe { std::slice::from_raw_parts(orb_panic_ptr(), n as usize) };
        let text = std::str::from_utf8(b).unwrap();
        assert!(text.contains("the host failed: test 171"), "{text}");
    }
}
