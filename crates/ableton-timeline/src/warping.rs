//! 6-Mode Audio Warping Engine (OWarper) with Transient Detection, Warp Markers,
//! and Pitch Transposition for Ableton Live 12.
//!
//! Implements all 6 Ableton warp algorithms:
//! 1. `Beats`: Transient slicing with loop modes (Off, Forward, Alternating) and decay envelope.
//! 2. `Tones`: Granular Synchronized Overlap-Add (SOLA) for monophonic signals.
//! 3. `Texture`: Granular time-stretching with stochastic grain flux modulation.
//! 4. `RePitch`: Variable speed-linked resampling (tape/vinyl vari-speed).
//! 5. `Complex`: Polyphonic phase vocoder with phase locking.
//! 6. `ComplexPro`: Phase vocoder with vocal formant & spectral envelope preservation.

use serde::{Deserialize, Serialize};
use std::f32::consts::PI;
use std::path::Path;

/// Ableton Live 12 Warping Modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WarpMode {
    Beats,
    Tones,
    Texture,
    RePitch,
    Complex,
    ComplexPro,
}

/// Slice loop modes for Beats warp algorithm.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransientLoopMode {
    Off,
    Forward,
    Alternating,
}

/// Piecewise linear warp marker anchoring musical beat to audio sample time.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WarpMarker {
    pub beat: f64,
    pub sample_time: f64, // In seconds
}

impl WarpMarker {
    pub const fn new(beat: f64, sample_time: f64) -> Self {
        Self { beat, sample_time }
    }
}

/// Configuration parameters for Beats mode.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BeatsWarpConfig {
    pub loop_mode: TransientLoopMode,
    pub envelope_decay: f32, // 0.0 .. 100.0 (100 = full sustain)
    pub preserve_attack_ms: f32, // 5.0 .. 50.0 ms (default 20.0 ms)
}

impl Default for BeatsWarpConfig {
    fn default() -> Self {
        Self {
            loop_mode: TransientLoopMode::Off,
            envelope_decay: 100.0,
            preserve_attack_ms: 20.0,
        }
    }
}

/// Configuration parameters for Tones mode.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TonesWarpConfig {
    pub grain_size_ms: f32, // 12.0 .. 100.0 ms (default 30.0 ms)
}

impl Default for TonesWarpConfig {
    fn default() -> Self {
        Self { grain_size_ms: 30.0 }
    }
}

/// Configuration parameters for Texture mode.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextureWarpConfig {
    pub grain_size_ms: f32, // 2.0 .. 260.0 ms (default 65.0 ms)
    pub flux: f32,          // 0.0 .. 1.0 (default 0.0)
}

impl Default for TextureWarpConfig {
    fn default() -> Self {
        Self {
            grain_size_ms: 65.0,
            flux: 0.25,
        }
    }
}

/// Configuration parameters for Complex Pro mode.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComplexProWarpConfig {
    pub formants: f32, // 0.0 .. 1.0 (1.0 = fully preserved formants)
    pub envelope: f32, // 0.0 .. 1.0 (envelope smoothing width, default 0.49)
}

impl Default for ComplexProWarpConfig {
    fn default() -> Self {
        Self {
            formants: 1.0,
            envelope: 0.49,
        }
    }
}

/// Comprehensive warping configuration for an audio clip.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WarperConfig {
    pub mode: WarpMode,
    pub pitch_coarse_st: i8,  // -48 .. +48 semitones
    pub pitch_fine_cents: i8, // -50 .. +50 cents
    pub gain_db: f32,         // -70.0 .. +12.0 dB
    pub beats_config: BeatsWarpConfig,
    pub tones_config: TonesWarpConfig,
    pub texture_config: TextureWarpConfig,
    pub complex_pro_config: ComplexProWarpConfig,
}

impl Default for WarperConfig {
    fn default() -> Self {
        Self {
            mode: WarpMode::Beats,
            pitch_coarse_st: 0,
            pitch_fine_cents: 0,
            gain_db: 0.0,
            beats_config: BeatsWarpConfig::default(),
            tones_config: TonesWarpConfig::default(),
            texture_config: TextureWarpConfig::default(),
            complex_pro_config: ComplexProWarpConfig::default(),
        }
    }
}

impl WarperConfig {
    /// Calculate the effective linear gain multiplier.
    pub fn linear_gain(&self) -> f32 {
        if self.gain_db <= -60.0 {
            0.0
        } else {
            10.0_f32.powf(self.gain_db.clamp(-70.0, 12.0) / 20.0)
        }
    }

    /// Calculate the total pitch transposition ratio rho.
    pub fn pitch_ratio(&self) -> f32 {
        let coarse = self.pitch_coarse_st.clamp(-48, 48) as f32;
        let fine = self.pitch_fine_cents.clamp(-50, 50) as f32 / 100.0;
        2.0_f32.powf((coarse + fine) / 12.0)
    }
}

// =========================================================================
// Radix-2 Cooley-Tukey FFT & IFFT (Zero External Dependencies)
// =========================================================================

/// In-place Radix-2 Fast Fourier Transform. `n` must be a power of 2.
pub fn fft(re: &mut [f32], im: &mut [f32]) {
    let n = re.len();
    assert!(n.is_power_of_two(), "FFT size must be a power of 2");
    assert_eq!(n, im.len(), "Real and imaginary lengths must match");

    // Bit-reversal permutation
    let mut j = 0;
    for i in 0..n - 1 {
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
        let mut k = n >> 1;
        while k <= j {
            j -= k;
            k >>= 1;
        }
        j += k;
    }

    // Cooley-Tukey butterflies
    let mut len = 2;
    while len <= n {
        let half = len >> 1;
        let angle = -2.0 * PI / (len as f32);
        let w_step_re = angle.cos();
        let w_step_im = angle.sin();

        let mut i = 0;
        while i < n {
            let mut w_re = 1.0f32;
            let mut w_im = 0.0f32;
            for k in 0..half {
                let u_re = re[i + k];
                let u_im = im[i + k];
                let v_re = re[i + k + half] * w_re - im[i + k + half] * w_im;
                let v_im = re[i + k + half] * w_im + im[i + k + half] * w_re;

                re[i + k] = u_re + v_re;
                im[i + k] = u_im + v_im;
                re[i + k + half] = u_re - v_re;
                im[i + k + half] = u_im - v_im;

                let next_w_re = w_re * w_step_re - w_im * w_step_im;
                let next_w_im = w_re * w_step_im + w_im * w_step_re;
                w_re = next_w_re;
                w_im = next_w_im;
            }
            i += len;
        }
        len <<= 1;
    }
}

/// In-place Inverse Radix-2 Fast Fourier Transform.
pub fn ifft(re: &mut [f32], im: &mut [f32]) {
    let n = re.len();
    assert!(n.is_power_of_two());
    assert_eq!(n, im.len());

    // Conjugate input
    for v in im.iter_mut() {
        *v = -*v;
    }

    fft(re, im);

    // Conjugate and normalize by 1/N
    let inv_n = 1.0 / (n as f32);
    for i in 0..n {
        re[i] *= inv_n;
        im[i] = -im[i] * inv_n;
    }
}

// =========================================================================
// Spectral Flux Transient Detection
// =========================================================================

/// Detect transient sample offsets using Spectral Flux & High-Frequency Content weighting.
pub fn detect_transients(audio: &[f32], channels: usize, sample_rate: f32) -> Vec<usize> {
    if audio.is_empty() || channels == 0 {
        return Vec::new();
    }

    let num_frames = audio.len() / channels;
    if num_frames < 1024 {
        return vec![0];
    }

    let fft_size = 1024;
    let hop_size = 256;
    let half_fft = fft_size / 2;

    // Hann window
    let mut window = vec![0.0f32; fft_size];
    for n in 0..fft_size {
        window[n] = 0.5 * (1.0 - (2.0 * PI * (n as f32) / (fft_size as f32)).cos());
    }

    let num_hops = (num_frames.saturating_sub(fft_size)) / hop_size;
    if num_hops < 4 {
        return vec![0];
    }

    let mut prev_mag = vec![0.0f32; half_fft];
    let mut hfc_flux = Vec::with_capacity(num_hops);

    let mut re = vec![0.0f32; fft_size];
    let mut im = vec![0.0f32; fft_size];

    for hop in 0..num_hops {
        let frame_offset = hop * hop_size;

        // Downmix to mono and apply Hann window
        for n in 0..fft_size {
            let idx = (frame_offset + n) * channels;
            let sample = if channels == 1 {
                audio[idx]
            } else {
                (audio[idx] + audio[idx + 1]) * 0.5
            };
            re[n] = sample * window[n];
            im[n] = 0.0;
        }

        fft(&mut re, &mut im);

        let mut flux = 0.0f32;
        for k in 0..half_fft {
            let mag = (re[k] * re[k] + im[k] * im[k]).sqrt();
            let diff = mag - prev_mag[k];
            if diff > 0.0 {
                // High frequency weighting
                flux += (k as f32 + 1.0) * diff;
            }
            prev_mag[k] = mag;
        }

        hfc_flux.push(flux);
    }

    // Adaptive peak picking with moving average and standard deviation
    let window_w = 7;
    let mut transients = Vec::new();
    transients.push(0); // Always anchor initial sample 0

    let min_interval_frames = (0.050 * sample_rate) as usize; // 50 ms minimum transient distance
    let mut last_transient_frame = 0;

    for m in window_w..hfc_flux.len().saturating_sub(window_w) {
        let val = hfc_flux[m];

        // Compute local mean and std dev
        let mut mean = 0.0f32;
        let mut count = 0.0f32;
        for j in m - window_w..=m + window_w {
            mean += hfc_flux[j];
            count += 1.0;
        }
        mean /= count;

        let mut var = 0.0f32;
        for j in m - window_w..=m + window_w {
            let diff = hfc_flux[j] - mean;
            var += diff * diff;
        }
        let std_dev = (var / count).sqrt();

        let threshold = mean + 1.2 * std_dev;

        // Is local maximum?
        let is_max = val > threshold
            && val >= hfc_flux[m - 1]
            && val >= hfc_flux[m - 2]
            && val >= hfc_flux[m + 1]
            && val >= hfc_flux[m + 2];

        if is_max {
            let sample_idx = m * hop_size;
            if sample_idx >= last_transient_frame + min_interval_frames {
                transients.push(sample_idx);
                last_transient_frame = sample_idx;
            }
        }
    }

    if transients.len() < 2 {
        // Fallback: regular 16th note divisions at default 120 bpm (125 ms intervals)
        let step = ((60.0 / 120.0 / 4.0) * sample_rate) as usize;
        let mut cur = step;
        while cur < num_frames {
            transients.push(cur);
            cur += step;
        }
    }

    transients
}

// =========================================================================
// Sinc & Hermite Cubic Resampling Helper
// =========================================================================

/// 4-point, 3rd-order Catmull-Rom cubic Hermite interpolation.
#[inline(always)]
pub fn interpolate_hermite_4pt(y0: f32, y1: f32, y2: f32, y3: f32, frac: f32) -> f32 {
    let c0 = y1;
    let c1 = 0.5 * (y2 - y0);
    let c2 = y0 - 2.5 * y1 + 2.0 * y2 - 0.5 * y3;
    let c3 = 0.5 * (y3 - y0) + 1.5 * (y1 - y2);
    ((c3 * frac + c2) * frac + c1) * frac + c0
}

/// Resample a multi-channel interleaved slice with pitch/speed ratio.
pub fn resample_slice(
    input: &[f32],
    channels: usize,
    speed_ratio: f64,
    target_len_frames: usize,
) -> Vec<f32> {
    if input.is_empty() || channels == 0 || target_len_frames == 0 {
        return Vec::new();
    }

    let input_frames = input.len() / channels;
    let mut output = vec![0.0f32; target_len_frames * channels];

    for f_out in 0..target_len_frames {
        let pos = (f_out as f64) * speed_ratio;
        let idx = pos.floor() as isize;
        let frac = (pos - idx as f64) as f32;

        for ch in 0..channels {
            let sample = |i: isize| -> f32 {
                let clamped = i.clamp(0, input_frames as isize - 1) as usize;
                input[clamped * channels + ch]
            };

            let y0 = sample(idx - 1);
            let y1 = sample(idx);
            let y2 = sample(idx + 1);
            let y3 = sample(idx + 2);

            let res = interpolate_hermite_4pt(y0, y1, y2, y3, frac);
            output[f_out * channels + ch] = if res.is_finite() { res } else { 0.0 };
        }
    }

    output
}

// =========================================================================
// OWarper Engine
// =========================================================================

/// Authoritative 6-Mode Audio Warping Engine (`OWarper`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OWarper {
    pub config: WarperConfig,
    pub sample_rate: f32,
    pub markers: Vec<WarpMarker>,
    pub transients: Vec<usize>,
}

impl OWarper {
    pub fn new(sample_rate: f32, config: WarperConfig) -> Self {
        let sr = if sample_rate <= 0.0 { 44100.0 } else { sample_rate };
        Self {
            config,
            sample_rate: sr,
            markers: Vec::new(),
            transients: Vec::new(),
        }
    }

    /// Add or update a warp marker. Monotonically sorted by beat.
    pub fn add_marker(&mut self, beat: f64, sample_time: f64) {
        if let Some(pos) = self.markers.iter_mut().find(|m| (m.beat - beat).abs() < 1e-6) {
            pos.sample_time = sample_time;
        } else {
            let idx = self.markers.partition_point(|m| m.beat < beat);
            self.markers.insert(idx, WarpMarker::new(beat, sample_time));
        }
    }

    /// Clear all warp markers.
    pub fn clear_markers(&mut self) {
        self.markers.clear();
    }

    /// Automatically analyze audio and set default markers and transient points.
    pub fn analyze_and_init(&mut self, audio: &[f32], channels: usize, default_bpm: f64) {
        self.transients = detect_transients(audio, channels, self.sample_rate);

        let total_frames = audio.len() / channels.max(1);
        let total_sec = total_frames as f64 / self.sample_rate as f64;
        let total_beats = total_sec * (default_bpm / 60.0);

        self.markers.clear();
        self.markers.push(WarpMarker::new(0.0, 0.0));
        self.markers.push(WarpMarker::new(total_beats, total_sec));
    }

    /// Map timeline beat to audio sample time in seconds via piecewise linear interpolation.
    pub fn beat_to_sample_time(&self, beat: f64) -> f64 {
        if self.markers.is_empty() {
            return beat * (60.0 / 120.0);
        }
        if self.markers.len() == 1 || beat <= self.markers[0].beat {
            let m = &self.markers[0];
            return m.sample_time + (beat - m.beat) * (60.0 / 120.0);
        }
        let last = self.markers.len() - 1;
        if beat >= self.markers[last].beat {
            let m0 = &self.markers[last - 1];
            let m1 = &self.markers[last];
            let d_beat = (m1.beat - m0.beat).max(1e-6);
            let slope = (m1.sample_time - m0.sample_time) / d_beat;
            return m1.sample_time + (beat - m1.beat) * slope;
        }

        let next_idx = self.markers.partition_point(|m| m.beat <= beat);
        let prev_idx = next_idx.saturating_sub(1);
        let m0 = &self.markers[prev_idx];
        let m1 = &self.markers[next_idx];
        let frac = (beat - m0.beat) / (m1.beat - m0.beat).max(1e-6);
        m0.sample_time + frac * (m1.sample_time - m0.sample_time)
    }

    /// Map audio sample time in seconds to timeline beat.
    pub fn sample_time_to_beat(&self, time_sec: f64) -> f64 {
        if self.markers.is_empty() {
            return time_sec * (120.0 / 60.0);
        }
        if self.markers.len() == 1 || time_sec <= self.markers[0].sample_time {
            let m = &self.markers[0];
            return m.beat + (time_sec - m.sample_time) * (120.0 / 60.0);
        }
        let last = self.markers.len() - 1;
        if time_sec >= self.markers[last].sample_time {
            let m0 = &self.markers[last - 1];
            let m1 = &self.markers[last];
            let d_time = (m1.sample_time - m0.sample_time).max(1e-6);
            let slope = (m1.beat - m0.beat) / d_time;
            return m1.beat + (time_sec - m1.sample_time) * slope;
        }

        let next_idx = self.markers.partition_point(|m| m.sample_time <= time_sec);
        let prev_idx = next_idx.saturating_sub(1);
        let m0 = &self.markers[prev_idx];
        let m1 = &self.markers[next_idx];
        let frac = (time_sec - m0.sample_time) / (m1.sample_time - m0.sample_time).max(1e-6);
        m0.beat + frac * (m1.beat - m0.beat)
    }

    /// Process time-stretch and pitch-shift across a whole audio clip or buffer.
    ///
    /// - `input`: Interleaved stereo or mono PCM samples.
    /// - `channels`: Channel count (1 or 2).
    /// - `stretch_ratio`: Time-stretch factor (0.5 = double speed / half duration, 2.0 = half speed / double duration).
    /// - `pitch_semitones`: Pitch shift in semitones (e.g. -12.0 .. +12.0, or from config).
    ///
    /// Guarantees strictly finite, non-NaN audio output.
    pub fn process_stretch(
        &self,
        input: &[f32],
        channels: usize,
        stretch_ratio: f64,
        pitch_semitones: f32,
    ) -> Vec<f32> {
        if input.is_empty() || channels == 0 {
            return Vec::new();
        }

        let stretch = stretch_ratio.clamp(0.1, 10.0);
        let pitch_ratio = 2.0_f32.powf(pitch_semitones / 12.0);
        let gain = self.config.linear_gain();

        if gain == 0.0 {
            let out_frames = ((input.len() / channels) as f64 * stretch).round() as usize;
            return vec![0.0f32; out_frames * channels];
        }

        let mut output = match self.config.mode {
            WarpMode::Beats => self.process_beats(input, channels, stretch, pitch_ratio),
            WarpMode::Tones => self.process_tones(input, channels, stretch, pitch_ratio),
            WarpMode::Texture => self.process_texture(input, channels, stretch, pitch_ratio),
            WarpMode::RePitch => self.process_repitch(input, channels, stretch, pitch_ratio),
            WarpMode::Complex => self.process_complex(input, channels, stretch, pitch_ratio, false),
            WarpMode::ComplexPro => self.process_complex(input, channels, stretch, pitch_ratio, true),
        };

        // Apply clip gain and guarantee finite, non-NaN values
        for sample in &mut output {
            if sample.is_nan() || sample.is_infinite() {
                *sample = 0.0;
            } else {
                *sample = (*sample * gain).clamp(-1.0, 1.0);
            }
        }

        output
    }

    // =====================================================================
    // 1. Beats Mode DSP
    // =====================================================================
    fn process_beats(
        &self,
        input: &[f32],
        channels: usize,
        stretch: f64,
        pitch_ratio: f32,
    ) -> Vec<f32> {
        let in_frames = input.len() / channels;
        let out_frames = (in_frames as f64 * stretch).round().max(1.0) as usize;
        let mut output = vec![0.0f32; out_frames * channels];

        // Ensure we have transient slices
        let mut slice_points = self.transients.clone();
        if slice_points.is_empty() || slice_points[0] != 0 {
            slice_points.insert(0, 0);
        }
        if *slice_points.last().unwrap() < in_frames {
            slice_points.push(in_frames);
        }

        let decay = self.config.beats_config.envelope_decay / 100.0;
        let loop_mode = self.config.beats_config.loop_mode;
        let attack_frames = ((self.config.beats_config.preserve_attack_ms / 1000.0)
            * self.sample_rate) as usize;

        for s_idx in 0..slice_points.len() - 1 {
            let start_in = slice_points[s_idx];
            let end_in = slice_points[s_idx + 1].min(in_frames);
            let orig_len = end_in.saturating_sub(start_in);
            if orig_len == 0 {
                continue;
            }

            let start_out = (start_in as f64 * stretch).round() as usize;
            let end_out = (end_in as f64 * stretch).round().min(out_frames as f64) as usize;
            let target_len = end_out.saturating_sub(start_out);
            if target_len == 0 {
                continue;
            }

            let slice_in = &input[start_in * channels..end_in * channels];

            // Resample slice for pitch transposition
            let resampled = if (pitch_ratio - 1.0).abs() > 1e-4 {
                let resample_len = (orig_len as f32 / pitch_ratio).round().max(1.0) as usize;
                resample_slice(slice_in, channels, pitch_ratio as f64, resample_len)
            } else {
                slice_in.to_vec()
            };
            let res_frames = resampled.len() / channels;

            // Render slice to target length based on LoopMode and Envelope
            for f in 0..target_len {
                let out_f = start_out + f;
                if out_f >= out_frames {
                    break;
                }

                // Envelope decay multiplier
                let env = if decay >= 0.999 {
                    1.0f32
                } else {
                    let tau = (decay.max(0.01) * target_len as f32).max(1.0);
                    (- (f as f32) / tau).exp()
                };

                let src_f = if f < attack_frames && f < res_frames {
                    // Attack section: 1.0x playback
                    f
                } else {
                    match loop_mode {
                        TransientLoopMode::Off => {
                            if f < res_frames {
                                f
                            } else {
                                // Silence tail
                                continue;
                            }
                        }
                        TransientLoopMode::Forward => {
                            let tail_start = attack_frames.min(res_frames.saturating_sub(1));
                            let tail_len = res_frames.saturating_sub(tail_start).max(1);
                            tail_start + ((f - attack_frames) % tail_len)
                        }
                        TransientLoopMode::Alternating => {
                            let tail_start = attack_frames.min(res_frames.saturating_sub(1));
                            let tail_len = res_frames.saturating_sub(tail_start).max(1);
                            let period = tail_len * 2;
                            let phase = (f - attack_frames) % period;
                            if phase < tail_len {
                                tail_start + phase
                            } else {
                                tail_start + (period - 1 - phase)
                            }
                        }
                    }
                };

                for ch in 0..channels {
                    let s = resampled[src_f * channels + ch] * env;
                    output[out_f * channels + ch] += s;
                }
            }
        }

        output
    }

    // =====================================================================
    // 2. Tones Mode DSP (Granular SOLA)
    // =====================================================================
    fn process_tones(
        &self,
        input: &[f32],
        channels: usize,
        stretch: f64,
        pitch_ratio: f32,
    ) -> Vec<f32> {
        let grain_ms = self.config.tones_config.grain_size_ms.clamp(12.0, 100.0);
        let grain_len = ((grain_ms / 1000.0) * self.sample_rate).round() as usize & !1;
        let grain_len = grain_len.clamp(64, 4096);
        let hop_a = grain_len / 2;
        let hop_s = ((hop_a as f64) * stretch).round().max(1.0) as usize;

        let in_frames = input.len() / channels;
        let out_frames = (in_frames as f64 * stretch).round().max(1.0) as usize;
        let mut output = vec![0.0f32; (out_frames + grain_len) * channels];
        let mut norm = vec![0.0f32; out_frames + grain_len];

        // Hann window
        let mut win = vec![0.0f32; grain_len];
        for n in 0..grain_len {
            win[n] = 0.5 * (1.0 - (2.0 * PI * (n as f32) / (grain_len as f32)).cos());
        }

        let mut in_pos = 0usize;
        let mut out_pos = 0usize;

        while in_pos + grain_len <= in_frames && out_pos < out_frames {
            let slice = &input[in_pos * channels..(in_pos + grain_len) * channels];

            // Resample for pitch shifting
            let resampled = if (pitch_ratio - 1.0).abs() > 1e-4 {
                let target_l = (grain_len as f32 / pitch_ratio).round().max(1.0) as usize;
                resample_slice(slice, channels, pitch_ratio as f64, target_l)
            } else {
                slice.to_vec()
            };
            let res_len = (resampled.len() / channels).min(grain_len);

            // Cross-correlation search for optimal phase overlap alignment
            let search_w = (grain_len / 8).min(32);
            let mut best_offset = 0isize;
            let mut max_corr = -1e9f32;

            if out_pos > 0 {
                for offset in -(search_w as isize)..=(search_w as isize) {
                    let mut corr = 0.0f32;
                    for k in 0..(grain_len / 4).min(res_len) {
                        let cur_out = (out_pos as isize + offset + k as isize).max(0) as usize;
                        if cur_out < out_frames {
                            let y_prev = output[cur_out * channels];
                            let x_curr = resampled[k * channels];
                            corr += y_prev * x_curr;
                        }
                    }
                    if corr > max_corr {
                        max_corr = corr;
                        best_offset = offset;
                    }
                }
            }

            let eff_out = (out_pos as isize + best_offset).max(0) as usize;

            for n in 0..res_len {
                let w = if n < grain_len { win[n] } else { 0.0 };
                let o_idx = eff_out + n;
                if o_idx < output.len() / channels {
                    norm[o_idx] += w;
                    for ch in 0..channels {
                        output[o_idx * channels + ch] += resampled[n * channels + ch] * w;
                    }
                }
            }

            in_pos += hop_a;
            out_pos += hop_s;
        }

        // Normalize overlap-add
        for f in 0..out_frames {
            let n = norm[f];
            if n > 1e-4 {
                for ch in 0..channels {
                    output[f * channels + ch] /= n;
                }
            }
        }

        output.truncate(out_frames * channels);
        output
    }

    // =====================================================================
    // 3. Texture Mode DSP (Stochastic Grain Flux)
    // =====================================================================
    fn process_texture(
        &self,
        input: &[f32],
        channels: usize,
        stretch: f64,
        pitch_ratio: f32,
    ) -> Vec<f32> {
        let grain_ms = self.config.texture_config.grain_size_ms.clamp(2.0, 260.0);
        let grain_len = ((grain_ms / 1000.0) * self.sample_rate).round() as usize & !1;
        let grain_len = grain_len.clamp(32, 8192);
        let flux = self.config.texture_config.flux.clamp(0.0, 1.0);

        let hop_a = grain_len / 2;
        let hop_s = ((hop_a as f64) * stretch).round().max(1.0) as usize;

        let in_frames = input.len() / channels;
        let out_frames = (in_frames as f64 * stretch).round().max(1.0) as usize;
        let mut output = vec![0.0f32; (out_frames + grain_len) * channels];
        let mut norm = vec![0.0f32; out_frames + grain_len];

        let mut win = vec![0.0f32; grain_len];
        for n in 0..grain_len {
            win[n] = 0.5 * (1.0 - (2.0 * PI * (n as f32) / (grain_len as f32)).cos());
        }

        let mut out_pos = 0usize;
        let mut step_count = 0usize;

        while out_pos < out_frames {
            let nominal_in = ((out_pos as f64) / stretch).round() as usize;

            // Deterministic pseudo-random jitter based on step_count
            let pseudo_rnd1 = ((step_count * 1103515245 + 12345) & 0x7fffffff) as f32 / 2147483648.0;
            let pseudo_rnd2 = ((step_count * 214013 + 2531011) & 0x7fffffff) as f32 / 2147483648.0;
            let jitter_frames = ((pseudo_rnd1 * 2.0 - 1.0) * flux * (grain_len as f32 * 0.5)) as isize;
            let gain_flux = 1.0 + 0.3 * flux * (pseudo_rnd2 - 0.5);

            let actual_in = (nominal_in as isize + jitter_frames).clamp(0, in_frames.saturating_sub(grain_len) as isize) as usize;

            if actual_in + grain_len <= in_frames {
                let slice = &input[actual_in * channels..(actual_in + grain_len) * channels];

                let resampled = if (pitch_ratio - 1.0).abs() > 1e-4 {
                    let target_l = (grain_len as f32 / pitch_ratio).round().max(1.0) as usize;
                    resample_slice(slice, channels, pitch_ratio as f64, target_l)
                } else {
                    slice.to_vec()
                };
                let res_len = (resampled.len() / channels).min(grain_len);

                for n in 0..res_len {
                    let w = win[n];
                    let o_idx = out_pos + n;
                    if o_idx < output.len() / channels {
                        norm[o_idx] += w;
                        for ch in 0..channels {
                            output[o_idx * channels + ch] += resampled[n * channels + ch] * w * gain_flux;
                        }
                    }
                }
            }

            out_pos += hop_s;
            step_count += 1;
        }

        for f in 0..out_frames {
            let n = norm[f];
            if n > 1e-4 {
                for ch in 0..channels {
                    output[f * channels + ch] /= n;
                }
            }
        }

        output.truncate(out_frames * channels);
        output
    }

    // =====================================================================
    // 4. RePitch Mode DSP (Vari-Speed Tape Resampling)
    // =====================================================================
    fn process_repitch(
        &self,
        input: &[f32],
        channels: usize,
        stretch: f64,
        pitch_ratio: f32,
    ) -> Vec<f32> {
        let speed = (1.0 / stretch) * (pitch_ratio as f64);
        let out_frames = ((input.len() / channels) as f64 * stretch).round().max(1.0) as usize;
        resample_slice(input, channels, speed, out_frames)
    }

    // =====================================================================
    // 5 & 6. Complex & ComplexPro Mode DSP (Phase Vocoder)
    // =====================================================================
    fn process_complex(
        &self,
        input: &[f32],
        channels: usize,
        stretch: f64,
        pitch_ratio: f32,
        is_pro: bool,
    ) -> Vec<f32> {
        let fft_size = 1024usize;
        let hop_a = 256usize;
        let hop_s = ((hop_a as f64) * stretch).round().max(1.0) as usize;

        let in_frames = input.len() / channels;
        let out_frames = (in_frames as f64 * stretch).round().max(1.0) as usize;
        let mut output = vec![0.0f32; (out_frames + fft_size) * channels];
        let mut norm = vec![0.0f32; out_frames + fft_size];

        let mut win = vec![0.0f32; fft_size];
        for n in 0..fft_size {
            win[n] = 0.5 * (1.0 - (2.0 * PI * (n as f32) / (fft_size as f32)).cos());
        }

        let num_hops = in_frames.saturating_sub(fft_size) / hop_a;
        if num_hops == 0 {
            return self.process_repitch(input, channels, stretch, pitch_ratio);
        }

        let half_fft = fft_size / 2;
        let mut last_phase = vec![vec![0.0f32; half_fft + 1]; channels];
        let mut synth_phase = vec![vec![0.0f32; half_fft + 1]; channels];

        let mut re = vec![0.0f32; fft_size];
        let mut im = vec![0.0f32; fft_size];
        let mut mag = vec![0.0f32; half_fft + 1];
        let mut phase = vec![0.0f32; half_fft + 1];

        let formants_preserved = if is_pro { self.config.complex_pro_config.formants } else { 0.0 };
        let env_width = if is_pro { self.config.complex_pro_config.envelope } else { 0.5 };

        for h in 0..num_hops {
            let in_pos = h * hop_a;
            let out_pos = h * hop_s;

            for ch in 0..channels {
                for n in 0..fft_size {
                    re[n] = input[(in_pos + n) * channels + ch] * win[n];
                    im[n] = 0.0;
                }

                fft(&mut re, &mut im);

                for k in 0..=half_fft {
                    mag[k] = (re[k] * re[k] + im[k] * im[k]).sqrt();
                    phase[k] = im[k].atan2(re[k]);
                }

                // Complex Pro: Extract spectral envelope via moving average
                let mut spectral_envelope = vec![1.0f32; half_fft + 1];
                if is_pro && formants_preserved > 0.0 {
                    let smooth_radius = (12.0 * (1.0 - 0.5 * env_width)).round().max(2.0) as usize;
                    for k in 0..=half_fft {
                        let k_min = k.saturating_sub(smooth_radius);
                        let k_max = (k + smooth_radius).min(half_fft);
                        let sum: f32 = mag[k_min..=k_max].iter().sum();
                        spectral_envelope[k] = (sum / (k_max - k_min + 1) as f32).max(1e-6);
                    }
                }

                // Phase Vocoder Phase Accumulation with Instantaneous Frequency
                let mut synth_mag = vec![0.0f32; half_fft + 1];
                let mut synth_p = vec![0.0f32; half_fft + 1];

                for k in 0..=half_fft {
                    let expected_omega = 2.0 * PI * (k as f32) / (fft_size as f32);
                    let delta_phi = phase[k] - last_phase[ch][k] - expected_omega * (hop_a as f32);

                    // Principal argument wrapping to [-PI, PI]
                    let wrapped = ((delta_phi + PI) % (2.0 * PI) + 2.0 * PI) % (2.0 * PI) - PI;
                    let inst_freq = expected_omega + wrapped / (hop_a as f32);

                    last_phase[ch][k] = phase[k];

                    // Synthesis bin mapping with pitch ratio
                    let target_k_f = (k as f32) * pitch_ratio;
                    let target_k = target_k_f.round() as usize;

                    if target_k <= half_fft {
                        synth_phase[ch][target_k] += (hop_s as f32) * inst_freq * pitch_ratio;
                        synth_p[target_k] = synth_phase[ch][target_k];

                        let mut m = mag[k];
                        if is_pro && formants_preserved > 0.0 {
                            // Preserve formant envelope
                            let orig_env = spectral_envelope[k];
                            let target_env = spectral_envelope[target_k];
                            let excitation = m / orig_env;
                            let adapted_env = orig_env * formants_preserved + target_env * (1.0 - formants_preserved);
                            m = excitation * adapted_env;
                        }
                        synth_mag[target_k] += m;
                    }
                }

                // Reconstruct spectrum
                re[0] = synth_mag[0] * synth_p[0].cos();
                im[0] = 0.0;
                re[half_fft] = synth_mag[half_fft] * synth_p[half_fft].cos();
                im[half_fft] = 0.0;

                for k in 1..half_fft {
                    re[k] = synth_mag[k] * synth_p[k].cos();
                    im[k] = synth_mag[k] * synth_p[k].sin();
                    re[fft_size - k] = re[k];
                    im[fft_size - k] = -im[k];
                }

                ifft(&mut re, &mut im);

                for n in 0..fft_size {
                    let o_idx = out_pos + n;
                    if o_idx < output.len() / channels {
                        let sample = re[n] * win[n];
                        output[o_idx * channels + ch] += sample;
                        if ch == 0 {
                            norm[o_idx] += win[n] * win[n];
                        }
                    }
                }
            }
        }

        // Normalize
        for f in 0..out_frames {
            let n = norm[f];
            if n > 1e-4 {
                for ch in 0..channels {
                    output[f * channels + ch] /= n;
                }
            }
        }

        output.truncate(out_frames * channels);
        output
    }
}

// =========================================================================
// WarpedAudioClip: High-Level Clip Wrapper
// =========================================================================

/// High-level audio clip capable of non-destructive 6-mode warping and pitch shift.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WarpedAudioClip {
    pub name: String,
    pub sample_rate: u32,
    pub channels: u16,
    pub length_beats: f64,
    pub is_warped: bool,
    pub groove_id: Option<u32>,
    pub warper: OWarper,
    #[serde(skip)]
    pub raw_samples: Vec<f32>,
}

impl WarpedAudioClip {
    pub fn new(
        name: impl Into<String>,
        sample_rate: u32,
        channels: u16,
        samples: Vec<f32>,
        bpm: f64,
    ) -> Self {
        let sr = sample_rate as f32;
        let mut warper = OWarper::new(sr, WarperConfig::default());
        warper.analyze_and_init(&samples, channels as usize, bpm);

        let total_frames = samples.len() / (channels as usize).max(1);
        let total_sec = total_frames as f64 / sr as f64;
        let length_beats = total_sec * (bpm / 60.0);

        Self {
            name: name.into(),
            sample_rate,
            channels,
            length_beats,
            is_warped: true,
            groove_id: None,
            warper,
            raw_samples: samples,
        }
    }

    /// Load from a RIFF WAV byte array.
    pub fn from_wav_bytes(name: impl Into<String>, bytes: &[u8], bpm: f64) -> Result<Self, String> {
        let (sample_rate, channels, samples) = decode_wav_pcm(bytes)?;
        Ok(Self::new(name, sample_rate, channels, samples, bpm))
    }

    /// Load from a WAV file on disk.
    pub fn from_wav_file<P: AsRef<Path>>(path: P, bpm: f64) -> Result<Self, String> {
        let bytes = std::fs::read(path.as_ref())
            .map_err(|e| format!("Failed to read WAV file: {}", e))?;
        let name = path.as_ref().file_stem().and_then(|s| s.to_str()).unwrap_or("AudioClip");
        Self::from_wav_bytes(name, &bytes, bpm)
    }

    /// Render warped audio at project BPM.
    pub fn render_at_bpm(&self, project_bpm: f64) -> Vec<f32> {
        if !self.is_warped {
            return self.raw_samples.clone();
        }
        let clip_bpm = (self.length_beats / (self.raw_samples.len() as f64 / (self.channels as usize * self.sample_rate as usize) as f64)) * 60.0;
        let stretch = (clip_bpm / project_bpm.max(20.0)).clamp(0.1, 10.0);
        let pitch_st = self.warper.config.pitch_coarse_st as f32 + self.warper.config.pitch_fine_cents as f32 / 100.0;
        self.warper.process_stretch(&self.raw_samples, self.channels as usize, stretch, pitch_st)
    }
}

// =========================================================================
// Lightweight RIFF WAV PCM Decoder Helper
// =========================================================================

pub fn decode_wav_pcm(bytes: &[u8]) -> Result<(u32, u16, Vec<f32>), String> {
    if bytes.len() < 44 {
        return Err("WAV byte slice too short".into());
    }
    if &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err("Invalid RIFF/WAVE header magic".into());
    }

    let mut pos = 12;
    let mut sample_rate = 44100u32;
    let mut channels = 2u16;
    let mut bits_per_sample = 16u16;
    let mut data_offset = 0;
    let mut data_len = 0;
    let mut is_float = false;

    while pos + 8 <= bytes.len() {
        let chunk_id = &bytes[pos..pos + 4];
        let chunk_size = u32::from_le_bytes(bytes[pos + 4..pos + 8].try_into().unwrap()) as usize;
        pos += 8;

        if chunk_id == b"fmt " {
            if chunk_size < 16 || pos + 16 > bytes.len() {
                return Err("Corrupt fmt chunk in WAV".into());
            }
            let audio_format = u16::from_le_bytes(bytes[pos..pos + 2].try_into().unwrap());
            is_float = audio_format == 3;
            channels = u16::from_le_bytes(bytes[pos + 2..pos + 4].try_into().unwrap());
            sample_rate = u32::from_le_bytes(bytes[pos + 4..pos + 8].try_into().unwrap());
            bits_per_sample = u16::from_le_bytes(bytes[pos + 14..pos + 16].try_into().unwrap());
            pos += chunk_size;
        } else if chunk_id == b"data" {
            data_offset = pos;
            data_len = chunk_size.min(bytes.len().saturating_sub(pos));
            break;
        } else {
            pos += chunk_size;
        }
    }

    if data_offset == 0 || data_len == 0 {
        return Err("No data chunk found in WAV".into());
    }

    let bytes_per_sample = (bits_per_sample / 8) as usize;
    if bytes_per_sample == 0 {
        return Err("Invalid bits per sample".into());
    }

    let data = &bytes[data_offset..data_offset + data_len];
    let mut samples = Vec::with_capacity(data_len / bytes_per_sample);

    if bytes_per_sample == 2 {
        for chunk in data.chunks_exact(2) {
            let s = i16::from_le_bytes(chunk.try_into().unwrap());
            samples.push(s as f32 / 32768.0);
        }
    } else if bytes_per_sample == 3 {
        for chunk in data.chunks_exact(3) {
            let s = (chunk[0] as i32) | ((chunk[1] as i32) << 8) | ((chunk[2] as i8 as i32) << 16);
            samples.push(s as f32 / 8388608.0);
        }
    } else if bytes_per_sample == 4 {
        if is_float {
            for chunk in data.chunks_exact(4) {
                samples.push(f32::from_le_bytes(chunk.try_into().unwrap()));
            }
        } else {
            for chunk in data.chunks_exact(4) {
                let s = i32::from_le_bytes(chunk.try_into().unwrap());
                samples.push(s as f32 / 2147483648.0);
            }
        }
    } else {
        return Err(format!("Unsupported bits_per_sample: {}", bits_per_sample));
    }

    Ok((sample_rate, channels, samples))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fft_ifft_roundtrip() {
        let n = 256;
        let mut re = vec![0.0f32; n];
        let mut im = vec![0.0f32; n];

        // Synthesize test pulse
        for i in 0..n {
            re[i] = (2.0 * PI * 5.0 * (i as f32) / (n as f32)).sin();
        }
        let original = re.clone();

        fft(&mut re, &mut im);
        ifft(&mut re, &mut im);

        for i in 0..n {
            assert!((re[i] - original[i]).abs() < 1e-4, "FFT/IFFT roundtrip error at {}", i);
        }
    }

    #[test]
    fn test_all_6_warp_modes_finite_and_stretched() {
        let sr = 44100.0;
        let num_samples = 44100; // 1 second of stereo 440Hz sine
        let mut test_audio = vec![0.0f32; num_samples * 2];
        for i in 0..num_samples {
            let s = (2.0 * PI * 440.0 * (i as f32) / sr).sin() * 0.5;
            test_audio[i * 2] = s;
            test_audio[i * 2 + 1] = s;
        }

        let modes = [
            WarpMode::Beats,
            WarpMode::Tones,
            WarpMode::Texture,
            WarpMode::RePitch,
            WarpMode::Complex,
            WarpMode::ComplexPro,
        ];

        for &mode in &modes {
            let mut config = WarperConfig::default();
            config.mode = mode;
            let mut warper = OWarper::new(sr, config);
            warper.analyze_and_init(&test_audio, 2, 120.0);

            // Test 0.5x stretch (double speed)
            let out_half = warper.process_stretch(&test_audio, 2, 0.5, 0.0);
            assert!(!out_half.is_empty());
            for s in &out_half {
                assert!(s.is_finite(), "Mode {:?} produced non-finite value at 0.5x", mode);
                assert!(!s.is_nan(), "Mode {:?} produced NaN at 0.5x", mode);
            }

            // Test 2.0x stretch (half speed)
            let out_double = warper.process_stretch(&test_audio, 2, 2.0, 0.0);
            assert!(!out_double.is_empty());
            for s in &out_double {
                assert!(s.is_finite(), "Mode {:?} produced non-finite value at 2.0x", mode);
                assert!(!s.is_nan(), "Mode {:?} produced NaN at 2.0x", mode);
            }

            // Test pitch shifting +7 and -5 semitones
            let out_pitched = warper.process_stretch(&test_audio, 2, 1.0, 7.0);
            for s in &out_pitched {
                assert!(s.is_finite(), "Mode {:?} produced non-finite value at +7 st", mode);
            }
        }
    }

    #[test]
    fn test_warp_markers_interpolation() {
        let mut warper = OWarper::new(44100.0, WarperConfig::default());
        warper.add_marker(0.0, 0.0);
        warper.add_marker(4.0, 2.0); // 120 BPM segment
        warper.add_marker(8.0, 5.0); // 80 BPM segment

        assert_eq!(warper.beat_to_sample_time(0.0), 0.0);
        assert_eq!(warper.beat_to_sample_time(2.0), 1.0);
        assert_eq!(warper.beat_to_sample_time(4.0), 2.0);
        assert_eq!(warper.beat_to_sample_time(6.0), 3.5);
        assert_eq!(warper.beat_to_sample_time(8.0), 5.0);

        assert_eq!(warper.sample_time_to_beat(1.0), 2.0);
        assert_eq!(warper.sample_time_to_beat(3.5), 6.0);
    }
}
