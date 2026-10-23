//! Live 12 Beat Repeat Device.
//!
//! Periodic loop slicer and audio buffer freeze with pitch transposition decay.

use oxide_core::{AudioBuffer, AudioEffectDevice, Device, DeviceCategory, MusicalTime};

pub struct BeatRepeat {
    pub name: String,
    pub enabled: bool,
    pub interval_beats: f64, // e.g. 1.0 (every bar/quarter)
    pub grid_beats: f64,     // e.g. 0.125 (1/32)
    pub chance: f32,         // 0.0 .. 1.0
    pub gate: f32,           // 0.1 .. 1.0 (fraction of grid that sounds)
    pub pitch_decay: f32,    // semitones per repeat
    pub dry_wet: f32,

    sample_rate: f32,
    capture_buf_l: Vec<f32>,
    capture_buf_r: Vec<f32>,
    captured_len: usize,
    repeat_pos: usize,
    is_repeating: bool,
    repeat_count: usize,
    last_beat: f64,
}

impl BeatRepeat {
    pub fn new(sample_rate: f32) -> Self {
        let sr = if sample_rate <= 0.0 { 44100.0 } else { sample_rate };
        let max_samples = (sr * 2.0) as usize;

        Self {
            name: "Beat Repeat".to_string(),
            enabled: true,
            interval_beats: 1.0,
            grid_beats: 0.25,
            chance: 0.6,
            gate: 0.8,
            pitch_decay: -1.0,
            dry_wet: 0.5,
            sample_rate: sr,
            capture_buf_l: vec![0.0; max_samples],
            capture_buf_r: vec![0.0; max_samples],
            captured_len: (sr * 0.25) as usize,
            repeat_pos: 0,
            is_repeating: false,
            repeat_count: 0,
            last_beat: 0.0,
        }
    }

    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        if sample_rate > 0.0 {
            *self = Self::new(sample_rate);
        }
    }
}

impl Default for BeatRepeat {
    fn default() -> Self {
        Self::new(44100.0)
    }
}

impl Device for BeatRepeat {
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
        self.capture_buf_l.fill(0.0);
        self.capture_buf_r.fill(0.0);
        self.repeat_pos = 0;
        self.is_repeating = false;
        self.repeat_count = 0;
        self.last_beat = 0.0;
    }
}

impl AudioEffectDevice for BeatRepeat {
    fn process_audio(&mut self, buffer: &mut AudioBuffer, time: &MusicalTime) {
        if !self.enabled {
            return;
        }

        let count = buffer.frame_count.min(1024);
        if count == 0 {
            return;
        }

        let current_beat = time.beat;
        // Check for interval boundary trigger
        let interval = self.interval_beats.max(0.25);
        if (current_beat / interval).floor() > (self.last_beat / interval).floor() {
            // Roll chance
            let roll = ((current_beat * 1234.56).fract() as f32).abs();
            if roll < self.chance {
                self.is_repeating = true;
                self.repeat_pos = 0;
                self.repeat_count = 0;
                let grid_samples = (self.grid_beats * (60.0 / time.bpm.max(20.0)) * self.sample_rate as f64) as usize;
                self.captured_len = grid_samples.clamp(64, self.capture_buf_l.len() - 1);
            } else {
                self.is_repeating = false;
            }
        }
        self.last_beat = current_beat;

        let gate_limit = (self.captured_len as f32 * self.gate) as usize;

        for i in 0..count {
            let dry_l = buffer.left[i];
            let dry_r = buffer.right[i];

            if !self.is_repeating {
                // Buffer audio continuously
                let pos = i % self.capture_buf_l.len();
                self.capture_buf_l[pos] = dry_l;
                self.capture_buf_r[pos] = dry_r;
                continue;
            }

            // Playing repeating slice
            let read_idx = self.repeat_pos % self.captured_len;
            let pitch_mult = 2.0_f32.powf((self.repeat_count as f32 * self.pitch_decay) / 12.0);
            let decay_gain = (0.95_f32).powi(self.repeat_count as i32);

            let (wet_l, wet_r) = if (self.repeat_pos % self.captured_len) < gate_limit {
                (
                    self.capture_buf_l[read_idx] * pitch_mult * decay_gain,
                    self.capture_buf_r[read_idx] * pitch_mult * decay_gain,
                )
            } else {
                (0.0, 0.0)
            };

            self.repeat_pos += 1;
            if self.repeat_pos.is_multiple_of(self.captured_len) {
                self.repeat_count += 1;
            }

            buffer.left[i] = dry_l * (1.0 - self.dry_wet) + wet_l * self.dry_wet;
            buffer.right[i] = dry_r * (1.0 - self.dry_wet) + wet_r * self.dry_wet;
        }
    }
}
