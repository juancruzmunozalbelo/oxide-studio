//! Live 12 Redux Device (Sample Rate & Bit Depth Reducer with Jitter).

use oxide_core::{AudioBuffer, AudioEffectDevice, Device, DeviceCategory, MusicalTime};

pub struct Redux {
    pub name: String,
    pub enabled: bool,
    pub downsample: usize, // 1 .. 256
    pub bit_depth: u8,     // 1 .. 16
    pub jitter: f32,       // 0.0 .. 1.0
    pub dry_wet: f32,

    held_l: f32,
    held_r: f32,
    counter: usize,
    rng_state: u32,
}

impl Redux {
    pub fn new() -> Self {
        Self {
            name: "Redux".to_string(),
            enabled: true,
            downsample: 8,
            bit_depth: 8,
            jitter: 0.0,
            dry_wet: 1.0,
            held_l: 0.0,
            held_r: 0.0,
            counter: 0,
            rng_state: 0x5245_4458,
        }
    }

    #[inline(always)]
    fn next_unit(&mut self) -> f32 {
        let mut x = self.rng_state;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng_state = x;
        ((x >> 8) as f32) * (1.0 / 16_777_216.0)
    }

    #[inline(always)]
    fn quantize_sample(x: f32, bits: u8) -> f32 {
        let s = if x.is_finite() { x.clamp(-1.0, 1.0) } else { 0.0 };
        if bits <= 1 {
            if s == 0.0 {
                0.0
            } else if s > 0.0 {
                1.0
            } else {
                -1.0
            }
        } else {
            let half_levels = 2.0_f32.powi((bits as i32) - 1);
            (s * half_levels).round() / half_levels
        }
    }
}

impl Default for Redux {
    fn default() -> Self {
        Self::new()
    }
}

impl Device for Redux {
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
        self.held_l = 0.0;
        self.held_r = 0.0;
        self.counter = 0;
        self.rng_state = 0x5245_4458;
    }
}

impl AudioEffectDevice for Redux {
    fn process_audio(&mut self, buffer: &mut AudioBuffer, _time: &MusicalTime) {
        if !self.enabled {
            return;
        }

        let count = buffer.frame_count.min(oxide_core::MAX_BUFFER_SIZE);
        if count == 0 {
            return;
        }

        let ds = self.downsample.max(1);
        let bits = self.bit_depth.clamp(1, 16);

        for i in 0..count {
            let dry_l = if buffer.left[i].is_finite() { buffer.left[i] } else { 0.0 };
            let dry_r = if buffer.right[i].is_finite() { buffer.right[i] } else { 0.0 };

            self.counter += 1;
            let jitter_mod = if self.jitter > 0.0 {
                let noise = self.next_unit();
                (noise * self.jitter * (ds as f32 * 0.5)) as isize
            } else {
                0
            };

            let effective_ds = (ds as isize + jitter_mod).max(1) as usize;

            if self.counter >= effective_ds {
                self.counter = 0;
                // Quantize amplitude to bit depth
                self.held_l = Self::quantize_sample(dry_l, bits);
                self.held_r = Self::quantize_sample(dry_r, bits);
            }

            buffer.left[i] = dry_l * (1.0 - self.dry_wet) + self.held_l * self.dry_wet;
            buffer.right[i] = dry_r * (1.0 - self.dry_wet) + self.held_r * self.dry_wet;
        }
    }

    fn set_parameter(&mut self, param_id: u32, value: f32) {
        if !value.is_finite() {
            return;
        }
        match param_id {
            0 => self.dry_wet = value.clamp(0.0, 1.0),
            1 => self.downsample = value.round().clamp(1.0, 256.0) as usize,
            2 => self.bit_depth = value.round().clamp(1.0, 16.0) as u8,
            3 => self.jitter = value.clamp(0.0, 1.0),
            _ => {}
        }
    }

    fn get_parameter(&self, param_id: u32) -> f32 {
        match param_id {
            0 => self.dry_wet,
            1 => self.downsample as f32,
            2 => self.bit_depth as f32,
            3 => self.jitter,
            _ => 0.0,
        }
    }
}
