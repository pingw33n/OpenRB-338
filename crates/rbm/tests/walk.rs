//! `rbm::Walk`, the container check that `Rbm::parse` runs over a whole file and the web build
//! runs over a picked file's slices (task-172): on valid and damaged mods it gives the result of
//! the parse before the walk existed, read whole or a few bytes at a time, and it reads only
//! headers, names and the INFO.

use rbm::{Chunk, INFO_SIZE, Info, Rbm, Walk, Walked, Want};

/// The chunks, the INFO and the warnings of a parse.
type Parsed = (Vec<Chunk>, Option<Info>, Vec<String>);

/// The parse of `Rbm::parse` before `Walk` (task-172), kept as the reference.
fn old_parse(data: &[u8]) -> Result<Parsed, String> {
    let be32 = |b: &[u8], at: usize| u32::from_be_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]]);
    if data.len() < 12 || &data[..4] != b"CAT " {
        return Err("not an IFF CAT".into());
    }
    let size = be32(data, 4) as usize;
    if size.checked_add(8) != Some(data.len()) {
        return Err(format!(
            "CAT size {size} does not match the file ({} bytes)",
            data.len()
        ));
    }
    let mut warnings = Vec::new();
    if &data[8..12] != b"PRBM" {
        warnings.push(format!(
            "CAT type {:?} is not PRBM",
            String::from_utf8_lossy(&data[8..12])
        ));
    }
    let mut chunks = Vec::new();
    let mut info = None;
    let (mut pos, end) = (12usize, data.len());
    while pos + 8 <= end {
        let tag = [data[pos], data[pos + 1], data[pos + 2], data[pos + 3]];
        let csize = be32(data, pos + 4) as usize;
        let body = pos + 8;
        let body_end = body
            .checked_add(csize)
            .filter(|&e| e <= end)
            .ok_or_else(|| format!("chunk at {pos} overruns the file"))?;
        let mut c = Chunk {
            ordinal: chunks.len(),
            tag,
            offset: body,
            size: csize,
            name: None,
            name_len: 0,
        };
        if &tag == b"EMBF" {
            let nul = data[body..body_end]
                .iter()
                .position(|&b| b == 0)
                .ok_or_else(|| format!("EMBF at {pos} has no file name"))?;
            c.name = Some(data[body..body + nul].iter().map(|&b| b as char).collect());
            c.name_len = nul + 1;
        } else if &tag == b"INFO" && csize == INFO_SIZE {
            info = Info::from_body(&data[body..body_end]);
        }
        chunks.push(c);
        pos = body_end + (csize & 1);
    }
    if pos != end {
        return Err(format!("trailing bytes at {pos}"));
    }
    Ok((chunks, info, warnings))
}

/// The walk fed only the bytes it asks for, as the web build feeds it `File.slice`s. Returns the
/// result and the number of bytes read.
fn lazy(data: &[u8]) -> (Result<Walked, String>, usize) {
    let mut w = Walk::new(data.len());
    let mut read = 0;
    loop {
        match w.want() {
            Want::Bytes { at, len } => {
                read += len;
                if let Err(e) = w.feed(&data[at..at + len]) {
                    return (Err(e.to_string()), read);
                }
            }
            Want::Done => return (w.finish().map_err(|e| e.to_string()), read),
        }
    }
}

fn chunk(tag: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let mut v = tag.to_vec();
    v.extend_from_slice(&(body.len() as u32).to_be_bytes());
    v.extend_from_slice(body);
    if body.len() % 2 == 1 {
        v.push(0);
    }
    v
}

fn info_body(name: &str) -> Vec<u8> {
    let mut b = vec![0u8; INFO_SIZE];
    b[..4].copy_from_slice(b"text");
    b[1003..1003 + name.len()].copy_from_slice(name.as_bytes());
    b
}

fn embf(name: &str, len: usize) -> Vec<u8> {
    let mut b = name.as_bytes().to_vec();
    b.push(0);
    b.extend((0..len).map(|i| (i * 7 + 1) as u8));
    b
}

fn cat(kind: &[u8; 4], chunks: &[Vec<u8>]) -> Vec<u8> {
    let body: Vec<u8> = chunks.concat();
    let mut v = b"CAT ".to_vec();
    v.extend_from_slice(&((body.len() + 4) as u32).to_be_bytes());
    v.extend_from_slice(kind);
    v.extend_from_slice(&body);
    v
}

fn fix_size(mut v: Vec<u8>) -> Vec<u8> {
    let n = (v.len() - 8) as u32;
    v[4..8].copy_from_slice(&n.to_be_bytes());
    v
}

fn cases() -> Vec<(&'static str, Vec<u8>)> {
    let head = chunk(b"HEAD", &[1, 2, 3, 4]);
    let good = cat(
        b"PRBM",
        &[
            head.clone(),
            chunk(b"EMBF", &embf("Kick.wav", 100_000)),
            chunk(b"EMBF", &embf("Odd.wav", 1001)),
            chunk(b"INFO", &info_body("Good Mod")),
        ],
    );
    let mut cases = vec![("valid", good.clone())];
    cases.push(("empty", Vec::new()));
    cases.push(("short", b"CAT \0\0".to_vec()));
    cases.push(("not a CAT", {
        let mut v = good.clone();
        v[0] = b'F';
        v
    }));
    cases.push(("CAT size off by one", {
        let mut v = good.clone();
        v.push(0);
        v
    }));
    cases.push((
        "trailing bytes",
        fix_size([good.clone(), vec![1, 2, 3]].concat()),
    ));
    cases.push((
        "trailing chunk header",
        fix_size([good.clone(), b"JUNK".to_vec()].concat()),
    ));
    cases.push(("chunk overrun", {
        let mut v = good.clone();
        let at = v.len() - (8 + INFO_SIZE) + 4;
        v[at..at + 4].copy_from_slice(&((INFO_SIZE + 2) as u32).to_be_bytes());
        v
    }));
    cases.push(("chunk size huge", {
        let mut v = good.clone();
        v[16..20].copy_from_slice(&u32::MAX.to_be_bytes());
        v
    }));
    cases.push((
        "EMBF without a NUL",
        cat(
            b"PRBM",
            &[
                head.clone(),
                chunk(b"EMBF", &[b'x'; 600]),
                chunk(b"INFO", &info_body("Bad")),
            ],
        ),
    ));
    cases.push((
        "EMBF empty",
        cat(
            b"PRBM",
            &[chunk(b"EMBF", &[]), chunk(b"INFO", &info_body("Bad"))],
        ),
    ));
    cases.push((
        "EMBF name of 700 bytes",
        cat(
            b"PRBM",
            &[
                chunk(b"EMBF", &embf(&"n".repeat(700), 10)),
                chunk(b"INFO", &info_body("Long")),
            ],
        ),
    ));
    cases.push((
        "EMBF NUL at its last byte",
        cat(
            b"PRBM",
            &[chunk(
                b"EMBF",
                &[b'a'; 511].iter().copied().chain([0]).collect::<Vec<_>>(),
            )],
        ),
    ));
    cases.push((
        "INFO first, two INFOs",
        cat(
            b"PRBM",
            &[
                chunk(b"INFO", &info_body("First")),
                chunk(b"EMBF", &embf("A.wav", 10)),
                chunk(b"INFO", &info_body("Second")),
            ],
        ),
    ));
    cases.push((
        "INFO of another size",
        cat(
            b"PRBM",
            &[head.clone(), chunk(b"INFO", &vec![0; INFO_SIZE - 1])],
        ),
    ));
    cases.push(("no INFO", cat(b"PRBM", std::slice::from_ref(&head))));
    cases.push((
        "odd chunk without its pad at the end",
        fix_size(cat(b"PRBM", &[head.clone(), chunk(b"ODD ", &[1, 2, 3])])[..35].to_vec()),
    ));
    cases.push((
        "not PRBM",
        cat(b"XXXX", &[head, chunk(b"INFO", &info_body("Other"))]),
    ));
    cases
}

#[test]
fn the_walk_gives_the_old_parse() {
    let failing: Vec<&str> = cases()
        .iter()
        .filter(|(_, d)| old_parse(d).is_err())
        .map(|(w, _)| *w)
        .collect();
    assert_eq!(
        failing,
        [
            "empty",
            "short",
            "not a CAT",
            "CAT size off by one",
            "trailing bytes",
            "trailing chunk header",
            "chunk overrun",
            "chunk size huge",
            "EMBF without a NUL",
            "EMBF empty",
            "odd chunk without its pad at the end",
        ]
    );
    for (what, data) in cases() {
        let old = old_parse(&data);
        let parsed = Rbm::parse(data.clone())
            .map(|r| {
                (
                    r.chunks().to_vec(),
                    r.info().cloned(),
                    r.warnings().to_vec(),
                )
            })
            .map_err(|e| e.to_string());
        let (walked, _) = lazy(&data);
        let walked = walked.map(|w| (w.chunks, w.info, w.warnings));
        match &old {
            Ok(o) => {
                assert_eq!(parsed.as_ref().ok(), Some(o), "{what}: Rbm::parse");
                assert_eq!(walked.as_ref().ok(), Some(o), "{what}: the lazy walk");
            }
            Err(e) => {
                assert!(
                    parsed.as_ref().is_err_and(|p| p.contains(e.as_str())),
                    "{what}: {parsed:?}"
                );
                assert_eq!(parsed.as_ref().err(), walked.as_ref().err(), "{what}");
            }
        }
    }
}

#[test]
fn the_walk_reads_headers_names_and_the_info_only() {
    let (_, data) = cases().into_iter().next().unwrap();
    let (walked, read) = lazy(&data);
    let w = walked.unwrap();
    assert_eq!(w.info.unwrap().name, "Good Mod");
    assert_eq!(
        w.chunks
            .iter()
            .filter_map(|c| c.name.clone())
            .collect::<Vec<_>>(),
        ["Kick.wav", "Odd.wav"]
    );
    // the CAT header, four chunk headers, two names (a 256-byte step each) and the INFO
    assert_eq!(read, 12 + 4 * 8 + 2 * 256 + INFO_SIZE);
    assert!(data.len() > 50 * read);
}
