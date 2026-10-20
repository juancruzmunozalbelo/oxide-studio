//! Live 12 Echo Device (Dual Tape Delay with Wow/Flutter, Ducking & Cross-feed).

use std::f32::consts::PI;
use ableton_core::{AudioBuffer, AudioEffectDevice, Device, DeviceCategory, MusicalTime};

pub struct Echo {
    pub name: String,
    pub enabled: bool,
    pub delay_time_l_ms: f32,
    pub delay_time_r_ms: f32,
    pub feedback: f32,
    pub cross_feed: f32, // 0.0 .. 1.0 (ping-pong cross-coupling)
    pub wow: f32,        // slow motor drift
    pub flutter: f32,    // fast motor jitter
    pub ducking_db: f32, // -30.0 .. 0.0
    pub dry_wet: f32,

    sample_rate: f32,
    buf_l: Vec<f32>,
    buf_r: Vec<f32>,
    write_pos: usize,
    wow_phase: f32,
    flutter_phase: f32,
    duck_envelope: f32,
    filter_state_l: f32,
    filter_state_r: f32,
}

impl Echo {
    pub fn new(sample_rate: f32) -> Self {
        let sr = if sample_rate <= 0.0 { 44100.0 } else { sample_rate };
        let max_samples = (sr * 2.5) as usize; // up to 2.5s delay
        Self {
            name: "Echo".to_string(),
            enabled: true,
            delay_time_l_ms: 375.0, // dotted 8th @ 120bpm
            delay_time_r_ms: 250.0, // 8th @ 120bpm
            feedback: 0.45,
            cross_feed: 0.3,
            wow: 0.15,
            flutter: 0.05,
            ducking_db: 0.0,
            dry_wet: 0.5,
            sample_rate: sr,
            buf_l: vec![0.0; max_samples.max(1024)],
            buf_r: vec![0.0; max_samples.max(1024)],
            write_pos: 0,
            wow_phase: 0.0,
            flutter_phase: 0.0,
            duck_envelope: 0.0,
            filter_state_l: 0.0,
            filter_state_r: 0.0,
        }
    }

    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        if sample_rate > 0.0 {
            *self = Self::new(sample_rate);
        }
    }
}

impl Default for Echo {
    fn default() -> Self {
        Self::new(44100.0)
    }
}

impl Device for Echo {
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
        self.buf_l.fill(0.0);
        self.buf_r.fill(0.0);
        self.write_pos = 0;
        self.wow_phase = 0.0;
        self.flutter_phase = 0.0;
        self.duck_envelope = 0.0;
        self.filter_state_l = 0.0;
        self.filter_state_r = 0.0;
    }
}

impl AudioEffectDevice for Echo {
    fn process_audio(&mut self, buffer: &mut AudioBuffer, _time: &MusicalTime) {
        if !self.enabled {
            return;
        }

        let count = buffer.frame_count.min(1024);
        if count == 0 {
            return;
        }

        let buf_len = self.buf_l.len();
        let base_delay_l = self.delay_time_l_ms * 0.001 * self.sample_rate;
        let base_delay_r = self.delay_time_r_ms * 0.001 * self.sample_rate;

        for i in 0..count {
            let dry_l = buffer.left[i];
            let dry_r = buffer.right[i];

            // Tape wow (~0.5 Hz) and flutter (~6.0 Hz) drift
            let wow_mod = (self.wow_phase * 2.0 * PI).sin() * self.wow * 20.0;
            let flutter_mod = (self.flutter_phase * 2.0 * PI).sin() * self.flutter * 5.0;

            self.wow_phase = (self.wow_phase + 0.5 / self.sample_rate).fract();
            self.flutter_phase = (self.flutter_phase + 6.0 / self.sample_rate).fract();

            let delay_samples_l = (base_delay_l + wow_mod + flutter_mod).clamp(1.0, (buf_len - 2) as f32);
            let delay_samples_r = (base_delay_r - wow_mod + flutter_mod).clamp(1.0, (buf_len - 2) as f32);

            let read_idx_l = ((self.write_pos as f32 - delay_samples_l + buf_len as f32) as usize) % buf_len;
            let read_idx_r = ((self.write_pos as f32 - delay_samples_r + buf_len as f32) as usize) % buf_len;

            let delayed_l = self.buf_l[read_idx_l];
            let delayed_r = self.buf_r[read_idx_r];

            // Feedback damping filter
            self.filter_state_l = self.filter_state_l * 0.4 + delayed_l * 0.6;
            self.filter_state_r = self.filter_state_r * 0.4 + delayed_r * 0.6;

            let fb_l = self.filter_state_l * (1.0 - self.cross_feed) + self.filter_state_r * self.cross_feed;
            let fb_r = self.filter_state_r * (1.0 - self.cross_feed) + self.filter_state_l * self.cross_feed;

            // Ducking envelope
            let dry_mag = dry_l.abs().max(dry_r.abs());
            self.duck_envelope = self.duck_envelope * 0.999 + dry_mag * 0.001;
            let duck_gain = if self.ducking_db < 0.0 {
                let att_db = self.duck_envelope * self.ducking_db;
                10.0_f32.powf(att_db / 20.0)
            } else {
                1.0
            };

            let wet_l = delayed_l * duck_gain;
            let wet_r = delayed_r * duck_gain;

            self.buf_l[self.write_pos] = (dry_l + fb_l * self.feedback).clamp(-2.0, 2.0);
            self.buf_r[self.write_pos] = (dry_r + fb_r * self.feedback).clamp(-2.0, 2.0);
            self.write_pos = (self.write_pos + 1) % buf_len;

            buffer.left[i] = dry_l * (1.0 - self.dry_wet) + wet_l * self.dry_wet;
            buffer.right[i] = dry_r * (1.0 - self.dry_wet) + wet_r * self.dry_wet;
        }
    }
}
