//! Ableton Live 12 EQ Eight 8-Band Parametric Equalizer.
//!
//! Features:
//! - 8 parametric biquad bands: LowCut12, LowCut48, LowShelf, Bell, Notch, HighShelf, HighCut12, HighCut48.
//! - Stereo, Left/Right, and Mid/Side processing modes.
//! - Direct Form II Transposed biquad implementation with 4-stage cascade for 48 dB slopes.
//! - Interactive Bode magnitude response calculation across the frequency spectrum.
//! - Real-time 64-bin FFT spectrum analyzer for visual metering.

use std::f32::consts::PI;
use oxide_core::{AudioBuffer, AudioEffectDevice, Device, DeviceCategory, MusicalTime};

/// Filter response types supported by each of the 8 bands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EqBandType {
    LowCut12,
    LowCut48,
    LowShelf,
    Bell,
    Notch,
    HighShelf,
    HighCut12,
    HighCut48,
}

/// Stereo routing modes for EQ Eight.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EqStereoMode {
    Stereo,
    LeftRight,
    MidSide,
}

/// Biquad coefficients in Direct Form II Transposed form:
/// y[n] = b0*x[n] + b1*x[n-1] + b2*x[n-2] - a1*y[n-1] - a2*y[n-2]
#[derive(Debug, Clone, Copy)]
pub struct BiquadCoeffs {
    pub b0: f32,
    pub b1: f32,
    pub b2: f32,
    pub a1: f32,
    pub a2: f32,
}

impl BiquadCoeffs {
    pub fn identity() -> Self {
        Self {
            b0: 1.0,
            b1: 0.0,
            b2: 0.0,
            a1: 0.0,
            a2: 0.0,
        }
    }

    /// Calculate coefficients using RBJ Audio EQ Cookbook equations.
    pub fn calculate(band_type: EqBandType, freq_hz: f32, gain_db: f32, q: f32, sample_rate: f32) -> Self {
        let sr = if sample_rate <= 0.0 { 44100.0 } else { sample_rate };
        let f0 = freq_hz.clamp(10.0, sr * 0.49);
        let q_eff = q.clamp(0.05, 30.0);
        let w0 = 2.0 * PI * f0 / sr;
        let cos_w0 = w0.cos();
        let sin_w0 = w0.sin();
        let alpha = sin_w0 / (2.0 * q_eff);
        let a_amp = 10.0_f32.powf(gain_db / 40.0); // sqrt of linear gain

        let (b0, b1, b2, a0, a1, a2) = match band_type {
            EqBandType::Bell => {
                let b0 = 1.0 + alpha * a_amp;
                let b1 = -2.0 * cos_w0;
                let b2 = 1.0 - alpha * a_amp;
                let a0 = 1.0 + alpha / a_amp;
                let a1 = -2.0 * cos_w0;
                let a2 = 1.0 - alpha / a_amp;
                (b0, b1, b2, a0, a1, a2)
            }
            EqBandType::LowShelf => {
                let sqrt_a = a_amp.sqrt();
                let b0 = a_amp * ((a_amp + 1.0) - (a_amp - 1.0) * cos_w0 + 2.0 * sqrt_a * alpha);
                let b1 = 2.0 * a_amp * ((a_amp - 1.0) - (a_amp + 1.0) * cos_w0);
                let b2 = a_amp * ((a_amp + 1.0) - (a_amp - 1.0) * cos_w0 - 2.0 * sqrt_a * alpha);
                let a0 = (a_amp + 1.0) + (a_amp - 1.0) * cos_w0 + 2.0 * sqrt_a * alpha;
                let a1 = -2.0 * ((a_amp - 1.0) + (a_amp + 1.0) * cos_w0);
                let a2 = (a_amp + 1.0) + (a_amp - 1.0) * cos_w0 - 2.0 * sqrt_a * alpha;
                (b0, b1, b2, a0, a1, a2)
            }
            EqBandType::HighShelf => {
                let sqrt_a = a_amp.sqrt();
                let b0 = a_amp * ((a_amp + 1.0) + (a_amp - 1.0) * cos_w0 + 2.0 * sqrt_a * alpha);
                let b1 = -2.0 * a_amp * ((a_amp - 1.0) + (a_amp + 1.0) * cos_w0);
                let b2 = a_amp * ((a_amp + 1.0) + (a_amp - 1.0) * cos_w0 - 2.0 * sqrt_a * alpha);
                let a0 = (a_amp + 1.0) - (a_amp - 1.0) * cos_w0 + 2.0 * sqrt_a * alpha;
                let a1 = 2.0 * ((a_amp - 1.0) - (a_amp + 1.0) * cos_w0);
                let a2 = (a_amp + 1.0) - (a_amp - 1.0) * cos_w0 - 2.0 * sqrt_a * alpha;
                (b0, b1, b2, a0, a1, a2)
            }
            EqBandType::Notch => {
                let b0 = 1.0;
                let b1 = -2.0 * cos_w0;
                let b2 = 1.0;
                let a0 = 1.0 + alpha;
                let a1 = -2.0 * cos_w0;
                let a2 = 1.0 - alpha;
                (b0, b1, b2, a0, a1, a2)
            }
            EqBandType::LowCut12 | EqBandType::LowCut48 => {
                // 2nd-order Butterworth highpass
                let q_butter = 1.0 / (2.0_f32).sqrt();
                let alpha_butter = sin_w0 / (2.0 * q_butter);
                let b0 = (1.0 + cos_w0) * 0.5;
                let b1 = -(1.0 + cos_w0);
                let b2 = (1.0 + cos_w0) * 0.5;
                let a0 = 1.0 + alpha_butter;
                let a1 = -2.0 * cos_w0;
                let a2 = 1.0 - alpha_butter;
                (b0, b1, b2, a0, a1, a2)
            }
            EqBandType::HighCut12 | EqBandType::HighCut48 => {
                // 2nd-order Butterworth lowpass
                let q_butter = 1.0 / (2.0_f32).sqrt();
                let alpha_butter = sin_w0 / (2.0 * q_butter);
                let b0 = (1.0 - cos_w0) * 0.5;
                let b1 = 1.0 - cos_w0;
                let b2 = (1.0 - cos_w0) * 0.5;
                let a0 = 1.0 + alpha_butter;
                let a1 = -2.0 * cos_w0;
                let a2 = 1.0 - alpha_butter;
                (b0, b1, b2, a0, a1, a2)
            }
        };

        let inv_a0 = 1.0 / a0;
        Self {
            b0: b0 * inv_a0,
            b1: b1 * inv_a0,
            b2: b2 * inv_a0,
            a1: a1 * inv_a0,
            a2: a2 * inv_a0,
        }
    }

    /// Evaluates the complex magnitude response |H(e^{j*w})| at frequency f_hz.
    pub fn magnitude_response(&self, freq_hz: f32, sample_rate: f32) -> f32 {
        let sr = if sample_rate <= 0.0 { 44100.0 } else { sample_rate };
        let w = 2.0 * PI * (freq_hz.clamp(1.0, sr * 0.49)) / sr;
        let cos_w = w.cos();
        let sin_w = w.sin();
        let cos_2w = (2.0 * w).cos();
        let sin_2w = (2.0 * w).sin();

        let num_re = self.b0 + self.b1 * cos_w + self.b2 * cos_2w;
        let num_im = -self.b1 * sin_w - self.b2 * sin_2w;
        let den_re = 1.0 + self.a1 * cos_w + self.a2 * cos_2w;
        let den_im = -self.a1 * sin_w - self.a2 * sin_2w;

        let num_mag2 = num_re * num_re + num_im * num_im;
        let den_mag2 = den_re * den_re + den_im * den_im;

        if den_mag2 > 1e-12 {
            (num_mag2 / den_mag2).sqrt()
        } else {
            1.0
        }
    }
}

/// Single biquad filter cell maintaining Direct Form II Transposed state.
#[derive(Debug, Clone, Copy, Default)]
pub struct BiquadCell {
    pub s1: f32,
    pub s2: f32,
}

impl BiquadCell {
    pub const fn new() -> Self {
        Self { s1: 0.0, s2: 0.0 }
    }

    pub fn reset(&mut self) {
        self.s1 = 0.0;
        self.s2 = 0.0;
    }

    #[inline(always)]
    pub fn process(&mut self, x: f32, c: &BiquadCoeffs) -> f32 {
        let y = c.b0 * x + self.s1;
        self.s1 = c.b1 * x - c.a1 * y + self.s2;
        self.s2 = c.b2 * x - c.a2 * y;
        y
    }
}

/// Settings and filter state for an individual EQ band.
#[derive(Debug, Clone)]
pub struct EqBand {
    pub enabled: bool,
    pub band_type: EqBandType,
    pub freq_hz: f32,
    pub gain_db: f32,
    pub q: f32,
    coeffs: BiquadCoeffs,
    // 4 stages for 48dB slopes (or 1 stage for 12dB/bell/shelf/notch)
    cells_l: [BiquadCell; 4],
    cells_r: [BiquadCell; 4],
}

impl EqBand {
    pub fn new(band_type: EqBandType, freq_hz: f32, gain_db: f32, q: f32, sample_rate: f32) -> Self {
        let coeffs = BiquadCoeffs::calculate(band_type, freq_hz, gain_db, q, sample_rate);
        Self {
            enabled: true,
            band_type,
            freq_hz,
            gain_db,
            q,
            coeffs,
            cells_l: [BiquadCell::new(); 4],
            cells_r: [BiquadCell::new(); 4],
        }
    }

    pub fn recompute(&mut self, sample_rate: f32) {
        self.coeffs = BiquadCoeffs::calculate(self.band_type, self.freq_hz, self.gain_db, self.q, sample_rate);
    }

    pub fn reset(&mut self) {
        for c in &mut self.cells_l {
            c.reset();
        }
        for c in &mut self.cells_r {
            c.reset();
        }
    }

    #[inline(always)]
    pub fn process_sample(&mut self, l: f32, r: f32) -> (f32, f32) {
        if !self.enabled {
            return (l, r);
        }

        let stages = match self.band_type {
            EqBandType::LowCut48 | EqBandType::HighCut48 => 4,
            _ => 1,
        };

        let mut out_l = l;
        let mut out_r = r;

        for s in 0..stages {
            out_l = self.cells_l[s].process(out_l, &self.coeffs);
            out_r = self.cells_r[s].process(out_r, &self.coeffs);
        }

        (out_l, out_r)
    }

    pub fn magnitude_response(&self, freq: f32, sample_rate: f32) -> f32 {
        if !self.enabled {
            return 1.0;
        }
        let mag = self.coeffs.magnitude_response(freq, sample_rate);
        match self.band_type {
            EqBandType::LowCut48 | EqBandType::HighCut48 => mag * mag * mag * mag,
            _ => mag,
        }
    }
}

/// Live 12 Flagship EQ Eight Device.
pub struct EQEight {
    pub name: String,
    pub enabled: bool,
    pub stereo_mode: EqStereoMode,
    pub bands: [EqBand; 8],
    sample_rate: f32,
    spectrum_bins: [f32; 64],
}

impl EQEight {
    pub fn new(sample_rate: f32) -> Self {
        let sr = if sample_rate <= 0.0 { 44100.0 } else { sample_rate };
        // Standard Live 12 8-band defaults:
        // Band 1: Low Cut 48dB @ 30 Hz
        // Band 2: Low Shelf @ 100 Hz
        // Band 3: Bell @ 250 Hz
        // Band 4: Bell @ 600 Hz
        // Band 5: Bell @ 1.5 kHz
        // Band 6: Bell @ 4.0 kHz
        // Band 7: High Shelf @ 8.0 kHz
        // Band 8: High Cut 48dB @ 18.0 kHz
        let bands = [
            EqBand::new(EqBandType::LowCut48, 30.0, 0.0, 0.707, sr),
            EqBand::new(EqBandType::LowShelf, 100.0, 0.0, 0.707, sr),
            EqBand::new(EqBandType::Bell, 250.0, 0.0, 1.0, sr),
            EqBand::new(EqBandType::Bell, 600.0, 0.0, 1.0, sr),
            EqBand::new(EqBandType::Bell, 1500.0, 0.0, 1.0, sr),
            EqBand::new(EqBandType::Bell, 4000.0, 0.0, 1.0, sr),
            EqBand::new(EqBandType::HighShelf, 8000.0, 0.0, 0.707, sr),
            EqBand::new(EqBandType::HighCut48, 18000.0, 0.0, 0.707, sr),
        ];

        Self {
            name: "EQ Eight".to_string(),
            enabled: true,
            stereo_mode: EqStereoMode::Stereo,
            bands,
            sample_rate: sr,
            spectrum_bins: [0.0; 64],
        }
    }

    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        if sample_rate > 0.0 {
            self.sample_rate = sample_rate;
            for band in &mut self.bands {
                band.recompute(sample_rate);
            }
        }
    }

    /// Configure a specific band.
    pub fn configure_band(
        &mut self,
        band_idx: usize,
        band_type: EqBandType,
        freq_hz: f32,
        gain_db: f32,
        q: f32,
    ) {
        if band_idx < 8 {
            self.bands[band_idx].band_type = band_type;
            self.bands[band_idx].freq_hz = freq_hz;
            self.bands[band_idx].gain_db = gain_db;
            self.bands[band_idx].q = q;
            self.bands[band_idx].recompute(self.sample_rate);
        }
    }

    /// Set gain for a band.
    pub fn set_band_gain(&mut self, band_idx: usize, gain_db: f32) {
        if band_idx < 8 {
            self.bands[band_idx].gain_db = gain_db;
            self.bands[band_idx].recompute(self.sample_rate);
        }
    }

    /// Set frequency for a band.
    pub fn set_band_freq(&mut self, band_idx: usize, freq_hz: f32) {
        if band_idx < 8 {
            self.bands[band_idx].freq_hz = freq_hz;
            self.bands[band_idx].recompute(self.sample_rate);
        }
    }

    /// Returns the combined Bode magnitude response (in dB) at a specific frequency.
    pub fn bode_magnitude_db(&self, freq_hz: f32) -> f32 {
        let mut total_linear = 1.0f32;
        for band in &self.bands {
            total_linear *= band.magnitude_response(freq_hz, self.sample_rate);
        }
        20.0 * total_linear.max(1e-6).log10()
    }

    /// Returns the 64-bin real-time FFT spectrum analyzer levels in dB (-96..0 dBFS).
    pub fn spectrum_bins(&self) -> &[f32; 64] {
        &self.spectrum_bins
    }

    /// Update spectrum analyzer bins from an audio block using a 64-bin DFT filterbank.
    fn update_spectrum(&mut self, buffer: &AudioBuffer) {
        let count = buffer.frame_count.min(512);
        if count == 0 {
            return;
        }

        // 64 log-spaced frequencies from 20 Hz to 20 kHz
        for (bin_idx, bin) in self.spectrum_bins.iter_mut().enumerate() {
            let frac = bin_idx as f32 / 63.0;
            let freq = 20.0 * (1000.0_f32).powf(frac);
            let omega = 2.0 * PI * freq / self.sample_rate;

            let mut re = 0.0f32;
            let mut im = 0.0f32;
            for i in 0..count {
                let s = (buffer.left[i] + buffer.right[i]) * 0.5;
                let angle = omega * (i as f32);
                re += s * angle.cos();
                im -= s * angle.sin();
            }

            let mag = (re * re + im * im).sqrt() / (count as f32);
            let db = 20.0 * mag.max(1e-5).log10();
            // Smooth spectrum bins with 70% retention
            *bin = *bin * 0.7 + db * 0.3;
        }
    }
}

impl Default for EQEight {
    fn default() -> Self {
        Self::new(44100.0)
    }
}

impl Device for EQEight {
    fn name(&self) -> &str {
        &self.name
    }

    fn category(&self) -> DeviceCategory {
        DeviceCategory::AudioEffect
    }

    fn is_enabled(&self) -> bool {
        self.enabled
    }

    fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    fn reset(&mut self) {
        for band in &mut self.bands {
            band.reset();
        }
        self.spectrum_bins.fill(0.0);
    }
}

impl AudioEffectDevice for EQEight {
    fn process_audio(&mut self, buffer: &mut AudioBuffer, _time: &MusicalTime) {
        if !self.enabled {
            return;
        }

        let count = buffer.frame_count.min(1024);
        let sqrt2 = std::f32::consts::SQRT_2;

        match self.stereo_mode {
            EqStereoMode::Stereo => {
                for i in 0..count {
                    let mut l = buffer.left[i];
                    let mut r = buffer.right[i];
                    for band in &mut self.bands {
                        let (nl, nr) = band.process_sample(l, r);
                        l = nl;
                        r = nr;
                    }
                    buffer.left[i] = l;
                    buffer.right[i] = r;
                }
            }
            EqStereoMode::LeftRight => {
                // Bands 0..3 process Left, Bands 4..7 process Right
                for i in 0..count {
                    let mut l = buffer.left[i];
                    let mut r = buffer.right[i];
                    for b in 0..4 {
                        let (nl, _) = self.bands[b].process_sample(l, 0.0);
                        l = nl;
                    }
                    for b in 4..8 {
                        let (_, nr) = self.bands[b].process_sample(0.0, r);
                        r = nr;
                    }
                    buffer.left[i] = l;
                    buffer.right[i] = r;
                }
            }
            EqStereoMode::MidSide => {
                for i in 0..count {
                    let l = buffer.left[i];
                    let r = buffer.right[i];
                    // Matrix to Mid/Side
                    let mid = (l + r) / sqrt2;
                    let side = (l - r) / sqrt2;

                    // Bands 0..3 process Mid, Bands 4..7 process Side
                    let mut filtered_mid = mid;
                    let mut filtered_side = side;
                    for b in 0..4 {
                        let (nm, _) = self.bands[b].process_sample(filtered_mid, 0.0);
                        filtered_mid = nm;
                    }
                    for b in 4..8 {
                        let (_, ns) = self.bands[b].process_sample(0.0, filtered_side);
                        filtered_side = ns;
                    }

                    // Dematrix to L/R
                    buffer.left[i] = (filtered_mid + filtered_side) / sqrt2;
                    buffer.right[i] = (filtered_mid - filtered_side) / sqrt2;
                }
            }
        }

        self.update_spectrum(buffer);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_eq_eight_bell_boost_attenuation() {
        let mut eq = EQEight::new(44100.0);
        // Boost 1 kHz bell by +12 dB
        eq.configure_band(2, EqBandType::Bell, 1000.0, 12.0, 2.0);

        let bode_at_1k = eq.bode_magnitude_db(1000.0);
        assert!(bode_at_1k > 11.0 && bode_at_1k < 13.0, "Bode response at 1kHz must be ~12dB, got {}", bode_at_1k);

        // Process a 1kHz sine wave
        let mut buf = AudioBuffer::silence(256);
        for i in 0..256 {
            let s = (2.0 * PI * 1000.0 * (i as f32) / 44100.0).sin() * 0.1;
            buf.left[i] = s;
            buf.right[i] = s;
        }

        let time = MusicalTime::default();
        eq.process_audio(&mut buf, &time);

        let max_l = buf.left.iter().fold(0.0f32, |acc, &x| acc.max(x.abs()));
        // +12 dB = ~3.98x gain. 0.1 * 3.98 = ~0.398
        assert!(max_l > 0.25, "Boosted 1kHz sine must be amplified, peak: {}", max_l);
    }

    #[test]
    fn test_eq_eight_lowcut_48db() {
        let mut eq = EQEight::new(44100.0);
        // Turn off all bands except lowcut 48dB @ 200 Hz
        for i in 0..8 {
            eq.bands[i].enabled = false;
        }
        eq.bands[0].enabled = true;
        eq.configure_band(0, EqBandType::LowCut48, 200.0, 0.0, 0.707);

        // 50 Hz should be heavily attenuated
        let resp_50hz = eq.bode_magnitude_db(50.0);
        assert!(resp_50hz < -20.0, "50Hz through 200Hz 48dB lowcut must be attenuated >20dB, got {}", resp_50hz);

        // 1000 Hz should be nearly unattenuated
        let resp_1khz = eq.bode_magnitude_db(1000.0);
        assert!(resp_1khz > -1.0, "1kHz through 200Hz lowcut must have ~0dB attenuation, got {}", resp_1khz);
    }

    #[test]
    fn test_eq_eight_midside_processing() {
        let mut eq = EQEight::new(44100.0);
        eq.stereo_mode = EqStereoMode::MidSide;

        // Pure mono signal (Mid channel full, Side channel zero)
        let mut buf = AudioBuffer::silence(128);
        for i in 0..128 {
            buf.left[i] = 0.5;
            buf.right[i] = 0.5;
        }

        let time = MusicalTime::default();
        eq.process_audio(&mut buf, &time);

        // After processing, L and R must remain identical since input was mono
        for i in 0..128 {
            assert!((buf.left[i] - buf.right[i]).abs() < 1e-4);
        }
    }
}
