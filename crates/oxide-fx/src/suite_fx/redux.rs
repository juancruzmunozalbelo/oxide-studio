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
    }
}

impl AudioEffectDevice for Redux {
    fn process_audio(&mut self, buffer: &mut AudioBuffer, _time: &MusicalTime) {
        if !self.enabled {
            return;
        }

        let count = buffer.frame_count.min(1024);
        if count == 0 {
            return;
        }

        let ds = self.downsample.max(1);
        let levels = 2.0_f32.powi(self.bit_depth.clamp(1, 16) as i32);

        for i in 0..count {
            let dry_l = buffer.left[i];
            let dry_r = buffer.right[i];

            self.counter += 1;
            let jitter_mod = if self.jitter > 0.0 {
                let noise = ((i as f32 * 123.45).sin() * 43758.545).fract();
                (noise * self.jitter * (ds as f32 * 0.5)) as isize
            } else {
                0
            };

            let effective_ds = (ds as isize + jitter_mod).max(1) as usize;

            if self.counter >= effective_ds {
                self.counter = 0;
                // Quantize amplitude to bit depth
                self.held_l = (dry_l * levels).round() / levels;
                self.held_r = (dry_r * levels).round() / levels;
            }

            buffer.left[i] = dry_l * (1.0 - self.dry_wet) + self.held_l * self.dry_wet;
            buffer.right[i] = dry_r * (1.0 - self.dry_wet) + self.held_r * self.dry_wet;
        }
    }
}
