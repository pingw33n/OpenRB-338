//! The resampler of an output whose device does not run at 44.1 kHz (moved unchanged from
//! [`crate::output`] for task-171, so the web output's fallback uses the same one): rubato's
//! asynchronous sinc resampler ([`rubato::Async::new_sinc`], 128 taps, a Blackman-Harris
//! window, 256x oversampling, cubic interpolation), fed one engine call (1024 frames) at a time
//! (`FixedAsync::Input`); its buffers are allocated when it is made and `process_into_buffer`
//! allocates nothing.

use rubato::audioadapter_buffers::direct::InterleavedSlice;
use rubato::{
    Async, FixedAsync, Resampler as _, ResamplerConstructionError, SincInterpolationParameters,
    SincInterpolationType, WindowFunction,
};

use crate::{Audio, CALL_FRAMES, RATE};

/// The resampler and its buffers.
pub(crate) struct Resample {
    rs: Async<f32>,
    input: Vec<f32>,
    output: Vec<f32>,
    /// The frames of `output` the last chunk produced, and those already handed out.
    pub(crate) len: usize,
    pub(crate) pos: usize,
    /// `Output::inject_resampler_error`.
    pub(crate) fail_next: bool,
}

impl Resample {
    pub(crate) fn new(rate: u32) -> Result<Self, ResamplerConstructionError> {
        let params = SincInterpolationParameters::new(128, WindowFunction::BlackmanHarris2)
            .oversampling_factor(256)
            .interpolation(SincInterpolationType::Cubic);
        let rs = Async::<f32>::new_sinc(
            rate as f64 / RATE as f64,
            1.1,
            &params,
            CALL_FRAMES,
            2,
            FixedAsync::Input,
        )?;
        let max = rs.output_frames_max();
        Ok(Resample {
            rs,
            input: vec![0.0; 2 * CALL_FRAMES],
            output: vec![0.0; 2 * max],
            len: 0,
            pos: 0,
            fail_next: false,
        })
    }

    /// One engine call through the resampler into `output`; the frames written, or `None` when
    /// the buffers or the resampler refuse (the sizes are the resampler's own, so only a defect
    /// or `Output::inject_resampler_error` gets there).
    fn chunk(&mut self, audio: &mut Audio) -> Option<usize> {
        audio.render_f32(&mut self.input);
        // a forced error: an output buffer too small for the resampler
        let max = if self.fail_next {
            self.fail_next = false;
            0
        } else {
            self.output.len() / 2
        };
        let inp = InterleavedSlice::new(&self.input[..], 2, CALL_FRAMES).ok()?;
        let mut outp = InterleavedSlice::new_mut(&mut self.output[..], 2, max).ok()?;
        let (_, written) = self.rs.process_into_buffer(&inp, &mut outp, None).ok()?;
        Some(written)
    }

    /// Fills `out` (interleaved stereo) at the device rate. On a resampler error it returns
    /// `false` and the caller silences the rest of the device callback; the engine call of the
    /// failed chunk is lost and the next callback starts a new chunk.
    pub(crate) fn pull(&mut self, audio: &mut Audio, out: &mut [f32]) -> bool {
        let frames = out.len() / 2;
        let mut i = 0;
        while i < frames {
            if self.pos == self.len {
                self.pos = 0;
                match self.chunk(audio) {
                    Some(n) => self.len = n,
                    None => {
                        self.len = 0;
                        return false;
                    }
                }
                continue;
            }
            let n = (self.len - self.pos).min(frames - i);
            out[2 * i..2 * (i + n)].copy_from_slice(&self.output[2 * self.pos..2 * (self.pos + n)]);
            self.pos += n;
            i += n;
        }
        true
    }
}
