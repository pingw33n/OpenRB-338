//! The web player's test page (task-171, `docs/web.md`): `web/demo/index.html` drives this
//! module (built by `scripts/web-demo.sh`) to play a corpus song with the built-in kit through
//! [`orb_player::web::WebOutput`] and a [`orb_player::remote::Player`], and shows the state the
//! checks need: the context's rate and state, the snapshot, the latency of a command, and the
//! worklet's statistics.
#![cfg(target_arch = "wasm32")]

use std::sync::Arc;

use orb_engine::samples::SampleSet;
use orb_engine::tables::Tables;
use orb_player::Command;
use orb_player::remote::Player;
use orb_player::web::WebOutput;
use wasm_bindgen::prelude::*;

/// The shipped corpus songs the page offers (tests/songs/shipped).
const SONGS: [(&str, &[u8]); 3] = [
    (
        "Fonda Workout",
        include_bytes!("../../../tests/songs/shipped/Fonda Workout.rbs"),
    ),
    (
        "Calm",
        include_bytes!("../../../tests/songs/shipped/Calm.rbs"),
    ),
    (
        "A Taste of Haste",
        include_bytes!("../../../tests/songs/shipped/A Taste of Haste.rbs"),
    ),
];

fn now_ms() -> f64 {
    web_sys::window()
        .and_then(|w| w.performance())
        .map_or(0.0, |p| p.now())
}

fn err(e: impl std::fmt::Display) -> JsValue {
    JsValue::from_str(&e.to_string())
}

#[wasm_bindgen]
pub struct Demo {
    out: WebOutput,
    player: Player,
    kit: Arc<SampleSet>,
    /// A tempo sent and when (performance.now(), ms), until the snapshot shows it.
    tempo_sent: Option<(u32, f64)>,
    /// The last tempo's latency, ms: from the send until a snapshot shows it applied.
    latency: Option<f64>,
    latencies: Vec<f64>,
    errors: Vec<String>,
}

/// Starts the output with the worklet module at `worklet_url` and a player with the built-in
/// kit (the context waits for a gesture).
#[wasm_bindgen]
pub async fn start(worklet_url: String) -> Result<Demo, JsValue> {
    let out = WebOutput::start(&worklet_url).await?;
    let kit = rbm::standard::builtin_samples();
    let player = Player::new(
        Arc::new(Tables::new()),
        Some(kit.clone()),
        Box::new(out.transport()),
    );
    Ok(Demo {
        out,
        player,
        kit,
        tempo_sent: None,
        latency: None,
        latencies: Vec::new(),
        errors: Vec::new(),
    })
}

#[wasm_bindgen]
impl Demo {
    /// The names of the songs built in.
    pub fn songs() -> Vec<String> {
        SONGS.iter().map(|(n, _)| n.to_string()).collect()
    }

    /// Opens built-in song `i`.
    pub fn load_builtin(&mut self, i: usize) -> Result<(), JsValue> {
        let (_, b) = SONGS.get(i).ok_or("no such song")?;
        self.load_file(b)
    }

    /// Opens an `.rbs` file's bytes, as the UI does: stop, then the song, current at once.
    pub fn load_file(&mut self, bytes: &[u8]) -> Result<(), JsValue> {
        let parsed = rbs::Song::parse(bytes).map_err(err)?;
        let song = orb_engine::load::convert(&parsed).map_err(|e| err(format!("{e:?}")))?;
        let e = self
            .player
            .build(&song, Some(self.kit.clone()))
            .map_err(|r| err(format!("{r:?}")))?;
        self.send(Command::Stop);
        self.send(Command::Load(e));
        Ok(())
    }

    fn send(&mut self, c: Command) {
        if self.player.send(c).is_err() {
            self.errors.push("a command was refused".into());
        }
    }

    pub fn play(&mut self) {
        self.send(Command::Play);
    }

    pub fn stop(&mut self) {
        self.send(Command::Stop);
    }

    /// Sends the tempo `bpm` and times until the snapshot shows it.
    pub fn set_tempo(&mut self, bpm: f64) {
        let t = (bpm * 1000.0).round() as u32;
        self.tempo_sent = Some((t, now_ms()));
        self.send(Command::Tempo(t));
    }

    /// Resumes the context (from a gesture's handler).
    pub fn resume(&self) -> Result<(), JsValue> {
        self.out.resume().map(|_| ())
    }

    /// The context, for the page's own readouts.
    pub fn context(&self) -> web_sys::AudioContext {
        self.out.context().clone()
    }

    /// Takes the worklet's messages; the state as JSON.
    pub fn poll(&mut self) -> String {
        let s = *self.player.snapshot();
        if let Some((t, at)) = self.tempo_sent
            && s.tempo == t
            && self.player.unapplied() == 0
        {
            let ms = now_ms() - at;
            self.latency = Some(ms);
            self.latencies.push(ms);
            self.tempo_sent = None;
        }
        self.errors.extend(self.player.take_errors());
        let ctx = self.out.context();
        let stats = self
            .out
            .worklet_stats()
            .and_then(|v| js_sys::JSON::stringify(&v).ok())
            .and_then(|s| s.as_string())
            .unwrap_or_else(|| "null".into());
        let n = self.latencies.len();
        let (lo, hi, mean) = if n == 0 {
            (0.0, 0.0, 0.0)
        } else {
            let lo = self.latencies.iter().cloned().fold(f64::INFINITY, f64::min);
            let hi = self.latencies.iter().cloned().fold(0.0, f64::max);
            (lo, hi, self.latencies.iter().sum::<f64>() / n as f64)
        };
        let errors: Vec<String> = self.errors.iter().map(|e| format!("{e:?}")).collect();
        format!(
            concat!(
                "{{\"state\":\"{:?}\",\"sampleRate\":{},\"workletRate\":{},",
                "\"baseLatency\":{},\"frames\":{},\"tick\":{},\"bar\":{},\"playing\":{},",
                "\"tempo\":{},\"generation\":{},\"unapplied\":{},\"latencyMs\":{},",
                "\"latencyCount\":{},\"latencyMinMs\":{:.1},\"latencyMaxMs\":{:.1},",
                "\"latencyMeanMs\":{:.1},\"worklet\":{},\"errors\":[{}]}}"
            ),
            ctx.state(),
            ctx.sample_rate(),
            self.player.rate().map_or("null".into(), |r| r.to_string()),
            js_sys::Reflect::get(ctx, &"baseLatency".into())
                .ok()
                .and_then(|v| v.as_f64())
                .map_or("null".into(), |v| v.to_string()),
            s.frames,
            s.tick,
            s.tick / 768 + 1,
            s.playing,
            s.tempo as f64 / 1000.0,
            s.generation,
            self.player.unapplied(),
            self.latency.map_or("null".into(), |l| format!("{l:.1}")),
            n,
            lo,
            hi,
            mean,
            stats,
            errors.join(",")
        )
    }
}
