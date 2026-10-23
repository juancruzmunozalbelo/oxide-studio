//! ADSR Envelope generator with RNBO convex attack lockout and exponential decay/release.
//!
//! Decompiled & reverse-engineered from Cycling '74 Max RNBO patcher and Ableton Learning Synths
//! (`src/engine/dsp-core.ts`, lines 22-105).

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnvStage {
    Idle = 0,
    Attack = 1,
    Decay = 2,
    Sustain = 3,
    Release = 4,
}

/// ADSR Envelope generator.
#[derive(Debug, Clone)]
pub struct AdsrEnvelope {
    pub sample_rate: f32,
    pub stage: EnvStage,
    pub value: f32,
}

impl AdsrEnvelope {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            sample_rate: if sample_rate <= 0.0 { 44100.0 } else { sample_rate },
            stage: EnvStage::Idle,
            value: 0.0,
        }
    }

    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        if sample_rate > 0.0 {
            self.sample_rate = sample_rate;
        }
    }

    /// Trigger the attack stage.
    pub fn trigger_attack(&mut self) {
        self.stage = EnvStage::Attack;
    }

    /// Trigger the release stage (only if not idle).
    pub fn trigger_release(&mut self) {
        if self.stage != EnvStage::Idle {
            self.stage = EnvStage::Release;
        }
    }

    /// Reset to idle state.
    pub fn reset(&mut self) {
        self.stage = EnvStage::Idle;
        self.value = 0.0;
    }

    /// Advance one sample with given parameters:
    ///
    /// - `attack_ms`: Attack time in milliseconds (clamped min 0.5)
    /// - `decay_ms`: Decay time in milliseconds (clamped min 1.0)
    /// - `sustain_level`: Sustain amplitude level in [0.0, 1.0]
    /// - `release_ms`: Release time in milliseconds (clamped min 1.0)
    #[inline(always)]
    pub fn next_sample(&mut self, attack_ms: f32, decay_ms: f32, sustain_level: f32, release_ms: f32) -> f32 {
        self.next(attack_ms, decay_ms, sustain_level, release_ms)
    }

    pub fn next(&mut self, attack_ms: f32, decay_ms: f32, sustain_level: f32, release_ms: f32) -> f32 {
        let sr = self.sample_rate;
        let sustain = sustain_level.clamp(0.0, 1.0);

        match self.stage {
            EnvStage::Idle => {
                self.value = 0.0;
            }
            EnvStage::Attack => {
                let attack_samples = (attack_ms.max(0.5) * 0.001 * sr).max(1.0);
                let step = 1.0 / attack_samples;
                // RNBO convex attack lockout curvature:
                self.value += step * (1.25 - 0.25 * self.value);
                if self.value >= 1.0 {
                    self.value = 1.0;
                    self.stage = EnvStage::Decay;
                }
            }
            EnvStage::Decay => {
                let decay_samples = (decay_ms.max(1.0) * 0.001 * sr).max(1.0);
                let coeff = (-4.6 / decay_samples).exp();
                self.value = sustain + (self.value - sustain) * coeff;
                if (self.value - sustain).abs() < 1e-4 {
                    self.value = sustain;
                    self.stage = EnvStage::Sustain;
                }
            }
            EnvStage::Sustain => {
                self.value = sustain;
            }
            EnvStage::Release => {
                let release_samples = (release_ms.max(1.0) * 0.001 * sr).max(1.0);
                let coeff = (-4.6 / release_samples).exp();
                self.value *= coeff;
                if self.value < 1e-4 {
                    self.value = 0.0;
                    self.stage = EnvStage::Idle;
                }
            }
        }

        self.value
    }

    pub fn get_value(&self) -> f32 {
        self.value
    }

    pub fn get_stage(&self) -> EnvStage {
        self.stage
    }

    pub fn is_idle(&self) -> bool {
        self.stage == EnvStage::Idle
    }

    pub fn is_active(&self) -> bool {
        self.stage != EnvStage::Idle
    }

    pub fn current_value(&self) -> f32 {
        self.value
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_adsr_full_cycle() {
        let mut env = AdsrEnvelope::new(1000.0); // 1 kHz sample rate = 1 sample per ms
        assert_eq!(env.get_stage(), EnvStage::Idle);

        // Attack: 10 ms
        env.trigger_attack();
        assert_eq!(env.get_stage(), EnvStage::Attack);

        let mut max_val = 0.0f32;
        for _ in 0..15 {
            let v = env.next(10.0, 50.0, 0.5, 50.0);
            if v > max_val {
                max_val = v;
            }
        }
        assert_eq!(max_val, 1.0);
        assert_eq!(env.get_stage(), EnvStage::Decay);

        // Decay to sustain 0.5: run 100 samples
        for _ in 0..100 {
            env.next(10.0, 50.0, 0.5, 50.0);
        }
        assert_eq!(env.get_stage(), EnvStage::Sustain);
        assert!((env.get_value() - 0.5).abs() < 1e-3);

        // Release: 20 ms
        env.trigger_release();
        assert_eq!(env.get_stage(), EnvStage::Release);
        for _ in 0..150 {
            env.next(10.0, 50.0, 0.5, 20.0);
        }
        assert_eq!(env.get_stage(), EnvStage::Idle);
        assert_eq!(env.get_value(), 0.0);
    }
}
