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
    capture_write_pos: usize,
    capture_start_pos: usize,
    captured_len: usize,
    repeat_pos: usize,
    repeat_read_pos: f64,
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
            capture_write_pos: 0,
            capture_start_pos: 0,
            captured_len: (sr * 0.25) as usize,
            repeat_pos: 0,
            repeat_read_pos: 0.0,
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
        self.capture_write_pos = 0;
        self.capture_start_pos = 0;
        self.repeat_pos = 0;
        self.repeat_read_pos = 0.0;
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

        let count = buffer.frame_count.min(oxide_core::MAX_BUFFER_SIZE);
        if count == 0 || self.capture_buf_l.is_empty() {
            return;
        }

        let buf_len = self.capture_buf_l.len();
        let current_beat = time.beat;
        // Check for interval boundary trigger
        let interval = self.interval_beats.max(0.25);
        if (current_beat / interval).floor() > (self.last_beat / interval).floor() {
            // Roll chance
            let roll = ((current_beat * 1234.56).fract() as f32).abs();
            if roll < self.chance {
                self.is_repeating = true;
                self.repeat_pos = 0;
                self.repeat_read_pos = 0.0;
                self.repeat_count = 0;
                let grid_samples =
                    (self.grid_beats * (60.0 / time.bpm.max(20.0)) * self.sample_rate as f64) as usize;
                self.captured_len = grid_samples.clamp(64, (buf_len - 1).max(64));
                self.capture_start_pos =
                    (self.capture_write_pos + buf_len - (self.captured_len % buf_len)) % buf_len;
            } else {
                self.is_repeating = false;
            }
        }
        self.last_beat = current_beat;

        let cap_len = self.captured_len.max(1);
        let gate_limit = ((cap_len as f32) * self.gate.clamp(0.0, 1.0)) as usize;

        for i in 0..count {
            let dry_l = buffer.left[i];
            let dry_r = buffer.right[i];

            if !self.is_repeating {
                // Buffer audio continuously across blocks into persistent circular ring
                self.capture_buf_l[self.capture_write_pos] = dry_l;
                self.capture_buf_r[self.capture_write_pos] = dry_r;
                self.capture_write_pos = (self.capture_write_pos + 1) % buf_len;
                continue;
            }

            // Step fractional read head by pitch ratio with linear interpolation
            let pitch_mult = 2.0_f64.powf((self.repeat_count as f64 * self.pitch_decay as f64) / 12.0);
            let decay_gain = (0.95_f32).powi(self.repeat_count as i32);

            let (wet_l, wet_r) = if self.repeat_pos < gate_limit {
                let wrapped = self.repeat_read_pos.rem_euclid(cap_len as f64);
                let idx0_rel = wrapped.floor() as usize;
                let frac = (wrapped - idx0_rel as f64) as f32;
                let idx1_rel = (idx0_rel + 1) % cap_len;
                let buf_idx0 = (self.capture_start_pos + idx0_rel) % buf_len;
                let buf_idx1 = (self.capture_start_pos + idx1_rel) % buf_len;

                let interp_l = self.capture_buf_l[buf_idx0]
                    + frac * (self.capture_buf_l[buf_idx1] - self.capture_buf_l[buf_idx0]);
                let interp_r = self.capture_buf_r[buf_idx0]
                    + frac * (self.capture_buf_r[buf_idx1] - self.capture_buf_r[buf_idx0]);
                (interp_l * decay_gain, interp_r * decay_gain)
            } else {
                (0.0, 0.0)
            };

            self.repeat_read_pos += pitch_mult;
            self.repeat_pos += 1;
            if self.repeat_pos >= cap_len {
                self.repeat_pos = 0;
                self.repeat_read_pos = 0.0;
                self.repeat_count += 1;
            }

            let dw = self.dry_wet.clamp(0.0, 1.0);
            buffer.left[i] = dry_l * (1.0 - dw) + wet_l * dw;
            buffer.right[i] = dry_r * (1.0 - dw) + wet_r * dw;
        }
    }

    fn set_parameter(&mut self, param_id: u32, value: f32) {
        if !value.is_finite() {
            return;
        }
        match param_id {
            0 => self.interval_beats = (value as f64).clamp(0.25, 16.0),
            1 => self.grid_beats = (value as f64).clamp(0.03125, 4.0),
            2 => self.chance = value.clamp(0.0, 1.0),
            3 => self.gate = value.clamp(0.1, 1.0),
            4 => self.pitch_decay = value.clamp(-12.0, 12.0),
            5 => self.dry_wet = value.clamp(0.0, 1.0),
            _ => {}
        }
    }

    fn get_parameter(&self, param_id: u32) -> f32 {
        match param_id {
            0 => self.interval_beats as f32,
            1 => self.grid_beats as f32,
            2 => self.chance,
            3 => self.gate,
            4 => self.pitch_decay,
            5 => self.dry_wet,
            _ => 0.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_beat_repeat_circular_capture_and_pitch_step_no_gain_clip() {
        let mut br = BeatRepeat::new(44100.0);
        br.chance = 1.0;
        br.dry_wet = 1.0;
        br.gate = 1.0;
        br.pitch_decay = 12.0; // +1 octave per repeat: should step faster, not double amplitude
        br.interval_beats = 1.0;
        br.grid_beats = 0.01;

        let mut time = MusicalTime::default();
        time.bpm = 120.0;
        time.beat = 0.25;

        // Feed two consecutive blocks before beat 1.0 trigger
        let mut b1 = AudioBuffer::silence(512);
        b1.left[..512].fill(0.4);
        b1.right[..512].fill(0.4);
        br.process_audio(&mut b1, &time);

        time.beat = 0.5;
        let mut b2 = AudioBuffer::silence(512);
        b2.left[..512].fill(0.8);
        b2.right[..512].fill(0.8);
        br.process_audio(&mut b2, &time);

        // Capture write position should advance across blocks (1024), not overwrite index 0
        assert_eq!(br.capture_write_pos, 1024);

        // Trigger repeat on beat 1.0 and advance into repeat_count >= 1
        time.beat = 1.0;
        let mut b3 = AudioBuffer::silence(1024);
        br.process_audio(&mut b3, &time);

        let max_amp = b3.left[..1024]
            .iter()
            .map(|s| s.abs())
            .fold(0.0f32, f32::max);
        assert!(max_amp > 0.3 && max_amp <= 0.81, "Pitch shift must not amplify amplitude, got {}", max_amp);
    }
}
