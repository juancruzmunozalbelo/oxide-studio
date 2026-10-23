//! Live 12 Spectral Resonator Device.
//!
//! Bank of polyphonic tuned resonator combs exciting harmonic partials from incoming audio.

use oxide_core::{AudioBuffer, AudioEffectDevice, Device, DeviceCategory, MusicalTime};

pub struct SpectralResonator {
    pub name: String,
    pub enabled: bool,
    pub root_hz: f32,
    pub decay_s: f32,
    pub harmonic_count: usize, // 1 .. 8
    pub unison_spread: f32,    // 0.0 .. 1.0 (detune spread across stereo)
    pub dry_wet: f32,

    sample_rate: f32,
    comb_buffers_l: Vec<Vec<f32>>,
    comb_buffers_r: Vec<Vec<f32>>,
    comb_indices_l: Vec<usize>,
    comb_indices_r: Vec<usize>,
}

impl SpectralResonator {
    pub fn new(sample_rate: f32) -> Self {
        let sr = if sample_rate <= 0.0 { 44100.0 } else { sample_rate };
        let max_len = (sr / 20.0) as usize; // down to 20 Hz
        let count = 8;

        let mut comb_buffers_l = Vec::with_capacity(count);
        let mut comb_buffers_r = Vec::with_capacity(count);
        for _ in 0..count {
            comb_buffers_l.push(vec![0.0; max_len.max(64)]);
            comb_buffers_r.push(vec![0.0; max_len.max(64)]);
        }

        Self {
            name: "Spectral Resonator".to_string(),
            enabled: true,
            root_hz: 110.0,
            decay_s: 1.5,
            harmonic_count: 4,
            unison_spread: 0.1,
            dry_wet: 0.5,
            sample_rate: sr,
            comb_buffers_l,
            comb_buffers_r,
            comb_indices_l: vec![0; count],
            comb_indices_r: vec![0; count],
        }
    }

    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        if sample_rate > 0.0 {
            *self = Self::new(sample_rate);
        }
    }
}

impl Default for SpectralResonator {
    fn default() -> Self {
        Self::new(44100.0)
    }
}

impl Device for SpectralResonator {
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
        for b in &mut self.comb_buffers_l {
            b.fill(0.0);
        }
        for b in &mut self.comb_buffers_r {
            b.fill(0.0);
        }
        self.comb_indices_l.fill(0);
        self.comb_indices_r.fill(0);
    }
}

impl AudioEffectDevice for SpectralResonator {
    fn process_audio(&mut self, buffer: &mut AudioBuffer, _time: &MusicalTime) {
        if !self.enabled {
            return;
        }

        let count = buffer.frame_count.min(1024);
        if count == 0 {
            return;
        }

        let num_harmonics = self.harmonic_count.clamp(1, 8);
        let feedback_gain = (-3.0 / (self.decay_s.max(0.1) * self.sample_rate / 200.0)).exp().min(0.99);

        for i in 0..count {
            let dry_l = buffer.left[i];
            let dry_r = buffer.right[i];

            let mut res_sum_l = 0.0f32;
            let mut res_sum_r = 0.0f32;

            for h in 0..num_harmonics {
                let harmonic_mult = (h + 1) as f32;
                let freq_l = (self.root_hz * harmonic_mult * (1.0 - self.unison_spread * 0.02))
                    .clamp(20.0, self.sample_rate * 0.45);
                let freq_r = (self.root_hz * harmonic_mult * (1.0 + self.unison_spread * 0.02))
                    .clamp(20.0, self.sample_rate * 0.45);

                let period_l = (self.sample_rate / freq_l) as usize;
                let period_r = (self.sample_rate / freq_r) as usize;

                let buf_len_l = self.comb_buffers_l[h].len();
                let buf_len_r = self.comb_buffers_r[h].len();

                let read_idx_l = (self.comb_indices_l[h] + buf_len_l - period_l.min(buf_len_l - 1)) % buf_len_l;
                let read_idx_r = (self.comb_indices_r[h] + buf_len_r - period_r.min(buf_len_r - 1)) % buf_len_r;

                let delayed_l = self.comb_buffers_l[h][read_idx_l];
                let delayed_r = self.comb_buffers_r[h][read_idx_r];

                let write_l = dry_l + delayed_l * feedback_gain;
                let write_r = dry_r + delayed_r * feedback_gain;

                let write_pos_l = self.comb_indices_l[h];
                let write_pos_r = self.comb_indices_r[h];

                self.comb_buffers_l[h][write_pos_l] = write_l.clamp(-2.0, 2.0);
                self.comb_buffers_r[h][write_pos_r] = write_r.clamp(-2.0, 2.0);

                self.comb_indices_l[h] = (write_pos_l + 1) % buf_len_l;
                self.comb_indices_r[h] = (write_pos_r + 1) % buf_len_r;

                res_sum_l += delayed_l;
                res_sum_r += delayed_r;
            }

            let wet_l = res_sum_l / (num_harmonics as f32).sqrt();
            let wet_r = res_sum_r / (num_harmonics as f32).sqrt();

            buffer.left[i] = dry_l * (1.0 - self.dry_wet) + wet_l * self.dry_wet;
            buffer.right[i] = dry_r * (1.0 - self.dry_wet) + wet_r * self.dry_wet;
        }
    }
}
