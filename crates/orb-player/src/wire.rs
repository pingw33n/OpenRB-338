//! The messages between the web player's two wasm instances (task-171, `docs/web.md`): the UI's
//! [`crate::remote::Player`] and the AudioWorklet's [`crate::worklet::Host`]. They share no
//! memory, so every message is a byte string (an `ArrayBuffer` through `postMessage`), in a
//! small little-endian format of this module: a tag byte, then the fields in order (lengths as
//! u32).
//!
//! A [`Command`] that carries a box the other instance cannot use goes as what rebuilds it
//! there:
//!
//! - [`Command::Load`]: the song's recipe ([`LoadImage`], or a new empty song), its kit, and the
//!   four pattern arrays of the engine as it is sent (so the UI's edits of a built engine, the new
//!   song's template, come along); see [`Load`];
//! - [`Command::Mod`] and [`Command::Preview`]: a kit id, the [`SampleSet`] sent once before
//!   ([`ToWorklet::Kit`]); a preview sample of no kit goes as its audio;
//! - [`Command::InitSong`] and [`Command::InitLoop`]: nothing, their box is an empty [`Pool`] made
//!   in the worklet.
//!
//! The worklet answers with [`FromWorklet`]: a snapshot after every engine call, the tracks when
//! they change.

use std::sync::Arc;

use orb_engine::frame::edit::PATTERNS;
use orb_engine::frame::{ChannelParams, Gate, MixerParams, NoteClock, Sounding};
use orb_engine::samples::{COUNT, FIRST_ID, Sample, SampleSet};
use orb_engine::seq::{Event, Pool};
use orb_engine::song::{Batch, Device, Globals, LoadForm, LoadSource, Song, TrackEvent};

use crate::{Command, LiveBlock, Preview, Snapshot, Tracks};

/// The tracks of a song (the lists of [`Tracks::events`] that hold events).
const TRACKS: usize = 9;

/// A message that does not parse: a defect of one side (both are built from the same code).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Malformed;

impl std::fmt::Display for Malformed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("a malformed web player message")
    }
}

impl std::error::Error for Malformed {}

type R<T> = Result<T, Malformed>;

/// The writer.
#[derive(Default)]
struct W(Vec<u8>);

impl W {
    fn u8(&mut self, v: u8) {
        self.0.push(v);
    }
    fn bool(&mut self, v: bool) {
        self.u8(v as u8);
    }
    fn u16(&mut self, v: u16) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn u32(&mut self, v: u32) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn i32(&mut self, v: i32) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn u64(&mut self, v: u64) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn f64(&mut self, v: f64) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn len(&mut self, n: usize) {
        self.u32(u32::try_from(n).expect("a web player message field over 4 GiB"));
    }
    fn bytes(&mut self, b: &[u8]) {
        self.len(b.len());
        self.0.extend_from_slice(b);
    }
    fn i16s(&mut self, a: &[i16]) {
        self.len(a.len());
        for &x in a {
            self.0.extend_from_slice(&x.to_le_bytes());
        }
    }
    fn device(&mut self, d: Device) {
        self.u8(d.id());
    }
}

/// The reader.
struct Rd<'a>(&'a [u8]);

impl<'a> Rd<'a> {
    fn take(&mut self, n: usize) -> R<&'a [u8]> {
        if self.0.len() < n {
            return Err(Malformed);
        }
        let (a, b) = self.0.split_at(n);
        self.0 = b;
        Ok(a)
    }
    fn arr<const N: usize>(&mut self) -> R<[u8; N]> {
        Ok(self.take(N)?.try_into().expect("N bytes"))
    }
    fn u8(&mut self) -> R<u8> {
        Ok(self.take(1)?[0])
    }
    fn bool(&mut self) -> R<bool> {
        match self.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(Malformed),
        }
    }
    fn u16(&mut self) -> R<u16> {
        Ok(u16::from_le_bytes(self.arr()?))
    }
    fn u32(&mut self) -> R<u32> {
        Ok(u32::from_le_bytes(self.arr()?))
    }
    fn i32(&mut self) -> R<i32> {
        Ok(i32::from_le_bytes(self.arr()?))
    }
    fn u64(&mut self) -> R<u64> {
        Ok(u64::from_le_bytes(self.arr()?))
    }
    fn f64(&mut self) -> R<f64> {
        Ok(f64::from_le_bytes(self.arr()?))
    }
    fn len(&mut self) -> R<usize> {
        Ok(self.u32()? as usize)
    }
    fn bytes(&mut self) -> R<&'a [u8]> {
        let n = self.len()?;
        self.take(n)
    }
    fn i16s(&mut self) -> R<Vec<i16>> {
        let n = self.len()?;
        let b = self.take(n.checked_mul(2).ok_or(Malformed)?)?;
        Ok(b.as_chunks::<2>()
            .0
            .iter()
            .map(|c| i16::from_le_bytes(*c))
            .collect())
    }
    fn device(&mut self) -> R<Device> {
        Device::from_id(self.u8()?).ok_or(Malformed)
    }
    fn end(&self) -> R<()> {
        if self.0.is_empty() {
            Ok(())
        } else {
            Err(Malformed)
        }
    }
}

/// What [`orb_engine::frame::Engine::load`] reads of a song ([`LoadSource`]), owned, to be sent
/// to the worklet and loaded there as the UI's [`Song`] would be.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadImage {
    pub form: LoadForm,
    pub globals: Globals,
    /// The data blocks by device id; only those of [`LoadForm::device_blocks`] are read.
    pub payloads: [Vec<u8>; 9],
    pub tracks: [Vec<TrackEvent>; 9],
    pub batches: Vec<Batch>,
}

impl LoadImage {
    /// The image of `song`.
    pub fn of(song: &Song) -> LoadImage {
        let mut payloads: [Vec<u8>; 9] = Default::default();
        for &d in song.form.device_blocks() {
            payloads[d as usize] = song.device_payload(d);
        }
        LoadImage {
            form: song.form,
            globals: song.globals.clone(),
            payloads,
            tracks: song.tracks.clone(),
            batches: song.batches.clone(),
        }
    }

    fn write(&self, w: &mut W) {
        w.u8(match self.form {
            LoadForm::Rbs => 0,
            LoadForm::Rb1x => 1,
        });
        w.0.extend_from_slice(&self.globals.payload());
        for p in &self.payloads {
            w.bytes(p);
        }
        for t in &self.tracks {
            w.len(t.len());
            for e in t {
                w.u32(e.tick);
                w.u8(e.ctrl);
                w.u8(e.value);
            }
        }
        w.len(self.batches.len());
        for b in &self.batches {
            w.u8(b.track);
            w.u32(b.len);
        }
    }

    fn read(r: &mut Rd) -> R<LoadImage> {
        let form = match r.u8()? {
            0 => LoadForm::Rbs,
            1 => LoadForm::Rb1x,
            _ => return Err(Malformed),
        };
        let globals = Globals::from_payload(&r.arr()?);
        let mut payloads: [Vec<u8>; 9] = Default::default();
        for p in &mut payloads {
            *p = r.bytes()?.to_vec();
        }
        let mut tracks: [Vec<TrackEvent>; 9] = Default::default();
        for t in &mut tracks {
            let n = r.len()?;
            for _ in 0..n {
                t.push(TrackEvent {
                    tick: r.u32()?,
                    ctrl: r.u8()?,
                    value: r.u8()?,
                });
            }
        }
        let n = r.len()?;
        let mut batches = Vec::new();
        for _ in 0..n {
            let track = r.u8()?;
            if track as usize >= TRACKS {
                return Err(Malformed);
            }
            batches.push(Batch {
                track,
                len: r.u32()?,
            });
        }
        // the load asks for exactly these blocks: a wrong length would be read past
        for &d in form.device_blocks() {
            if payloads[d as usize].len() != d.data_len() {
                return Err(Malformed);
            }
        }
        Ok(LoadImage {
            form,
            globals,
            payloads,
            tracks,
            batches,
        })
    }
}

impl LoadSource for LoadImage {
    fn form(&self) -> LoadForm {
        self.form
    }

    fn globals(&self) -> &Globals {
        &self.globals
    }

    fn device_payload(&self, dev: Device) -> Vec<u8> {
        self.payloads[dev as usize].clone()
    }

    fn tracks(&self) -> &[Vec<TrackEvent>; 9] {
        &self.tracks
    }

    fn batches(&self) -> &[Batch] {
        &self.batches
    }
}

/// A [`Command::Load`] as sent: the worklet builds the song from `image` (`None`: a new empty
/// song, [`crate::Player::build_new`]) with the kit `kit` (`None`: silent drum machines), then
/// writes each pattern of `patterns` (the arrays of the 303s, the 808 and the 909, in that
/// order) that differs from the one it built ([`orb_engine::frame::Engine::pattern`]), and
/// queues it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Load {
    pub image: Option<LoadImage>,
    pub kit: Option<u32>,
    pub patterns: [Vec<u8>; 4],
}

/// A message to the worklet.
pub enum ToWorklet {
    /// The player's start ([`crate::Player::with_call_frames`]): a new empty song with the kit
    /// `kit`, engine calls of `call_frames` frames. Sent once, first; the worklet is silent
    /// until it.
    Init { kit: Option<u32>, call_frames: u32 },
    /// A sample set, known from now on as `id`.
    Kit { id: u32, samples: Arc<SampleSet> },
    /// The sample set `id` is no longer referred to (the songs that use it keep it).
    KitDrop(u32),
    /// A command that carries everything it needs.
    Command(Command),
    /// [`Command::Load`].
    Load(Box<Load>),
    /// [`Command::Mod`] with the kit `id`.
    Mod(u32),
    /// [`Command::Preview`] with the sample `slot` (a resource id) of the kit `kit`.
    PreviewKit { kit: u32, slot: u16 },
}

// The tags of ToWorklet.
const T_INIT: u8 = 1;
const T_KIT: u8 = 2;
const T_KIT_DROP: u8 = 3;
const T_COMMAND: u8 = 4;
const T_LOAD: u8 = 5;
const T_MOD: u8 = 6;
const T_PREVIEW_KIT: u8 = 7;

// The tags of the commands inside T_COMMAND.
mod c {
    pub const PLAY: u8 = 0;
    pub const STOP: u8 = 1;
    pub const LOCATE: u8 = 2;
    pub const POSITION: u8 = 3;
    pub const NOTE_CLOCK: u8 = 4;
    pub const RELOCATE: u8 = 5;
    pub const NOTE: u8 = 6;
    pub const NOTES_OFF: u8 = 7;
    pub const GATE: u8 = 8;
    pub const SONG_MODE: u8 = 9;
    pub const RECORD: u8 = 10;
    pub const LOOP_ON: u8 = 11;
    pub const LOOP_START: u8 = 12;
    pub const LOOP_END: u8 = 13;
    pub const TEMPO: u8 = 14;
    pub const SHUFFLE: u8 = 15;
    pub const VINTAGE: u8 = 16;
    pub const INIT_SONG: u8 = 17;
    pub const INIT_LOOP: u8 = 18;
    pub const UNDO_INIT: u8 = 19;
    pub const PARAM: u8 = 20;
    pub const MASTER: u8 = 21;
    pub const CHANNEL_ENABLED: u8 = 22;
    pub const DEVICE_DATA: u8 = 23;
    pub const DEVICE_PARAM: u8 = 24;
    pub const MIXER_GLOBAL: u8 = 25;
    pub const MIXER_CHANNEL: u8 = 26;
    pub const PATTERN_FIELD: u8 = 27;
    pub const PATTERN_HEADER: u8 = 28;
    pub const PATTERN: u8 = 29;
    pub const STEP_FIELD: u8 = 30;
    pub const STEP: u8 = 31;
    pub const PREVIEW_AUDIO: u8 = 32;
    pub const PREVIEW_STOP: u8 = 33;
}

fn write_note_clock(w: &mut W, n: &NoteClock) {
    match *n {
        NoteClock::Off => w.u8(0),
        NoteClock::Run => w.u8(1),
        NoteClock::At(t) => {
            w.u8(2);
            w.f64(t);
        }
    }
}

fn read_note_clock(r: &mut Rd) -> R<NoteClock> {
    match r.u8()? {
        0 => Ok(NoteClock::Off),
        1 => Ok(NoteClock::Run),
        2 => Ok(NoteClock::At(r.f64()?)),
        _ => Err(Malformed),
    }
}

fn write_gate(w: &mut W, g: &Gate) {
    w.bool(g.on);
    w.bool(g.open);
    w.u8(g.pattern);
}

fn read_gate(r: &mut Rd) -> R<Gate> {
    Ok(Gate {
        on: r.bool()?,
        open: r.bool()?,
        pattern: r.u8()?,
    })
}

/// Writes a command that carries everything it needs; `false` (nothing written) for
/// [`Command::Load`], [`Command::Mod`] and [`Command::Preview`], which the caller sends as
/// their own messages.
fn write_command(w: &mut W, cmd: &Command) -> bool {
    match cmd {
        Command::Play => w.u8(c::PLAY),
        Command::Stop => w.u8(c::STOP),
        Command::Locate(t) => {
            w.u8(c::LOCATE);
            w.u32(*t);
        }
        Command::Position(t) => {
            w.u8(c::POSITION);
            w.f64(*t);
        }
        Command::NoteClock(n) => {
            w.u8(c::NOTE_CLOCK);
            write_note_clock(w, n);
        }
        Command::Relocate(d) => {
            w.u8(c::RELOCATE);
            w.device(*d);
        }
        Command::Note {
            device,
            note,
            velocity,
        } => {
            w.u8(c::NOTE);
            w.device(*device);
            w.u8(*note);
            w.u8(*velocity);
        }
        Command::NotesOff(d) => {
            w.u8(c::NOTES_OFF);
            w.device(*d);
        }
        Command::Gate { device, gate } => {
            w.u8(c::GATE);
            w.device(*device);
            write_gate(w, gate);
        }
        Command::SongMode(v) => {
            w.u8(c::SONG_MODE);
            w.bool(*v);
        }
        Command::Record(v) => {
            w.u8(c::RECORD);
            w.bool(*v);
        }
        Command::LoopOn(v) => {
            w.u8(c::LOOP_ON);
            w.bool(*v);
        }
        Command::LoopStart(v) => {
            w.u8(c::LOOP_START);
            w.u32(*v);
        }
        Command::LoopEnd(v) => {
            w.u8(c::LOOP_END);
            w.u32(*v);
        }
        Command::Tempo(v) => {
            w.u8(c::TEMPO);
            w.u32(*v);
        }
        Command::Shuffle(v) => {
            w.u8(c::SHUFFLE);
            w.u8(*v);
        }
        Command::Vintage(v) => {
            w.u8(c::VINTAGE);
            w.bool(*v);
        }
        Command::InitSong(_) => w.u8(c::INIT_SONG),
        Command::InitLoop(_) => w.u8(c::INIT_LOOP),
        Command::UndoInit => w.u8(c::UNDO_INIT),
        Command::Param {
            device,
            ctrl,
            value,
        } => {
            w.u8(c::PARAM);
            w.device(*device);
            w.u8(*ctrl);
            w.u8(*value);
        }
        Command::Master(v) => {
            w.u8(c::MASTER);
            w.u8(*v);
        }
        Command::ChannelEnabled { channel, on } => {
            w.u8(c::CHANNEL_ENABLED);
            w.u8(*channel);
            w.bool(*on);
        }
        Command::DeviceData { device, payload } => {
            w.u8(c::DEVICE_DATA);
            w.device(*device);
            w.bytes(payload);
        }
        Command::DeviceParam {
            device,
            ctrl,
            value,
        } => {
            w.u8(c::DEVICE_PARAM);
            w.device(*device);
            w.u8(*ctrl);
            w.u8(*value);
        }
        Command::MixerGlobal { id, value } => {
            w.u8(c::MIXER_GLOBAL);
            w.u8(*id);
            w.u8(*value);
        }
        Command::MixerChannel {
            channel,
            field,
            value,
        } => {
            w.u8(c::MIXER_CHANNEL);
            w.u8(*channel);
            w.u8(*field);
            w.u8(*value);
        }
        Command::PatternField {
            device,
            pattern,
            field,
            value,
        } => {
            w.u8(c::PATTERN_FIELD);
            w.device(*device);
            w.u8(*pattern);
            w.u8(*field);
            w.u8(*value);
        }
        Command::PatternHeader {
            device,
            pattern,
            header,
        } => {
            w.u8(c::PATTERN_HEADER);
            w.device(*device);
            w.u8(*pattern);
            w.0.extend_from_slice(header);
        }
        Command::Pattern {
            device,
            pattern,
            bytes,
        } => {
            w.u8(c::PATTERN);
            w.device(*device);
            w.u8(*pattern);
            w.bytes(bytes);
        }
        Command::StepField {
            device,
            pattern,
            step,
            field,
            value,
        } => {
            w.u8(c::STEP_FIELD);
            w.device(*device);
            w.u8(*pattern);
            w.u8(*step);
            w.u8(*field);
            w.u8(*value);
        }
        Command::Step {
            device,
            pattern,
            step,
            bytes,
        } => {
            w.u8(c::STEP);
            w.device(*device);
            w.u8(*pattern);
            w.u8(*step);
            w.0.extend_from_slice(bytes);
        }
        Command::PreviewStop => w.u8(c::PREVIEW_STOP),
        Command::Load(_) | Command::Mod(_) | Command::Preview(_) => return false,
    }
    true
}

fn read_command(r: &mut Rd) -> R<Command> {
    Ok(match r.u8()? {
        c::PLAY => Command::Play,
        c::STOP => Command::Stop,
        c::LOCATE => Command::Locate(r.u32()?),
        c::POSITION => Command::Position(r.f64()?),
        c::NOTE_CLOCK => Command::NoteClock(read_note_clock(r)?),
        c::RELOCATE => Command::Relocate(r.device()?),
        c::NOTE => Command::Note {
            device: r.device()?,
            note: r.u8()?,
            velocity: r.u8()?,
        },
        c::NOTES_OFF => Command::NotesOff(r.device()?),
        c::GATE => Command::Gate {
            device: r.device()?,
            gate: read_gate(r)?,
        },
        c::SONG_MODE => Command::SongMode(r.bool()?),
        c::RECORD => Command::Record(r.bool()?),
        c::LOOP_ON => Command::LoopOn(r.bool()?),
        c::LOOP_START => Command::LoopStart(r.u32()?),
        c::LOOP_END => Command::LoopEnd(r.u32()?),
        c::TEMPO => Command::Tempo(r.u32()?),
        c::SHUFFLE => Command::Shuffle(r.u8()?),
        c::VINTAGE => Command::Vintage(r.bool()?),
        c::INIT_SONG => Command::InitSong(Box::new(Pool::new())),
        c::INIT_LOOP => Command::InitLoop(Box::new(Pool::new())),
        c::UNDO_INIT => Command::UndoInit,
        c::PARAM => Command::Param {
            device: r.device()?,
            ctrl: r.u8()?,
            value: r.u8()?,
        },
        c::MASTER => Command::Master(r.u8()?),
        c::CHANNEL_ENABLED => Command::ChannelEnabled {
            channel: r.u8()?,
            on: r.bool()?,
        },
        c::DEVICE_DATA => Command::DeviceData {
            device: r.device()?,
            payload: r.bytes()?.into(),
        },
        c::DEVICE_PARAM => Command::DeviceParam {
            device: r.device()?,
            ctrl: r.u8()?,
            value: r.u8()?,
        },
        c::MIXER_GLOBAL => Command::MixerGlobal {
            id: r.u8()?,
            value: r.u8()?,
        },
        c::MIXER_CHANNEL => Command::MixerChannel {
            channel: r.u8()?,
            field: r.u8()?,
            value: r.u8()?,
        },
        c::PATTERN_FIELD => Command::PatternField {
            device: r.device()?,
            pattern: r.u8()?,
            field: r.u8()?,
            value: r.u8()?,
        },
        c::PATTERN_HEADER => Command::PatternHeader {
            device: r.device()?,
            pattern: r.u8()?,
            header: r.arr()?,
        },
        c::PATTERN => Command::Pattern {
            device: r.device()?,
            pattern: r.u8()?,
            bytes: r.bytes()?.into(),
        },
        c::STEP_FIELD => Command::StepField {
            device: r.device()?,
            pattern: r.u8()?,
            step: r.u8()?,
            field: r.u8()?,
            value: r.u8()?,
        },
        c::STEP => Command::Step {
            device: r.device()?,
            pattern: r.u8()?,
            step: r.u8()?,
            bytes: r.arr()?,
        },
        c::PREVIEW_AUDIO => {
            let a = r.i16s()?;
            Command::Preview(Box::new(Preview::new(Arc::new(Sample::from_audio(&a)))))
        }
        c::PREVIEW_STOP => Command::PreviewStop,
        _ => return Err(Malformed),
    })
}

impl ToWorklet {
    /// The message's bytes. A [`ToWorklet::Command`] of [`Command::Load`] or [`Command::Mod`]
    /// cannot be written (they go as [`ToWorklet::Load`] and [`ToWorklet::Mod`]); a
    /// [`Command::Preview`] goes as its sample's audio.
    ///
    /// # Panics
    /// On a [`ToWorklet::Command`] of [`Command::Load`] or [`Command::Mod`].
    pub fn encode(&self) -> Vec<u8> {
        let mut w = W::default();
        match self {
            ToWorklet::Init { kit, call_frames } => {
                w.u8(T_INIT);
                write_kit_ref(&mut w, *kit);
                w.u32(*call_frames);
            }
            ToWorklet::Kit { id, samples } => {
                w.u8(T_KIT);
                w.u32(*id);
                for i in 0..COUNT {
                    w.i16s(samples.get(FIRST_ID + i as u16).audio());
                }
            }
            ToWorklet::KitDrop(id) => {
                w.u8(T_KIT_DROP);
                w.u32(*id);
            }
            ToWorklet::Command(Command::Preview(p)) => {
                w.u8(T_COMMAND);
                w.u8(c::PREVIEW_AUDIO);
                w.i16s(p.sample().audio());
            }
            ToWorklet::Command(cmd) => {
                w.u8(T_COMMAND);
                assert!(
                    write_command(&mut w, cmd),
                    "Load and Mod go as their own messages"
                );
            }
            ToWorklet::Load(l) => {
                w.u8(T_LOAD);
                match &l.image {
                    None => w.u8(0),
                    Some(i) => {
                        w.u8(1);
                        i.write(&mut w);
                    }
                }
                write_kit_ref(&mut w, l.kit);
                for p in &l.patterns {
                    w.bytes(p);
                }
            }
            ToWorklet::Mod(id) => {
                w.u8(T_MOD);
                w.u32(*id);
            }
            ToWorklet::PreviewKit { kit, slot } => {
                w.u8(T_PREVIEW_KIT);
                w.u32(*kit);
                w.u16(*slot);
            }
        }
        w.0
    }

    pub fn decode(b: &[u8]) -> Result<ToWorklet, Malformed> {
        let mut r = Rd(b);
        let m = match r.u8()? {
            T_INIT => ToWorklet::Init {
                kit: read_kit_ref(&mut r)?,
                call_frames: r.u32()?,
            },
            T_KIT => {
                let id = r.u32()?;
                let mut s = Vec::with_capacity(COUNT);
                for _ in 0..COUNT {
                    s.push(Sample::from_audio(&r.i16s()?));
                }
                ToWorklet::Kit {
                    id,
                    samples: Arc::new(SampleSet::from_samples(s)),
                }
            }
            T_KIT_DROP => ToWorklet::KitDrop(r.u32()?),
            T_COMMAND => ToWorklet::Command(read_command(&mut r)?),
            T_LOAD => {
                let image = match r.u8()? {
                    0 => None,
                    1 => Some(LoadImage::read(&mut r)?),
                    _ => return Err(Malformed),
                };
                let kit = read_kit_ref(&mut r)?;
                let mut patterns: [Vec<u8>; 4] = Default::default();
                for p in &mut patterns {
                    *p = r.bytes()?.to_vec();
                }
                ToWorklet::Load(Box::new(Load {
                    image,
                    kit,
                    patterns,
                }))
            }
            T_MOD => ToWorklet::Mod(r.u32()?),
            T_PREVIEW_KIT => ToWorklet::PreviewKit {
                kit: r.u32()?,
                slot: r.u16()?,
            },
            _ => return Err(Malformed),
        };
        r.end()?;
        Ok(m)
    }
}

fn write_kit_ref(w: &mut W, kit: Option<u32>) {
    match kit {
        None => w.u8(0),
        Some(id) => {
            w.u8(1);
            w.u32(id);
        }
    }
}

fn read_kit_ref(r: &mut Rd) -> R<Option<u32>> {
    match r.u8()? {
        0 => Ok(None),
        1 => Ok(Some(r.u32()?)),
        _ => Err(Malformed),
    }
}

/// The patterns of each pattern device's array, in [`Load::patterns`] order.
pub const PATTERN_DEVICES: [Device; 4] = [
    Device::Tb303_1,
    Device::Tb303_2,
    Device::Tr808,
    Device::Tr909,
];

/// The patterns of `p` (a [`Load::patterns`] array of `d`) that differ from `built`, by index.
pub fn changed_patterns<'a>(
    d: Device,
    p: &'a [u8],
    built: &'a [u8],
) -> impl Iterator<Item = (u8, &'a [u8])> + 'a {
    let len = orb_engine::frame::edit::PatternShape::of(d).map_or(0, |s| s.len);
    (0..PATTERNS).filter_map(move |i| {
        let r = i * len..(i + 1) * len;
        let (a, b) = (p.get(r.clone())?, built.get(r)?);
        (a != b).then_some((i as u8, a))
    })
}

/// A message from the worklet.
#[derive(Debug, Clone, PartialEq)]
pub enum FromWorklet {
    /// The worklet runs: its wasm instance is up, at the AudioContext's `rate`.
    Ready { rate: u32 },
    /// The state after an engine call. `received` is the count of [`ToWorklet`] messages the
    /// worklet had taken before that call, which the call has applied
    /// ([`crate::remote::Player::unapplied`]).
    Snapshot {
        snapshot: Box<Snapshot>,
        received: u64,
    },
    /// The current song's tracks, after they changed ([`crate::Player::tracks`]).
    Tracks(Box<Tracks>),
    /// The worklet failed: a malformed message, or its wasm instance trapped (the text says
    /// what). It plays silence from then on.
    Error(String),
}

const F_READY: u8 = 1;
const F_SNAPSHOT: u8 = 2;
const F_TRACKS: u8 = 3;
const F_ERROR: u8 = 4;

fn write_live(w: &mut W, l: &LiveBlock) {
    w.bytes(l.as_slice());
}

fn read_live(r: &mut Rd) -> R<LiveBlock> {
    let b = r.bytes()?;
    if b.len() > LiveBlock::MAX {
        return Err(Malformed);
    }
    let mut l = LiveBlock::default();
    l.set(b);
    Ok(l)
}

fn write_mixer(w: &mut W, m: &MixerParams) {
    w.u8(m.master);
    w.u8(m.comp_id);
    w.u8(m.pcf_id);
    for ch in &m.channels {
        for v in [ch.enabled, ch.level, ch.pan, ch.send, ch.dist] {
            w.u8(v);
        }
    }
}

fn read_mixer(r: &mut Rd) -> R<MixerParams> {
    let mut m = MixerParams {
        master: r.u8()?,
        comp_id: r.u8()?,
        pcf_id: r.u8()?,
        channels: [ChannelParams::default(); 4],
    };
    for ch in &mut m.channels {
        *ch = ChannelParams {
            enabled: r.u8()?,
            level: r.u8()?,
            pan: r.u8()?,
            send: r.u8()?,
            dist: r.u8()?,
        };
    }
    Ok(m)
}

fn write_snapshot(w: &mut W, s: &Snapshot) {
    w.u64(s.frames);
    w.u32(s.generation);
    w.bool(s.pending);
    w.i32(s.tick);
    w.bool(s.playing);
    w.bool(s.recording);
    w.bool(s.song_mode);
    w.bool(s.loop_on);
    w.u32(s.loop_start);
    w.u32(s.loop_end);
    w.u32(s.tempo);
    w.u8(s.shuffle);
    w.bool(s.vintage);
    write_mixer(w, &s.mixer);
    w.0.extend_from_slice(&s.channel_meters);
    w.0.extend_from_slice(&s.master_meters);
    for p in &s.patterns {
        match p {
            None => w.u8(0),
            Some(p) => {
                w.u8(1);
                w.u8(*p);
            }
        }
    }
    for v in &s.sounding {
        w.u32(v.mask);
        w.u32(v.ac);
        for &h in &v.hits {
            w.u32(h);
        }
    }
    for &g in &s.gate_closed {
        w.bool(g);
    }
    for l in s.live.iter().chain(&s.saved) {
        write_live(w, l);
    }
    write_mixer(w, &s.saved_mixer);
    w.u64(s.faulted_calls);
    w.u64(s.output_errors);
    w.bool(s.host_sync);
    w.u64(s.midi_in);
}

fn read_snapshot(r: &mut Rd) -> R<Snapshot> {
    let mut s = Snapshot {
        frames: r.u64()?,
        generation: r.u32()?,
        pending: r.bool()?,
        tick: r.i32()?,
        playing: r.bool()?,
        recording: r.bool()?,
        song_mode: r.bool()?,
        loop_on: r.bool()?,
        loop_start: r.u32()?,
        loop_end: r.u32()?,
        tempo: r.u32()?,
        shuffle: r.u8()?,
        vintage: r.bool()?,
        mixer: read_mixer(r)?,
        channel_meters: r.arr()?,
        master_meters: r.arr()?,
        ..Snapshot::default()
    };
    for p in &mut s.patterns {
        *p = match r.u8()? {
            0 => None,
            1 => Some(r.u8()?),
            _ => return Err(Malformed),
        };
    }
    for v in &mut s.sounding {
        let mut x = Sounding {
            mask: r.u32()?,
            ac: r.u32()?,
            hits: [0; 16],
        };
        for h in &mut x.hits {
            *h = r.u32()?;
        }
        *v = x;
    }
    for g in &mut s.gate_closed {
        *g = r.bool()?;
    }
    for l in s.live.iter_mut() {
        *l = read_live(r)?;
    }
    for l in s.saved.iter_mut() {
        *l = read_live(r)?;
    }
    s.saved_mixer = read_mixer(r)?;
    s.faulted_calls = r.u64()?;
    s.output_errors = r.u64()?;
    s.host_sync = r.bool()?;
    s.midi_in = r.u64()?;
    Ok(s)
}

/// The bytes of [`FromWorklet::Tracks`] of `t`, without the copy into a box.
pub fn encode_tracks(t: &Tracks) -> Vec<u8> {
    let mut w = W::default();
    w.u8(F_TRACKS);
    w.u32(t.generation);
    w.u64(t.revision);
    w.u64(t.merges);
    for k in 0..TRACKS {
        let l = t.events.list(k);
        w.len(l.len());
        for e in l {
            w.i32(e.tick);
            w.u8(e.ctrl);
            w.u8(e.value);
        }
    }
    w.0
}

impl FromWorklet {
    pub fn encode(&self) -> Vec<u8> {
        let mut w = W::default();
        match self {
            FromWorklet::Ready { rate } => {
                w.u8(F_READY);
                w.u32(*rate);
            }
            FromWorklet::Snapshot { snapshot, received } => {
                w.u8(F_SNAPSHOT);
                w.u64(*received);
                write_snapshot(&mut w, snapshot);
            }
            FromWorklet::Tracks(t) => return encode_tracks(t),
            FromWorklet::Error(e) => {
                w.u8(F_ERROR);
                w.bytes(e.as_bytes());
            }
        }
        w.0
    }

    pub fn decode(b: &[u8]) -> Result<FromWorklet, Malformed> {
        let mut r = Rd(b);
        let m = match r.u8()? {
            F_READY => FromWorklet::Ready { rate: r.u32()? },
            F_SNAPSHOT => {
                let received = r.u64()?;
                FromWorklet::Snapshot {
                    snapshot: Box::new(read_snapshot(&mut r)?),
                    received,
                }
            }
            F_TRACKS => {
                let generation = r.u32()?;
                let revision = r.u64()?;
                let merges = r.u64()?;
                let mut lists: Vec<Vec<Event>> = Vec::with_capacity(TRACKS);
                for _ in 0..TRACKS {
                    let n = r.len()?;
                    let mut l = Vec::new();
                    for _ in 0..n {
                        l.push(Event {
                            tick: r.i32()?,
                            ctrl: r.u8()?,
                            value: r.u8()?,
                        });
                    }
                    lists.push(l);
                }
                FromWorklet::Tracks(Box::new(Tracks {
                    generation,
                    revision,
                    merges,
                    events: Pool::from_lists(lists.iter().map(|l| &l[..])),
                }))
            }
            F_ERROR => FromWorklet::Error(String::from_utf8_lossy(r.bytes()?).into_owned()),
            _ => return Err(Malformed),
        };
        r.end()?;
        Ok(m)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip(c: Command) -> Command {
        match ToWorklet::decode(&ToWorklet::Command(c).encode()).unwrap() {
            ToWorklet::Command(c) => c,
            _ => panic!("not a command"),
        }
    }

    /// Every command that goes as itself comes back the same (compared by its bytes, as Command
    /// has no PartialEq).
    #[test]
    fn commands_round_trip() {
        let all = vec![
            Command::Play,
            Command::Stop,
            Command::Locate(1536),
            Command::Position(12.75),
            Command::NoteClock(NoteClock::At(-3.5)),
            Command::NoteClock(NoteClock::Run),
            Command::NoteClock(NoteClock::Off),
            Command::Relocate(Device::Tr909),
            Command::Note {
                device: Device::Tb303_2,
                note: 60,
                velocity: 99,
            },
            Command::NotesOff(Device::Tr808),
            Command::Gate {
                device: Device::Tr808,
                gate: Gate {
                    on: true,
                    open: false,
                    pattern: 9,
                },
            },
            Command::SongMode(true),
            Command::Record(true),
            Command::LoopOn(false),
            Command::LoopStart(768),
            Command::LoopEnd(7680),
            Command::Tempo(133_000),
            Command::Shuffle(64),
            Command::Vintage(true),
            Command::InitSong(Box::new(Pool::new())),
            Command::InitLoop(Box::new(Pool::new())),
            Command::UndoInit,
            Command::Param {
                device: Device::Delay,
                ctrl: 3,
                value: 70,
            },
            Command::Master(90),
            Command::ChannelEnabled {
                channel: 2,
                on: false,
            },
            Command::DeviceData {
                device: Device::Distortion,
                payload: vec![1, 2, 3].into(),
            },
            Command::DeviceParam {
                device: Device::Pcf,
                ctrl: 4,
                value: 5,
            },
            Command::MixerGlobal { id: 1, value: 2 },
            Command::MixerChannel {
                channel: 3,
                field: 4,
                value: 5,
            },
            Command::PatternField {
                device: Device::Tb303_1,
                pattern: 7,
                field: 1,
                value: 16,
            },
            Command::PatternHeader {
                device: Device::Tr909,
                pattern: 31,
                header: [1, 9],
            },
            Command::Pattern {
                device: Device::Tb303_1,
                pattern: 3,
                bytes: (0..98).collect::<Vec<u8>>().into(),
            },
            Command::StepField {
                device: Device::Tr808,
                pattern: 2,
                step: 15,
                field: 11,
                value: 1,
            },
            Command::Step {
                device: Device::Tr808,
                pattern: 2,
                step: 4,
                bytes: [9; 12],
            },
            Command::PreviewStop,
        ];
        for c in all {
            let mut a = W::default();
            assert!(write_command(&mut a, &c));
            let mut b = W::default();
            assert!(write_command(&mut b, &round_trip(c)));
            assert_eq!(a.0, b.0);
        }
    }

    #[test]
    fn preview_audio_round_trip() {
        let s = Arc::new(Sample::from_audio(&[1, -2, 3, i16::MIN, i16::MAX]));
        match round_trip(Command::Preview(Box::new(Preview::new(s.clone())))) {
            Command::Preview(p) => assert_eq!(*p.sample(), *s),
            _ => panic!("not a preview"),
        }
    }

    #[test]
    fn snapshot_round_trip() {
        let mut s = Snapshot {
            frames: 1 << 40,
            generation: 3,
            pending: true,
            tick: -5,
            playing: true,
            recording: true,
            song_mode: true,
            loop_on: true,
            loop_start: 768,
            loop_end: 1536,
            tempo: 128_500,
            shuffle: 33,
            vintage: true,
            channel_meters: [1, 2, 3, 4],
            master_meters: [5, 6],
            patterns: [None, Some(0), Some(31), Some(7)],
            gate_closed: [true, false, true, false],
            faulted_calls: 2,
            output_errors: 9,
            host_sync: true,
            midi_in: 77,
            ..Snapshot::default()
        };
        s.mixer.master = 12;
        s.mixer.channels[3].dist = 8;
        s.saved_mixer.pcf_id = 4;
        s.sounding[2] = Sounding {
            mask: 0xF0F0,
            ac: 3,
            hits: [7; 16],
        };
        s.live[3].set(&[9; LiveBlock::MAX]);
        s.saved[8].set(&[1, 2, 3]);
        let m = FromWorklet::Snapshot {
            snapshot: Box::new(s),
            received: 42,
        };
        assert_eq!(FromWorklet::decode(&m.encode()).unwrap(), m);
    }

    #[test]
    fn tracks_round_trip() {
        let e = |tick, ctrl, value| Event { tick, ctrl, value };
        let lists = [vec![e(0, 1, 2), e(24, 3, 4)], vec![], vec![e(768, 0, 127)]];
        let t = Tracks {
            generation: 2,
            revision: 5,
            merges: 1,
            events: Pool::from_lists(lists.iter().map(|l| &l[..])),
        };
        let m = FromWorklet::Tracks(Box::new(t));
        assert_eq!(FromWorklet::decode(&m.encode()).unwrap(), m);
    }

    #[test]
    fn truncated_and_trailing_bytes_are_malformed() {
        let b = ToWorklet::Command(Command::Tempo(120_000)).encode();
        assert!(ToWorklet::decode(&b[..b.len() - 1]).is_err());
        let mut long = b.clone();
        long.push(0);
        assert!(ToWorklet::decode(&long).is_err());
        assert!(ToWorklet::decode(&[]).is_err());
        assert!(FromWorklet::decode(&[0xEE]).is_err());
    }
}
