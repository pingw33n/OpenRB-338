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
/// The memory folder of the song collection's songs: where File > Open Collection... starts.
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
}
