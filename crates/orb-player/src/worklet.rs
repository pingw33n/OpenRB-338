//! The worklet half of the web player (task-171, `docs/web.md`): what the AudioWorklet's wasm
//! instance runs (`crates/orb-worklet`). [`Host`] owns a native [`Player`] and its [`Audio`];
//! the UI's messages ([`ToWorklet`]) become the player's commands, its output is rendered
//! planar for the AudioWorklet, and its state goes back ([`FromWorklet`]).
//!
//! The worklet runs both the port's messages and `process()` on its one thread, so the commands
//! of the messages taken before a render quantum are in the player's queue when it starts, and
//! the engine call that starts in it applies them, as natively ([`crate`] doc, "Timing").
//!
//! At 44.1 kHz (the AudioContext the web output asks for, task-170) the engine's samples go out
//! unchanged, `x / 32768` as f32 ([`Audio::render_f32`]); at another rate (a browser that
//! refused 44.1 kHz) they go through the native output's resampler.

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use orb_engine::samples::SampleSet;
use orb_engine::tables::Tables;

use crate::resample::Resample;
use crate::wire::{self, FromWorklet, PATTERN_DEVICES, ToWorklet};
use crate::{Audio, Command, Player, Preview, RATE};

/// The largest render quantum [`Host::render`] takes, in frames (the AudioWorklet's is 128).
pub const MAX_QUANTUM: usize = 4096;

/// The worklet's state.
pub struct Host {
    rate: u32,
    tables: Arc<Tables>,
    player: Option<(Player, Audio)>,
    resample: Option<Resample>,
    kits: HashMap<u32, Arc<SampleSet>>,
    /// Commands the player's queue did not take yet, in order.
    backlog: VecDeque<Command>,
    /// The messages taken so far.
    received: u64,
    /// The frames of the last snapshot sent.
    sent_frames: Option<u64>,
    sent_tracks: Option<(u32, u64)>,
    outgoing: VecDeque<Vec<u8>>,
    /// Interleaved stereo of one quantum, then the planar output.
    interleaved: Vec<f32>,
    planar: Vec<f32>,
    failed: bool,
}

impl Host {
    /// A host for an AudioContext of `rate` Hz. It is silent until [`ToWorklet::Init`].
    pub fn new(rate: u32) -> Host {
        let resample = if rate == RATE {
            None
        } else {
            Resample::new(rate).ok()
        };
        let mut h = Host {
            rate,
            tables: Arc::new(Tables::new()),
            player: None,
            resample,
            kits: HashMap::new(),
            backlog: VecDeque::new(),
            received: 0,
            sent_frames: None,
            sent_tracks: None,
            outgoing: VecDeque::new(),
            interleaved: vec![0.0; 2 * MAX_QUANTUM],
            planar: vec![0.0; 2 * MAX_QUANTUM],
            failed: false,
        };
        if rate != RATE && h.resample.is_none() {
            h.fail(format!("no resampler from 44100 Hz to {rate} Hz"));
        }
        h.outgoing.push_back(FromWorklet::Ready { rate }.encode());
        h
    }

    /// The AudioContext's rate.
    pub fn rate(&self) -> u32 {
        self.rate
    }

    fn fail(&mut self, e: String) {
        self.failed = true;
        self.outgoing.push_back(FromWorklet::Error(e).encode());
    }

    /// Takes one message from the UI.
    pub fn receive(&mut self, msg: &[u8]) {
        self.received += 1;
        if self.failed {
            return;
        }
        let m = match ToWorklet::decode(msg) {
            Ok(m) => m,
            Err(e) => return self.fail(e.to_string()),
        };
        match m {
            ToWorklet::Init { kit, call_frames } => {
                let kit = kit.and_then(|k| self.kits.get(&k).cloned());
                self.player = Some(Player::with_call_frames(
                    self.tables.clone(),
                    kit,
                    call_frames as usize,
                ));
            }
            ToWorklet::Kit { id, samples } => {
                self.kits.insert(id, samples);
            }
            ToWorklet::KitDrop(id) => {
                self.kits.remove(&id);
            }
            ToWorklet::Command(c) => self.backlog.push_back(c),
            ToWorklet::Load(l) => {
                let Some((p, _)) = &self.player else {
                    return self.fail("a load before the player's start".into());
                };
                let kit = l.kit.and_then(|k| self.kits.get(&k).cloned());
                let built = match &l.image {
                    None => Ok(p.build_new(kit)),
                    Some(i) => p.build_from(i, kit),
                };
                let mut e = match built {
                    Ok(e) => e,
                    // the UI built the same song and does not send a refused one
                    Err(r) => return self.fail(format!("a load refused in the worklet: {r:?}")),
                };
                for (c, d) in PATTERN_DEVICES.into_iter().enumerate() {
                    let edits: Vec<(u8, Vec<u8>)> = match e.frame.devices.channels[c].patterns_mut()
                    {
                        Some(built) => wire::changed_patterns(d, &l.patterns[c], built)
                            .map(|(i, b)| (i, b.to_vec()))
                            .collect(),
                        None => Vec::new(),
                    };
                    for (i, b) in edits {
                        e.pattern(d, i, &b);
                    }
                }
                self.backlog.push_back(Command::Load(e));
            }
            ToWorklet::Mod(id) => match self.kits.get(&id) {
                Some(s) => self
                    .backlog
                    .push_back(Command::Mod(Box::new(crate::ModSwap::new(s.clone())))),
                None => return self.fail(format!("an unknown kit {id}")),
            },
            ToWorklet::PreviewKit { kit, slot } => match self.kits.get(&kit) {
                Some(s) => self
                    .backlog
                    .push_back(Command::Preview(Box::new(Preview::new(s.shared(slot))))),
                None => return self.fail(format!("an unknown kit {kit}")),
            },
        }
        self.flush();
    }

    /// Moves the backlog into the player's queue, as far as it takes.
    fn flush(&mut self) {
        let Some((p, _)) = &mut self.player else {
            return;
        };
        p.collect_garbage();
        while let Some(c) = self.backlog.pop_front() {
            if let Err(c) = p.send(c) {
                self.backlog.push_front(c);
                break;
            }
        }
    }

    /// Renders `frames` frames (at most [`MAX_QUANTUM`]) and returns them planar: `frames`
    /// left samples, then `frames` right. Silence before the player starts and after a failure.
    pub fn render(&mut self, frames: usize) -> &[f32] {
        let frames = frames.min(MAX_QUANTUM);
        self.flush();
        let received = self.received;
        let out = &mut self.interleaved[..2 * frames];
        match &mut self.player {
            Some((_, a)) if !self.failed => match &mut self.resample {
                None => a.render_f32(out),
                Some(r) => {
                    if !r.pull(a, out) {
                        a.output_error();
                        out.fill(0.0);
                    }
                }
            },
            _ => out.fill(0.0),
        }
        let (l, r) = self.planar[..2 * frames].split_at_mut(frames);
        for (i, lr) in out.as_chunks::<2>().0.iter().enumerate() {
            l[i] = lr[0];
            r[i] = lr[1];
        }
        self.publish(received);
        &self.planar[..2 * frames]
    }

    /// Queues the snapshot after a new engine call, and the tracks when they changed.
    fn publish(&mut self, received: u64) {
        let Some((p, a)) = &mut self.player else {
            return;
        };
        let s = *a.last_snapshot();
        if self.sent_frames != Some(s.frames) {
            self.sent_frames = Some(s.frames);
            self.outgoing.push_back(
                FromWorklet::Snapshot {
                    snapshot: Box::new(s),
                    received,
                }
                .encode(),
            );
        }
        let t = p.tracks();
        let at = Some((t.generation, t.revision));
        if self.sent_tracks != at {
            self.sent_tracks = at;
            self.outgoing.push_back(wire::encode_tracks(t));
        }
    }

    /// The next message for the UI.
    pub fn take_outgoing(&mut self) -> Option<Vec<u8>> {
        self.outgoing.pop_front()
    }
}
