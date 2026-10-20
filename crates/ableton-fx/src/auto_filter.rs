//! Cytomic ZDF Auto Filter with Analog Drive Models, Envelope Follower & Stereo LFO.
//!
//! Features:
//! - Cytomic ZDF State Variable Filter: LP12, LP24, HP12, HP24, BP12, BP24, Notch, Morph.
//! - 5 Analog Circuit Drive Models: Clean, OSR, MS2, SMP, PRD.
//! - Dynamic envelope follower with attack, release, and bipolar depth.
//! - Stereo LFO with sine, triangle, saw, square, and S&H waveforms + stereo phase offset.

use std::f32::consts::PI;
use ableton_core::{AudioBuffer, AudioEffectDevice, Device, DeviceCategory, MusicalTime};
pub use ableton_dsp::{CircuitModel, CytomicFilterType, CytomicSvfStage};

/// LFO waveforms for filter frequency modulation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutoFilterLfoShape {
    Sine,
    Triangle,
    SawDown,
    Square,
    SampleAndHold,
}

/// Cytomic ZDF Auto Filter Device.
pub struct AutoFilter {
    pub name: String,
    pub enabled: bool,
    pub filter_type: CytomicFilterType,
    pub circuit_model: CircuitModel,
    pub cutoff_hz: f32,
    pub resonance: f32, // 0.0 .. 1.0
    pub drive: f32,     // 1.0 = unity
    pub morph: f32,     // 0.0 = LP, 0.33 = BP, 0.66 = HP, 1.0 = Notch

    // Envelope follower
    pub env_amount: f32, // -5.0 .. +5.0 octaves
    pub env_attack_ms: f32,
    pub env_release_ms: f32,

    // Stereo LFO
    pub lfo_enabled: bool,
    pub lfo_rate_hz: f32,
    pub lfo_amount: f32, // 0.0 .. 5.0 octaves
    pub lfo_shape: AutoFilterLfoShape,
    pub lfo_stereo_phase_deg: f32, // 0.0 .. 180.0 degrees

    pub dry_wet: f32,

    sample_rate: f32,
    stage1_l: CytomicSvfStage,
    stage1_r: CytomicSvfStage,
    stage2_l: CytomicSvfStage,
    stage2_r: CytomicSvfStage,
    env_follower: f32,
    lfo_phase: f32,
    sh_held_l: f32,
    sh_held_r: f32,
}

impl AutoFilter {
    pub fn new(sample_rate: f32) -> Self {
        let sr = if sample_rate <= 0.0 { 44100.0 } else { sample_rate };
        Self {
            name: "Auto Filter".to_string(),
            enabled: true,
            filter_type: CytomicFilterType::Lowpass24,
            circuit_model: CircuitModel::Clean,
            cutoff_hz: 1000.0,
            resonance: 0.2,
            drive: 1.0,
            morph: 0.0,
            env_amount: 0.0,
            env_attack_ms: 10.0,
            env_release_ms: 100.0,
            lfo_enabled: false,
            lfo_rate_hz: 1.0,
            lfo_amount: 1.0,
            lfo_shape: AutoFilterLfoShape::Sine,
            lfo_stereo_phase_deg: 90.0,
            dry_wet: 1.0,
            sample_rate: sr,
            stage1_l: CytomicSvfStage::new(),
            stage1_r: CytomicSvfStage::new(),
            stage2_l: CytomicSvfStage::new(),
            stage2_r: CytomicSvfStage::new(),
            env_follower: 0.0,
            lfo_phase: 0.0,
            sh_held_l: 0.0,
            sh_held_r: 0.0,
        }
    }

    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        if sample_rate > 0.0 {
            self.sample_rate = sample_rate;
        }
    }

    #[inline(always)]
    fn evaluate_lfo(shape: AutoFilterLfoShape, phase: f32) -> f32 {
        let norm_phase = phase.fract();
        match shape {
            AutoFilterLfoShape::Sine => (norm_phase * 2.0 * PI).sin(),
            AutoFilterLfoShape::Triangle => {
                if norm_phase < 0.5 {
                    4.0 * norm_phase - 1.0
                } else {
                    3.0 - 4.0 * norm_phase
                }
            }
            AutoFilterLfoShape::SawDown => 1.0 - 2.0 * norm_phase,
            AutoFilterLfoShape::Square => {
                if norm_phase < 0.5 { 1.0 } else { -1.0 }
            }
            AutoFilterLfoShape::SampleAndHold => 0.0, // Handled per cycle
        }
    }
}

impl Default for AutoFilter {
    fn default() -> Self {
        Self::new(44100.0)
    }
}

impl Device for AutoFilter {
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
        self.stage1_l.reset();
        self.stage1_r.reset();
        self.stage2_l.reset();
        self.stage2_r.reset();
        self.env_follower = 0.0;
        self.lfo_phase = 0.0;
        self.sh_held_l = 0.0;
        self.sh_held_r = 0.0;
    }
}

impl AudioEffectDevice for AutoFilter {
    fn process_audio(&mut self, buffer: &mut AudioBuffer, _time: &MusicalTime) {
        if !self.enabled {
            return;
        }

        let count = buffer.frame_count.min(1024);
        if count == 0 {
            return;
        }

        let alpha_a = (-1.0 / (self.env_attack_ms.max(0.1) * 0.001 * self.sample_rate)).exp();
        let alpha_r = (-1.0 / (self.env_release_ms.max(1.0) * 0.001 * self.sample_rate)).exp();
        let lfo_step = self.lfo_rate_hz / self.sample_rate;
        let lfo_offset_norm = self.lfo_stereo_phase_deg / 360.0;

        let q_eff = 1.0 / (1.0 - self.resonance.clamp(0.0, 0.99) * 0.96).max(0.01);
        let k = 1.0 / q_eff;

        for i in 0..count {
            let dry_l = buffer.left[i];
            let dry_r = buffer.right[i];

            // Envelope follower tracking
            let in_mag = dry_l.abs().max(dry_r.abs());
            if in_mag > self.env_follower {
                self.env_follower = alpha_a * self.env_follower + (1.0 - alpha_a) * in_mag;
            } else {
                self.env_follower = alpha_r * self.env_follower + (1.0 - alpha_r) * in_mag;
            }

            // LFO evaluation
            let mut lfo_val_l = 0.0f32;
            let mut lfo_val_r = 0.0f32;
            if self.lfo_enabled {
                if self.lfo_shape == AutoFilterLfoShape::SampleAndHold {
                    if self.lfo_phase < lfo_step {
                        self.sh_held_l = (self.lfo_phase * 9876.5).sin();
                        self.sh_held_r = ((self.lfo_phase + lfo_offset_norm) * 9876.5).sin();
                    }
                    lfo_val_l = self.sh_held_l;
                    lfo_val_r = self.sh_held_r;
                } else {
                    lfo_val_l = Self::evaluate_lfo(self.lfo_shape, self.lfo_phase);
                    lfo_val_r = Self::evaluate_lfo(self.lfo_shape, self.lfo_phase + lfo_offset_norm);
                }
                self.lfo_phase = (self.lfo_phase + lfo_step).fract();
            }

            // Modulated cutoff frequencies
            let mod_octaves_l = self.env_amount * self.env_follower + self.lfo_amount * lfo_val_l;
            let mod_octaves_r = self.env_amount * self.env_follower + self.lfo_amount * lfo_val_r;

            let fc_l = (self.cutoff_hz * 2.0_f32.powf(mod_octaves_l)).clamp(10.0, self.sample_rate * 0.48);
            let fc_r = (self.cutoff_hz * 2.0_f32.powf(mod_octaves_r)).clamp(10.0, self.sample_rate * 0.48);

            let g_l = (PI * fc_l / self.sample_rate).tan();
            let g_r = (PI * fc_r / self.sample_rate).tan();

            // Stage 1 SVF
            let (lp1_l, bp1_l, hp1_l, notch1_l) =
                self.stage1_l.process(dry_l, g_l, k, self.circuit_model, self.drive);
            let (lp1_r, bp1_r, hp1_r, notch1_r) =
                self.stage1_r.process(dry_r, g_r, k, self.circuit_model, self.drive);

            let (wet_l, wet_r) = match self.filter_type {
                CytomicFilterType::Lowpass12 => (lp1_l, lp1_r),
                CytomicFilterType::Highpass12 => (hp1_l, hp1_r),
                CytomicFilterType::Bandpass12 => (bp1_l, bp1_r),
                CytomicFilterType::Notch12 => (notch1_l, notch1_r),
                CytomicFilterType::Lowpass24 => {
                    let (lp2_l, _, _, _) = self.stage2_l.process(lp1_l, g_l, k, self.circuit_model, 1.0);
                    let (lp2_r, _, _, _) = self.stage2_r.process(lp1_r, g_r, k, self.circuit_model, 1.0);
                    (lp2_l, lp2_r)
                }
                CytomicFilterType::Highpass24 => {
                    let (_, _, hp2_l, _) = self.stage2_l.process(hp1_l, g_l, k, self.circuit_model, 1.0);
                    let (_, _, hp2_r, _) = self.stage2_r.process(hp1_r, g_r, k, self.circuit_model, 1.0);
                    (hp2_l, hp2_r)
                }
                CytomicFilterType::Bandpass24 => {
                    let (_, bp2_l, _, _) = self.stage2_l.process(bp1_l, g_l, k, self.circuit_model, 1.0);
                    let (_, bp2_r, _, _) = self.stage2_r.process(bp1_r, g_r, k, self.circuit_model, 1.0);
                    (bp2_l, bp2_r)
                }
                CytomicFilterType::Morph => {
                    // Continuous interpolation: LP -> BP -> HP -> Notch
                    let m = self.morph.clamp(0.0, 1.0) * 3.0;
                    let out_l = if m < 1.0 {
                        lp1_l * (1.0 - m) + bp1_l * m
                    } else if m < 2.0 {
                        let m2 = m - 1.0;
                        bp1_l * (1.0 - m2) + hp1_l * m2
                    } else {
                        let m3 = m - 2.0;
                        hp1_l * (1.0 - m3) + notch1_l * m3
                    };
                    let out_r = if m < 1.0 {
                        lp1_r * (1.0 - m) + bp1_r * m
                    } else if m < 2.0 {
                        let m2 = m - 1.0;
                        bp1_r * (1.0 - m2) + hp1_r * m2
                    } else {
                        let m3 = m - 2.0;
                        hp1_r * (1.0 - m3) + notch1_r * m3
                    };
                    (out_l, out_r)
                }
            };

            buffer.left[i] = (dry_l * (1.0 - self.dry_wet) + wet_l * self.dry_wet).clamp(-3.0, 3.0);
            buffer.right[i] = (dry_r * (1.0 - self.dry_wet) + wet_r * self.dry_wet).clamp(-3.0, 3.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auto_filter_drive_models_bounded() {
        let models = [
            CircuitModel::Clean,
            CircuitModel::OSR,
            CircuitModel::MS2,
            CircuitModel::SMP,
            CircuitModel::PRD,
        ];

        for model in models {
            let mut filter = AutoFilter::new(44100.0);
            filter.circuit_model = model;
            filter.drive = 3.0;
            filter.resonance = 0.8;

            let mut buf = AudioBuffer::silence(256);
            for i in 0..256 {
                buf.left[i] = 0.8;
                buf.right[i] = 0.8;
            }

            let time = MusicalTime::default();
            filter.process_audio(&mut buf, &time);

            for (i, (&l, &r)) in buf.left[0..256].iter().zip(buf.right[0..256].iter()).enumerate() {
                assert!(!l.is_nan(), "{:?} left sample at {} is NaN", model, i);
                assert!(!r.is_nan(), "{:?} right sample at {} is NaN", model, i);
                assert!(l.is_finite(), "{:?} left sample at {} not finite", model, i);
                assert!(r.is_finite(), "{:?} right sample at {} not finite", model, i);
                assert!(l.abs() <= 3.0, "{:?} left sample exceeded boundary: {}", model, l);
            }
        }
    }
}
