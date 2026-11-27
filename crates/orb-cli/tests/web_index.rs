//! `orb web-index` (task-173): the index of the rb-songs and rb-mods collections for the web
//! version, on a small fixture built here from a corpus song and the built-in kit
//! (`assets/Standard.rbm`): two runs give the same bytes, the columns, the mod lookup rule of
//! task-150/151 (checked against orb's own `mods::scan` and `mods::find_in_dir` over the same
//! folder), a mod in a subfolder that must not match, an unreadable song, an unrecrypted mod, a
//! file that is no mod and a file that cannot be read. The format is in docs/web.md section 8.

mod common;

use std::collections::HashMap;
use std::path::Path;
use std::process::{Command, Output};

use common::{root, standard_mod};
use orb_cli::mods;
use serde_json::Value;

const SONGS_COMMIT: &str = "6e0b92e17727dff14010f66deadfa3634f5919bb";
const MODS_COMMIT: &str = "bfbd8c94ea669fdd57c156bd7b544540054d9dfd";

fn put(path: &Path, bytes: &[u8]) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, bytes).unwrap();
}

/// A song of the corpus with its mod name changed.
fn song_naming(mod_name: &str) -> Vec<u8> {
    let bytes = std::fs::read(root().join("tests/songs/shipped/Calm.rbs")).unwrap();
    let mut s = rbs::Song::parse(&bytes).unwrap();
    s.glob.mod_name = mod_name.into();
    s.write().unwrap()
}

/// The standard mod with a byte of its clear part changed: it still parses, but its key pages
/// are no longer the embedded window, so it is not recrypted.
fn unrecrypted() -> Vec<u8> {
    let mut b = std::fs::read(standard_mod()).unwrap();
    let r = rbm::Rbm::parse(b.clone()).unwrap();
    let c = r.embedded().next().unwrap();
    b[c.offset + c.name_len + 5] ^= 0x55;
    assert!(rbm::Rbm::parse(b.clone()).is_ok());
    b
}

/// A file nobody can read (not on Windows, and not as root, where it stays readable).
fn unreadable(path: &Path, bytes: &[u8]) -> bool {
    put(path, bytes);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o0)).unwrap();
    }
    if std::fs::read(path).is_ok() {
        std::fs::remove_file(path).unwrap();
        return false;
    }
    true
}

/// `songs/` and `mods/` under `dir`; whether the unreadable files could be made.
fn fixture(dir: &Path) -> bool {
    let calm = std::fs::read(root().join("tests/songs/shipped/Calm.rbs")).unwrap();
    put(&dir.join("songs/Calm.rbs"), &calm);
    put(
        &dir.join("songs/sub folder/Upper.RBS"),
        &song_naming("STANDARD REBIRTH"),
    );
    put(
        &dir.join("songs/sub folder/Gone.rbs"),
        &song_naming("No Such Mod"),
    );
    put(&dir.join("songs/broken.rbs"), b"this is not a song");
    put(&dir.join("songs/readme.txt"), b"not indexed");
    let std = std::fs::read(standard_mod()).unwrap();
    // three mods share the INFO name: "B-std.rbm" sorts before "a-std.rbm" by bytes
    put(&dir.join("mods/a-std.rbm"), &std);
    put(&dir.join("mods/B-std.rbm"), &std);
    put(&dir.join("mods/unrec.rbm"), &unrecrypted());
    put(&dir.join("mods/garbage.rbm"), b"not a mod");
    // orb reads the top level of the mod folder only: this one never matches
    put(&dir.join("mods/sub/deep.rbm"), &std);
    let a = unreadable(&dir.join("songs/locked.rbs"), &calm);
    let b = unreadable(&dir.join("mods/locked.rbm"), &std);
    a && b
}

fn run(dir: &Path, out: &Path, commits: bool) -> Output {
    let mut c = Command::new(env!("CARGO_BIN_EXE_orb"));
    c.arg("web-index")
        .arg("--songs")
        .arg(dir.join("songs"))
        .arg("--mods")
        .arg(dir.join("mods"));
    if commits {
        c.args(["--songs-commit", SONGS_COMMIT, "--mods-commit", MODS_COMMIT]);
    }
    c.arg("-o").arg(out).output().unwrap()
}

/// The rows of the table `rows` as maps from the column names of `cols`.
fn table(v: &Value, cols: &str, rows: &str) -> Vec<HashMap<String, Value>> {
    let cols: Vec<&str> = v[cols]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c.as_str().unwrap())
        .collect();
    v[rows]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| {
            let r = r.as_array().unwrap();
            assert_eq!(r.len(), cols.len(), "{r:?}");
            cols.iter()
                .map(|c| c.to_string())
                .zip(r.iter().cloned())
                .collect()
        })
        .collect()
}

#[test]
fn index_of_a_fixture() {
    let tmp = tempfile::tempdir().unwrap();
    let locked = fixture(tmp.path());
    let (a, b) = (tmp.path().join("a.json"), tmp.path().join("b.json"));
    for out in [&a, &b] {
        let o = run(tmp.path(), out, true);
        assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    }
    // two runs, the same bytes
    let bytes = std::fs::read(&a).unwrap();
    assert_eq!(bytes, std::fs::read(&b).unwrap());
    assert!(bytes.ends_with(b"\n"));
    let v: Value = serde_json::from_slice(&bytes).unwrap();

    assert_eq!(v["format"], "orb-web-index");
    assert_eq!(v["version"], 1);
    assert_eq!(v["songs_repo"]["name"], "rb-songs");
    assert_eq!(v["songs_repo"]["commit"], SONGS_COMMIT);
    assert_eq!(
        v["songs_repo"]["raw_base"],
        format!("https://raw.githubusercontent.com/pingw33n/rb-songs/{SONGS_COMMIT}/")
    );
    assert_eq!(
        v["mods_repo"]["raw_base"],
        format!("https://raw.githubusercontent.com/pingw33n/rb-mods/{MODS_COMMIT}/")
    );
    assert_eq!(v["mods_dir"], "mods");
    assert_eq!(
        v["song_dirs"],
        serde_json::json!(["songs", "songs/sub folder"])
    );

    // the files: only .rbs / .rbm in any letter case; the mods of the top level only; sorted
    let dirs: Vec<&str> = v["song_dirs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d.as_str().unwrap())
        .collect();
    let songs = table(&v, "song_columns", "songs");
    let song_path = |s: &HashMap<String, Value>| {
        format!(
            "{}/{}",
            dirs[s["dir"].as_u64().unwrap() as usize],
            s["file"].as_str().unwrap()
        )
    };
    let paths: Vec<String> = songs.iter().map(song_path).collect();
    let mut want = vec![
        "songs/Calm.rbs",
        "songs/broken.rbs",
        "songs/sub folder/Gone.rbs",
        "songs/sub folder/Upper.RBS",
    ];
    if locked {
        want.insert(2, "songs/locked.rbs");
    }
    assert_eq!(paths, want);
    let mods = table(&v, "mod_columns", "mods");
    let mod_files: Vec<&str> = mods.iter().map(|m| m["file"].as_str().unwrap()).collect();
    let mut want = vec!["B-std.rbm", "a-std.rbm", "garbage.rbm"];
    if locked {
        want.push("locked.rbm");
    }
    want.push("unrec.rbm");
    assert_eq!(mod_files, want);

    // a song: the columns, the size and the hash of the file
    let song = |p: &str| &songs[paths.iter().position(|x| x == p).unwrap()];
    let calm = song("songs/Calm.rbs");
    let file = std::fs::read(root().join("tests/songs/shipped/Calm.rbs")).unwrap();
    assert_eq!(calm["size"], file.len());
    assert_eq!(calm["sha256"].as_str().unwrap().len(), 43);
    assert!(!calm["sha256"].as_str().unwrap().contains(['+', '/', '=']));
    assert_eq!(calm["error"], Value::Null);
    assert_eq!(calm["format"], "rbs");
    assert_eq!(calm["version"], "4.1");
    assert_eq!(calm["title"], "it down");
    assert_eq!(calm["mod_name"], "Standard ReBirth");
    assert_eq!(calm["mod_variants"], serde_json::json!([]));
    assert_eq!(calm["tempo_bpm"], 130.0);
    assert_eq!(calm["last_event_bar"], 176);

    // the mod lookup is orb's own, over the same folder: the INFO name only, ASCII case ignored,
    // every match in the byte order of the file names, and no mod of a subfolder
    let mod_dir = tmp.path().join("mods");
    let scanned = mods::scan(&mod_dir).unwrap();
    for p in ["songs/sub folder/Upper.RBS", "songs/sub folder/Gone.rbs"] {
        let s = song(p);
        let name = s["mod_name"].as_str().unwrap();
        let got: Vec<&str> = s["mod_variants"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| mod_files[i.as_u64().unwrap() as usize])
            .collect();
        let all: Vec<String> = scanned
            .iter()
            .filter(|(_, n)| n.as_deref().is_some_and(|n| mods::names_match(n, name)))
            .map(|(p, _)| p.file_name().unwrap().to_str().unwrap().to_string())
            .collect();
        assert_eq!(got, all, "{p}");
        let first = mods::find_in_dir(&mod_dir, name).unwrap();
        assert_eq!(
            got.first().map(|f| mod_dir.join(f)),
            first,
            "{p}: the first variant is the mod orb loads"
        );
        assert!(!got.contains(&"deep.rbm"), "{p}");
    }
    // the case that is the point: it matches by name, and the subfolder's copy does not exist
    assert_eq!(
        song("songs/sub folder/Upper.RBS")["mod_variants"],
        serde_json::json!([0, 1, if locked { 4 } else { 3 }])
    );
    assert_eq!(
        song("songs/sub folder/Gone.rbs")["mod_variants"],
        serde_json::json!([])
    );

    // an unreadable song is listed with its error and no facts
    let broken = song("songs/broken.rbs");
    assert!(broken["error"].as_str().unwrap().contains("not a valid"));
    assert_eq!(broken["size"], 18);
    assert_eq!(broken["title"], Value::Null);
    assert_eq!(broken["mod_variants"], Value::Null);
    if locked {
        let l = song("songs/locked.rbs");
        assert!(l["error"].as_str().unwrap().contains("cannot read"));
        assert_eq!((&l["size"], &l["sha256"]), (&Value::Null, &Value::Null));
    }

    // mods
    let std = std::fs::read(standard_mod()).unwrap();
    let m = |f: &str| &mods[mod_files.iter().position(|x| *x == f).unwrap()];
    let s = m("a-std.rbm");
    assert_eq!(s["size"], std.len());
    assert_eq!(s["sha256"].as_str().unwrap().len(), 43);
    assert_eq!(s["name"], "Standard ReBirth");
    assert_eq!(s["recrypted"], 1);
    assert_eq!(s["error"], Value::Null);
    let u = m("unrec.rbm");
    assert_eq!(u["name"], "Standard ReBirth");
    assert_eq!(u["recrypted"], 0);
    let g = m("garbage.rbm");
    assert_eq!(g["name"], Value::Null);
    assert_eq!(g["recrypted"], 0);
    assert!(g["error"].as_str().unwrap().contains("not a ReBirth mod"));
    if locked {
        let l = m("locked.rbm");
        assert_eq!((&l["size"], &l["sha256"]), (&Value::Null, &Value::Null));
        assert!(l["error"].as_str().unwrap().contains("cannot read"));
    }

    // without commits the repositories say so; the rest is the same
    let c = tmp.path().join("c.json");
    assert!(run(tmp.path(), &c, false).status.success());
    let w: Value = serde_json::from_slice(&std::fs::read(&c).unwrap()).unwrap();
    assert_eq!(w["songs_repo"]["commit"], Value::Null);
    assert_eq!(w["songs_repo"]["raw_base"], Value::Null);
    assert_eq!(w["songs"], v["songs"]);
    assert_eq!(w["mods"], v["mods"]);
}

#[test]
fn hidden_from_the_user_help() {
    let help = |args: &[&str]| {
        let o = Command::new(env!("CARGO_BIN_EXE_orb"))
            .args(args)
            .output()
            .unwrap();
        assert!(o.status.success());
        String::from_utf8(o.stdout).unwrap()
    };
    assert!(!help(&["--help"]).contains("web-index"));
    assert!(help(&["web-index", "--help"]).contains("orb-web-index"));
}
