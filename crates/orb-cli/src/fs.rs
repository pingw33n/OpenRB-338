//! The file system the library and the UI read songs and mods from and write them to.
//!
//! Natively each function is the `std::fs` (or `Path`) call it names, unchanged. On wasm32 (the
//! UI's web build, task-172) the browser gives the page no file system: the files are those the
//! user picked or dropped onto the page and those the page saved, held in memory for the session
//! (lost on a reload). Paths there are absolute, with `/` separators; a folder exists when it
//! was created or holds a file.

use std::io;
use std::path::{Path, PathBuf};

/// What [`metadata`] tells of a path (following links natively, as `std::fs::metadata`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Meta {
    pub is_dir: bool,
    pub len: u64,
    pub readonly: bool,
}

#[cfg(not(target_arch = "wasm32"))]
mod imp {
    use super::*;

    pub fn read(p: &Path) -> io::Result<Vec<u8>> {
        std::fs::read(p)
    }

    pub fn write(p: &Path, bytes: &[u8]) -> io::Result<()> {
        std::fs::write(p, bytes)
    }

    pub fn rename(from: &Path, to: &Path) -> io::Result<()> {
        std::fs::rename(from, to)
    }

    pub fn remove_file(p: &Path) -> io::Result<()> {
        std::fs::remove_file(p)
    }

    pub fn create_dir_all(p: &Path) -> io::Result<()> {
        std::fs::create_dir_all(p)
    }

    pub fn metadata(p: &Path) -> io::Result<Meta> {
        let m = std::fs::metadata(p)?;
        Ok(Meta {
            is_dir: m.is_dir(),
            len: m.len(),
            readonly: m.permissions().readonly(),
        })
    }

    pub fn read_dir(dir: &Path) -> io::Result<Vec<PathBuf>> {
        Ok(std::fs::read_dir(dir)?
            .filter_map(Result::ok)
            .map(|e| e.path())
            .collect())
    }

    pub fn is_dir(p: &Path) -> bool {
        p.is_dir()
    }

    pub fn is_file(p: &Path) -> bool {
        p.is_file()
    }

    pub fn exists(p: &Path) -> bool {
        p.exists()
    }

    /// Natively every file is read when asked for: none is lazy.
    pub fn is_lazy(_p: &Path) -> bool {
        false
    }

    /// Natively every file is read when asked for: `None`.
    pub fn lazy_info(_p: &Path) -> Option<Option<rbm::Info>> {
        None
    }
}

#[cfg(target_arch = "wasm32")]
mod imp {
    use std::cell::RefCell;
    use std::collections::{BTreeMap, BTreeSet};
    use std::path::Component;
    use std::sync::Arc;

    use super::*;

    #[derive(Default)]
    struct Mem {
        files: BTreeMap<PathBuf, Arc<Vec<u8>>>,
        /// Files whose bytes are not here yet ([`insert_lazy`]): their size and the mod's INFO.
        lazy: BTreeMap<PathBuf, (u64, Option<rbm::Info>)>,
        dirs: BTreeSet<PathBuf>,
    }

    thread_local! {
        static MEM: RefCell<Mem> = RefCell::new(Mem::default());
    }

    /// `p` made absolute (from `/`) with `.` and `..` resolved.
    pub fn normal(p: &Path) -> PathBuf {
        let mut out = PathBuf::from("/");
        for c in p.components() {
            match c {
                Component::Normal(n) => out.push(n),
                Component::ParentDir => {
                    out.pop();
                }
                _ => {}
            }
        }
        out
    }

    fn not_found(p: &Path) -> io::Error {
        io::Error::new(
            io::ErrorKind::NotFound,
            format!("{}: no such file", p.display()),
        )
    }

    fn dir_exists(m: &Mem, p: &Path) -> bool {
        p == Path::new("/")
            || m.dirs.contains(p)
            || m.files
                .range(p.to_path_buf()..)
                .next()
                .is_some_and(|(f, _)| f.starts_with(p) && f != p)
    }

    fn add_parents(m: &mut Mem, p: &Path) {
        let mut d = p.parent();
        while let Some(x) = d {
            if x == Path::new("/") {
                break;
            }
            m.dirs.insert(x.to_path_buf());
            d = x.parent();
        }
    }

    pub fn read(p: &Path) -> io::Result<Vec<u8>> {
        let p = normal(p);
        MEM.with_borrow(|m| {
            if m.lazy.contains_key(&p) {
                return Err(io::Error::new(
                    io::ErrorKind::WouldBlock,
                    format!("{}: not read from the computer yet", p.display()),
                ));
            }
            m.files
                .get(&p)
                .map(|b| b.to_vec())
                .ok_or_else(|| not_found(&p))
        })
    }

    pub fn write(p: &Path, bytes: &[u8]) -> io::Result<()> {
        let p = normal(p);
        MEM.with_borrow_mut(|m| {
            if dir_exists(m, &p) {
                return Err(io::Error::other(format!("{}: is a folder", p.display())));
            }
            add_parents(m, &p);
            m.lazy.remove(&p);
            m.files.insert(p, Arc::new(bytes.to_vec()));
            Ok(())
        })
    }

    pub fn rename(from: &Path, to: &Path) -> io::Result<()> {
        let (from, to) = (normal(from), normal(to));
        MEM.with_borrow_mut(|m| {
            let b = m.files.remove(&from).ok_or_else(|| not_found(&from))?;
            add_parents(m, &to);
            m.files.insert(to, b);
            Ok(())
        })
    }

    pub fn remove_file(p: &Path) -> io::Result<()> {
        let p = normal(p);
        MEM.with_borrow_mut(|m| {
            let lazy = m.lazy.remove(&p).is_some();
            (m.files.remove(&p).is_some() || lazy).then_some(())
        })
        .ok_or_else(|| not_found(&p))
    }

    pub fn create_dir_all(p: &Path) -> io::Result<()> {
        let p = normal(p);
        MEM.with_borrow_mut(|m| {
            if m.files.contains_key(&p) {
                return Err(io::Error::other(format!("{}: is a file", p.display())));
            }
            add_parents(m, &p);
            if p != Path::new("/") {
                m.dirs.insert(p);
            }
            Ok(())
        })
    }

    pub fn metadata(p: &Path) -> io::Result<Meta> {
        let p = normal(p);
        MEM.with_borrow(|m| {
            let len = m
                .files
                .get(&p)
                .map(|b| b.len() as u64)
                .or_else(|| m.lazy.get(&p).map(|l| l.0));
            if let Some(len) = len {
                Some(Meta {
                    is_dir: false,
                    len,
                    readonly: false,
                })
            } else {
                dir_exists(m, &p).then_some(Meta {
                    is_dir: true,
                    len: 0,
                    readonly: false,
                })
            }
        })
        .ok_or_else(|| not_found(&p))
    }

    pub fn read_dir(dir: &Path) -> io::Result<Vec<PathBuf>> {
        let dir = normal(dir);
        MEM.with_borrow(|m| {
            if !dir_exists(m, &dir) {
                return Err(not_found(&dir));
            }
            let children: BTreeSet<PathBuf> = m
                .files
                .keys()
                .chain(m.lazy.keys())
                .chain(m.dirs.iter())
                .filter_map(|p| {
                    let rest = p.strip_prefix(&dir).ok()?;
                    let first = rest.components().next()?;
                    Some(dir.join(first))
                })
                .collect();
            Ok(children.into_iter().collect())
        })
    }

    pub fn is_dir(p: &Path) -> bool {
        metadata(p).is_ok_and(|m| m.is_dir)
    }

    pub fn is_file(p: &Path) -> bool {
        metadata(p).is_ok_and(|m| !m.is_dir)
    }

    pub fn exists(p: &Path) -> bool {
        metadata(p).is_ok()
    }

    /// A file of `len` bytes whose bytes are read later (a mod of a picked mods folder, kept
    /// as the browser's file handle until a song uses it): it is listed and has its size, its
    /// mod `info` answers [`lazy_info`], and [`read`] fails with `WouldBlock` until [`write`]
    /// gives the bytes.
    pub fn insert_lazy(p: &Path, len: u64, info: Option<rbm::Info>) {
        let p = normal(p);
        MEM.with_borrow_mut(|m| {
            add_parents(m, &p);
            m.files.remove(&p);
            m.lazy.insert(p, (len, info));
        });
    }

    /// Whether `p` is a file whose bytes are not read yet ([`insert_lazy`]).
    pub fn is_lazy(p: &Path) -> bool {
        let p = normal(p);
        MEM.with_borrow(|m| m.lazy.contains_key(&p))
    }

    /// The INFO of a mod not read yet ([`insert_lazy`]): `Some(None)` for a file that is no
    /// mod with an INFO, `None` for any other path.
    pub fn lazy_info(p: &Path) -> Option<Option<rbm::Info>> {
        let p = normal(p);
        MEM.with_borrow(|m| m.lazy.get(&p).map(|l| l.1.clone()))
    }

    /// Removes `dir` and everything under it (a mods folder picked again replaces the old).
    pub fn remove_dir_all(dir: &Path) {
        let dir = normal(dir);
        MEM.with_borrow_mut(|m| {
            m.files.retain(|p, _| !p.starts_with(&dir));
            m.lazy.retain(|p, _| !p.starts_with(&dir));
            m.dirs.retain(|p| !p.starts_with(&dir));
        });
    }
}

pub use imp::*;

#[cfg(all(test, target_arch = "wasm32"))]
mod tests {
    use super::*;

    #[test]
    fn memory_files_and_folders() {
        let p = Path::new("/Songs/a.rbs");
        assert!(!exists(p));
        write(p, b"x").unwrap();
        assert!(is_file(p) && is_dir(Path::new("/Songs")) && is_dir(Path::new("/")));
        assert_eq!(read(Path::new("/Songs/../Songs/a.rbs")).unwrap(), b"x");
        assert_eq!(
            read_dir(Path::new("/")).unwrap(),
            vec![PathBuf::from("/Songs")]
        );
        rename(p, Path::new("/Songs/b.rbs")).unwrap();
        assert!(!exists(p));
        assert_eq!(metadata(Path::new("/Songs/b.rbs")).unwrap().len, 1);
        remove_dir_all(Path::new("/Songs"));
        assert!(!exists(Path::new("/Songs")));
    }
}
