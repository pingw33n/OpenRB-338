//! The output encoders: WAV (byte-identical to the engine export's file), FLAC and Ogg Vorbis.

use std::error::Error;
#[cfg(feature = "ogg")]
use std::num::{NonZeroU8, NonZeroU32};

use flacenc::component::BitRepr;
use flacenc::error::Verify;
#[cfg(feature = "ogg")]
use vorbis_rs::{VorbisBitrateManagementStrategy, VorbisEncoderBuilder};

pub const SAMPLE_RATE: u32 = 44100;

/// The Ogg stream serial of every .ogg file orb writes ("rb", the project's first name; kept so
/// the files stay the same).
#[cfg(feature = "ogg")]
const OGG_SERIAL: i32 = 0x7262;

/// The container chosen by the output file's extension.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Wav,
    Flac,
    Ogg,
}

impl Format {
    pub fn from_extension(ext: Option<&str>) -> Option<Format> {
        match ext?.to_ascii_lowercase().as_str() {
            "wav" => Some(Format::Wav),
            "flac" => Some(Format::Flac),
            "ogg" => Some(Format::Ogg),
            _ => None,
        }
    }
}

/// Encodes interleaved 16-bit stereo samples.
pub fn encode(format: Format, samples: &[i16], quality: f32) -> Result<Vec<u8>, Box<dyn Error>> {
    match format {
        Format::Wav => Ok(orb_engine::frame::export::wav_bytes(samples)),
        Format::Flac => flac(samples),
        Format::Ogg => ogg(samples, quality),
    }
}

fn flac(samples: &[i16]) -> Result<Vec<u8>, Box<dyn Error>> {
    #[allow(unused_mut)]
    let mut config = flacenc::config::Encoder::default();
    // wasm32 has no threads: flacenc's parallel encoder (feature `par`) would spawn some; the
    // frames are the same either way
    #[cfg(target_arch = "wasm32")]
    {
        config.multithread = false;
    }
    let config = config
        .into_verified()
        .map_err(|e| format!("FLAC encoder configuration: {e:?}"))?;
    let pcm: Vec<i32> = samples.iter().map(|&s| s as i32).collect();
    let source = flacenc::source::MemSource::from_samples(&pcm, 2, 16, SAMPLE_RATE as usize);
    let stream = flacenc::encode_with_fixed_block_size(&config, source, config.block_size)
        .map_err(|e| format!("FLAC encoding failed: {e:?}"))?;
    let mut sink = flacenc::bitsink::ByteSink::new();
    stream
        .write(&mut sink)
        .map_err(|e| format!("FLAC output failed: {e:?}"))?;
    Ok(sink.as_slice().to_vec())
}

/// The FLAC file of [`encode`] made in steps, a few FLAC frames at a time (the web's export
/// encodes on the page's one thread, a slice per frame, task-172): flacenc's single-threaded
/// `encode_with_fixed_block_size`, step by step, with the configuration [`encode`] uses. The
/// file is the same as [`encode`]'s (flacenc's parallel encoder makes the same frames; the
/// test `the_flac_in_steps_is_the_whole_flac`).
pub struct FlacSteps {
    config: flacenc::error::Verified<flacenc::config::Encoder>,
    src: flacenc::source::MemSource,
    fill: (flacenc::source::FrameBuf, flacenc::source::Context),
    stream: flacenc::component::Stream,
    /// The FLAC frames encoded and in all.
    done: usize,
    total: usize,
    finished: bool,
}

impl FlacSteps {
    /// The encoder of `samples` (interleaved 16-bit stereo); nothing is encoded yet.
    pub fn new(samples: &[i16]) -> Result<Self, Box<dyn Error>> {
        let mut config = flacenc::config::Encoder::default();
        config.multithread = false;
        let config = config
            .into_verified()
            .map_err(|e| format!("FLAC encoder configuration: {e:?}"))?;
        let block = config.block_size;
        let pcm: Vec<i32> = samples.iter().map(|&s| s as i32).collect();
        let src = flacenc::source::MemSource::from_samples(&pcm, 2, 16, SAMPLE_RATE as usize);
        let fill = (
            flacenc::source::FrameBuf::with_size(2, block)
                .map_err(|e| format!("FLAC encoder: {e:?}"))?,
            flacenc::source::Context::new(16, 2),
        );
        let mut stream = flacenc::component::Stream::new(SAMPLE_RATE as usize, 2, 16)
            .map_err(|e| format!("FLAC encoder: {e:?}"))?;
        // as flacenc's encoder does (the reference encoder's rule for a single short frame)
        stream
            .stream_info_mut()
            .set_block_sizes(block, block)
            .map_err(|e| format!("FLAC encoder: {e:?}"))?;
        Ok(FlacSteps {
            total: (samples.len() / 2).div_ceil(block),
            config,
            src,
            fill,
            stream,
            done: 0,
            finished: false,
        })
    }

    /// The FLAC frames encoded so far and in all.
    pub fn progress(&self) -> (usize, usize) {
        (self.done, self.total)
    }

    /// Encodes up to `frames` FLAC frames; true once all are encoded.
    pub fn step(&mut self, frames: usize) -> Result<bool, Box<dyn Error>> {
        use flacenc::source::Source;
        for _ in 0..frames {
            if self.finished {
                break;
            }
            let read = self
                .src
                .read_samples(self.config.block_size, &mut self.fill)
                .map_err(|e| format!("FLAC encoding failed: {e:?}"))?;
            if read == 0 {
                self.finished = true;
                break;
            }
            let frame = flacenc::encode_fixed_size_frame(
                &self.config,
                &self.fill.0,
                self.fill.1.current_frame_number().unwrap_or(0),
                self.stream.stream_info(),
            )
            .map_err(|e| format!("FLAC encoding failed: {e:?}"))?;
            self.stream.add_frame(frame);
            self.done += 1;
        }
        Ok(self.finished)
    }

    /// The file, once [`FlacSteps::step`] has returned true.
    pub fn finish(mut self) -> Result<Vec<u8>, Box<dyn Error>> {
        use flacenc::source::Source;
        let ctx = &self.fill.1;
        let total = self.src.len_hint().unwrap_or_else(|| ctx.total_samples());
        let md5 = ctx.md5_digest();
        let info = self.stream.stream_info_mut();
        info.set_md5_digest(&md5);
        info.set_total_samples(total);
        let mut sink = flacenc::bitsink::ByteSink::new();
        self.stream
            .write(&mut sink)
            .map_err(|e| format!("FLAC output failed: {e:?}"))?;
        Ok(sink.as_slice().to_vec())
    }
}

/// Without the feature `ogg` (the web build) there is no Vorbis encoder.
#[cfg(not(feature = "ogg"))]
fn ogg(_samples: &[i16], _quality: f32) -> Result<Vec<u8>, Box<dyn Error>> {
    Err("Ogg Vorbis is not available in this build".into())
}

#[cfg(feature = "ogg")]
fn ogg(samples: &[i16], quality: f32) -> Result<Vec<u8>, Box<dyn Error>> {
    let mut out = Vec::new();
    // A fixed stream serial, so that the same samples give the same file.
    let mut enc = VorbisEncoderBuilder::new_with_serial(
        NonZeroU32::new(SAMPLE_RATE).unwrap(),
        NonZeroU8::new(2).unwrap(),
        &mut out,
        OGG_SERIAL,
    )
    .bitrate_management_strategy(VorbisBitrateManagementStrategy::QualityVbr {
        target_quality: quality,
    })
    .build()?;
    for chunk in samples.chunks(2 * 4096) {
        let l: Vec<f32> = chunk
            .iter()
            .step_by(2)
            .map(|&s| s as f32 / 32768.0)
            .collect();
        let r: Vec<f32> = chunk
            .iter()
            .skip(1)
            .step_by(2)
            .map(|&s| s as f32 / 32768.0)
            .collect();
        enc.encode_audio_block([&l[..], &r[..]])?;
    }
    enc.finish()?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Stereo test audio: a sine on the left, a noise on the right, `frames` long.
    fn audio(frames: usize) -> Vec<i16> {
        let mut seed = 0x1234_5678u32;
        (0..frames)
            .flat_map(|i| {
                seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let l = ((i as f64 * 0.031).sin() * 20000.0) as i16;
                [l, (seed >> 16) as i16]
            })
            .collect()
    }

    /// The web's FLAC in steps (task-172) is the file of `encode`, whose frames flacenc's
    /// parallel encoder makes natively: for a long and a short input, one and three frames a
    /// step.
    #[test]
    fn the_flac_in_steps_is_the_whole_flac() {
        for frames in [4096 * 5 + 123, 100, 0] {
            let samples = audio(frames);
            let whole = encode(Format::Flac, &samples, 0.5).unwrap();
            for per in [1, 3] {
                let mut s = FlacSteps::new(&samples).unwrap();
                let mut steps = 0;
                while !s.step(per).unwrap() {
                    steps += 1;
                }
                assert_eq!(s.progress().0, s.progress().1, "{frames} {per}");
                assert!(steps <= s.progress().1.div_ceil(per), "{frames} {per}");
                assert!(
                    s.finish().unwrap() == whole,
                    "{frames} frames, {per} a step"
                );
            }
        }
    }
}
