//! Spectral Phase-Vocoder & Formant-Preserving Audio Warp Engine.
//!
//! Implements Ableton Live 12.4.6 Suite warp modes:
//! - `RePitch`: Cubic Hermite resampling coupling playback rate and pitch.
//! - `Beats`: Transient-preserving granular windowing at 16th-note boundaries.
//! - `Tones`: Overlapping Hann-windowed pitch-synchronous overlap-add (PSOLA).
//! - `Texture`: Stochastic granular cloud with PRNG flux jitter.
//! - `Complex`: STFT phase-vocoder with Laroche-Dolson phase locking.
//! - `ComplexPro`: Formant-preserving spectral envelope liftering/decomposition.
//! - `StreamingDiskCache`: Pre-allocated double-buffered 16384-sample disk streaming cache.

use std::f32::consts::PI;

pub const FFT_SIZE: usize = 2048;
pub const HOP_SIZE: usize = 512; // 75% overlap (4x oversampled)
pub const PAGE_SIZE: usize = 16384;
pub const MAX_WARP_BUFFER: usize = 8192;

/// Supported Ableton Live warp modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WarpMode {
    Beats,
    Tones,
    Texture,
    RePitch,
    Complex,
    ComplexPro,
}

/// Loop behavior for slice tails in Beats mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BeatsLoopMode {
    Off,
    Forward,
    Alternate,
}

/// Parameters for Complex Pro formant-preserving mode.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ComplexProParams {
    /// Formant preservation factor: 0.0 = standard pitch shift, 1.0 = full formant retention.
    pub formants_envelope: f32,
    /// Spectral envelope filter order (8.0 .. 256.0 bins).
    pub envelope_order: f32,
}

impl Default for ComplexProParams {
    fn default() -> Self {
        Self {
            formants_envelope: 1.0,
            envelope_order: 64.0,
        }
    }
}

impl ComplexProParams {
    pub fn new(formants_envelope: f32, envelope_order: f32) -> Self {
        Self {
            formants_envelope: formants_envelope.clamp(0.0, 1.0),
            envelope_order: envelope_order.clamp(8.0, 256.0),
        }
    }
}

/// Parameters for Texture granular mode.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextureParams {
    /// Grain duration in milliseconds (2.0 .. 500.0 ms).
    pub grain_size_ms: f32,
    /// Stochastic position jitter / flux factor (0.0 .. 1.0).
    pub flux: f32,
}

impl Default for TextureParams {
    fn default() -> Self {
        Self {
            grain_size_ms: 50.0,
            flux: 0.5,
        }
    }
}

impl TextureParams {
    pub fn new(grain_size_ms: f32, flux: f32) -> Self {
        Self {
            grain_size_ms: grain_size_ms.clamp(2.0, 500.0),
            flux: flux.clamp(0.0, 1.0),
        }
    }
}

/// Parameters for Beats transient preservation mode.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BeatsParams {
    pub transient_preserve: bool,
    pub transient_decay_ms: f32,
    pub loop_mode: BeatsLoopMode,
    pub slice_division_16th: bool,
}

impl Default for BeatsParams {
    fn default() -> Self {
        Self {
            transient_preserve: true,
            transient_decay_ms: 50.0,
            loop_mode: BeatsLoopMode::Off,
            slice_division_16th: true,
        }
    }
}

/// Simple 32-bit XorShift PRNG for deterministic grain flux jitter.
#[derive(Debug, Clone, Copy)]
pub struct WarpPrng {
    state: u32,
}

impl WarpPrng {
    pub fn new(seed: u32) -> Self {
        Self {
            state: if seed == 0 { 0x12345678 } else { seed },
        }
    }

    pub fn next_u32(&mut self) -> u32 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.state = x;
        x
    }

    /// Return uniform float in [-1.0, 1.0].
    pub fn next_f32_signed(&mut self) -> f32 {
        let val = (self.next_u32() >> 8) as f32 / 16777216.0;
        val * 2.0 - 1.0
    }
}

/// Pre-allocated Radix-2 2048-point FFT helper for zero-allocation execution.
#[derive(Clone)]
pub struct Radix2Fft2048 {
    bit_rev: [u16; FFT_SIZE],
    twiddle_cos: [f32; FFT_SIZE / 2],
    twiddle_sin: [f32; FFT_SIZE / 2],
}

impl Radix2Fft2048 {
    pub fn new() -> Self {
        let mut bit_rev = [0u16; FFT_SIZE];
        for i in 0..FFT_SIZE {
            let mut rev = 0u16;
            for b in 0..11 {
                if (i & (1 << b)) != 0 {
                    rev |= 1 << (10 - b);
                }
            }
            bit_rev[i] = rev;
        }

        let mut twiddle_cos = [0.0f32; FFT_SIZE / 2];
        let mut twiddle_sin = [0.0f32; FFT_SIZE / 2];
        for k in 0..(FFT_SIZE / 2) {
            let angle = -2.0 * PI * (k as f32) / (FFT_SIZE as f32);
            twiddle_cos[k] = angle.cos();
            twiddle_sin[k] = angle.sin();
        }

        Self {
            bit_rev,
            twiddle_cos,
            twiddle_sin,
        }
    }

    pub fn forward(&self, real: &mut [f32; FFT_SIZE], imag: &mut [f32; FFT_SIZE]) {
        for i in 0..FFT_SIZE {
            let j = self.bit_rev[i] as usize;
            if i < j {
                real.swap(i, j);
                imag.swap(i, j);
            }
        }

        let mut len = 2;
        while len <= FFT_SIZE {
            let half = len / 2;
            let step = FFT_SIZE / len;
            for k in 0..half {
                let w_re = self.twiddle_cos[k * step];
                let w_im = self.twiddle_sin[k * step];
                let mut i = 0;
                while i < FFT_SIZE {
                    let u_re = real[i + k];
                    let u_im = imag[i + k];
                    let v_re = real[i + k + half] * w_re - imag[i + k + half] * w_im;
                    let v_im = real[i + k + half] * w_im + imag[i + k + half] * w_re;

                    real[i + k] = u_re + v_re;
                    imag[i + k] = u_im + v_im;
                    real[i + k + half] = u_re - v_re;
                    imag[i + k + half] = u_im - v_im;
                    i += len;
                }
            }
            len <<= 1;
        }
    }

    pub fn inverse(&self, real: &mut [f32; FFT_SIZE], imag: &mut [f32; FFT_SIZE]) {
        for i in 0..FFT_SIZE {
            let j = self.bit_rev[i] as usize;
            if i < j {
                real.swap(i, j);
                imag.swap(i, j);
            }
        }

        let mut len = 2;
        while len <= FFT_SIZE {
            let half = len / 2;
            let step = FFT_SIZE / len;
            for k in 0..half {
                let w_re = self.twiddle_cos[k * step];
                let w_im = -self.twiddle_sin[k * step]; // Conjugate twiddle for inverse
                let mut i = 0;
                while i < FFT_SIZE {
                    let u_re = real[i + k];
                    let u_im = imag[i + k];
                    let v_re = real[i + k + half] * w_re - imag[i + k + half] * w_im;
                    let v_im = real[i + k + half] * w_im + imag[i + k + half] * w_re;

                    real[i + k] = u_re + v_re;
                    imag[i + k] = u_im + v_im;
                    real[i + k + half] = u_re - v_re;
                    imag[i + k + half] = u_im - v_im;
                    i += len;
                }
            }
            len <<= 1;
        }

        let inv_n = 1.0 / (FFT_SIZE as f32);
        for i in 0..FFT_SIZE {
            real[i] *= inv_n;
            imag[i] *= inv_n;
        }
    }
}

/// Pre-allocated double-buffered disk streaming cache for audio clips.
#[derive(Clone)]
pub struct StreamingDiskCache {
    page_a: [f32; PAGE_SIZE],
    page_a_offset: usize,
    page_a_valid: bool,

    page_b: [f32; PAGE_SIZE],
    page_b_offset: usize,
    page_b_valid: bool,
}

impl Default for StreamingDiskCache {
    fn default() -> Self {
        Self::new()
    }
}

impl StreamingDiskCache {
    pub fn new() -> Self {
        Self {
            page_a: [0.0; PAGE_SIZE],
            page_a_offset: 0,
            page_a_valid: false,
            page_b: [0.0; PAGE_SIZE],
            page_b_offset: 0,
            page_b_valid: false,
        }
    }

    /// Load audio data into page 0 (A) or page 1 (B) at a given file frame offset.
    pub fn load_page(&mut self, page_idx: usize, file_offset: usize, data: &[f32]) {
        let copy_len = data.len().min(PAGE_SIZE);
        if page_idx == 0 {
            self.page_a[..copy_len].copy_from_slice(&data[..copy_len]);
            if copy_len < PAGE_SIZE {
                self.page_a[copy_len..].fill(0.0);
            }
            self.page_a_offset = file_offset;
            self.page_a_valid = true;
        } else {
            self.page_b[..copy_len].copy_from_slice(&data[..copy_len]);
            if copy_len < PAGE_SIZE {
                self.page_b[copy_len..].fill(0.0);
            }
            self.page_b_offset = file_offset;
            self.page_b_valid = true;
        }
    }

    /// Read a single audio sample at `global_frame` from cached pages.
    #[inline(always)]
    pub fn read_sample(&self, global_frame: usize) -> f32 {
        if self.page_a_valid
            && global_frame >= self.page_a_offset
            && global_frame < self.page_a_offset + PAGE_SIZE
        {
            return self.page_a[global_frame - self.page_a_offset];
        }

        if self.page_b_valid
            && global_frame >= self.page_b_offset
            && global_frame < self.page_b_offset + PAGE_SIZE
        {
            return self.page_b[global_frame - self.page_b_offset];
        }

        0.0
    }

    /// Read a contiguous block of frames into `out` without allocating.
    pub fn read_frames(&self, start_frame: usize, out: &mut [f32]) -> usize {
        let mut read = 0;
        for i in 0..out.len() {
            let f = start_frame + i;
            out[i] = self.read_sample(f);
            read += 1;
        }
        read
    }

    /// Check if the playhead is nearing the page boundary and requires prefetching next page.
    pub fn needs_prefetch(&self, playhead: usize) -> Option<usize> {
        let threshold = PAGE_SIZE / 2;
        if self.page_a_valid
            && playhead >= self.page_a_offset
            && playhead < self.page_a_offset + PAGE_SIZE
        {
            if playhead >= self.page_a_offset + threshold {
                let next_offset = self.page_a_offset + PAGE_SIZE;
                if !self.page_b_valid || self.page_b_offset != next_offset {
                    return Some(next_offset);
                }
            }
        } else if self.page_b_valid
            && playhead >= self.page_b_offset
            && playhead < self.page_b_offset + PAGE_SIZE
        {
            if playhead >= self.page_b_offset + threshold {
                let next_offset = self.page_b_offset + PAGE_SIZE;
                if !self.page_a_valid || self.page_a_offset != next_offset {
                    return Some(next_offset);
                }
            }
        }
        None
    }
}

/// Cubic Hermite 4-point interpolation helper.
#[inline(always)]
fn hermite4(y0: f32, y1: f32, y2: f32, y3: f32, frac: f32) -> f32 {
    let c0 = y1;
    let c1 = 0.5 * (y2 - y0);
    let c2 = y0 - 2.5 * y1 + 2.0 * y2 - 0.5 * y3;
    let c3 = 0.5 * (y3 - y0) + 1.5 * (y1 - y2);
    ((c3 * frac + c2) * frac + c1) * frac + c0
}

/// Real-time spectral warp processor with zero heap allocations during `process_block`.
pub struct SpectralWarpProcessor {
    pub mode: WarpMode,
    pub stretch_ratio: f32,
    pub pitch_semitones: f32,
    pub sample_rate: f32,
    pub complex_pro_params: ComplexProParams,
    pub texture_params: TextureParams,
    pub beats_params: BeatsParams,

    // Pre-allocated DSP buffers
    fft: Radix2Fft2048,
    hann_window: [f32; FFT_SIZE],
    fft_real: [f32; FFT_SIZE],
    fft_imag: [f32; FFT_SIZE],
    mag: [f32; FFT_SIZE],
    phase: [f32; FFT_SIZE],
    synth_phase: [f32; FFT_SIZE],
    prev_anal_phase: [f32; FFT_SIZE],
    envelope_orig: [f32; FFT_SIZE / 2],
    envelope_shifted: [f32; FFT_SIZE / 2],
    ola_buffer: [f32; MAX_WARP_BUFFER],
    ola_read_pos: usize,
    ola_write_pos: usize,

    // Granular & Hermite state
    repitch_pos: f64,
    grain_phase_a: f32,
    grain_phase_b: f32,
    grain_read_a: f32,
    grain_read_b: f32,
    pub prng: WarpPrng,
}

impl SpectralWarpProcessor {
    pub fn new(sample_rate: f32) -> Self {
        let mut hann_window = [0.0f32; FFT_SIZE];
        for i in 0..FFT_SIZE {
            hann_window[i] = 0.5 * (1.0 - (2.0 * PI * (i as f32) / (FFT_SIZE as f32)).cos());
        }

        Self {
            mode: WarpMode::Complex,
            stretch_ratio: 1.0,
            pitch_semitones: 0.0,
            sample_rate,
            complex_pro_params: ComplexProParams::default(),
            texture_params: TextureParams::default(),
            beats_params: BeatsParams::default(),

            fft: Radix2Fft2048::new(),
            hann_window,
            fft_real: [0.0; FFT_SIZE],
            fft_imag: [0.0; FFT_SIZE],
            mag: [0.0; FFT_SIZE],
            phase: [0.0; FFT_SIZE],
            synth_phase: [0.0; FFT_SIZE],
            prev_anal_phase: [0.0; FFT_SIZE],
            envelope_orig: [0.0; FFT_SIZE / 2],
            envelope_shifted: [0.0; FFT_SIZE / 2],
            ola_buffer: [0.0; MAX_WARP_BUFFER],
            ola_read_pos: 0,
            ola_write_pos: 0,

            repitch_pos: 0.0,
            grain_phase_a: 0.0,
            grain_phase_b: 0.5,
            grain_read_a: 0.0,
            grain_read_b: 0.0,
            prng: WarpPrng::new(0x42),
        }
    }

    pub fn set_mode(&mut self, mode: WarpMode) {
        self.mode = mode;
    }

    pub fn set_stretch_ratio(&mut self, ratio: f32) {
        self.stretch_ratio = ratio.clamp(0.25, 4.0);
    }

    pub fn set_pitch_semitones(&mut self, semitones: f32) {
        self.pitch_semitones = semitones.clamp(-24.0, 24.0);
    }

    /// Reset internal phase and delay state.
    pub fn reset(&mut self) {
        self.synth_phase.fill(0.0);
        self.prev_anal_phase.fill(0.0);
        self.ola_buffer.fill(0.0);
        self.ola_read_pos = 0;
        self.ola_write_pos = 0;
        self.repitch_pos = 0.0;
        self.grain_phase_a = 0.0;
        self.grain_phase_b = 0.5;
        self.grain_read_a = 0.0;
        self.grain_read_b = 0.0;
    }

    /// Zero-allocation audio block processing callback.
    /// Time-stretches and pitch-shifts `input` into `output` according to current mode.
    pub fn process_block(&mut self, input: &[f32], output: &mut [f32]) {
        match self.mode {
            WarpMode::RePitch => self.process_repitch(input, output),
            WarpMode::Beats => self.process_beats(input, output),
            WarpMode::Tones => self.process_tones(input, output),
            WarpMode::Texture => self.process_texture(input, output),
            WarpMode::Complex => self.process_complex(input, output, false),
            WarpMode::ComplexPro => self.process_complex(input, output, true),
        }
    }

    /// RePitch mode: Cubic Hermite resampling coupling pitch to speed.
    fn process_repitch(&mut self, input: &[f32], output: &mut [f32]) {
        if input.is_empty() || output.is_empty() {
            return;
        }

        // Speed is locked to pitch scale / stretch ratio
        let pitch_factor = 2.0_f32.powf(self.pitch_semitones / 12.0);
        let speed = (pitch_factor / self.stretch_ratio) as f64;
        let in_len = input.len();

        for sample in output.iter_mut() {
            let idx = self.repitch_pos.floor() as usize;
            let frac = (self.repitch_pos - idx as f64) as f32;

            let y0 = if idx > 0 { input[(idx - 1).min(in_len - 1)] } else { input[0] };
            let y1 = input[idx.min(in_len - 1)];
            let y2 = input[(idx + 1).min(in_len - 1)];
            let y3 = input[(idx + 2).min(in_len - 1)];

            *sample = hermite4(y0, y1, y2, y3, frac);
            self.repitch_pos += speed;
            if self.repitch_pos >= in_len as f64 {
                self.repitch_pos %= in_len as f64;
            }
        }
    }

    /// Beats mode: Transient-preserving granular windowing at 16th-note boundaries.
    fn process_beats(&mut self, input: &[f32], output: &mut [f32]) {
        if input.is_empty() || output.is_empty() {
            return;
        }

        // Default 16th-note slice length @ 120 BPM = 5512 samples
        let slice_len = (self.sample_rate * 0.125) as usize;
        let decay_frames = ((self.beats_params.transient_decay_ms * 0.001) * self.sample_rate) as usize;
        let transient_hold = slice_len / 8; // Preserve attack transient unattenuated

        let in_len = input.len();
        let stretch = self.stretch_ratio as f64;

        for (i, sample) in output.iter_mut().enumerate() {
            let slice_idx = ((i as f64 / stretch) as usize) / slice_len;
            let mut local_pos = ((i as f64 / stretch) as usize) % slice_len;

            // Handle slice loop mode when stretched beyond 1.0
            if local_pos >= slice_len && self.beats_params.loop_mode != BeatsLoopMode::Off {
                match self.beats_params.loop_mode {
                    BeatsLoopMode::Forward => {
                        local_pos %= slice_len;
                    }
                    BeatsLoopMode::Alternate => {
                        let cycle = (local_pos / slice_len) % 2;
                        local_pos %= slice_len;
                        if cycle == 1 {
                            local_pos = slice_len - 1 - local_pos;
                        }
                    }
                    BeatsLoopMode::Off => {}
                }
            }

            let input_frame = (slice_idx * slice_len + local_pos) % in_len;
            let raw = input[input_frame];

            // Apply transient preservation curve
            let envelope = if local_pos < transient_hold {
                1.0
            } else if local_pos < transient_hold + decay_frames {
                let frac = (local_pos - transient_hold) as f32 / decay_frames as f32;
                (-3.0 * frac).exp()
            } else {
                0.05
            };

            *sample = raw * envelope;
        }
    }

    /// Tones mode: Pitch-synchronous overlap-add (PSOLA) with constant Hann grains.
    fn process_tones(&mut self, input: &[f32], output: &mut [f32]) {
        if input.is_empty() || output.is_empty() {
            return;
        }

        let grain_len = (self.sample_rate * 0.035) as usize; // 35 ms PSOLA grain
        let in_len = input.len();
        let pitch_rate = 2.0_f32.powf(self.pitch_semitones / 12.0);
        let advance = 1.0 / (grain_len as f32);

        for sample in output.iter_mut() {
            // Grain A
            let win_a = 0.5 * (1.0 - (2.0 * PI * self.grain_phase_a).cos());
            let idx_a = (self.grain_read_a as usize) % in_len;
            let val_a = input[idx_a] * win_a;

            // Grain B (interleaved 180 degrees)
            let win_b = 0.5 * (1.0 - (2.0 * PI * self.grain_phase_b).cos());
            let idx_b = (self.grain_read_b as usize) % in_len;
            let val_b = input[idx_b] * win_b;

            *sample = (val_a + val_b) * 0.707;

            self.grain_phase_a += advance;
            self.grain_read_a += pitch_rate;
            if self.grain_phase_a >= 1.0 {
                self.grain_phase_a = 0.0;
                self.grain_read_a = (self.repitch_pos as f32) % (in_len as f32);
            }

            self.grain_phase_b += advance;
            self.grain_read_b += pitch_rate;
            if self.grain_phase_b >= 1.0 {
                self.grain_phase_b = 0.0;
                self.grain_read_b = (self.repitch_pos as f32) % (in_len as f32);
            }

            self.repitch_pos += (1.0 / self.stretch_ratio) as f64;
            if self.repitch_pos >= in_len as f64 {
                self.repitch_pos %= in_len as f64;
            }
        }
    }

    /// Texture mode: Overlapping Hann grain cloud with stochastic flux jitter.
    fn process_texture(&mut self, input: &[f32], output: &mut [f32]) {
        if input.is_empty() || output.is_empty() {
            return;
        }

        let grain_len = ((self.texture_params.grain_size_ms * 0.001) * self.sample_rate).max(64.0) as usize;
        let in_len = input.len();
        let pitch_rate = 2.0_f32.powf(self.pitch_semitones / 12.0);
        let advance = 1.0 / (grain_len as f32);

        for sample in output.iter_mut() {
            let win_a = 0.5 * (1.0 - (2.0 * PI * self.grain_phase_a).cos());
            let idx_a = ((self.grain_read_a as usize) + in_len) % in_len;
            let val_a = input[idx_a] * win_a;

            let win_b = 0.5 * (1.0 - (2.0 * PI * self.grain_phase_b).cos());
            let idx_b = ((self.grain_read_b as usize) + in_len) % in_len;
            let val_b = input[idx_b] * win_b;

            *sample = (val_a + val_b) * 0.707;

            self.grain_phase_a += advance;
            self.grain_read_a += pitch_rate;
            if self.grain_phase_a >= 1.0 {
                self.grain_phase_a = 0.0;
                let jitter = self.prng.next_f32_signed() * self.texture_params.flux * (grain_len as f32);
                self.grain_read_a = (self.repitch_pos as f32 + jitter).max(0.0) % (in_len as f32);
            }

            self.grain_phase_b += advance;
            self.grain_read_b += pitch_rate;
            if self.grain_phase_b >= 1.0 {
                self.grain_phase_b = 0.0;
                let jitter = self.prng.next_f32_signed() * self.texture_params.flux * (grain_len as f32);
                self.grain_read_b = (self.repitch_pos as f32 + jitter).max(0.0) % (in_len as f32);
            }

            self.repitch_pos += (1.0 / self.stretch_ratio) as f64;
            if self.repitch_pos >= in_len as f64 {
                self.repitch_pos %= in_len as f64;
            }
        }
    }

    /// Complex & ComplexPro mode: STFT phase-vocoder with Laroche-Dolson phase locking
    /// and formant-preserving spectral envelope decomposition.
    fn process_complex(&mut self, input: &[f32], output: &mut [f32], formant_preserve: bool) {
        if input.is_empty() || output.is_empty() {
            return;
        }

        let in_len = input.len();
        let hop_a = HOP_SIZE;
        let hop_s = ((hop_a as f32) * self.stretch_ratio).round().max(64.0) as usize;
        let pitch_factor = 2.0_f32.powf(self.pitch_semitones / 12.0);

        let mut out_idx = 0;
        let total_out = output.len();

        while out_idx < total_out {
            // Read from OLA buffer if available
            while self.ola_read_pos < self.ola_write_pos && out_idx < total_out {
                output[out_idx] = self.ola_buffer[self.ola_read_pos];
                self.ola_read_pos += 1;
                out_idx += 1;
            }

            if out_idx >= total_out {
                break;
            }

            // Compact OLA buffer
            if self.ola_read_pos > 0 {
                let remaining = self.ola_write_pos - self.ola_read_pos;
                self.ola_buffer.copy_within(self.ola_read_pos..self.ola_write_pos, 0);
                self.ola_read_pos = 0;
                self.ola_write_pos = remaining;
                self.ola_buffer[remaining..].fill(0.0);
            }

            // Prepare analysis window
            let frame_start = (self.repitch_pos as usize) % in_len;
            for i in 0..FFT_SIZE {
                let in_val = input[(frame_start + i) % in_len];
                self.fft_real[i] = in_val * self.hann_window[i];
                self.fft_imag[i] = 0.0;
            }

            // Forward FFT
            self.fft.forward(&mut self.fft_real, &mut self.fft_imag);

            // Compute magnitude & phase
            let half = FFT_SIZE / 2;
            for k in 0..half {
                let re = self.fft_real[k];
                let im = self.fft_imag[k];
                let m = (re * re + im * im).sqrt();
                self.mag[k] = m;
                self.phase[k] = im.atan2(re);
            }

            // Extract spectral envelope for ComplexPro formant preservation
            if formant_preserve {
                let order = (self.complex_pro_params.envelope_order as usize).clamp(8, half / 2);
                for k in 0..half {
                    let start = if k >= order / 2 { k - order / 2 } else { 0 };
                    let end = (k + order / 2).min(half);
                    let sum: f32 = self.mag[start..end].iter().sum();
                    self.envelope_orig[k] = sum / (end - start) as f32;
                }
            }

            // Phase-vocoder phase unwrapping & Laroche-Dolson peak locking
            let omega_hop = 2.0 * PI * (hop_a as f32) / (FFT_SIZE as f32);
            for k in 1..(half - 1) {
                // Peak detection
                let is_peak = self.mag[k] > self.mag[k - 1] && self.mag[k] > self.mag[k + 1];
                if is_peak {
                    let expected_advance = (k as f32) * omega_hop;
                    let mut delta_phase = self.phase[k] - self.prev_anal_phase[k] - expected_advance;

                    // Wrap into [-PI, PI]
                    delta_phase -= 2.0 * PI * (delta_phase / (2.0 * PI)).round();
                    let true_freq = (k as f32) * (2.0 * PI / (FFT_SIZE as f32)) + delta_phase / (hop_a as f32);

                    // Update synthesis phase
                    self.synth_phase[k] += true_freq * (hop_s as f32);

                    // Lock adjacent bins
                    self.synth_phase[k - 1] = self.synth_phase[k] + (self.phase[k - 1] - self.phase[k]);
                    self.synth_phase[k + 1] = self.synth_phase[k] + (self.phase[k + 1] - self.phase[k]);
                } else if self.synth_phase[k].abs() < 1e-4 {
                    self.synth_phase[k] += (k as f32) * (2.0 * PI * (hop_s as f32) / (FFT_SIZE as f32));
                }
                self.prev_anal_phase[k] = self.phase[k];
            }

            // Formant envelope correction for ComplexPro
            if formant_preserve {
                let formant_weight = self.complex_pro_params.formants_envelope;
                for k in 0..half {
                    // Find shifted frequency index
                    let orig_k = ((k as f32) / pitch_factor).round() as usize;
                    let shifted_env = if orig_k < half {
                        self.envelope_orig[orig_k]
                    } else {
                        self.envelope_orig[half - 1]
                    };
                    self.envelope_shifted[k] = shifted_env;

                    // Formant filter gain: restores original vocal timbre envelope
                    let gain = (self.envelope_orig[k] + 1e-5) / (shifted_env + 1e-5);
                    let weighted_gain = gain.powf(formant_weight).clamp(0.1, 8.0);
                    self.mag[k] *= weighted_gain;
                }
            }

            // Reconstruct complex spectrum (conjugate symmetric for real IFFT)
            self.fft_real[0] = self.mag[0];
            self.fft_imag[0] = 0.0;
            for k in 1..half {
                let re = self.mag[k] * self.synth_phase[k].cos();
                let im = self.mag[k] * self.synth_phase[k].sin();
                self.fft_real[k] = re;
                self.fft_imag[k] = im;

                // Hermite conjugate symmetry
                self.fft_real[FFT_SIZE - k] = re;
                self.fft_imag[FFT_SIZE - k] = -im;
            }
            self.fft_real[half] = self.mag[half - 1];
            self.fft_imag[half] = 0.0;

            // Inverse FFT
            self.fft.inverse(&mut self.fft_real, &mut self.fft_imag);

            // Overlap-add into output ring buffer
            let window_gain = 2.0 / 3.0; // Normalization for 75% Hann overlap
            let end_pos = (self.ola_write_pos + FFT_SIZE).min(MAX_WARP_BUFFER);
            for i in 0..(end_pos - self.ola_write_pos) {
                let synth_sample = self.fft_real[i] * self.hann_window[i] * window_gain;
                self.ola_buffer[self.ola_write_pos + i] += synth_sample;
            }
            self.ola_write_pos = (self.ola_write_pos + hop_s).min(MAX_WARP_BUFFER);

            // Advance analysis playhead
            self.repitch_pos += hop_a as f64;
            if self.repitch_pos >= in_len as f64 {
                self.repitch_pos %= in_len as f64;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Generate a 440 Hz test tone of specified frame length.
    fn make_test_tone(sample_rate: f32, frames: usize) -> Vec<f32> {
        let mut buf = vec![0.0f32; frames];
        for (i, sample) in buf.iter_mut().enumerate() {
            let phase = 2.0 * PI * 440.0 * (i as f32) / sample_rate;
            *sample = phase.sin() * 0.7;
        }
        buf
    }

    #[test]
    fn test_all_warp_modes_render_finite_audio() {
        let sample_rate = 44100.0;
        let input = make_test_tone(sample_rate, 4096);
        let modes = [
            WarpMode::Beats,
            WarpMode::Tones,
            WarpMode::Texture,
            WarpMode::RePitch,
            WarpMode::Complex,
            WarpMode::ComplexPro,
        ];

        for &mode in &modes {
            let mut processor = SpectralWarpProcessor::new(sample_rate);
            processor.set_mode(mode);
            processor.set_stretch_ratio(1.25);
            processor.set_pitch_semitones(2.0);

            let mut output = vec![0.0f32; 1024];
            processor.process_block(&input, &mut output);

            // Verify finite and non-empty energy
            let energy: f32 = output.iter().map(|s| s * s).sum();
            assert!(
                energy > 1e-4,
                "Mode {:?} produced insufficient audio energy: {}",
                mode,
                energy
            );

            for &s in &output {
                assert!(s.is_finite(), "Mode {:?} rendered non-finite sample", mode);
            }
        }
    }

    #[test]
    fn test_repitch_mode_hermite_interpolation() {
        let sample_rate = 44100.0;
        let input = make_test_tone(sample_rate, 2048);
        let mut processor = SpectralWarpProcessor::new(sample_rate);
        processor.set_mode(WarpMode::RePitch);

        // Test half speed (stretch = 2.0)
        processor.set_stretch_ratio(2.0);
        processor.set_pitch_semitones(0.0);
        let mut out_slow = vec![0.0f32; 512];
        processor.process_block(&input, &mut out_slow);

        // Test double speed (stretch = 0.5)
        processor.set_stretch_ratio(0.5);
        let mut out_fast = vec![0.0f32; 512];
        processor.process_block(&input, &mut out_fast);

        assert!(out_slow.iter().all(|s| s.is_finite()));
        assert!(out_fast.iter().all(|s| s.is_finite()));
        assert_ne!(out_slow, out_fast);
    }

    #[test]
    fn test_complex_pro_formant_preservation() {
        let sample_rate = 44100.0;
        let input = make_test_tone(sample_rate, 4096);

        // Run with full formant preservation (1.0)
        let mut proc_preserve = SpectralWarpProcessor::new(sample_rate);
        proc_preserve.set_mode(WarpMode::ComplexPro);
        proc_preserve.complex_pro_params = ComplexProParams::new(1.0, 64.0);
        proc_preserve.set_pitch_semitones(7.0); // +7 semitones
        let mut out_preserve = vec![0.0f32; 1024];
        proc_preserve.process_block(&input, &mut out_preserve);

        // Run with zero formant preservation (0.0)
        let mut proc_shift = SpectralWarpProcessor::new(sample_rate);
        proc_shift.set_mode(WarpMode::ComplexPro);
        proc_shift.complex_pro_params = ComplexProParams::new(0.0, 64.0);
        proc_shift.set_pitch_semitones(7.0);
        let mut out_shift = vec![0.0f32; 1024];
        proc_shift.process_block(&input, &mut out_shift);

        assert!(out_preserve.iter().all(|s| s.is_finite()));
        assert!(out_shift.iter().all(|s| s.is_finite()));

        // Formant envelope modification must yield distinct spectral profiles
        let diff: f32 = out_preserve
            .iter()
            .zip(out_shift.iter())
            .map(|(a, b)| (a - b).abs())
            .sum();
        assert!(
            diff > 0.01,
            "Formant preservation did not alter spectral output: diff={}",
            diff
        );
    }

    #[test]
    fn test_complex_pro_formant_preservation_pearson_correlation() {
        let sample_rate = 44100.0;
        let frames = 4096;
        let mut input = vec![0.0f32; frames];
        // Rich harmonic formant-like signal
        for i in 0..frames {
            let t = i as f32 / sample_rate;
            input[i] = (2.0 * PI * 220.0 * t).sin() * 0.5
                + (2.0 * PI * 440.0 * t).sin() * 0.4
                + (2.0 * PI * 880.0 * t).sin() * 0.3
                + (2.0 * PI * 1320.0 * t).sin() * 0.2
                + (2.0 * PI * 1760.0 * t).sin() * 0.15;
        }

        let fft = Radix2Fft2048::new();
        let mut hann = [0.0f32; FFT_SIZE];
        for i in 0..FFT_SIZE {
            hann[i] = 0.5 * (1.0 - (2.0 * PI * (i as f32) / (FFT_SIZE as f32)).cos());
        }

        let compute_env = |samples: &[f32]| -> Vec<f32> {
            let half = FFT_SIZE / 2;
            let mut real = [0.0f32; FFT_SIZE];
            let mut imag = [0.0f32; FFT_SIZE];
            let len = samples.len().min(FFT_SIZE);
            for i in 0..len {
                real[i] = samples[i] * hann[i];
            }
            fft.forward(&mut real, &mut imag);
            let mut mag = vec![0.0f32; half];
            for k in 0..half {
                mag[k] = (real[k] * real[k] + imag[k] * imag[k]).sqrt();
            }
            let order = 64;
            let mut env = vec![0.0f32; half];
            for k in 0..half {
                let start = if k >= order / 2 { k - order / 2 } else { 0 };
                let end = (k + order / 2).min(half);
                let sum: f32 = mag[start..end].iter().sum();
                env[k] = sum / (end - start) as f32;
            }
            env
        };

        let orig_env = compute_env(&input);

        // Run with formant preservation (formants_envelope = 1.0)
        let mut proc_preserve = SpectralWarpProcessor::new(sample_rate);
        proc_preserve.set_mode(WarpMode::ComplexPro);
        proc_preserve.complex_pro_params = ComplexProParams::new(1.0, 64.0);
        proc_preserve.set_pitch_semitones(12.0);
        let mut out_preserve = vec![0.0f32; FFT_SIZE];
        proc_preserve.process_block(&input, &mut out_preserve);
        let preserve_env = compute_env(&out_preserve);

        // Run without formant preservation (formants_envelope = 0.0)
        let mut proc_shift = SpectralWarpProcessor::new(sample_rate);
        proc_shift.set_mode(WarpMode::ComplexPro);
        proc_shift.complex_pro_params = ComplexProParams::new(0.0, 64.0);
        proc_shift.set_pitch_semitones(12.0);
        let mut out_shift = vec![0.0f32; FFT_SIZE];
        proc_shift.process_block(&input, &mut out_shift);
        let shift_env = compute_env(&out_shift);

        let pearson = |a: &[f32], b: &[f32]| -> f32 {
            let n = a.len() as f32;
            let mean_a = a.iter().sum::<f32>() / n;
            let mean_b = b.iter().sum::<f32>() / n;
            let mut num = 0.0f32;
            let mut den_a = 0.0f32;
            let mut den_b = 0.0f32;
            for (&x, &y) in a.iter().zip(b.iter()) {
                let da = x - mean_a;
                let db = y - mean_b;
                num += da * db;
                den_a += da * da;
                den_b += db * db;
            }
            if den_a > 1e-12 && den_b > 1e-12 {
                num / (den_a.sqrt() * den_b.sqrt())
            } else {
                0.0
            }
        };

        let corr_preserve = pearson(&orig_env, &preserve_env);
        let corr_shift = pearson(&orig_env, &shift_env);

        println!("Formant correlation with preservation: {:.4}", corr_preserve);
        println!("Formant correlation without preservation: {:.4}", corr_shift);

        assert!(
            corr_preserve > 0.92,
            "Formant preservation correlation must be > 0.92, got {}",
            corr_preserve
        );
        assert!(
            corr_preserve > corr_shift,
            "Preserved correlation ({}) must exceed shifted correlation ({})",
            corr_preserve,
            corr_shift
        );
    }

    #[test]
    fn test_texture_flux_jitter_determinism() {
        let sample_rate = 44100.0;
        let input = make_test_tone(sample_rate, 4096);

        // Run texture with seed 0x1234
        let mut proc1 = SpectralWarpProcessor::new(sample_rate);
        proc1.set_mode(WarpMode::Texture);
        proc1.texture_params = TextureParams::new(40.0, 0.8);
        proc1.prng = WarpPrng::new(0x1234);
        let mut out1 = vec![0.0f32; 512];
        proc1.process_block(&input, &mut out1);

        // Run texture with identical seed
        let mut proc2 = SpectralWarpProcessor::new(sample_rate);
        proc2.set_mode(WarpMode::Texture);
        proc2.texture_params = TextureParams::new(40.0, 0.8);
        proc2.prng = WarpPrng::new(0x1234);
        let mut out2 = vec![0.0f32; 512];
        proc2.process_block(&input, &mut out2);

        assert_eq!(out1, out2, "Texture PRNG flux must be deterministic");
    }

    #[test]
    fn test_streaming_disk_cache_paging() {
        let mut cache = StreamingDiskCache::new();

        // Populate page 0: 0..16384 (values: index as f32)
        let page0: Vec<f32> = (0..PAGE_SIZE).map(|i| i as f32).collect();
        cache.load_page(0, 0, &page0);

        // Populate page 1: 16384..32768 (values: index as f32)
        let page1: Vec<f32> = (PAGE_SIZE..PAGE_SIZE * 2).map(|i| i as f32).collect();
        cache.load_page(1, PAGE_SIZE, &page1);

        // Read across page boundary (16380 to 16390)
        let mut boundary_read = [0.0f32; 10];
        let read_count = cache.read_frames(PAGE_SIZE - 5, &mut boundary_read);
        assert_eq!(read_count, 10);

        // Verify smooth transition across pages
        for (i, &val) in boundary_read.iter().enumerate() {
            let expected = (PAGE_SIZE - 5 + i) as f32;
            assert_eq!(val, expected);
        }

        // Verify prefetch triggers when playhead exceeds half page
        let prefetch_needed = cache.needs_prefetch(PAGE_SIZE - 100);
        // Page 1 is already loaded at PAGE_SIZE, so no prefetch needed
        assert!(prefetch_needed.is_none());
    }
}
