//! `orb web-index` (task-173): an index of the rb-songs and rb-mods collections for the web
//! version (task-174). The page cannot list a repository or read a gigabyte of mods to find one
//! by its INFO name, so the deploy builds this file once and the page fetches it, then only the
//! song and the mod it needs. The format is in docs/web.md section 8: compact JSON (each table
//! has its columns once and its rows as arrays, folders and mods are numbers), because the page
//! downloads it.
//!
//! Every fact comes from code orb already has: a song's from [`crate::info::SongFacts`] (`orb
//! info --json`), a mod's name from `Rbm::parse` and its INFO (the rule of [`crate::mods::scan`],
//! which reads the top level of the mod folder only), and a song's mod by
//! [`crate::mods::names_match`] over the mods in [`crate::mods::byte_order`] (the rule of `orb
//! render`: the INFO name only, ASCII case ignored, the first of several that share a name).

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::info::SongFacts;
use crate::mods::{self, STANDARD};
use crate::{Fail, fail};

/// The format version of the index (`"version"`).
pub const VERSION: u32 = 1;

/// The columns of a row of `songs`.
pub const SONG_COLUMNS: [&str; 12] = [
    "dir",
    "file",
    "size",
    "sha256",
    "error",
    "format",
    "version",
    "title",
    "mod_name",
    "tempo_bpm",
    "last_event_bar",
    "mod_variants",
];

/// The columns of a row of `mods`.
pub const MOD_COLUMNS: [&str; 6] = ["file", "size", "sha256", "name", "recrypted", "error"];

/// Where the raw files of a GitHub repository are served (with `Access-Control-Allow-Origin: *`).
const RAW: &str = "https://raw.githubusercontent.com";

/// What `orb web-index` is given.
pub struct Options<'a> {
    pub songs: &'a Path,
    pub mods: &'a Path,
    pub songs_commit: Option<&'a str>,
    pub mods_commit: Option<&'a str>,
}

/// A file of a collection.
struct Entry {
    /// The folder's path relative to the repository's root, `/` separated, UTF-8.
    dir: String,
    /// The file's name.
    file: String,
    path: PathBuf,
}

/// The name of the folder `dir`: the first part of the paths below it, so that they are
/// relative to the repository, whose root holds that folder.
fn top_name(dir: &Path) -> Result<String, Fail> {
    let abs = std::path::absolute(dir).unwrap_or_else(|_| dir.to_path_buf());
    abs.file_name()
        .and_then(|n| n.to_str())
        .map(str::to_string)
        .ok_or_else(|| {
            fail(
                1,
                format!("{}: not a folder with a UTF-8 name", dir.display()),
            )
        })
}

/// The files of `dir` with the extension `ext` (ASCII case ignored), and with `deep` those of
/// the folders below it too, sorted by the bytes of their paths. The mods are not deep:
/// [`mods::scan`] reads the top level of the mod folder only, and so does the web page.
fn walk(dir: &Path, ext: &str, deep: bool) -> Result<Vec<Entry>, Fail> {
    fn rec(dir: &Path, rel: &str, ext: &str, deep: bool, out: &mut Vec<Entry>) -> Result<(), Fail> {
        let rd = crate::fs::read_dir(dir)
            .map_err(|e| fail(1, format!("{}: cannot read the folder: {e}", dir.display())))?;
        for p in rd {
            let name = p
                .file_name()
                .and_then(|n| n.to_str())
                .ok_or_else(|| fail(1, format!("{}: the name is not UTF-8", p.display())))?;
            if crate::fs::is_dir(&p) {
                if deep {
                    rec(&p, &format!("{rel}/{name}"), ext, deep, out)?;
                }
            } else if crate::fs::is_file(&p)
                && p.extension().is_some_and(|x| x.eq_ignore_ascii_case(ext))
            {
                out.push(Entry {
                    dir: rel.to_string(),
                    file: name.to_string(),
                    path: p,
                });
            }
        }
        Ok(())
    }
    let mut out = Vec::new();
    rec(dir, &top_name(dir)?, ext, deep, &mut out)?;
    out.sort_by(|a, b| (a.dir.as_bytes(), &a.file).cmp(&(b.dir.as_bytes(), &b.file)));
    Ok(out)
}

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

/// Base64url without padding (RFC 4648 section 5): 43 characters for a SHA-256.
fn base64url(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for c in bytes.chunks(3) {
        let n = c.iter().fold(0u32, |n, b| n << 8 | u32::from(*b)) << (8 * (3 - c.len()));
        for i in 0..=c.len() {
            s.push(B64[(n >> (18 - 6 * i) & 63) as usize] as char);
        }
    }
    s
}

fn repo(name: &str, commit: Option<&str>) -> Value {
    json!({
        "name": name,
        "commit": commit,
        "raw_base": commit.map(|c| format!("{RAW}/pingw33n/{name}/{c}/")),
    })
}

/// A mod of the index.
struct ModEntry {
    /// The file name (the mods are in one folder).
    file: String,
    row: Vec<Value>,
    name: Option<String>,
}

/// The bytes of a file, or the error text of an entry (a file that cannot be read is listed,
/// and does not stop the run).
fn read(path: &Path) -> Result<Vec<u8>, String> {
    crate::fs::read(path).map_err(|e| format!("cannot read: {e}"))
}

/// `size` and `sha256` of a file's bytes, null when it was not read.
fn size_hash(bytes: Option<&[u8]>) -> [Value; 2] {
    [
        bytes.map(<[u8]>::len).into(),
        bytes.map(|b| base64url(&Sha256::digest(b))).into(),
    ]
}

fn mod_entry(e: &Entry) -> ModEntry {
    let bytes = read(&e.path);
    let [size, sha] = size_hash(bytes.as_deref().ok());
    let (mut name, mut recrypted, mut error) = (None, 0, None);
    match bytes.and_then(|b| rbm::Rbm::parse(b).map_err(|x| x.to_string())) {
        Ok(r) => {
            name = r.info().map(|i| i.name.clone());
            // as orb-ui's ModStatus: the embedded key bytes cover every slot (docs/rbm.md 12.4)
            recrypted = u8::from(rbm::KeyTable::embedded(&r).is_some_and(|t| r.slots_covered(&t)));
        }
        Err(x) => error = Some(x),
    }
    ModEntry {
        file: e.file.clone(),
        row: vec![
            e.file.clone().into(),
            size,
            sha,
            name.clone().into(),
            recrypted.into(),
            error.into(),
        ],
        name,
    }
}

fn song_entry(e: &Entry, dirs: &BTreeSet<&str>, mods: &[ModEntry], order: &[usize]) -> Value {
    let bytes = read(&e.path);
    let [size, sha] = size_hash(bytes.as_deref().ok());
    let facts = match bytes {
        Ok(_) => SongFacts::of_file(&e.path).map_err(|x| x.1),
        Err(x) => Err(x),
    };
    let dir = dirs
        .iter()
        .position(|d| *d == e.dir)
        .expect("a listed folder");
    let mut row: Vec<Value> = vec![dir.into(), e.file.clone().into(), size, sha];
    match facts {
        Ok(f) => {
            // the mods whose INFO name is the song's, in orb's order; the built-in kit has none
            let variants: Vec<usize> = if f.mod_name == STANDARD {
                Vec::new()
            } else {
                order
                    .iter()
                    .copied()
                    .filter(|&i| {
                        mods[i]
                            .name
                            .as_deref()
                            .is_some_and(|n| mods::names_match(n, &f.mod_name))
                    })
                    .collect()
            };
            row.extend([
                Value::Null,
                if f.rb1x { "rb1x" } else { "rbs" }.into(),
                f.version.clone().into(),
                f.window_title.clone().into(),
                f.mod_name.clone().into(),
                f.bpm.into(),
                f.last_event_bar().into(),
                variants.into(),
            ]);
        }
        Err(x) => {
            row.push(x.into());
            row.resize(SONG_COLUMNS.len(), Value::Null);
        }
    }
    Value::Array(row)
}

/// The index of the two collections, as compact JSON ending in a newline.
pub fn build(o: &Options) -> Result<String, Fail> {
    let mod_files = walk(o.mods, "rbm", false)?;
    let mods: Vec<ModEntry> = mod_files.iter().map(mod_entry).collect();
    // the lookup order of orb: the bytes of the file name, then the path (one folder here)
    let mut order: Vec<usize> = (0..mods.len()).collect();
    order.sort_by(|&a, &b| mods::byte_order(Path::new(&mods[a].file), Path::new(&mods[b].file)));

    let song_files = walk(o.songs, "rbs", true)?;
    let dirs: BTreeSet<&str> = song_files.iter().map(|e| e.dir.as_str()).collect();
    let songs: Vec<Value> = song_files
        .iter()
        .map(|e| song_entry(e, &dirs, &mods, &order))
        .collect();
    let index = json!({
        "format": "orb-web-index",
        "version": VERSION,
        "songs_repo": repo("rb-songs", o.songs_commit),
        "mods_repo": repo("rb-mods", o.mods_commit),
        "mods_dir": top_name(o.mods)?,
        "song_dirs": dirs,
        "song_columns": SONG_COLUMNS,
        "songs": songs,
        "mod_columns": MOD_COLUMNS,
        "mods": mods.into_iter().map(|m| Value::Array(m.row)).collect::<Vec<_>>(),
    });
    let mut out = serde_json::to_string(&index).expect("a JSON value serializes");
    out.push('\n');
    Ok(out)
}
