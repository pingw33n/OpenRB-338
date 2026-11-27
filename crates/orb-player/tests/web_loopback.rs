//! The web player's two halves (task-171): [`orb_player::remote::Player`] talking to
//! [`orb_player::worklet::Host`] through the wire format, in a loopback, play as the native
//! player does: the same samples, snapshots and tracks for the same commands.

mod common;

use std::sync::{Arc, Mutex};

use common::{kit, manifest, samples, song, tables};
use orb_engine::frame::edit::PatternShape;
use orb_engine::song::Device;
use orb_player::remote::{self, Transport};
use orb_player::worklet::Host;
use orb_player::{Audio, Command, Player};

/// The AudioWorklet's render quantum.
const QUANTUM: usize = 128;

/// Messages are taken by the host at once, as the worklet's port takes them between quanta.
struct Loopback(Arc<Mutex<Host>>);

impl Transport for Loopback {
    fn post(&mut self, msg: Vec<u8>) {
        self.0.lock().unwrap().receive(&msg);
    }

    fn poll(&mut self) -> Option<Vec<u8>> {
        self.0.lock().unwrap().take_outgoing()
    }
}

/// The native player and the web one side by side.
struct Pair {
    native: (Player, Audio),
    web: remote::Player,
    host: Arc<Mutex<Host>>,
}

impl Pair {
    fn new() -> Pair {
        let host = Arc::new(Mutex::new(Host::new(44_100)));
        let web = remote::Player::new(tables(), Some(samples()), Box::new(Loopback(host.clone())));
        Pair {
            native: Player::new(tables(), Some(samples())),
            web,
            host,
        }
    }

    /// Both render `quanta` quanta; their samples must be equal, bit for bit.
    fn render(&mut self, quanta: usize) {
        let mut a = vec![0f32; 2 * QUANTUM];
        for q in 0..quanta {
            self.native.1.render_f32(&mut a);
            let mut h = self.host.lock().unwrap();
            let b = h.render(QUANTUM);
            for i in 0..QUANTUM {
                assert_eq!(
                    (a[2 * i].to_bits(), a[2 * i + 1].to_bits()),
                    (b[i].to_bits(), b[QUANTUM + i].to_bits()),
                    "quantum {q}, frame {i}"
                );
            }
        }
        assert_eq!(self.web.snapshot(), self.native.0.snapshot());
        assert_eq!(self.web.tracks(), self.native.0.tracks());
        assert_eq!(self.web.unapplied(), 0);
        assert!(self.web.take_errors().is_empty());
    }

    fn send(&mut self, c: impl Fn() -> Command) {
        assert!(self.native.0.send(c()).is_ok());
        assert!(self.web.send(c()).is_ok());
    }
}

fn diag(name: &str) -> orb_engine::song::Song {
    song(&manifest()["diag"][name])
}

#[test]
fn plays_as_the_native_player() {
    let mut p = Pair::new();
    assert_eq!(p.web.rate(), None);
    p.render(3);
    assert_eq!(p.web.rate(), Some(44_100));

    // a song from a file, played
    let s = &manifest()["shipped"]["Calm"];
    let s = song(s);
    let e = p.native.0.build(&s, Some(samples())).unwrap();
    assert!(p.native.0.send(Command::Load(e)).is_ok());
    let e = p.web.build(&s, Some(samples())).unwrap();
    assert!(p.web.send(Command::Load(e)).is_ok());
    p.send(|| Command::Play);
    p.render(200);

    // knobs, the tempo, a pattern edit, the mixer
    p.send(|| Command::Tempo(141_000));
    p.send(|| Command::DeviceParam {
        device: Device::Tb303_1,
        ctrl: 3,
        value: 20,
    });
    p.render(37);
    p.send(|| Command::Pattern {
        device: Device::Tr909,
        pattern: 0,
        bytes: vec![1; PatternShape::of(Device::Tr909).unwrap().len].into(),
    });
    p.send(|| Command::MixerChannel {
        channel: 1,
        field: 2,
        value: 100,
    });
    p.render(150);

    // a mod change to a kit the worklet does not have yet, then the startup sound
    let k = kit(|x| x / 2);
    assert!(p.native.0.set_mod(k.clone()).is_ok());
    assert!(p.web.set_mod(k.clone()).is_ok());
    p.render(80);
    assert!(p.native.0.startup_sound(&k).is_ok());
    assert!(p.web.startup_sound(&k).is_ok());
    p.render(120);

    // recording into the song's tracks, merged at record off
    p.send(|| Command::SongMode(true));
    p.send(|| Command::Record(true));
    p.render(40);
    for v in [90, 20, 70, 10] {
        p.send(|| Command::DeviceParam {
            device: Device::Tb303_2,
            ctrl: 5,
            value: v,
        });
        p.render(60);
    }
    p.send(|| Command::Record(false));
    p.render(40);
    assert!(p.web.tracks().merges > 0);

    // a new song with an edited pattern (as the UI's template), loaded behind a stop
    p.send(|| Command::Stop);
    let edit = |e: &mut orb_engine::frame::Engine| {
        let len = PatternShape::of(Device::Tb303_1).unwrap().len;
        e.pattern(Device::Tb303_1, 0, &vec![2; len]);
        e.pattern(
            Device::Tr808,
            5,
            &vec![1; PatternShape::of(Device::Tr808).unwrap().len],
        );
    };
    let mut e = p.native.0.build_new(Some(samples()));
    edit(&mut e);
    assert!(p.native.0.send(Command::Load(e)).is_ok());
    let mut e = p.web.build_new(Some(samples()));
    edit(&mut e);
    assert!(p.web.send(Command::Load(e)).is_ok());
    p.send(|| Command::Play);
    p.render(150);

    // Initialize Song and its Undo
    p.send(|| Command::InitSong(Box::new(orb_engine::seq::Pool::new())));
    p.render(10);
    p.send(|| Command::UndoInit);
    p.render(10);
    p.send(|| Command::PreviewStop);
    p.send(|| Command::Stop);
    p.render(20);
}

/// A song the worklet builds from its load image ([`orb_player::wire::LoadImage`]) plays as the
/// song itself, for every song of the built-in-kit corpus: the first second of each, and its
/// tracks.
#[test]
fn every_corpus_song_loads_the_same_from_its_image() {
    let m = manifest();
    let mut n = 0;
    for set in ["diag", "app", "shipped"] {
        for (name, entry) in m[set].as_object().unwrap() {
            let s = song(entry);
            let mut p = Pair::new();
            let e = p.native.0.build(&s, Some(samples())).unwrap();
            assert!(p.native.0.send(Command::Load(e)).is_ok());
            let e = p.web.build(&s, Some(samples())).unwrap();
            assert!(p.web.send(Command::Load(e)).is_ok());
            p.send(|| Command::Play);
            p.render(350);
            assert_eq!(p.web.snapshot().generation, 1, "{set}/{name}");
            n += 1;
        }
    }
    assert_eq!(n, 164);
}

/// A loaded song waits behind a playing one, and Play switches to it, as natively.
#[test]
fn pending_song_switch() {
    let mut p = Pair::new();
    let (a, b) = (diag("808_bass"), diag("303_slide"));
    let e = p.native.0.build(&a, Some(samples())).unwrap();
    assert!(p.native.0.send(Command::Load(e)).is_ok());
    let e = p.web.build(&a, Some(samples())).unwrap();
    assert!(p.web.send(Command::Load(e)).is_ok());
    p.send(|| Command::Play);
    p.render(40);
    let e = p.native.0.build(&b, None).unwrap();
    assert!(p.native.0.send(Command::Load(e)).is_ok());
    let e = p.web.build(&b, None).unwrap();
    assert!(p.web.send(Command::Load(e)).is_ok());
    p.render(40);
    assert!(p.web.snapshot().pending);
    p.send(|| Command::Play);
    p.render(40);
    assert!(!p.web.snapshot().pending);
    assert_eq!(p.web.snapshot().generation, 2);
}

/// The snapshot shows a command only after the engine call that applies it: until then
/// [`remote::Player::unapplied`] counts it.
#[test]
fn unapplied_counts_until_the_call() {
    let mut p = Pair::new();
    p.render(8);
    assert!(p.web.send(Command::Tempo(99_000)).is_ok());
    assert_eq!(p.web.unapplied(), 1);
    // the call in progress does not take it; the next one does (8 quanta per call)
    let mut quanta = 0;
    while p.web.unapplied() > 0 {
        p.host.lock().unwrap().render(QUANTUM);
        p.web.snapshot();
        quanta += 1;
        assert!(quanta <= 8, "not applied after a call");
    }
    assert_eq!(p.web.snapshot().tempo, 99_000);
}

/// An engine the web player did not build cannot be sent: its recipe is unknown.
#[test]
#[should_panic(expected = "built by Player::build")]
fn foreign_engine_panics() {
    let mut p = Pair::new();
    let e = p.native.0.build_new(None);
    let _ = p.web.send(Command::Load(e));
}

/// A wrong-length block is refused as natively.
#[test]
fn wrong_lengths_are_refused() {
    let mut p = Pair::new();
    assert!(
        p.web
            .send(Command::DeviceData {
                device: Device::Delay,
                payload: vec![0; 3].into(),
            })
            .is_err()
    );
    assert!(
        p.web
            .send(Command::Pattern {
                device: Device::Tb303_1,
                pattern: 0,
                bytes: vec![0; 5].into(),
            })
            .is_err()
    );
}

/// At a device rate other than 44.1 kHz (a browser that refused it) the host resamples: the
/// output runs at the rate and is not silent.
#[test]
fn resamples_at_another_rate() {
    let host = Arc::new(Mutex::new(Host::new(48_000)));
    let mut web = remote::Player::new(tables(), Some(samples()), Box::new(Loopback(host.clone())));
    let e = web.build(&diag("808_bass"), Some(samples())).unwrap();
    assert!(web.send(Command::Load(e)).is_ok());
    assert!(web.send(Command::Play).is_ok());
    let mut peak = 0f32;
    for _ in 0..400 {
        let mut h = host.lock().unwrap();
        for x in h.render(QUANTUM) {
            peak = peak.max(x.abs());
        }
    }
    assert!(peak > 0.01, "silent: {peak}");
    let f = web.snapshot().frames;
    assert_eq!(web.rate(), Some(48_000));
    // 400 quanta at 48 kHz are about 400 * 128 * 44100 / 48000 engine frames
    assert!((46_000..=49_000).contains(&f), "{f} frames");
}
