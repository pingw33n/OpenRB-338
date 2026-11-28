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
//! - The worklet can be replaced ([`Player::reconnect`], task-329): the player starts the new one
//!   as it started the first, keeps its snapshots' frames and generations counting up, and gives
//!   the commands that put the new song where the old one was.

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

/// The transport commands the latest snapshot does not show yet that [`Player::reconnect`]
/// keeps at most (the oldest are dropped).
pub const TAIL: usize = 64;

/// A message posted that decides where the song is and whether it plays.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Moved {
    Play,
    Stop,
    Locate(u32),
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
    call_frames: u32,
    /// What the snapshots of the current worklet add to its frames and generations: those of
    /// the worklets before it ([`Player::reconnect`]).
    frames_base: u64,
    generation_base: u32,
    /// The Play, Stop and Locate messages posted that the latest snapshot does not show
    /// applied, by message number.
    tail: VecDeque<(u64, Moved)>,
    /// The number of the last Load message posted while the latest snapshot does not show it
    /// applied (`tail` holds only what came after it).
    load_at: Option<u64>,
    /// The worklet is gone ([`Player::stop_offline`]): what it posted before it went is not
    /// taken, until [`Player::reconnect`].
    offline: bool,
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
            frames_base: 0,
            generation_base: 0,
            tail: VecDeque::new(),
            load_at: None,
            offline: false,
            call_frames: u32::try_from(call_frames.max(1)).unwrap_or(u32::MAX),
        };
        p.init(samples);
        p
    }

    /// Starts the worklet's player: a new empty song with `samples`.
    fn init(&mut self, samples: Option<Arc<SampleSet>>) {
        let kit = samples.map(|s| self.kit(&s));
        let call_frames = self.call_frames;
        self.post(ToWorklet::Init { kit, call_frames });
    }

    /// Goes on with a new worklet through `transport` (task-329, `docs/web.md` section 4.1:
    /// the old one's context was closed, interrupted or silent): the new worklet is started as
    /// [`Player::new`] starts one, with `samples` (the open song's kit). The snapshot stays the
    /// old worklet's until the new one sends its first; the frames and generations of the new
    /// worklet's snapshots and tracks go on from the old ones'.
    ///
    /// The song is not sent: the caller loads it again ([`Command::Load`] of an engine this
    /// player builds), then sends the commands returned, which put it where the old worklet's
    /// song was: the position and play state of its latest snapshot, then the Play, Stop and
    /// Locate messages that snapshot does not show applied (a Play pressed while the old
    /// worklet was silent), or, when a Load was posted that it does not show, only the
    /// messages after that Load.
    pub fn reconnect(
        &mut self,
        transport: Box<dyn Transport>,
        samples: Option<Arc<SampleSet>>,
    ) -> Vec<Command> {
        self.poll();
        let mut then = Vec::new();
        if self.load_at.is_none() {
            then.push(Command::Locate(self.snapshot.tick.max(0) as u32));
            if self.snapshot.playing {
                then.push(Command::Play);
            }
        }
        then.extend(self.tail.drain(..).map(|(_, m)| match m {
            Moved::Play => Command::Play,
            Moved::Stop => Command::Stop,
            Moved::Locate(t) => Command::Locate(t),
        }));
        self.load_at = None;
        self.transport = transport;
        self.frames_base = self.snapshot.frames;
        self.generation_base = self.snapshot.generation;
        self.kits.clear();
        self.posted = 0;
        self.applied = 0;
        self.rate = None;
        self.offline = false;
        self.init(samples);
        then
    }

    /// Keeps a message just posted that moves the song, for [`Player::reconnect`].
    fn moved(&mut self, m: Option<Moved>) {
        match m {
            None => {
                self.tail.clear();
                self.load_at = Some(self.posted);
            }
            Some(m) => {
                self.tail.push_back((self.posted, m));
                if self.tail.len() > TAIL {
                    self.tail.pop_front();
                }
            }
        }
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
                self.moved(None);
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
            c => {
                let m = match c {
                    Command::Play => Some(Moved::Play),
                    Command::Stop => Some(Moved::Stop),
                    Command::Locate(t) => Some(Moved::Locate(t)),
                    _ => None,
                };
                self.post(ToWorklet::Command(c));
                if let Some(m) = m {
                    self.moved(Some(m));
                }
            }
        }
        Ok(())
    }

    /// Takes the messages that have arrived from the worklet.
    pub fn poll(&mut self) {
        while let Some(b) = self.transport.poll() {
            if self.offline {
                continue;
            }
            match FromWorklet::decode(&b) {
                Ok(FromWorklet::Ready { rate }) => self.rate = Some(rate),
                Ok(FromWorklet::Snapshot { snapshot, received }) => {
                    self.snapshot = *snapshot;
                    self.snapshot.frames += self.frames_base;
                    self.snapshot.generation += self.generation_base;
                    self.applied = received;
                    while self.tail.front().is_some_and(|&(n, _)| n <= received) {
                        self.tail.pop_front();
                    }
                    if self.load_at.is_some_and(|n| n <= received) {
                        self.load_at = None;
                    }
                }
                Ok(FromWorklet::Tracks(t)) => {
                    self.tracks = *t;
                    self.tracks.generation += self.generation_base;
                }
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

    /// Stop while the worklet is gone (task-329: the page was hidden and its context closed, so
    /// no snapshot will come until [`Player::reconnect`]): Stop is posted (and kept for the
    /// reconnect), and the snapshot shows the song stopped at once, with silent meters, as one
    /// engine call after the Stop would, so that the panel does not show a song playing that
    /// nobody hears. The messages posted count as applied: no snapshot will show them, and what
    /// the old worklet still posts is dropped.
    pub fn stop_offline(&mut self) {
        // a snapshot already on its way from the old worklet would show the song playing again
        self.poll();
        self.offline = true;
        let _ = self.send(Command::Stop);
        let s = &mut self.snapshot;
        s.playing = false;
        s.recording = false;
        s.pending = false;
        s.channel_meters = [0; 4];
        s.master_meters = [0; 2];
        s.frames += u64::from(self.call_frames);
        self.applied = self.posted;
    }

    /// The worklet's errors so far, taken.
    pub fn take_errors(&mut self) -> Vec<String> {
        std::mem::take(&mut self.errors)
    }
}
