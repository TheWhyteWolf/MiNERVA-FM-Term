//! Spectrum analysis for the visualiser pane: 2048-sample Hann window →
//! realfft → dB-normalised magnitudes mapped onto 56 columns with the web
//! edition's bin curve (t^1.7) and gamma (v^0.85), EMA-smoothed per column
//! (0.78, mimicking the AnalyserNode's smoothing).

use realfft::{RealFftPlanner, RealToComplex};
use std::sync::Arc;

pub const COLS: usize = 56;
const FFT_SIZE: usize = 2048;
const SMOOTHING: f32 = 0.78;
const DB_MIN: f32 = -100.0;
const DB_MAX: f32 = -30.0;

pub struct Spectrum {
    fft: Arc<dyn RealToComplex<f32>>,
    window: Vec<f32>, // rolling mono window, FFT_SIZE long
    hann: Vec<f32>,
    input: Vec<f32>,
    output: Vec<realfft::num_complex::Complex<f32>>,
    cols: [f32; COLS],
}

impl Spectrum {
    pub fn new() -> Spectrum {
        let fft = RealFftPlanner::<f32>::new().plan_fft_forward(FFT_SIZE);
        let input = fft.make_input_vec();
        let output = fft.make_output_vec();
        let hann = (0..FFT_SIZE)
            .map(|i| {
                let x = std::f32::consts::TAU * i as f32 / FFT_SIZE as f32;
                0.5 * (1.0 - x.cos())
            })
            .collect();
        Spectrum {
            fft,
            window: vec![0.0; FFT_SIZE],
            hann,
            input,
            output,
            cols: [0.0; COLS],
        }
    }

    /// Feed freshly played mono samples into the rolling window.
    pub fn feed(&mut self, samples: &[f32]) {
        let n = samples.len().min(FFT_SIZE);
        self.window.drain(..n);
        self.window.extend_from_slice(&samples[samples.len() - n..]);
    }

    /// Recompute the 56 column magnitudes (0..1) from the current window.
    pub fn update(&mut self) {
        for i in 0..FFT_SIZE {
            self.input[i] = self.window[i] * self.hann[i];
        }
        if self.fft.process(&mut self.input, &mut self.output).is_err() {
            return;
        }
        let bins = self.output.len();
        for c in 0..COLS {
            let t = c as f32 / COLS as f32;
            let bin = ((t.powf(1.7) * bins as f32) as usize).min(bins - 1);
            let mag = self.output[bin].norm() * 2.0 / FFT_SIZE as f32;
            let db = 20.0 * (mag + 1e-10).log10();
            let v = ((db - DB_MIN) / (DB_MAX - DB_MIN)).clamp(0.0, 1.0).powf(0.85);
            self.cols[c] = self.cols[c] * SMOOTHING + v * (1.0 - SMOOTHING);
        }
    }

    pub fn bars(&self) -> &[f32; COLS] {
        &self.cols
    }
}

/// Map spectrum column `c` to a terminal x position, spreading the 56 bars
/// across `width` cells (centre of each equal-width slot).
pub fn bar_x(c: usize, width: usize) -> usize {
    (c * width + width / 2) / COLS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bin_mapping_is_monotonic_and_bounded() {
        let bins = 1025usize;
        let mut last = 0usize;
        for c in 0..COLS {
            let t = c as f32 / COLS as f32;
            let bin = ((t.powf(1.7) * bins as f32) as usize).min(bins - 1);
            assert!(bin >= last, "bin map must not go backwards");
            assert!(bin < bins);
            last = bin;
        }
        // The curve should compress bass: first quarter of columns covers
        // well under a quarter of the bins.
        let quarter = ((0.25f32).powf(1.7) * bins as f32) as usize;
        assert!(quarter < bins / 4);
    }

    #[test]
    fn bar_positions_fit_width() {
        for width in [56usize, 80, 120, 200] {
            for c in 0..COLS {
                assert!(bar_x(c, width) < width);
            }
        }
    }
}
