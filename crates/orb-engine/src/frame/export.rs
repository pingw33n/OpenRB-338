//! The export driver: "Export Loop as Audio File" as Rebirth.exe runs it (`sub_427988` →
//! `sub_4280D5`, `docs/export_state.md` section 4) after `N` idle blocks, which is how
//! `tools/rbhost` reproduces the app's exports (`docs/export_state.md` sections 12 and 13).

use super::{Activation, BLOCK, Engine, LoadRefused};
use crate::song::{Globals, Song};

/// The frames of one realtime MME call (`docs/export_state.md` section 2.1): the idle blocks are
/// rendered 16 at a time, as the app's stream and rbhost do.
pub const REALTIME_FRAMES: usize = 1024;
/// The frames of a full export chunk (`cmp [ebp+var_14], 800h` at 0x428186).
pub const CHUNK_FRAMES: i32 = 2048;
/// `dbl_4BBE08`: ticks per quarter note.
const PPQ: f64 = f64::from_bits(0x4068_0000_0000_0000); // 192.0
/// `dbl_4BBE10`: 60 s * 44100 frames * 1000 (the tempo is BPM * 1000).
const FRAMES_MILLI_MINUTE: f64 = f64::from_bits(0x41E3_B6D8_3000_0000); // 2646000000.0

/// What the export queries from the engine before it starts (msg 20, ids 4, 12 and 16).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExportRange {
    /// BPM * 1000.
    pub tempo: u32,
    /// The loop start and end in ticks.
    pub loop_start: u32,
    pub loop_end: u32,
}

impl From<&Globals> for ExportRange {
    fn from(g: &Globals) -> Self {
        ExportRange {
            tempo: g.tempo,
            loop_start: g.loop_start,
            loop_end: g.loop_end,
        }
    }
}

/// The frame counts of the export's `SafeSoundCallback` calls, `sub_4280D5` (0x428139..0x4281DB):
/// ```text
/// pos = (double)start
/// do  n = ftol((end - pos) / (tempo * (192.0 / 2646000000.0)))
///     if n >= 2048: n = 2048 else last
///     step = (tempo * n) * (192.0 / 2646000000.0)
///     render n; pos = pos + step
/// until last
/// ```
/// The values are 32-bit `fild`s; `192/2646e6` is divided at run time; `__ftol` truncates.
pub fn chunk_sizes(r: ExportRange) -> Vec<i32> {
    let tempo = r.tempo as i32 as f64;
    let end = r.loop_end as i32 as f64;
    let mut pos = r.loop_start as i32 as f64;
    let mut out = Vec::new();
    loop {
        let mut n = ((end - pos) / (tempo * (PPQ / FRAMES_MILLI_MINUTE))) as i64 as i32;
        let last = n < CHUNK_FRAMES;
        if !last {
            n = CHUNK_FRAMES;
        }
        let step = (tempo * n as f64) * (PPQ / FRAMES_MILLI_MINUTE);
        out.push(n);
        pos += step;
        if last {
            return out;
        }
    }
}

/// The exported frame count: the sum of the chunks (the file gets `4*n` bytes per chunk).
pub fn frame_count(r: ExportRange) -> usize {
    chunk_sizes(r).iter().map(|&n| n.max(0) as usize).sum()
}

/// Renders `idle_blocks` 64-frame blocks with the transport stopped: `idle_blocks / 16` calls of
/// 1024 frames, then one call for the remainder (rbhost's `render_idle`).
pub fn render_idle(engine: &mut Engine, idle_blocks: u32) {
    render_idle_into(engine, idle_blocks, &mut vec![0i16; 2 * REALTIME_FRAMES]);
}

/// [`render_idle`] into the caller's buffer (at least 1024 frames).
fn render_idle_into(engine: &mut Engine, idle_blocks: u32, buf: &mut [i16]) {
    for _ in 0..idle_blocks / 16 {
        engine.render(buf, REALTIME_FRAMES);
    }
    let rest = (idle_blocks % 16) as usize * BLOCK;
    if rest > 0 {
        engine.render(buf, rest);
    }
}

/// The export as rbhost runs it (`tools/rbhost/rbhost.c`, `docs/export_state.md` section 4) on
/// a song the caller has loaded ([`Engine::load`]) and activated ([`Engine::activate`]):
/// `idle_blocks` idle blocks, stop, locate to the loop start, play, the chunks of
/// [`chunk_sizes`], then stop, msg 33 and a 0-frame call (which drops the last chunk's leftover).
/// Each message is applied by a 0-frame call that renders nothing, so the messages are applied
/// here directly. Returns interleaved int16 stereo.
///
/// One int16 buffer, zeroed once, serves the idle calls and every chunk, as rbhost's static
/// `chunk_buf` does; the app's export loop `sub_4280D5` likewise renders every chunk into the
/// one buffer `sub_427988` allocates (`operator new(0x2000)` at 0x427C2F, not cleared; the
/// callback at 0x4281C3, the write at 0x4281F2). A render call that ends in a
/// [`super::Fault`] writes nothing ([`Engine::fault`]), so its chunk repeats what the buffer
/// held: the previous chunk's samples, or the idle calls', or zeros.
pub fn export(engine: &mut Engine, range: ExportRange, idle_blocks: u32) -> Vec<i16> {
    let mut e = Export::new(range, idle_blocks);
    while !e.step(engine, usize::MAX) {}
    e.into_samples()
}

/// [`export`] in steps (the web build's export runs on the page's one thread, a slice per
/// frame, task-172): the same engine calls in the same order, on the same one buffer.
/// [`Export::step`] makes up to a given number of the calls that render frames (the idle calls
/// and the chunks); the 0-frame calls and the messages between them go with the first chunk
/// (the start) and the last chunk (the end).
pub struct Export {
    range: ExportRange,
    buf: Vec<i16>,
    /// The idle calls of 1024 frames, and the frames of the last idle call (0: none).
    idle_calls: u32,
    idle_rest: usize,
    chunks: Vec<i32>,
    /// The rendering calls made so far: the idle calls first, then the chunks.
    done: usize,
    out: Vec<i16>,
    finished: bool,
}

impl Export {
    /// The export of `range` after `idle_blocks` idle blocks, of a song the caller has loaded
    /// and activated (as for [`export`]); nothing is rendered yet.
    pub fn new(range: ExportRange, idle_blocks: u32) -> Export {
        Export {
            range,
            buf: vec![0i16; 2 * CHUNK_FRAMES as usize],
            idle_calls: idle_blocks / 16,
            idle_rest: (idle_blocks % 16) as usize * BLOCK,
            chunks: chunk_sizes(range),
            done: 0,
            out: Vec::with_capacity(2 * frame_count(range)),
            finished: false,
        }
    }

    /// The idle calls, the remainder's included.
    fn idle_total(&self) -> usize {
        self.idle_calls as usize + usize::from(self.idle_rest > 0)
    }

    /// The rendering calls of the whole export.
    pub fn total(&self) -> usize {
        self.idle_total() + self.chunks.len()
    }

    /// The rendering calls made so far (of [`Export::total`]).
    pub fn done(&self) -> usize {
        self.done
    }

    /// Makes up to `calls` rendering calls; true once the export is complete (the samples are
    /// then [`Export::into_samples`], and a further step does nothing).
    pub fn step(&mut self, engine: &mut Engine, calls: usize) -> bool {
        let idle = self.idle_total();
        let mut left = calls;
        while !self.finished && left > 0 {
            if self.done < self.idle_calls as usize {
                engine.render(&mut self.buf, REALTIME_FRAMES);
            } else if self.done < idle {
                engine.render(&mut self.buf, self.idle_rest);
            } else {
                let i = self.done - idle;
                if i == 0 {
                    engine.stop();
                    engine.render(&mut [], 0);
                    engine.locate(self.range.loop_start);
                    engine.play();
                    engine.render(&mut [], 0);
                }
                if let Some(&n) = self.chunks.get(i) {
                    // rbhost writes only positive counts; the app would pass a negative byte
                    // count to its writer, which never happens for a loop of positive length
                    let n = n.max(0) as usize;
                    engine.render(&mut self.buf, n);
                    self.out.extend_from_slice(&self.buf[..2 * n]);
                }
                if i + 1 >= self.chunks.len() {
                    engine.stop();
                    engine.reset();
                    engine.render(&mut [], 0);
                    self.finished = true;
                }
            }
            self.done += 1;
            left -= 1;
        }
        self.finished
    }

    /// The exported samples, interleaved int16 stereo (all of them once [`Export::step`] has
    /// returned true).
    pub fn into_samples(self) -> Vec<i16> {
        self.out
    }
}

/// rbhost's whole run on a new song (by default, `--idle-blocks N`): load `song`, activate it
/// over the stopped "Untitled song" ([`Activation::ReplacesCurrent`]), then [`export`] with the
/// range the export queries (msg 20 ids 4, 12, 16, which read the loaded globals). A load the
/// pool refuses gives no export: the app opens no window for the song.
pub fn export_song(
    engine: &mut Engine,
    song: &Song,
    idle_blocks: u32,
) -> Result<Vec<i16>, LoadRefused> {
    let mut e = export_song_steps(engine, song, idle_blocks)?;
    while !e.step(engine, usize::MAX) {}
    Ok(e.into_samples())
}

/// [`export_song`] in steps ([`Export`]): loads and activates `song` and returns the export,
/// whose rendering calls are its steps'.
pub fn export_song_steps(
    engine: &mut Engine,
    song: &Song,
    idle_blocks: u32,
) -> Result<Export, LoadRefused> {
    engine.load(song)?;
    engine.activate(Activation::ReplacesCurrent {
        current_playing: false,
    });
    Ok(Export::new(ExportRange::from(&song.globals), idle_blocks))
}

/// The 44-byte header rbhost and the app write before the samples (16-bit stereo 44100 Hz PCM).
pub fn wav_header(frames: usize) -> [u8; 44] {
    let data = (frames * 4) as u32;
    let mut h = [0u8; 44];
    h[0..4].copy_from_slice(b"RIFF");
    h[4..8].copy_from_slice(&(36 + data).to_le_bytes());
    h[8..16].copy_from_slice(b"WAVEfmt ");
    h[16..20].copy_from_slice(&16u32.to_le_bytes());
    h[20..22].copy_from_slice(&1u16.to_le_bytes());
    h[22..24].copy_from_slice(&2u16.to_le_bytes());
    h[24..28].copy_from_slice(&44100u32.to_le_bytes());
    h[28..32].copy_from_slice(&(44100u32 * 4).to_le_bytes());
    h[32..34].copy_from_slice(&4u16.to_le_bytes());
    h[34..36].copy_from_slice(&16u16.to_le_bytes());
    h[36..40].copy_from_slice(b"data");
    h[40..44].copy_from_slice(&data.to_le_bytes());
    h
}

/// The whole WAV file of an export.
pub fn wav_bytes(samples: &[i16]) -> Vec<u8> {
    let mut b = Vec::with_capacity(44 + 2 * samples.len());
    b.extend_from_slice(&wav_header(samples.len() / 2));
    for s in samples {
        b.extend_from_slice(&s.to_le_bytes());
    }
    b
}
