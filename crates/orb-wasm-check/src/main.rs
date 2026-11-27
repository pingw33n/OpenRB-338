//! The renderer of the wasm-vs-native check (task-169, docs/building.md "The wasm build"): renders
//! the entries of the reference corpus (`tests/refs/manifest.json`) as the golden tests do
//! (`corpus_golden.rs`, `corpus_mods.rs`: a fresh engine per song, every device, the built-in kit
//! or the entry's recrypted mod) and prints one line per entry:
//!
//! ```text
//! SET/NAME FRAMES SHA256 equal|DIFF
//! ```
//!
//! `SHA256` is the SHA-256 of the whole WAV, and the last word says whether it is the manifest's
//! `wav_sha256`. The same program is built natively and for wasm32-wasip1 (run under wasmtime,
//! reading the files through WASI), and `scripts/wasm-check.sh` requires the two outputs to be
//! identical. A summary goes to stderr, so stdout holds only the lines.
//!
//! ```text
//! orb-wasm-check [--root DIR] [--set diag,app,shipped] [--mod-dir DIR]
//! ```
//!
//! `--root` is the workspace root (the manifest's paths are relative to it; default `.`), `--set`
//! the manifest sets (default `diag,app,shipped`, the ones that need no external input), and
//! `--mod-dir` the folder of the recrypted shipped mods that the sets `mod` and `modapp` need.

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;

use orb_engine::frame::export;
use orb_engine::frame::{Devices, Engine};
use orb_engine::samples::SampleSet;
use orb_engine::tables::Tables;
use serde_json::Value;
use sha2::{Digest, Sha256};

/// The sets that need no external input.
const DEFAULT_SETS: &str = "diag,app,shipped";

#[derive(Debug, PartialEq)]
struct Args {
    root: PathBuf,
    sets: Vec<String>,
    mod_dir: Option<PathBuf>,
}

fn parse_args(mut it: impl Iterator<Item = String>) -> Result<Args, String> {
    let mut a = Args {
        root: PathBuf::from("."),
        sets: split_sets(DEFAULT_SETS),
        mod_dir: None,
    };
    while let Some(arg) = it.next() {
        let mut value = || it.next().ok_or_else(|| format!("{arg} needs a value"));
        match arg.as_str() {
            "--root" => a.root = PathBuf::from(value()?),
            "--set" => a.sets = split_sets(&value()?),
            "--mod-dir" => a.mod_dir = Some(PathBuf::from(value()?)),
            _ => return Err(format!("unknown argument {arg}")),
        }
    }
    if a.sets.is_empty() {
        return Err("--set names no set".into());
    }
    Ok(a)
}

fn split_sets(s: &str) -> Vec<String> {
    s.split(',')
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

fn sha256_hex(b: &[u8]) -> String {
    Sha256::digest(b)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn read(p: &Path) -> Result<Vec<u8>, String> {
    std::fs::read(p).map_err(|e| format!("{}: {e}", p.display()))
}

/// What a render needs besides the entry.
struct Ctx {
    root: PathBuf,
    mod_dir: Option<PathBuf>,
    tables: Arc<Tables>,
    builtin: Arc<SampleSet>,
}

/// The entry's render: (frames, the SHA-256 of the WAV).
fn render(ctx: &Ctx, e: &Value) -> Result<(usize, String), String> {
    let song_path = ctx
        .root
        .join(e["song"].as_str().ok_or("entry without song")?);
    let data = read(&song_path)?;
    let parsed =
        rbs::Song::parse(&data).map_err(|err| format!("{}: {err}", song_path.display()))?;
    let song = orb_engine::load::convert(&parsed)
        .map_err(|err| format!("{}: {err:?}", song_path.display()))?;
    let set = match e["mod"]["file"].as_str() {
        None => ctx.builtin.clone(),
        Some(file) => {
            let dir = ctx.mod_dir.as_ref().ok_or("a mod entry needs --mod-dir")?;
            let rbm = rbm::Rbm::parse(read(&dir.join(file))?).map_err(|e| e.to_string())?;
            let t = rbm::KeyTable::embedded(&rbm).ok_or(format!("{file} is not recrypted"))?;
            let loaded = rbm
                .load_samples(&t, &ctx.builtin)
                .map_err(|e| e.to_string())?;
            Arc::new(loaded.set)
        }
    };
    let idle = e["idle_blocks"]
        .as_u64()
        .ok_or("entry without idle_blocks")? as u32;
    let mut eng = Engine::new(ctx.tables.clone(), Devices::implemented(Some(set)));
    let got = export::export_song(&mut eng, &song, idle).map_err(|e| format!("{e:?}"))?;
    Ok((got.len() / 2, sha256_hex(&export::wav_bytes(&got))))
}

/// The output line of an entry.
fn line(label: &str, e: &Value, frames: usize, sha: &str) -> String {
    let verdict = if e["wav_sha256"].as_str() == Some(sha) {
        "equal"
    } else {
        "DIFF"
    };
    format!("{label} {frames} {sha} {verdict}")
}

/// Renders the job: its line, or the error with the label.
fn render_job(ctx: &Ctx, (label, e): &(String, &Value)) -> Result<String, String> {
    render(ctx, e)
        .map(|(frames, sha)| line(label, e, frames, &sha))
        .map_err(|err| format!("{label}: {err}"))
}

/// Renders every job, on wasm (no threads) one after another. The results are in the order of
/// `jobs`.
#[cfg(target_family = "wasm")]
fn render_all(ctx: &Ctx, jobs: &[(String, &Value)]) -> Vec<Result<String, String>> {
    jobs.iter().map(|j| render_job(ctx, j)).collect()
}

/// Renders every job, natively on all the cores. The results are in the order of `jobs`.
#[cfg(not(target_family = "wasm"))]
fn render_all(ctx: &Ctx, jobs: &[(String, &Value)]) -> Vec<Result<String, String>> {
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};
    let out: Vec<Mutex<Option<Result<String, String>>>> =
        jobs.iter().map(|_| Mutex::new(None)).collect();
    let next = AtomicUsize::new(0);
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
    std::thread::scope(|sc| {
        for _ in 0..threads.min(jobs.len()) {
            sc.spawn(|| {
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    let Some(job) = jobs.get(i) else { break };
                    *out[i].lock().unwrap() = Some(render_job(ctx, job));
                }
            });
        }
    });
    out.into_iter()
        .map(|m| m.into_inner().unwrap().unwrap())
        .collect()
}

fn run(args: Args) -> Result<bool, String> {
    let mp = args.root.join("tests/refs/manifest.json");
    let manifest: Value =
        serde_json::from_slice(&read(&mp)?).map_err(|e| format!("{}: {e}", mp.display()))?;
    let mut jobs = Vec::new();
    for set in &args.sets {
        let entries = manifest[set.as_str()]
            .as_object()
            .ok_or(format!("the manifest has no set {set}"))?;
        jobs.extend(entries.iter().map(|(name, e)| (format!("{set}/{name}"), e)));
    }
    let ctx = Ctx {
        root: args.root,
        mod_dir: args.mod_dir,
        tables: Arc::new(Tables::new()),
        builtin: rbm::standard::builtin_samples(),
    };
    let (mut ok, mut equal) = (true, 0);
    for r in render_all(&ctx, &jobs) {
        match r {
            Ok(l) => {
                equal += usize::from(l.ends_with(" equal"));
                println!("{l}");
            }
            Err(err) => {
                ok = false;
                eprintln!("error: {err}");
            }
        }
    }
    eprintln!(
        "{} entries rendered, {equal} equal to the manifest",
        jobs.len()
    );
    Ok(ok)
}

fn main() -> ExitCode {
    let args = match parse_args(std::env::args().skip(1)) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("orb-wasm-check: {e}");
            return ExitCode::from(2);
        }
    };
    match run(args) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(e) => {
            eprintln!("orb-wasm-check: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(v: &[&str]) -> Result<Args, String> {
        parse_args(v.iter().map(|s| s.to_string()))
    }

    #[test]
    fn parses_the_arguments() {
        assert_eq!(
            args(&[]).unwrap(),
            Args {
                root: ".".into(),
                sets: vec!["diag".into(), "app".into(), "shipped".into()],
                mod_dir: None
            }
        );
        let a = args(&["--root", "r", "--set", "mod,modapp", "--mod-dir", "m"]).unwrap();
        assert_eq!(a.root, PathBuf::from("r"));
        assert_eq!(a.sets, ["mod", "modapp"]);
        assert_eq!(a.mod_dir, Some(PathBuf::from("m")));
        assert!(args(&["--set"]).is_err());
        assert!(args(&["--set", ","]).is_err());
        assert!(args(&["--bogus"]).is_err());
    }

    /// A diag song renders as the manifest says, and a mod entry without --mod-dir is an error.
    #[test]
    fn renders_a_manifest_entry() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let m: Value =
            serde_json::from_slice(&read(&root.join("tests/refs/manifest.json")).unwrap()).unwrap();
        let ctx = Ctx {
            root,
            mod_dir: None,
            tables: Arc::new(Tables::new()),
            builtin: rbm::standard::builtin_samples(),
        };
        let e = &m["diag"]["303_accent0"];
        let (frames, sha) = render(&ctx, e).unwrap();
        assert_eq!(frames as u64, e["frames"].as_u64().unwrap());
        assert_eq!(
            line("diag/x", e, frames, &sha),
            format!("diag/x {frames} {sha} equal")
        );
        assert!(line("diag/x", e, frames, "0").ends_with(" DIFF"));
        let (_, me) = m["modapp"].as_object().unwrap().iter().next().unwrap();
        assert_eq!(render(&ctx, me).unwrap_err(), "a mod entry needs --mod-dir");
    }
}
