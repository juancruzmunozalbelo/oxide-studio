//! Live 12 Corpus Device (Physical Modal Acoustic Resonator Bank).

use std::f32::consts::PI;
use oxide_core::{AudioBuffer, AudioEffectDevice, Device, DeviceCategory, MusicalTime};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CorpusResonatorType {
    Beam,
    Marimba,
    String,
    Plate,
    Pipe,
}

#[derive(Debug, Clone, Copy)]
struct ModalFilter {
    c1: f32,
    c2: f32,
    y1: f32,
    y2: f32,
}

impl ModalFilter {
    fn new(freq: f32, decay_s: f32, sr: f32) -> Self {
        let f0 = freq.clamp(20.0, sr * 0.45);
        let tau = decay_s.max(0.01);
        let r = (-1.0 / (tau * sr)).exp();
        let theta = 2.0 * PI * f0 / sr;
        let c1 = 2.0 * r * theta.cos();
        let c2 = -r * r;
        Self { c1, c2, y1: 0.0, y2: 0.0 }
    }

    #[inline(always)]
    fn process(&mut self, x: f32) -> f32 {
        let y0 = x + self.c1 * self.y1 + self.c2 * self.y2;
        self.y2 = self.y1;
        self.y1 = y0;
        y0
    }

    fn reset(&mut self) {
        self.y1 = 0.0;
        self.y2 = 0.0;
    }
}

pub struct Corpus {
    pub name: String,
    pub enabled: bool,
    pub resonator_type: CorpusResonatorType,
    pub tune_hz: f32,
    pub decay_s: f32,
    pub material: f32, // -1.0 (wood) .. +1.0 (metal)
    pub dry_wet: f32,

    sample_rate: f32,
    modes_l: Vec<ModalFilter>,
    modes_r: Vec<ModalFilter>,
}

impl Corpus {
    pub fn new(sample_rate: f32) -> Self {
        let sr = if sample_rate <= 0.0 { 44100.0 } else { sample_rate };
        let mut s = Self {
            name: "Corpus".to_string(),
            enabled: true,
            resonator_type: CorpusResonatorType::Plate,
            tune_hz: 220.0,
            decay_s: 1.0,
            material: 0.0,
            dry_wet: 0.5,
            sample_rate: sr,
            modes_l: Vec::new(),
            modes_r: Vec::new(),
        };
        s.rebuild_modes();
        s
    }

    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        if sample_rate > 0.0 {
            self.sample_rate = sample_rate;
            self.rebuild_modes();
        }
    }

    fn rebuild_modes(&mut self) {
        let ratios: &[f32] = match self.resonator_type {
            CorpusResonatorType::Beam => &[1.0, 2.76, 5.4, 8.93, 13.34],
            CorpusResonatorType::Marimba => &[1.0, 3.98, 9.2, 16.5],
            CorpusResonatorType::String => &[1.0, 2.0, 3.0, 4.0, 5.0, 6.0],
            CorpusResonatorType::Plate => &[1.0, 1.59, 2.14, 2.3, 2.65, 3.16],
            CorpusResonatorType::Pipe => &[1.0, 3.0, 5.0, 7.0],
        };

        self.modes_l.clear();
        self.modes_r.clear();

        for (idx, &ratio) in ratios.iter().enumerate() {
            let mode_decay = self.decay_s / (1.0 + (idx as f32) * (1.0 - self.material * 0.5));
            let f_l = self.tune_hz * ratio;
            let f_r = self.tune_hz * ratio * 1.002; // slight stereo spread
            self.modes_l.push(ModalFilter::new(f_l, mode_decay, self.sample_rate));
            self.modes_r.push(ModalFilter::new(f_r, mode_decay, self.sample_rate));
        }
    }
}

impl Default for Corpus {
    fn default() -> Self {
        Self::new(44100.0)
    }
}

impl Device for Corpus {
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
        for m in &mut self.modes_l {
            m.reset();
        }
        for m in &mut self.modes_r {
            m.reset();
        }
    }
}

impl AudioEffectDevice for Corpus {
    fn process_audio(&mut self, buffer: &mut AudioBuffer, _time: &MusicalTime) {
        if !self.enabled {
            return;
        }

        let count = buffer.frame_count.min(1024);
        if count == 0 {
            return;
        }

        let num_modes = self.modes_l.len();
        if num_modes == 0 {
            return;
        }

        let norm_gain = 0.2 / (num_modes as f32).sqrt();

        for i in 0..count {
            let dry_l = buffer.left[i];
            let dry_r = buffer.right[i];

            let mut wet_l = 0.0f32;
            let mut wet_r = 0.0f32;

            for m in 0..num_modes {
                wet_l += self.modes_l[m].process(dry_l);
                wet_r += self.modes_r[m].process(dry_r);
            }

            wet_l *= norm_gain;
            wet_r *= norm_gain;

            buffer.left[i] = (dry_l * (1.0 - self.dry_wet) + wet_l * self.dry_wet).clamp(-3.0, 3.0);
            buffer.right[i] = (dry_r * (1.0 - self.dry_wet) + wet_r * self.dry_wet).clamp(-3.0, 3.0);
        }
    }
}
