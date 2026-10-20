//! Cytomic Zero-Delay Feedback (ZDF) multi-mode filter.
//!
//! Implements Andrew Simper's (Cytomic) ZDF State Variable Filter with:
//! - 12 dB and 24 dB slopes for LP, HP, BP, plus 12 dB Notch and continuous Morph
//! - 5 analog circuit drive models: Clean, OSR, MS2, SMP, PRD
//! - Series, Parallel, and Split dual-filter routing topologies
//! - Unconditional stability under high resonance with Padé/tanh non-linear saturation

use std::f32::consts::PI;

/// Multi-mode filter frequency response types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CytomicFilterType {
    Lowpass12,
    Lowpass24,
    Highpass12,
    Highpass24,
    Bandpass12,
    Bandpass24,
    Notch12,
    Morph,
}

/// Analog circuit drive simulation models.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitModel {
    Clean,
    OSR,
    MS2,
    SMP,
    PRD,
}

/// Dual filter routing topology.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterRouting {
    Series,
    Parallel,
    Split,
}

/// Single 2-pole Cytomic ZDF State Variable Filter (SVF) stage.
#[derive(Debug, Clone)]
pub struct CytomicSvfStage {
    pub s1: f32,
    pub s2: f32,
}

impl CytomicSvfStage {
    pub fn new() -> Self {
        Self { s1: 0.0, s2: 0.0 }
    }

    pub fn reset(&mut self) {
        self.s1 = 0.0;
        self.s2 = 0.0;
    }

    #[inline(always)]
    fn soft_clip(x: f32) -> f32 {
        if x > 3.0 {
            1.0
        } else if x < -3.0 {
            -1.0
        } else {
            let x2 = x * x;
            (x * (27.0 + x2)) / (27.0 + 9.0 * x2)
        }
    }

    /// Process one audio sample through the 2-pole SVF stage.
    ///
    /// - `input`: Input audio sample
    /// - `g`: Prewarped frequency cutoff parameter: tan(pi * fc / fs)
    /// - `k`: Damping parameter: 1.0 / Q
    /// - `model`: Analog circuit saturation model
    /// - `drive`: Drive gain (1.0 = unity)
    ///
    /// Returns (lowpass, bandpass, highpass, notch).
    #[inline(always)]
    pub fn process(
        &mut self,
        input: f32,
        g: f32,
        k: f32,
        model: CircuitModel,
        drive: f32,
    ) -> (f32, f32, f32, f32) {
        let driven_in = match model {
            CircuitModel::Clean => input * drive,
            CircuitModel::OSR => Self::soft_clip(input * drive * 1.2),
            CircuitModel::MS2 => input * drive,
            CircuitModel::SMP => {
                let x = (input * drive).clamp(-1.5, 1.5);
                x - 0.333333 * x * x * x
            }
            CircuitModel::PRD => (input * drive).tanh(),
        };

        let g_eff = g.clamp(0.0001, 10.0);
        let k_eff = k.max(0.001);

        // Compute highpass component v0 using zero-delay feedback
        let v0 = match model {
            CircuitModel::MS2 => {
                // Non-linear feedback resonance clipping (MS-20 Sallen-Key diode clamp)
                let s1_sat = Self::soft_clip(self.s1 * 1.4);
                let denom = 1.0 + g_eff * (k_eff + g_eff);
                (driven_in - (k_eff + g_eff) * s1_sat - self.s2) / denom
            }
            CircuitModel::PRD => {
                // Transistor ladder style non-linear feedback
                let denom = 1.0 + g_eff * (k_eff + g_eff);
                let diff = driven_in - (k_eff + g_eff) * self.s1 - self.s2;
                Self::soft_clip(diff / denom)
            }
            _ => {
                let denom = 1.0 + g_eff * (k_eff + g_eff);
                (driven_in - (k_eff + g_eff) * self.s1 - self.s2) / denom
            }
        };

        // Integrator outputs
        let v1 = self.s1 + g_eff * v0;
        let v2 = self.s2 + g_eff * v1;

        // State update with bounded limits to prevent numerical explosion during self-oscillation
        self.s1 = (2.0 * v1 - self.s1).clamp(-4.0, 4.0);
        self.s2 = (2.0 * v2 - self.s2).clamp(-4.0, 4.0);

        let lp = v2;
        let bp = v1;
        let hp = v0;
        let notch = v0 + v2;

        (lp, bp, hp, notch)
    }
}

/// Full Cytomic ZDF multi-mode filter supporting 12/24 dB slopes, morphing, and circuit models.
#[derive(Debug, Clone)]
pub struct CytomicZdfFilter {
    pub sample_rate: f32,
    pub filter_type: CytomicFilterType,
    pub circuit_model: CircuitModel,
    pub stage1: CytomicSvfStage,
    pub stage2: CytomicSvfStage,
}

impl CytomicZdfFilter {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            sample_rate: if sample_rate <= 0.0 { 44100.0 } else { sample_rate },
            filter_type: CytomicFilterType::Lowpass24,
            circuit_model: CircuitModel::Clean,
            stage1: CytomicSvfStage::new(),
            stage2: CytomicSvfStage::new(),
        }
    }

    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        if sample_rate > 0.0 {
            self.sample_rate = sample_rate;
        }
    }

    pub fn reset(&mut self) {
        self.stage1.reset();
        self.stage2.reset();
    }

    /// Process a single audio sample.
    ///
    /// - `input`: Audio sample
    /// - `cutoff_hz`: Cutoff frequency in Hz (20.0 .. 20000.0)
    /// - `resonance_norm`: Resonance in [0.0, 1.0] (1.0 = self-oscillation)
    /// - `morph_norm`: Morph parameter in [0.0, 1.0] (0 = LP, 0.33 = BP, 0.66 = HP, 1.0 = Notch)
    /// - `drive_db`: Drive gain in dB (0.0 = unity)
    pub fn process_sample(
        &mut self,
        input: f32,
        cutoff_hz: f32,
        resonance_norm: f32,
        morph_norm: f32,
        drive_db: f32,
    ) -> f32 {
        let sr = self.sample_rate;
        let nyquist_limit = (sr * 0.48).min(20000.0);
        let fc = cutoff_hz.clamp(10.0, nyquist_limit);

        // Prewarped cutoff parameter g = tan(pi * fc / fs)
        let g = (PI * fc / sr).tan().clamp(0.0001, 20.0);

        // Resonance mapping: 0.0 -> damping k = 2.0 (Q = 0.5, critical); 1.0 -> damping k = 0.01 (Q = 100)
        let res = resonance_norm.clamp(0.0, 1.0);
        let k = 2.0 * (1.0 - 0.995 * res.powf(0.75));

        // Drive linear scale
        let drive = 10.0f32.powf(drive_db.clamp(0.0, 24.0) / 20.0);

        let (lp1, bp1, hp1, notch1) = self.stage1.process(input, g, k, self.circuit_model, drive);

        let out_sample = match self.filter_type {
            CytomicFilterType::Lowpass12 => lp1,
            CytomicFilterType::Highpass12 => hp1,
            CytomicFilterType::Bandpass12 => bp1,
            CytomicFilterType::Notch12 => notch1,
            CytomicFilterType::Lowpass24 => {
                let (lp2, _, _, _) = self.stage2.process(lp1, g, k, self.circuit_model, 1.0);
                lp2
            }
            CytomicFilterType::Highpass24 => {
                let (_, _, hp2, _) = self.stage2.process(hp1, g, k, self.circuit_model, 1.0);
                hp2
            }
            CytomicFilterType::Bandpass24 => {
                let (_, bp2, _, _) = self.stage2.process(bp1, g, k, self.circuit_model, 1.0);
                bp2
            }
            CytomicFilterType::Morph => {
                let m = morph_norm.clamp(0.0, 1.0);
                if m < 0.333333 {
                    let t = m / 0.333333;
                    (1.0 - t) * lp1 + t * bp1
                } else if m < 0.666667 {
                    let t = (m - 0.333333) / 0.333334;
                    (1.0 - t) * bp1 + t * hp1
                } else {
                    let t = (m - 0.666667) / 0.333333;
                    (1.0 - t) * hp1 + t * notch1
                }
            }
        };

        // Safety clipping under extreme self-oscillation
        out_sample.clamp(-2.0, 2.0)
    }
}

/// Dual Cytomic ZDF filter unit with Series, Parallel, and Split routing.
#[derive(Debug, Clone)]
pub struct DualCytomicFilter {
    pub filter1: CytomicZdfFilter,
    pub filter2: CytomicZdfFilter,
    pub routing: FilterRouting,
}

impl DualCytomicFilter {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            filter1: CytomicZdfFilter::new(sample_rate),
            filter2: CytomicZdfFilter::new(sample_rate),
            routing: FilterRouting::Series,
        }
    }

    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        self.filter1.set_sample_rate(sample_rate);
        self.filter2.set_sample_rate(sample_rate);
    }

    pub fn reset(&mut self) {
        self.filter1.reset();
        self.filter2.reset();
    }

    /// Process audio through dual filters according to routing topology.
    ///
    /// - In `Series`: input passes through Filter 1, then Filter 2.
    /// - In `Parallel`: input passes through both filters and their outputs are summed (scaled by 0.5).
    /// - In `Split`: `osc1_sample` routes to Filter 1, `osc2_sample` routes to Filter 2, and outputs are summed.
    pub fn process(
        &mut self,
        osc1_sample: f32,
        osc2_sample: f32,
        cutoff1: f32,
        res1: f32,
        morph1: f32,
        cutoff2: f32,
        res2: f32,
        morph2: f32,
    ) -> f32 {
        match self.routing {
            FilterRouting::Series => {
                let mixed_in = osc1_sample + osc2_sample;
                let stage1_out = self.filter1.process_sample(mixed_in, cutoff1, res1, morph1, 0.0);
                self.filter2.process_sample(stage1_out, cutoff2, res2, morph2, 0.0)
            }
            FilterRouting::Parallel => {
                let mixed_in = osc1_sample + osc2_sample;
                let out1 = self.filter1.process_sample(mixed_in, cutoff1, res1, morph1, 0.0);
                let out2 = self.filter2.process_sample(mixed_in, cutoff2, res2, morph2, 0.0);
                0.5 * (out1 + out2)
            }
            FilterRouting::Split => {
                let out1 = self.filter1.process_sample(osc1_sample, cutoff1, res1, morph1, 0.0);
                let out2 = self.filter2.process_sample(osc2_sample, cutoff2, res2, morph2, 0.0);
                0.5 * (out1 + out2)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cytomic_svf_lowpass_attenuation() {
        // Process low frequency 100 Hz vs high frequency 10 kHz through 500 Hz cutoff
        let mut filter_low = CytomicZdfFilter::new(44100.0);
        filter_low.filter_type = CytomicFilterType::Lowpass24;
        let mut filter_high = CytomicZdfFilter::new(44100.0);
        filter_high.filter_type = CytomicFilterType::Lowpass24;

        let mut low_mag = 0.0f32;
        let mut high_mag = 0.0f32;

        for i in 0..1000 {
            let t = i as f32 / 44100.0;
            let low_in = (2.0 * PI * 100.0 * t).sin();
            let high_in = (2.0 * PI * 10000.0 * t).sin();

            let out_low = filter_low.process_sample(low_in, 500.0, 0.1, 0.0, 0.0);
            if i > 500 {
                low_mag = low_mag.max(out_low.abs());
            }

            let out_high = filter_high.process_sample(high_in, 500.0, 0.1, 0.0, 0.0);
            if i > 500 {
                high_mag = high_mag.max(out_high.abs());
            }
        }

        assert!(low_mag > 0.5, "Passband audio must not be heavily attenuated");
        assert!(high_mag < 0.05, "Stopband audio at 10 kHz must be rolled off > 20 dB");
    }

    #[test]
    fn test_cytomic_svf_self_oscillation_stability() {
        let mut filter = CytomicZdfFilter::new(44100.0);
        filter.filter_type = CytomicFilterType::Lowpass24;
        filter.circuit_model = CircuitModel::MS2;

        // Feed impulse then let self-oscillate at resonance 1.0
        let _ = filter.process_sample(1.0, 1000.0, 1.0, 0.0, 0.0);

        for _ in 0..2048 {
            let out = filter.process_sample(0.0, 1000.0, 1.0, 0.0, 0.0);
            assert!(out.is_finite(), "Output must be finite");
            assert!(out.abs() <= 2.0, "Self-oscillation must remain bounded <= 2.0");
        }
    }

    #[test]
    fn test_circuit_models_execution() {
        for model in [
            CircuitModel::Clean,
            CircuitModel::OSR,
            CircuitModel::MS2,
            CircuitModel::SMP,
            CircuitModel::PRD,
        ] {
            let mut filter = CytomicZdfFilter::new(44100.0);
            filter.circuit_model = model;
            let out = filter.process_sample(0.5, 1000.0, 0.5, 0.0, 6.0);
            assert!(out.is_finite());
        }
    }
}
