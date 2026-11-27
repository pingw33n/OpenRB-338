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

/// Where one collection comes from: its folder, or a part made from it earlier
/// ([`songs_part`], [`mods_part`]).
pub enum Source<'a> {
    Dir(&'a Path),
    Part(&'a Path),
}

/// What `orb web-index` is given.
pub struct Options<'a> {
    pub songs: Source<'a>,
    pub mods: Source<'a>,
    pub songs_commit: Option<&'a str>,
    pub mods_commit: Option<&'a str>,
}

/// What the index derives from the rb-songs collection alone (task-322): the song rows without
/// `mod_variants`, which needs the mods, and the folders they index.
pub struct SongsPart {
    dirs: Vec<String>,
    /// The rows of the first `SONG_COLUMNS.len() - 1` columns.
    rows: Vec<Vec<Value>>,
}

/// What the index derives from the rb-mods collection alone: the folder's name and the mod rows.
pub struct ModsPart {
    dir: String,
    rows: Vec<Vec<Value>>,
}

const SONGS_PART: &str = "orb-web-index-songs-part";
const MODS_PART: &str = "orb-web-index-mods-part";

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

fn mod_row(e: &Entry) -> Vec<Value> {
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
    vec![
        e.file.clone().into(),
        size,
        sha,
        name.into(),
        recrypted.into(),
        error.into(),
    ]
}

fn song_row(e: &Entry, dirs: &BTreeSet<&str>) -> Vec<Value> {
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
        Ok(f) => row.extend([
            Value::Null,
            if f.rb1x { "rb1x" } else { "rbs" }.into(),
            f.version.clone().into(),
            f.window_title.clone().into(),
            f.mod_name.clone().into(),
            f.bpm.into(),
            f.last_event_bar().into(),
        ]),
        Err(x) => {
            row.push(x.into());
            row.resize(SONG_COLUMNS.len() - 1, Value::Null);
        }
    }
    row
}

/// The part of the mods in the folder `dir`.
pub fn mods_part(dir: &Path) -> Result<ModsPart, Fail> {
    let files = walk(dir, "rbm", false)?;
    Ok(ModsPart {
        dir: top_name(dir)?,
        rows: files.iter().map(mod_row).collect(),
    })
}

/// The part of the songs in the folder `dir` and all the folders below it.
pub fn songs_part(dir: &Path) -> Result<SongsPart, Fail> {
    let files = walk(dir, "rbs", true)?;
    let dirs: BTreeSet<&str> = files.iter().map(|e| e.dir.as_str()).collect();
    Ok(SongsPart {
        rows: files.iter().map(|e| song_row(e, &dirs)).collect(),
        dirs: dirs.into_iter().map(str::to_string).collect(),
    })
}

fn part_json(format: &str, key: &str, name: Value, rows: &[Vec<Value>]) -> String {
    let mut out = serde_json::to_string(&json!({
        "format": format,
        "version": VERSION,
        key: name,
        "rows": rows,
    }))
    .expect("a JSON value serializes");
    out.push('\n');
    out
}

impl SongsPart {
    /// The part as compact JSON ending in a newline.
    pub fn to_json(&self) -> String {
        part_json(SONGS_PART, "song_dirs", json!(self.dirs), &self.rows)
    }
}

impl ModsPart {
    /// The part as compact JSON ending in a newline.
    pub fn to_json(&self) -> String {
        part_json(MODS_PART, "mods_dir", json!(self.dir), &self.rows)
    }
}

/// A part file: its `key` value and its rows (the version and the format are checked).
fn read_part(path: &Path, format: &str, key: &str) -> Result<(Value, Vec<Vec<Value>>), Fail> {
    let bad = |m: &str| fail(1, format!("{}: {m}", path.display()));
    let bytes = crate::fs::read(path).map_err(|e| bad(&format!("cannot read: {e}")))?;
    let mut v: Value = serde_json::from_slice(&bytes).map_err(|e| bad(&e.to_string()))?;
    if v["format"] != format || v["version"] != VERSION {
        return Err(bad(&format!("not a {format} part of version {VERSION}")));
    }
    let rows = serde_json::from_value(v["rows"].take()).map_err(|e| bad(&e.to_string()))?;
    Ok((v[key].take(), rows))
}

impl SongsPart {
    pub fn read(path: &Path) -> Result<Self, Fail> {
        let (dirs, rows) = read_part(path, SONGS_PART, "song_dirs")?;
        let dirs = serde_json::from_value(dirs)
            .map_err(|e| fail(1, format!("{}: {e}", path.display())))?;
        Ok(Self { dirs, rows })
    }
}

impl ModsPart {
    pub fn read(path: &Path) -> Result<Self, Fail> {
        let (dir, rows) = read_part(path, MODS_PART, "mods_dir")?;
        let dir =
            serde_json::from_value(dir).map_err(|e| fail(1, format!("{}: {e}", path.display())))?;
        Ok(Self { dir, rows })
    }
}

/// The index of the two parts, as compact JSON ending in a newline. `mod_variants` is computed
/// here, from both parts, so a part made long ago resolves against the mods of today.
pub fn combine(
    songs: SongsPart,
    mods: ModsPart,
    songs_commit: Option<&str>,
    mods_commit: Option<&str>,
) -> String {
    let names: Vec<Option<&str>> = mods.rows.iter().map(|r| r[3].as_str()).collect();
    let file = |i: usize| mods.rows[i][0].as_str().unwrap_or("");
    // the lookup order of orb: the bytes of the file name, then the path (one folder here)
    let mut order: Vec<usize> = (0..mods.rows.len()).collect();
    order.sort_by(|&a, &b| mods::byte_order(Path::new(file(a)), Path::new(file(b))));
    let song_rows: Vec<Value> = songs
        .rows
        .into_iter()
        .map(|mut row| {
            let variants = if row[4].is_null() {
                let mod_name = row[8].as_str().unwrap_or("");
                // the mods whose INFO name is the song's, in orb's order; the built-in kit has
                // none
                let v: Vec<usize> = if mod_name == STANDARD {
                    Vec::new()
                } else {
                    order
                        .iter()
                        .copied()
                        .filter(|&i| names[i].is_some_and(|n| mods::names_match(n, mod_name)))
                        .collect()
                };
                v.into()
            } else {
                Value::Null
            };
            row.push(variants);
            Value::Array(row)
        })
        .collect();
    let index = json!({
        "format": "orb-web-index",
        "version": VERSION,
        "songs_repo": repo("rb-songs", songs_commit),
        "mods_repo": repo("rb-mods", mods_commit),
        "mods_dir": mods.dir,
        "song_dirs": songs.dirs,
        "song_columns": SONG_COLUMNS,
        "songs": song_rows,
        "mod_columns": MOD_COLUMNS,
        "mods": mods.rows,
    });
    let mut out = serde_json::to_string(&index).expect("a JSON value serializes");
    out.push('\n');
    out
}

/// The index of the two collections, each from its folder or from a part.
pub fn build(o: &Options) -> Result<String, Fail> {
    let mods = match o.mods {
        Source::Dir(d) => mods_part(d)?,
        Source::Part(p) => ModsPart::read(p)?,
    };
    let songs = match o.songs {
        Source::Dir(d) => songs_part(d)?,
        Source::Part(p) => SongsPart::read(p)?,
    };
    Ok(combine(songs, mods, o.songs_commit, o.mods_commit))
}
