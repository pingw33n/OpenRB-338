//! The engine frame against the reference corpus (`tests/refs/manifest.json`, task-021).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use orb_engine::frame::export::{self, ExportRange};
use orb_engine::frame::{Devices, Engine};
use orb_engine::tables::Tables;
use serde_json::Value;
use sha2::{Digest, Sha256};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn manifest() -> Value {
    let p = root().join("tests/refs/manifest.json");
    serde_json::from_slice(&std::fs::read(p).unwrap()).unwrap()
}

fn song(entry: &Value) -> orb_engine::song::Song {
    let p = root().join(entry["song"].as_str().unwrap());
    let data = std::fs::read(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
    orb_engine::load::convert(&rbs::Song::parse(&data).unwrap()).unwrap()
}

/// AC #2: the export driver's frame count equals the reference's for every diagnostic and
/// shipped song of the corpus.
#[test]
fn frame_counts_match_the_corpus() {
    let m = manifest();
    let mut checked = 0;
    for set in ["diag", "shipped"] {
        for (name, e) in m[set].as_object().unwrap() {
            let s = song(e);
            let got = export::frame_count(ExportRange::from(&s.globals));
            let want = e["frames"].as_u64().unwrap() as usize;
            assert_eq!(got, want, "{set}/{name}");
            checked += 1;
        }
    }
    assert_eq!(checked, 145 + 8);
}

/// AC #3: the silence song renders exactly equal to its reference (the WAV's sha256, header
/// included), at the manifest's idle count.
#[test]
fn silence_matches_its_reference() {
    let m = manifest();
    let e = &m["diag"]["silence"];
    let s = song(e);
    let mut eng = Engine::new(Arc::new(Tables::new()), Devices::placeholders());
    let idle = e["idle_blocks"].as_u64().unwrap() as u32;
    let samples = export::export_song(&mut eng, &s, idle).unwrap();
    assert_eq!(samples.len() / 2, e["frames"].as_u64().unwrap() as usize);
    let hash: String = Sha256::digest(export::wav_bytes(&samples))
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    assert_eq!(hash, e["wav_sha256"].as_str().unwrap());
}

/// The export in steps (`export::Export`, the web build's export, task-172) makes the same
/// calls as the one-shot export: a shipped song with the built-in kit (Calm), stepped one call and
/// seven calls at a time, equals `export_song` and its reference's sha256; the steps count up to
/// the total (the 9088 idle blocks' 568 calls, then the chunks).
#[test]
fn the_export_in_steps_equals_the_whole_export() {
    let m = manifest();
    let e = &m["shipped"]["Calm"];
    let s = song(e);
    let idle = e["idle_blocks"].as_u64().unwrap() as u32;
    let engine = || {
        Engine::new(
            Arc::new(Tables::new()),
            Devices::implemented(Some(rbm::standard::builtin_samples())),
        )
    };
    let whole = export::export_song(&mut engine(), &s, idle).unwrap();
    for per in [1, 7] {
        let mut eng = engine();
        let mut x = export::export_song_steps(&mut eng, &s, idle).unwrap();
        let total = x.total();
        assert_eq!(
            total,
            (idle as usize).div_ceil(16) + export::chunk_sizes(ExportRange::from(&s.globals)).len()
        );
        let mut steps = 0;
        while !x.step(&mut eng, per) {
            steps += 1;
            assert!(x.done() < total);
        }
        assert_eq!(x.done(), total);
        assert_eq!(steps, total.div_ceil(per) - 1, "per {per}");
        let samples = x.into_samples();
        assert!(samples == whole, "per {per}");
    }
    let hash: String = Sha256::digest(export::wav_bytes(&whole))
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    assert_eq!(hash, e["wav_sha256"].as_str().unwrap());
}
