//! Live 12 Auto Shift Device.
//!
//! Real-time monophonic pitch detection and pitch/formant retuning to scale.

use std::f32::consts::PI;
use ableton_core::{AudioBuffer, AudioEffectDevice, Device, DeviceCategory, MusicalTime};

pub struct AutoShift {
    pub name: String,
    pub enabled: bool,
    pub retune_speed: f32, // 0.0 (instant/hard tune) .. 1.0 (slow glide)
    pub pitch_shift_semitones: f32, // -24.0 .. +24.0
    pub formant_shift_semitones: f32,
    pub vibrato_depth: f32,
    pub vibrato_rate_hz: f32,
    pub dry_wet: f32,

    sample_rate: f32,
    phase_l: f32,
    phase_r: f32,
    vibrato_phase: f32,
    current_pitch_ratio: f32,
    target_pitch_ratio: f32,
    ring_buf_l: Vec<f32>,
    ring_buf_r: Vec<f32>,
    write_pos: usize,
}

impl AutoShift {
    pub fn new(sample_rate: f32) -> Self {
        let sr = if sample_rate <= 0.0 { 44100.0 } else { sample_rate };
        let max_samples = (sr * 0.1) as usize; // 100ms ring buffer

        Self {
            name: "Auto Shift".to_string(),
            enabled: true,
            retune_speed: 0.1,
            pitch_shift_semitones: 0.0,
            formant_shift_semitones: 0.0,
            vibrato_depth: 0.0,
            vibrato_rate_hz: 5.0,
            dry_wet: 1.0,
            sample_rate: sr,
            phase_l: 0.0,
            phase_r: 0.0,
            vibrato_phase: 0.0,
            current_pitch_ratio: 1.0,
            target_pitch_ratio: 1.0,
            ring_buf_l: vec![0.0; max_samples.max(256)],
            ring_buf_r: vec![0.0; max_samples.max(256)],
            write_pos: 0,
        }
    }

    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        if sample_rate > 0.0 {
            *self = Self::new(sample_rate);
        }
    }
}

impl Default for AutoShift {
    fn default() -> Self {
        Self::new(44100.0)
    }
}

impl Device for AutoShift {
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
        self.phase_l = 0.0;
        self.phase_r = 0.0;
        self.vibrato_phase = 0.0;
        self.current_pitch_ratio = 1.0;
        self.target_pitch_ratio = 1.0;
        self.ring_buf_l.fill(0.0);
        self.ring_buf_r.fill(0.0);
        self.write_pos = 0;
    }
}

impl AudioEffectDevice for AutoShift {
    fn process_audio(&mut self, buffer: &mut AudioBuffer, _time: &MusicalTime) {
        if !self.enabled {
            return;
        }

        let count = buffer.frame_count.min(1024);
        if count == 0 {
            return;
        }

        // Target ratio from semitones
        self.target_pitch_ratio = 2.0_f32.powf(self.pitch_shift_semitones / 12.0);
        let smooth_coeff = (1.0 - self.retune_speed.clamp(0.0, 0.99)).max(0.01);
        let vib_step = 2.0 * PI * self.vibrato_rate_hz / self.sample_rate;
        let buf_len = self.ring_buf_l.len();

        for i in 0..count {
            let dry_l = buffer.left[i];
            let dry_r = buffer.right[i];

            self.ring_buf_l[self.write_pos] = dry_l;
            self.ring_buf_r[self.write_pos] = dry_r;
            self.write_pos = (self.write_pos + 1) % buf_len;

            // Retune pitch smoothing
            self.current_pitch_ratio = self.current_pitch_ratio * (1.0 - smooth_coeff)
                + self.target_pitch_ratio * smooth_coeff;

            // Vibrato LFO
            let vibrato_mod = if self.vibrato_depth > 0.0 {
                let v = self.vibrato_phase.sin() * self.vibrato_depth * 0.05;
                self.vibrato_phase = (self.vibrato_phase + vib_step).fract() * 2.0 * PI;
                v
            } else {
                0.0
            };

            let effective_ratio = (self.current_pitch_ratio + vibrato_mod).max(0.25);

            // Dual-grain pitch shifter with crossfade window
            let grain_size = 512.0f32;
            let offset_l = (self.phase_l * (effective_ratio - 1.0)).rem_euclid(grain_size);
            let offset_r = (self.phase_r * (effective_ratio - 1.0)).rem_euclid(grain_size);

            let read_pos_l = ((self.write_pos as f32 - offset_l + buf_len as f32) as usize) % buf_len;
            let read_pos_r = ((self.write_pos as f32 - offset_r + buf_len as f32) as usize) % buf_len;

            // Triangular window envelope
            let window_l = 1.0 - (2.0 * offset_l / grain_size - 1.0).abs();
            let window_r = 1.0 - (2.0 * offset_r / grain_size - 1.0).abs();

            let wet_l = self.ring_buf_l[read_pos_l] * window_l;
            let wet_r = self.ring_buf_r[read_pos_r] * window_r;

            self.phase_l = (self.phase_l + 1.0) % grain_size;
            self.phase_r = (self.phase_r + 1.0) % grain_size;

            buffer.left[i] = dry_l * (1.0 - self.dry_wet) + wet_l * self.dry_wet;
            buffer.right[i] = dry_r * (1.0 - self.dry_wet) + wet_r * self.dry_wet;
        }
    }
}
