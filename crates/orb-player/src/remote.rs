//! The UI half of the web player (task-171, `docs/web.md`): a [`Player`] whose engine runs
//! elsewhere, in the AudioWorklet's wasm instance ([`crate::worklet::Host`]), reached only by
//! byte messages ([`crate::wire`]) through a [`Transport`].
//!
//! It has the methods of the native [`crate::Player`] that the UI uses, with the same
//! signatures, so the UI's port to the player works on either: [`Player::send`],
//! [`Player::snapshot`], [`Player::tracks`], [`Player::collect_garbage`], [`Player::build`],
//! [`Player::build_new`], [`Player::load`], [`Player::set_mod`], [`Player::startup_sound`] and
//! [`Player::process`]. What differs:
//!
//! - A song is built here as natively ([`Player::build`] on this player's own process globals: the
//!   load's refusal, the tracks and the patterns are the UI's to read), and built again in the
//!   worklet from its recipe when it is sent ([`wire::Load`]). The recipe is kept for each engine
//!   this player built, under a build id unique in the process that the engine carries
//!   ([`Engine::build_id`]); [`Command::Load`] of an engine this player did not build panics, as
//!   the native player's does. After it is built, an engine may be changed only in its patterns
//!   ([`orb_engine::frame::Engine::pattern`], what the new song's template does): the four pattern
//!   arrays go with the recipe, nothing else does.
//! - A sample set is sent once ([`wire::ToWorklet::Kit`]) and then named by an id; the player keeps
//!   the last [`KITS`] it sent.
//! - The queue never refuses a command: [`Player::send`] fails only for a data block or pattern of
//!   the wrong length, as natively.
//! - The snapshot arrives a message later than the engine call it shows, and a command reaches the
//!   engine after its message: [`Player::unapplied`] counts the messages the latest snapshot does
//!   not show yet.
//! - Nothing comes back to be dropped: the worklet drops what its player lets go of, and
//!   [`Player::collect_garbage`] returns 0.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use orb_engine::frame::edit::PatternShape;
use orb_engine::frame::{Engine, LoadRefused};
use orb_engine::process::ProcessState;
use orb_engine::samples::{FIRST_ID, LAST_ID, STARTUP_ID, SampleSet};
use orb_engine::song::Song;
use orb_engine::tables::Tables;

use crate::wire::{self, FromWorklet, LoadImage, PATTERN_DEVICES, ToWorklet};
use crate::{CALL_FRAMES, Command, LoadError, Snapshot, Tracks};

/// The sample sets the player remembers having sent (the oldest is dropped).
pub const KITS: usize = 4;
/// The built engines whose recipe the player keeps until they are sent (the oldest is
/// forgotten: an engine built that long before it is sent cannot be sent).
pub const RECIPES: usize = 8;

/// The way to the worklet: byte messages, in order, both ways.
pub trait Transport: Send {
    /// Sends a message to the worklet.
    fn post(&mut self, msg: Vec<u8>);
    /// The next message from the worklet, if one has arrived.
    fn poll(&mut self) -> Option<Vec<u8>>;
}

/// What rebuilds an engine this player built.
struct Recipe {
    /// The engine's [`Engine::build_id`].
    id: u64,
    image: Option<LoadImage>,
    kit: Option<Arc<SampleSet>>,
}

/// The next build id, unique in the process (0 is an engine no web player built).
static NEXT_BUILD: AtomicU64 = AtomicU64::new(1);

/// The UI half.
pub struct Player {
    transport: Box<dyn Transport>,
    tables: Arc<Tables>,
    process: ProcessState,
    snapshot: Snapshot,
    tracks: Tracks,
    /// In a cell so that [`Player::build`] takes `&self`, as natively.
    recipes: RefCell<VecDeque<Recipe>>,
    /// The sample sets sent, most recently used last, with their ids.
    kits: VecDeque<(u32, Arc<SampleSet>)>,
    next_kit: u32,
    /// The messages posted, and those the latest snapshot shows applied.
    posted: u64,
    applied: u64,
    /// The worklet's rate once it is up.
    rate: Option<u32>,
    errors: Vec<String>,
}

impl Player {
    /// [`crate::Player::new`] over `transport`: the worklet starts a new empty song with
    /// `samples`.
    pub fn new(
        tables: Arc<Tables>,
        samples: Option<Arc<SampleSet>>,
        transport: Box<dyn Transport>,
    ) -> Player {
        Self::with_call_frames(tables, samples, CALL_FRAMES, transport)
    }

    /// [`crate::Player::with_call_frames`] over `transport`.
    pub fn with_call_frames(
        tables: Arc<Tables>,
        samples: Option<Arc<SampleSet>>,
        call_frames: usize,
        transport: Box<dyn Transport>,
    ) -> Player {
        // the snapshot until the worklet's first: the one the native player starts with
        let (mut local, _) =
            crate::Player::with_call_frames(tables.clone(), samples.clone(), call_frames);
        let snapshot = *local.snapshot();
        let tracks = local.tracks().clone();
        let mut p = Player {
            transport,
            tables,
            process: ProcessState::new(),
            snapshot,
            tracks,
            recipes: RefCell::new(VecDeque::new()),
            kits: VecDeque::new(),
            next_kit: 1,
            posted: 0,
            applied: 0,
            rate: None,
            errors: Vec::new(),
        };
        let kit = samples.map(|s| p.kit(&s));
        let call_frames = u32::try_from(call_frames.max(1)).unwrap_or(u32::MAX);
        p.post(ToWorklet::Init { kit, call_frames });
        p
    }

    fn post(&mut self, m: ToWorklet) {
        self.transport.post(m.encode());
        self.posted += 1;
    }

    /// The id of `samples` in the worklet, sending them first if they are new there.
    fn kit(&mut self, samples: &Arc<SampleSet>) -> u32 {
        if let Some(i) = self.kits.iter().position(|(_, s)| Arc::ptr_eq(s, samples)) {
            let k = self.kits.remove(i).expect("in range");
            let id = k.0;
            self.kits.push_back(k);
            return id;
        }
        let id = self.next_kit;
        self.next_kit += 1;
        self.post(ToWorklet::Kit {
            id,
            samples: samples.clone(),
        });
        self.kits.push_back((id, samples.clone()));
        if self.kits.len() > KITS
            && let Some((old, _)) = self.kits.pop_front()
        {
            self.post(ToWorklet::KitDrop(old));
        }
        id
    }

    fn remember(&self, e: &mut Engine, image: Option<LoadImage>, kit: Option<Arc<SampleSet>>) {
        let id = NEXT_BUILD.fetch_add(1, Ordering::Relaxed);
        e.build_id = id;
        let mut recipes = self.recipes.borrow_mut();
        recipes.push_back(Recipe { id, image, kit });
        if recipes.len() > RECIPES {
            recipes.pop_front();
        }
    }

    /// The process globals of the songs this player builds (the worklet's songs have their
    /// own, shared the same way).
    pub fn process(&self) -> &ProcessState {
        &self.process
    }

    /// [`crate::Player::build`]; the recipe is kept for [`Command::Load`].
    pub fn build(
        &self,
        song: &Song,
        samples: Option<Arc<SampleSet>>,
    ) -> Result<Box<Engine>, LoadRefused> {
        let mut e = Box::new(Engine::new(
            self.tables.clone(),
            orb_engine::frame::Devices::implemented_with(samples.clone(), self.process.clone()),
        ));
        e.load(song)?;
        self.remember(&mut e, Some(LoadImage::of(song)), samples);
        Ok(e)
    }

    /// [`crate::Player::build_new`]; the recipe is kept for [`Command::Load`].
    pub fn build_new(&self, samples: Option<Arc<SampleSet>>) -> Box<Engine> {
        let mut e = Box::new(Engine::new(
            self.tables.clone(),
            orb_engine::frame::Devices::implemented_with(samples.clone(), self.process.clone()),
        ));
        self.remember(&mut e, None, samples);
        e
    }

    /// [`Player::build`] and [`Command::Load`]. A refused song is not sent.
    pub fn load(&mut self, song: &Song, samples: Option<Arc<SampleSet>>) -> Result<(), LoadError> {
        let e = self.build(song, samples).map_err(LoadError::Refused)?;
        self.send(Command::Load(e)).map_err(LoadError::Queue)
    }

    /// [`Command::Mod`] with `samples`.
    pub fn set_mod(&mut self, samples: Arc<SampleSet>) -> Result<(), Command> {
        self.send(Command::Mod(Box::new(crate::ModSwap::new(samples))))
    }

    /// [`Command::Preview`] with the startup sound of `samples`.
    pub fn startup_sound(&mut self, samples: &SampleSet) -> Result<(), Command> {
        self.send(Command::Preview(Box::new(crate::Preview::new(
            samples.shared(STARTUP_ID),
        ))))
    }

    /// Sends a command to the worklet. A data block or a pattern of the wrong length is given
    /// back, as natively; nothing else is.
    ///
    /// # Panics
    /// On [`Command::Load`] of an engine this player did not build (or built more than
    /// [`RECIPES`] builds ago).
    pub fn send(&mut self, c: Command) -> Result<(), Command> {
        if let Command::DeviceData { device, payload } = &c
            && payload.len() != device.data_len()
        {
            return Err(c);
        }
        if let Command::Pattern { device, bytes, .. } = &c
            && PatternShape::of(*device).map(|s| s.len) != Some(bytes.len())
        {
            return Err(c);
        }
        match c {
            Command::Load(mut e) => {
                let r = {
                    let mut recipes = self.recipes.borrow_mut();
                    let i = recipes
                        .iter()
                        .position(|r| e.build_id != 0 && r.id == e.build_id)
                        .expect("a song for this player is built by Player::build");
                    recipes.remove(i).expect("in range")
                };
                let kit = r.kit.as_ref().map(|s| self.kit(s));
                let patterns = std::array::from_fn(|c| {
                    e.frame.devices.channels[c]
                        .patterns_mut()
                        .map(|p| p.to_vec())
                        .unwrap_or_default()
                });
                debug_assert_eq!(e.frame.devices.channels.len(), PATTERN_DEVICES.len());
                self.post(ToWorklet::Load(Box::new(wire::Load {
                    image: r.image,
                    kit,
                    patterns,
                })));
            }
            Command::Mod(m) => {
                let id = self.kit(m.samples());
                self.post(ToWorklet::Mod(id));
            }
            Command::Preview(p) => {
                let found = self.kits.iter().find_map(|(id, s)| {
                    (FIRST_ID..=LAST_ID)
                        .find(|&slot| Arc::ptr_eq(&s.shared(slot), &p.sample))
                        .map(|slot| (*id, slot))
                });
                match found {
                    Some((kit, slot)) => self.post(ToWorklet::PreviewKit { kit, slot }),
                    None => self.post(ToWorklet::Command(Command::Preview(p))),
                }
            }
            c => self.post(ToWorklet::Command(c)),
        }
        Ok(())
    }

    /// Takes the messages that have arrived from the worklet.
    pub fn poll(&mut self) {
        while let Some(b) = self.transport.poll() {
            match FromWorklet::decode(&b) {
                Ok(FromWorklet::Ready { rate }) => self.rate = Some(rate),
                Ok(FromWorklet::Snapshot { snapshot, received }) => {
                    self.snapshot = *snapshot;
                    self.applied = received;
                }
                Ok(FromWorklet::Tracks(t)) => self.tracks = *t,
                Ok(FromWorklet::Error(e)) => self.errors.push(e),
                Err(e) => self.errors.push(e.to_string()),
            }
        }
    }

    /// Nothing comes back to be dropped (the worklet drops its own); 0.
    pub fn collect_garbage(&mut self) -> usize {
        0
    }

    /// The latest snapshot from the worklet (after taking the messages that have arrived).
    pub fn snapshot(&mut self) -> &Snapshot {
        self.poll();
        &self.snapshot
    }

    /// The current song's tracks, as the worklet last sent them.
    pub fn tracks(&mut self) -> &Tracks {
        self.poll();
        &self.tracks
    }

    /// The messages posted that the latest snapshot does not show applied yet (0: the snapshot
    /// shows every command sent).
    pub fn unapplied(&self) -> u64 {
        self.posted - self.applied.min(self.posted)
    }

    /// The worklet's output rate once it runs (44100 unless the browser refused that rate).
    pub fn rate(&self) -> Option<u32> {
        self.rate
    }

    /// The worklet's errors so far, taken.
    pub fn take_errors(&mut self) -> Vec<String> {
        std::mem::take(&mut self.errors)
    }
}
