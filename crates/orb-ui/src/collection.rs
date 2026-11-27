//! The rb-songs and rb-mods collections of the web page (task-174, `docs/web.md` section 9):
//! the index `orb web-index` writes (`docs/web.md` section 8, format 1), read into the files the
//! page lists, and the check of a downloaded file against it.
//!
//! The page enters every file of the index into its memory file system as a lazy file
//! (`orb_cli::fs::insert_lazy`, [`File::mem_path`]: `/rb-songs/songs/...` and
//! `/rb-mods/mods/...`), so the in-app browser lists the songs' folders and the Mods menu, the
//! mod lookup by INFO name and the remembered variant preference work on the collection as on a
//! mods folder. A file's bytes are downloaded from its commit-pinned raw URL ([`File::url`])
//! when it is used, and kept only if their SHA-256 is the index's ([`File::check`]). The module
//! builds on every target (its tests run natively); only the web page uses it.

use serde_json::Value;
use sha2::{Digest, Sha256};

/// The memory folder of the song collection's root (the repository `rb-songs`).
pub const SONGS_ROOT: &str = "/rb-songs";
/// The memory folder of the mod collection's root (the repository `rb-mods`).
pub const MODS_ROOT: &str = "/rb-mods";
/// The memory folder of the mod collection's mods: the Mods folder of a web page load
/// (`docs/web.md` section 9).
pub const MODS_FOLDER: &str = "/rb-mods/mods";
/// The memory folder of the song collection's songs: where File > Open from Collection... starts.
pub const SONGS_FOLDER: &str = "/rb-songs/songs";

/// Whether the Mods folder `dir` is the mod collection (Mods > Use Collection is checked).
pub fn is_mods_folder(dir: Option<&std::path::Path>) -> bool {
    dir == Some(std::path::Path::new(MODS_FOLDER))
}

/// A file of the index: where it is, how large, its SHA-256 and where to download it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct File {
    /// The repository's name (`rb-songs`, `rb-mods`).
    pub repo: String,
    /// The path in the repository (`songs/Folder/Song.rbs`, `mods/Mod.rbm`).
    pub path: String,
    /// The size in bytes; none for a file the index could not read.
    pub size: Option<u64>,
    /// The SHA-256; none for a file the index could not read.
    pub sha256: Option<[u8; 32]>,
    /// The raw URL base of the repository at the index's commit; none without a commit.
    pub raw_base: Option<String>,
}

impl File {
    /// The file's path in the page's memory file system: `/<repo>/<path>`.
    pub fn mem_path(&self) -> std::path::PathBuf {
        std::path::PathBuf::from(format!("/{}/{}", self.repo, self.path))
    }

    /// The file's name (the last part of its path).
    pub fn name(&self) -> &str {
        self.path.rsplit('/').next().unwrap_or(&self.path)
    }

    /// The URL of the file's bytes: the commit-pinned raw base and the path, percent-encoded
    /// (`docs/web.md` section 8); none when the index has no commit for the repository.
    pub fn url(&self) -> Option<String> {
        Some(format!(
            "{}{}",
            self.raw_base.as_ref()?,
            encode_path(&self.path)
        ))
    }

    /// Whether `bytes` are this file: its size and its SHA-256 as the index has them.
    pub fn check(&self, bytes: &[u8]) -> Result<(), String> {
        let want = self
            .sha256
            .ok_or_else(|| format!("{}: the collection index has no hash of it", self.name()))?;
        if self.size.is_some_and(|n| n != bytes.len() as u64) {
            return Err(format!(
                "{}: the download has {} bytes, the collection index says {}; it was not used. \
                 The collection may have changed since this page was built: reload the page.",
                self.name(),
                bytes.len(),
                self.size.unwrap_or(0)
            ));
        }
        if Sha256::digest(bytes).as_slice() != want {
            return Err(format!(
                "{}: the download is not the file of the collection index (its SHA-256 differs); \
                 it was not used. Reload the page.",
                self.name()
            ));
        }
        Ok(())
    }

    /// The SHA-256 as the index writes it (base64url without padding): the key of the page's
    /// download cache.
    pub fn key(&self) -> Option<String> {
        self.sha256.map(|h| base64url(&h))
    }
}

/// A song of the index.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Song {
    pub file: File,
    /// Why orb cannot read it (then the facts are none).
    pub error: Option<String>,
    /// The INFO name of its mod.
    pub mod_name: Option<String>,
    /// The indexes in [`Index::mods`] of the mods of that name, in orb's lookup order.
    pub mod_variants: Vec<usize>,
}

/// A mod of the index.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mod {
    pub file: File,
    /// Its INFO menu name; none for a file that is no mod with an INFO.
    pub name: Option<String>,
    /// Whether its embedded key covers every sample slot (`docs/rbm.md` 12.4).
    pub recrypted: bool,
    pub error: Option<String>,
}

/// The index of the two collections (`index.json`, format 1).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Index {
    pub songs: Vec<Song>,
    pub mods: Vec<Mod>,
    /// The mods' folder in `rb-mods` (`mods`).
    pub mods_dir: String,
    /// The songs' top folder in `rb-songs` (`songs`): the shortest of `song_dirs`.
    pub songs_dir: String,
}

impl Index {
    /// Reads the text of `index.json`. A format or version it does not know is an error.
    pub fn parse(text: &str) -> Result<Self, String> {
        let v: Value = serde_json::from_str(text).map_err(|e| format!("not JSON: {e}"))?;
        if v["format"] != "orb-web-index" {
            return Err("not an orb web index".into());
        }
        if v["version"] != 1 {
            return Err(format!("index version {} is not 1", v["version"]));
        }
        let repo = |k: &str| -> Result<(String, Option<String>), String> {
            let r = &v[k];
            let name = r["name"]
                .as_str()
                .ok_or_else(|| format!("{k}: no name"))?
                .to_string();
            if name.is_empty() || name.contains(['/', '\\']) || name.starts_with('.') {
                return Err(format!("{k}: a bad name {name:?}"));
            }
            Ok((name, r["raw_base"].as_str().map(str::to_string)))
        };
        let (songs_repo, songs_raw) = repo("songs_repo")?;
        let (mods_repo, mods_raw) = repo("mods_repo")?;
        let mods_dir = v["mods_dir"].as_str().ok_or("no mods_dir")?.to_string();
        let song_dirs: Vec<String> = strings(&v["song_dirs"]).ok_or("no song_dirs")?;
        let songs_dir = song_dirs
            .iter()
            .min_by_key(|d| d.len())
            .cloned()
            .unwrap_or_default();

        let mods_table = Table::new(&v, "mod_columns", "mods")?;
        let mut mods = Vec::new();
        for row in mods_table.rows {
            let get = |c: &str| mods_table.cell(row, c);
            let file = file_of(
                &mods_repo,
                &mods_raw,
                format!(
                    "{mods_dir}/{}",
                    text_of(get("file")).ok_or("a mod with no file")?
                ),
                get,
            )?;
            mods.push(Mod {
                file,
                name: text_of(get("name")),
                recrypted: get("recrypted").as_u64() == Some(1),
                error: text_of(get("error")),
            });
        }

        let songs_table = Table::new(&v, "song_columns", "songs")?;
        let mut songs = Vec::new();
        for row in songs_table.rows {
            let get = |c: &str| songs_table.cell(row, c);
            let dir = get("dir")
                .as_u64()
                .and_then(|d| song_dirs.get(d as usize))
                .ok_or("a song with no folder")?;
            let file = file_of(
                &songs_repo,
                &songs_raw,
                format!(
                    "{dir}/{}",
                    text_of(get("file")).ok_or("a song with no file")?
                ),
                get,
            )?;
            let mod_variants = get("mod_variants")
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|i| i.as_u64().map(|i| i as usize))
                        .filter(|&i| i < mods.len())
                        .collect()
                })
                .unwrap_or_default();
            songs.push(Song {
                file,
                error: text_of(get("error")),
                mod_name: text_of(get("mod_name")),
                mod_variants,
            });
        }
        Ok(Index {
            songs,
            mods,
            mods_dir,
            songs_dir,
        })
    }

    /// The memory folder of the song collection (`/rb-songs/songs`).
    pub fn songs_folder(&self) -> std::path::PathBuf {
        let repo = self
            .songs
            .first()
            .map_or("rb-songs", |s| s.file.repo.as_str());
        std::path::PathBuf::from(format!("/{repo}/{}", self.songs_dir))
    }

    /// The memory folder of the mod collection (`/rb-mods/mods`).
    pub fn mods_folder(&self) -> std::path::PathBuf {
        let repo = self
            .mods
            .first()
            .map_or("rb-mods", |m| m.file.repo.as_str());
        std::path::PathBuf::from(format!("/{repo}/{}", self.mods_dir))
    }
}

/// A table of the index: its columns and its rows.
struct Table<'a> {
    columns: Vec<String>,
    rows: &'a [Value],
}

impl<'a> Table<'a> {
    fn new(v: &'a Value, columns: &str, rows: &str) -> Result<Self, String> {
        Ok(Table {
            columns: strings(&v[columns]).ok_or_else(|| format!("no {columns}"))?,
            rows: v[rows]
                .as_array()
                .map(Vec::as_slice)
                .ok_or_else(|| format!("no {rows}"))?,
        })
    }

    /// The cell of column `c` in `row` (null when the column or the cell is missing).
    fn cell(&self, row: &'a Value, c: &str) -> &'a Value {
        static NULL: Value = Value::Null;
        self.columns
            .iter()
            .position(|x| x == c)
            .and_then(|i| row.get(i))
            .unwrap_or(&NULL)
    }
}

fn strings(v: &Value) -> Option<Vec<String>> {
    v.as_array()?
        .iter()
        .map(|s| s.as_str().map(str::to_string))
        .collect()
}

fn text_of(v: &Value) -> Option<String> {
    v.as_str().map(str::to_string)
}

fn file_of<'a>(
    repo: &str,
    raw_base: &Option<String>,
    path: String,
    get: impl Fn(&str) -> &'a Value,
) -> Result<File, String> {
    // a path that would leave the repository's folder in memory is refused
    if path
        .split('/')
        .any(|p| p.is_empty() || p == "." || p == "..")
    {
        return Err(format!("a bad path {path:?}"));
    }
    let sha256 = match get("sha256").as_str() {
        Some(s) => Some(
            unbase64url(s)
                .and_then(|b| <[u8; 32]>::try_from(b).ok())
                .ok_or_else(|| format!("{path}: a bad sha256 {s:?}"))?,
        ),
        None => None,
    };
    Ok(File {
        repo: repo.to_string(),
        path,
        size: get("size").as_u64(),
        sha256,
        raw_base: raw_base.clone(),
    })
}

/// `path` with every byte but the unreserved characters and `/` percent-encoded.
pub fn encode_path(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    for &b in path.as_bytes() {
        if b.is_ascii_alphanumeric() || b"-._~/".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

// ---- opening from the index: the random song and the links -----------------------------------

impl Song {
    /// Whether the song can be opened, by what the index says (no download): orb read it, the
    /// index has its hash, and its mod resolves. With `local` (a mods folder of the computer is
    /// in use) a mod resolves when `local` finds it by INFO name; else it does when it is the
    /// built-in kit (`mod_name` is the standard mod's, `docs/web.md` section 8.1) or the
    /// collection has a mod of that name (`mod_variants` is not empty).
    pub fn can_open(&self, local: &mut Option<&mut dyn FnMut(&str) -> bool>) -> bool {
        if self.error.is_some() || self.file.sha256.is_none() || self.file.size.is_none() {
            return false;
        }
        match (self.mod_name.as_deref(), local.as_mut()) {
            (Some(rbm::standard::NAME), _) => true,
            (Some(name), Some(f)) => f(name),
            (Some(_), None) => !self.mod_variants.is_empty(),
            (None, _) => false,
        }
    }
}

impl Index {
    /// The song of the index at `path` (the path in the repository: `songs/Folder/Song.rbs`).
    pub fn song(&self, path: &str) -> Option<&Song> {
        self.songs.iter().find(|s| s.file.path == path)
    }

    /// The mod `song` resolves to by itself (the first of its `mod_variants`: what `orb render`
    /// loads, and what a link to the song opens); none for the built-in kit or a mod the
    /// collection lacks.
    pub fn default_mod(&self, song: &Song) -> Option<&Mod> {
        self.mods.get(*song.mod_variants.first()?)
    }

    /// The mod of the index at `path` (`mods/Mod.rbm`).
    pub fn mod_at(&self, path: &str) -> Option<&Mod> {
        self.mods.iter().find(|m| m.file.path == path)
    }

    /// File > Open from Collection (Random): one song chosen uniformly among those that
    /// [`Song::can_open`], not `current` (a song's path in the repository, the one open) unless
    /// it is the only one. `r` is a random number in `[0, 1)`; none when no song can open.
    pub fn random_song(
        &self,
        mut local: Option<&mut dyn FnMut(&str) -> bool>,
        current: Option<&str>,
        r: f64,
    ) -> Option<&Song> {
        let mut all: Vec<&Song> = self
            .songs
            .iter()
            .filter(|s| s.can_open(&mut local))
            .collect();
        if all.len() > 1 {
            all.retain(|s| Some(s.file.path.as_str()) != current);
        }
        let i = ((r * all.len() as f64) as usize).min(all.len().saturating_sub(1));
        all.get(i).copied()
    }
}

/// What the page's address names (`?song=` and `?mod=`, `docs/web.md` section 9.4), each by its
/// path in the index (stable across commits of the collections):
/// - `song` only: that song of the song collection, with the mod it resolves to;
/// - `song` and `mod`: the song with that mod of the mod collection instead;
/// - `mod` only: the demo song of that mod (Mods > Demo Song).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Link {
    /// The song's path in the repository: `songs/Folder/Song.rbs`.
    pub song: Option<String>,
    /// The mod's path in the repository: `mods/Mod.rbm`.
    pub mod_: Option<String>,
}

impl Link {
    /// The link a query string (`?song=...&mod=...`, with or without the `?`) holds: its first
    /// `song` and its first `mod` parameter, percent-decoded. A path may be empty or unknown:
    /// the lookup in the index says so. None when it has neither.
    pub fn parse(query: &str) -> Option<Link> {
        let mut link = Link::default();
        for pair in query.trim_start_matches('?').split('&') {
            let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
            match k {
                "song" if link.song.is_none() => link.song = Some(decode(v)),
                "mod" if link.mod_.is_none() => link.mod_ = Some(decode(v)),
                _ => {}
            }
        }
        (link.song.is_some() || link.mod_.is_some()).then_some(link)
    }

    /// The song's path in the repository of a file of the memory file system (`/rb-songs/...`);
    /// None for any other path.
    pub fn song_of_mem_path(path: &std::path::Path) -> Option<String> {
        let p = path.to_str()?;
        Some(p.strip_prefix(SONGS_ROOT)?.strip_prefix('/')?.into())
    }

    /// The mod's path in the repository of a file of the memory file system (`/rb-mods/...`);
    /// None for any other path (a mod of the computer has no link).
    pub fn mod_of_mem_path(path: &std::path::Path) -> Option<String> {
        let p = path.to_str()?;
        Some(p.strip_prefix(MODS_ROOT)?.strip_prefix('/')?.into())
    }

    /// The parameters, with the paths percent-encoded and `/` kept.
    fn params(&self) -> Vec<String> {
        let mut v = Vec::new();
        if let Some(s) = &self.song {
            v.push(format!("song={}", encode_path(s)));
        }
        if let Some(m) = &self.mod_ {
            v.push(format!("mod={}", encode_path(m)));
        }
        v
    }

    /// `query` with its link parameters replaced by `link` (or removed, for `None`); the other
    /// parameters stay. Gives the new query with its `?`, or "" when it is empty.
    pub fn with_link(query: &str, link: Option<&Link>) -> String {
        replace_params(
            query,
            &["song", "mod"],
            link.map(Link::params).unwrap_or_default(),
        )
    }
}

/// `query` with its `skin` parameter set to `skin` (`modern` or `classic`, task-326; the
/// address always names the skin shown); the other parameters stay.
pub fn with_skin(query: &str, skin: &str) -> String {
    replace_params(query, &["skin"], vec![format!("skin={skin}")])
}

/// `query` without the parameters named `keys`, then with `add` at the end.
fn replace_params(query: &str, keys: &[&str], add: Vec<String>) -> String {
    let mut parts: Vec<String> = query
        .trim_start_matches('?')
        .split('&')
        .filter(|p| !p.is_empty())
        .filter(|p| !keys.contains(&p.split('=').next().unwrap_or("")))
        .map(String::from)
        .collect();
    parts.extend(add);
    if parts.is_empty() {
        String::new()
    } else {
        format!("?{}", parts.join("&"))
    }
}

/// Percent-decodes a query value (`+` is a space); bytes that are not UTF-8 become U+FFFD.
fn decode(s: &str) -> String {
    let b = s.as_bytes();
    let hex = |c: u8| (c as char).to_digit(16);
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'%' if i + 2 < b.len() && hex(b[i + 1]).is_some() && hex(b[i + 2]).is_some() => {
                out.push((hex(b[i + 1]).unwrap() * 16 + hex(b[i + 2]).unwrap()) as u8);
                i += 3;
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

/// Base64url without padding, as the index writes a hash.
pub fn base64url(bytes: &[u8]) -> String {
    let mut out = String::new();
    for c in bytes.chunks(3) {
        let n = c.iter().fold(0u32, |n, &b| n << 8 | b as u32) << (8 * (3 - c.len()));
        for i in 0..=c.len() {
            out.push(B64[(n >> (18 - 6 * i) & 63) as usize] as char);
        }
    }
    out
}

/// The bytes of base64url text without padding; none for a character outside the alphabet or
/// a length no encoding has.
pub fn unbase64url(s: &str) -> Option<Vec<u8>> {
    if s.len() % 4 == 1 {
        return None;
    }
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let (mut n, mut bits) = (0u32, 0);
    for c in s.bytes() {
        let v = B64.iter().position(|&x| x == c)? as u32;
        n = n << 6 | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((n >> bits) as u8);
            n &= (1 << bits) - 1;
        }
    }
    // the bits left over must be zero (one encoding per value)
    (n == 0).then_some(out)
}

/// The most the page's download cache holds: 256 MB (256 x 2^20 bytes; the user's addition of
/// 2026-11-27). Past it the least recently used files are evicted ([`CacheLru::admit`]).
pub const CACHE_CAP: u64 = 256 << 20;

/// The record of the page's download cache (the Cache API keeps no sizes or use times): each
/// file's key (its SHA-256), size and last use, the use a tick of a counter that only grows.
/// The page keeps it as JSON in `localStorage` ([`CacheLru::parse`], [`CacheLru::to_text`]).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CacheLru {
    clock: u64,
    /// `(key, size, last use)`.
    entries: Vec<(String, u64, u64)>,
}

impl CacheLru {
    /// The record from its text; empty for no text or text it cannot read.
    pub fn parse(text: &str) -> Self {
        let Ok(v) = serde_json::from_str::<Value>(text) else {
            return Self::default();
        };
        let entries = v["entries"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|e| {
                        Some((
                            e.get(0)?.as_str()?.to_string(),
                            e.get(1)?.as_u64()?,
                            e.get(2)?.as_u64()?,
                        ))
                    })
                    .collect()
            })
            .unwrap_or_default();
        CacheLru {
            clock: v["clock"].as_u64().unwrap_or(0),
            entries,
        }
    }

    pub fn to_text(&self) -> String {
        serde_json::json!({"clock": self.clock, "entries": self.entries}).to_string()
    }

    /// The bytes the cache holds by the record.
    pub fn total(&self) -> u64 {
        self.entries.iter().map(|e| e.1).sum()
    }

    /// Whether the record has `key`.
    pub fn has(&self, key: &str) -> bool {
        self.entries.iter().any(|e| e.0 == key)
    }

    /// A use of the file `key` of `size` bytes: a cache hit, or a file about to be stored. It
    /// becomes the most recently used, and the least recently used others are evicted until
    /// the total fits in `cap`: the keys to delete from the cache. None for a file larger than
    /// `cap`, which is not stored (and leaves the record).
    pub fn admit(&mut self, key: &str, size: u64, cap: u64) -> Option<Vec<String>> {
        self.forget(key);
        if size > cap {
            return None;
        }
        let mut evicted = Vec::new();
        while self.total() + size > cap {
            let (i, _) = self.entries.iter().enumerate().min_by_key(|(_, e)| e.2)?;
            evicted.push(self.entries.remove(i).0);
        }
        self.clock += 1;
        self.entries.push((key.to_string(), size, self.clock));
        Some(evicted)
    }

    /// Takes `key` out of the record (a failed cache write, a file deleted).
    pub fn forget(&mut self, key: &str) {
        self.entries.retain(|e| e.0 != key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn use_collection_is_checked_for_the_collection_only() {
        use std::path::Path;
        assert!(is_mods_folder(Some(Path::new("/rb-mods/mods"))));
        // a picked folder, none (natively unset)
        assert!(!is_mods_folder(Some(Path::new("/Mods/Picked"))));
        assert!(!is_mods_folder(None));
        // the folder the index gives is the same
        let i = Index::parse(&index()).unwrap();
        assert!(is_mods_folder(Some(&i.mods_folder())));
        assert_eq!(i.songs_folder(), Path::new(SONGS_FOLDER));
    }

    #[test]
    fn cache_evicts_the_least_recently_used() {
        let mut c = CacheLru::default();
        assert_eq!(c.admit("a", 40, 100), Some(vec![]));
        assert_eq!(c.admit("b", 40, 100), Some(vec![]));
        // a hit counts as a use: "a" is now newer than "b"
        assert_eq!(c.admit("a", 40, 100), Some(vec![]));
        assert_eq!(c.total(), 80);
        // past the cap: evict until it fits, the least recently used first
        assert_eq!(c.admit("c", 50, 100), Some(vec!["b".to_string()]));
        assert_eq!(c.admit("d", 90, 100), Some(vec!["a".into(), "c".into()]));
        assert_eq!(c.total(), 90);
        // a file larger than the cap is not stored
        assert_eq!(c.admit("e", 101, 100), None);
        assert!(!c.has("e") && c.has("d"));
        // the record survives its text
        let back = CacheLru::parse(&c.to_text());
        assert_eq!(back, c);
        assert_eq!(CacheLru::parse("garbage"), CacheLru::default());
        c.forget("d");
        assert_eq!(c.total(), 0);
        assert_eq!(CACHE_CAP, 268_435_456);
    }

    fn index() -> String {
        concat!(
            r#"{"format":"orb-web-index","version":1,"#,
            r#""songs_repo":{"name":"rb-songs","commit":"c1","#,
            r#""raw_base":"https://raw.githubusercontent.com/pingw33n/rb-songs/c1/"},"#,
            r#""mods_repo":{"name":"rb-mods","commit":null,"raw_base":null},"#,
            r#""mods_dir":"mods","song_dirs":["songs","songs/A b"],"#,
            r#""song_columns":["dir","file","size","sha256","error","format","version","title","#,
            r#""mod_name","tempo_bpm","last_event_bar","mod_variants"],"#,
            r#""songs":[[1,"x&y.rbs",3,"ungWv48Bz-pBQUDeXa4iI7ADYaOWF3qctBD_YfIAFa0",null,"rbs","#,
            r#""4.2","T","M",120,4,[1,0]],"#,
            r#"[0,"bad.rbs",null,null,"cannot read",null,null,null,null,null,null,null]],"#,
            r#""mod_columns":["file","size","sha256","name","recrypted","error"],"#,
            r#""mods":[["a.rbm",5,"ungWv48Bz-pBQUDeXa4iI7ADYaOWF3qctBD_YfIAFa0","M",1,null],"#,
            r#"["b.rbm",5,"ungWv48Bz-pBQUDeXa4iI7ADYaOWF3qctBD_YfIAFa0",null,0,"no INFO"]]}"#
        )
        .into()
    }

    #[test]
    fn parses_format_1() {
        let i = Index::parse(&index()).unwrap();
        assert_eq!(i.songs.len(), 2);
        assert_eq!(i.mods.len(), 2);
        let s = &i.songs[0];
        assert_eq!(s.file.path, "songs/A b/x&y.rbs");
        assert_eq!(
            s.file.mem_path(),
            std::path::Path::new("/rb-songs/songs/A b/x&y.rbs")
        );
        assert_eq!(
            s.file.url().unwrap(),
            "https://raw.githubusercontent.com/pingw33n/rb-songs/c1/songs/A%20b/x%26y.rbs"
        );
        assert_eq!(s.mod_name.as_deref(), Some("M"));
        assert_eq!(s.mod_variants, vec![1, 0]);
        assert_eq!(s.file.name(), "x&y.rbs");
        assert!(s.file.check(b"abc").is_ok());
        assert!(s.file.check(b"abd").unwrap_err().contains("SHA-256"));
        assert!(s.file.check(b"abcd").unwrap_err().contains("4 bytes"));
        let bad = &i.songs[1];
        assert_eq!(bad.error.as_deref(), Some("cannot read"));
        assert_eq!(bad.file.sha256, None);
        assert!(bad.file.check(b"").is_err());
        assert_eq!(
            i.mods[0].file.mem_path(),
            std::path::Path::new("/rb-mods/mods/a.rbm")
        );
        assert_eq!(i.mods[0].name.as_deref(), Some("M"));
        assert!(i.mods[0].recrypted);
        assert!(!i.mods[1].recrypted);
        // no commit, no URL
        assert_eq!(i.mods[0].file.url(), None);
        assert_eq!(i.songs_folder(), std::path::Path::new("/rb-songs/songs"));
        assert_eq!(i.mods_folder(), std::path::Path::new("/rb-mods/mods"));
    }

    #[test]
    fn refuses_other_formats_and_paths() {
        assert!(Index::parse("{}").is_err());
        assert!(Index::parse("[").is_err());
        let v2 = index().replace(r#""version":1"#, r#""version":2"#);
        assert!(Index::parse(&v2).unwrap_err().contains("version"));
        let up = index().replace(r#""bad.rbs""#, r#""..""#);
        assert!(Index::parse(&up).unwrap_err().contains("bad path"));
        let hash = index().replace("AFa0", "AFa1");
        assert!(Index::parse(&hash).unwrap_err().contains("sha256"));
    }

    #[test]
    fn base64url_round_trips() {
        for n in 0..40 {
            let b: Vec<u8> = (0..n).map(|i| (i * 37 + 11) as u8).collect();
            assert_eq!(unbase64url(&base64url(&b)).unwrap(), b, "{n}");
        }
        assert_eq!(base64url(&Sha256::digest(b"abc")).len(), 43);
        assert_eq!(unbase64url("a"), None);
        assert_eq!(unbase64url("a+"), None);
    }

    /// The index `orb web-index` writes is read back: the paths, sizes and hashes of its files.
    #[test]
    fn reads_what_orb_web_index_writes() {
        let tmp = tempfile::tempdir().unwrap();
        let songs = tmp.path().join("songs");
        let mods = tmp.path().join("mods");
        std::fs::create_dir_all(songs.join("Sub dir")).unwrap();
        std::fs::create_dir_all(&mods).unwrap();
        let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let song = std::fs::read(repo.join("tests/songs/shipped/Clodhopper.rbs")).unwrap();
        std::fs::write(songs.join("Sub dir/Clod #1.rbs"), &song).unwrap();
        let std_mod = std::fs::read(repo.join("assets/Standard.rbm")).unwrap();
        std::fs::write(mods.join("Std.rbm"), &std_mod).unwrap();
        let text = orb_cli::webindex::build(&orb_cli::webindex::Options {
            songs: &songs,
            mods: &mods,
            songs_commit: Some("abc"),
            mods_commit: Some("def"),
        })
        .unwrap();
        let i = Index::parse(&text).unwrap();
        assert_eq!(i.songs.len(), 1);
        let s = &i.songs[0].file;
        assert_eq!(
            s.mem_path(),
            std::path::Path::new("/rb-songs/songs/Sub dir/Clod #1.rbs")
        );
        assert_eq!(
            s.url().unwrap(),
            concat!(
                "https://raw.githubusercontent.com/pingw33n/rb-songs/abc/",
                "songs/Sub%20dir/Clod%20%231.rbs"
            )
        );
        s.check(&song).unwrap();
        let m = &i.mods[0];
        assert_eq!(m.name.as_deref(), Some("Standard ReBirth"));
        assert_eq!(m.file.size, Some(std_mod.len() as u64));
        m.file.check(&std_mod).unwrap();
        assert!(m.file.url().unwrap().ends_with("/rb-mods/def/mods/Std.rbm"));
    }

    fn song(path: &str, mod_name: Option<&str>, variants: &[usize], error: bool) -> Song {
        Song {
            file: File {
                repo: "rb-songs".into(),
                path: path.into(),
                size: (!error).then_some(1),
                sha256: (!error).then_some([0; 32]),
                raw_base: None,
            },
            error: error.then(|| "cannot read".to_string()),
            mod_name: mod_name.map(String::from),
            mod_variants: variants.to_vec(),
        }
    }

    fn many() -> Index {
        Index {
            songs: vec![
                song("songs/a.rbs", Some("Standard ReBirth"), &[], false),
                song("songs/b.rbs", Some("M"), &[0], false),
                song("songs/c.rbs", Some("Missing"), &[], false),
                song("songs/d.rbs", Some("M"), &[0], true),
                song("songs/e.rbs", Some("M"), &[0], false),
            ],
            ..Index::default()
        }
    }

    #[test]
    fn random_picks_only_songs_that_can_open_and_not_the_open_one() {
        let i = many();
        // a, b, e can open (c: its mod is not in the collection; d: a read error)
        let at = |r: f64, cur: Option<&str>| {
            i.random_song(None, cur, r)
                .map(|s| s.file.path.clone())
                .unwrap()
        };
        assert_eq!(at(0.0, None), "songs/a.rbs");
        assert_eq!(at(0.5, None), "songs/b.rbs");
        assert_eq!(at(0.999, None), "songs/e.rbs");
        // uniform over the candidates: every one of them at its share of [0, 1)
        let mut n = std::collections::HashMap::new();
        for k in 0..300 {
            *n.entry(at(k as f64 / 300.0, None)).or_insert(0) += 1;
        }
        assert_eq!(n.len(), 3);
        assert!(n.values().all(|&c| c == 100));
        // not the open song
        for k in 0..30 {
            assert_ne!(at(k as f64 / 30.0, Some("songs/b.rbs")), "songs/b.rbs");
        }
        // a mods folder of the computer decides by its lookup: only "Missing" is there
        let mut local = |name: &str| name == "Missing";
        let s = i.random_song(Some(&mut local), None, 0.7).unwrap();
        assert_eq!(s.file.path, "songs/c.rbs");
        // the open song is the only one that can open: it is the answer
        let one = Index {
            songs: vec![i.songs[0].clone(), i.songs[3].clone()],
            ..Index::default()
        };
        let s = one.random_song(None, Some("songs/a.rbs"), 0.3).unwrap();
        assert_eq!(s.file.path, "songs/a.rbs");
        // none can open
        let none = Index {
            songs: vec![i.songs[3].clone()],
            ..Index::default()
        };
        assert!(none.random_song(None, None, 0.3).is_none());
    }

    #[test]
    fn links_parse_and_write() {
        let song = |p: &str| Link {
            song: Some(p.into()),
            mod_: None,
        };
        assert_eq!(
            Link::parse("?song=songs%2FA%20b%2Fx%26y.rbs"),
            Some(song("songs/A b/x&y.rbs"))
        );
        assert_eq!(
            Link::parse("song=songs/a+b.rbs&x=1"),
            Some(song("songs/a b.rbs"))
        );
        // a song and a mod, a mod alone (its demo); the old ?mod-demo= is not a link
        assert_eq!(
            Link::parse("?debug-panic&mod=mods%2FM.rbm&song=x"),
            Some(Link {
                song: Some("x".into()),
                mod_: Some("mods/M.rbm".into())
            })
        );
        assert_eq!(
            Link::parse("?mod=mods%2FM.rbm"),
            Some(Link {
                song: None,
                mod_: Some("mods/M.rbm".into())
            })
        );
        assert_eq!(Link::parse("?mod-demo=mods%2FM.rbm"), None);
        assert_eq!(Link::parse("?song="), Some(song("")));
        assert_eq!(Link::parse("?debug-panic"), None);
        assert_eq!(Link::parse(""), None);
        // a bad escape stays as it is; bytes that are not UTF-8 do not fail
        assert_eq!(Link::parse("?song=100%"), Some(song("100%")));
        assert_eq!(Link::parse("?song=%ZZ"), Some(song("%ZZ")));
        assert_eq!(Link::parse("?song=%FF"), Some(song("\u{fffd}")));
        // written: encoded, '/' kept, other parameters kept, the old link replaced
        let l = Link {
            song: Some("songs/A b/x&y+z.rbs".into()),
            mod_: Some("mods/M 1.rbm".into()),
        };
        let q = Link::with_link("?debug-panic", Some(&l));
        assert_eq!(
            q,
            "?debug-panic&song=songs/A%20b/x%26y%2Bz.rbs&mod=mods/M%201.rbm"
        );
        assert_eq!(Link::parse(&q), Some(l.clone()));
        assert_eq!(Link::with_link(&q, None), "?debug-panic");
        assert_eq!(Link::with_link("?song=a&mod=b", None), "");
        let m = Link {
            song: None,
            mod_: Some("mods/M.rbm".into()),
        };
        assert_eq!(
            Link::with_link("?song=a&mod=b", Some(&m)),
            "?mod=mods/M.rbm"
        );
        // the skin parameter: set, replaced, the others kept
        assert_eq!(with_skin("", "classic"), "?skin=classic");
        assert_eq!(
            with_skin("?skin=bad&song=a&x", "modern"),
            "?song=a&x&skin=modern"
        );
        // the memory paths
        let p = std::path::Path::new("/rb-songs/songs/A b/x.rbs");
        assert_eq!(
            Link::song_of_mem_path(p).as_deref(),
            Some("songs/A b/x.rbs")
        );
        assert_eq!(Link::mod_of_mem_path(p), None);
        let m = std::path::Path::new("/rb-mods/mods/M.rbm");
        assert_eq!(Link::mod_of_mem_path(m).as_deref(), Some("mods/M.rbm"));
        let local = std::path::Path::new("/Songs/x.rbs");
        assert_eq!(Link::song_of_mem_path(local), None);
        assert_eq!(
            Link::mod_of_mem_path(std::path::Path::new("/Mods/a/M.rbm")),
            None
        );
    }

    #[test]
    fn the_index_finds_a_song_and_a_mod_by_path() {
        let i = Index::parse(&index()).unwrap();
        assert!(i.song("songs/A b/x&y.rbs").is_some());
        assert!(i.song("songs/nope.rbs").is_none());
        assert!(i.mod_at("mods/a.rbm").is_some());
        // the first variant is the mod the song resolves to by itself
        let d = i.default_mod(i.song("songs/A b/x&y.rbs").unwrap()).unwrap();
        assert_eq!(d.file.path, "mods/b.rbm");
        assert!(i.default_mod(i.song("songs/bad.rbs").unwrap()).is_none());
        assert!(i.mod_at("a.rbm").is_none());
    }
}
