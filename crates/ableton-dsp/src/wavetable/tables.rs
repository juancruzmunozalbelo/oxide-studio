//! Wavetable sets and continuous 2D wavetable morphing.

use std::f32::consts::PI;

pub const TABLE_SIZE: usize = 2048;
pub const NUM_FRAMES: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WavetableCategory {
    BasicShapes,
    Harmonics,
    Formant,
    Digital,
    Vintage,
}

/// A wavetable set holding NUM_FRAMES single-cycle waveforms of TABLE_SIZE samples.
#[derive(Debug, Clone)]
pub struct WavetableSet {
    pub frames: [[f32; TABLE_SIZE]; NUM_FRAMES],
}

impl WavetableSet {
    /// Generate a factory wavetable set by category.
    pub fn generate(category: WavetableCategory) -> Self {
        let mut frames = [[0.0f32; TABLE_SIZE]; NUM_FRAMES];

        for frame_idx in 0..NUM_FRAMES {
            let morph = frame_idx as f32 / (NUM_FRAMES - 1) as f32; // 0.0 .. 1.0

            for i in 0..TABLE_SIZE {
                let phase = i as f32 / TABLE_SIZE as f32; // 0.0 .. 1.0
                let angle = 2.0 * PI * phase;

                frames[frame_idx][i] = match category {
                    WavetableCategory::BasicShapes => {
                        // Morph: Sine (0.0) -> Triangle (0.33) -> Saw (0.66) -> Square (1.0)
                        if morph < 0.333333 {
                            let t = morph / 0.333333;
                            let sine = angle.sin();
                            let tri = 2.0 * (2.0 * (phase - (phase + 0.5).floor()).abs()) - 1.0;
                            (1.0 - t) * sine + t * tri
                        } else if morph < 0.666667 {
                            let t = (morph - 0.333333) / 0.333334;
                            let tri = 2.0 * (2.0 * (phase - (phase + 0.5).floor()).abs()) - 1.0;
                            let saw = 2.0 * (phase - (phase + 0.5).floor());
                            (1.0 - t) * tri + t * saw
                        } else {
                            let t = (morph - 0.666667) / 0.333333;
                            let saw = 2.0 * (phase - (phase + 0.5).floor());
                            let sq = if phase < 0.5 { 1.0 } else { -1.0 };
                            (1.0 - t) * saw + t * sq
                        }
                    }
                    WavetableCategory::Harmonics => {
                        // Additive harmonic series with morphing spectral tilt and odd/even balance
                        let mut sum = 0.0f32;
                        let num_partials = 16;
                        for h in 1..=num_partials {
                            let weight = if h % 2 == 0 {
                                morph * (1.0 / h as f32)
                            } else {
                                (1.0 - morph * 0.5) * (1.0 / (h as f32).powf(1.0 + morph))
                            };
                            sum += weight * (h as f32 * angle).sin();
                        }
                        sum * 0.7
                    }
                    WavetableCategory::Formant => {
                        // Vowel formant sweeps: A (0.0) -> E (0.25) -> I (0.5) -> O (0.75) -> U (1.0)
                        let center_f1 = 2.0 + morph * 8.0;
                        let center_f2 = 6.0 + (1.0 - morph) * 12.0;
                        let v1 = (center_f1 * angle).sin() * (-(angle - PI).powi(2) / 1.5).exp();
                        let v2 = 0.6 * (center_f2 * angle).sin() * (-(angle - 1.2 * PI).powi(2) / 1.0).exp();
                        (v1 + v2) * 1.2
                    }
                    WavetableCategory::Digital => {
                        // Complex FM and digital sync sweep
                        let mod_idx = 1.0 + morph * 6.0;
                        let carrier = (angle + mod_idx * (2.0 * angle).sin()).sin();
                        let fold = ((carrier * (1.0 + morph * 2.0)).sin()).clamp(-1.0, 1.0);
                        fold * 0.8
                    }
                    WavetableCategory::Vintage => {
                        // Saturated analog-style waves with subtle asymmetric clipping
                        let raw_saw = 2.0 * (phase - (phase + 0.5).floor());
                        let drive = 1.0 + morph * 2.5;
                        let sat = (raw_saw * drive).tanh();
                        // Add subtle sub-octave warmth
                        let warm = (angle * 0.5).sin() * 0.15 * morph;
                        sat * 0.85 + warm
                    }
                };
            }
        }

        Self { frames }
    }

    /// Read sample with continuous 2D position morphing and Hermite/linear phase interpolation.
    ///
    /// - `phase`: normalized cycle phase in [0.0, 1.0)
    /// - `table_pos`: normalized position across frames in [0.0, 1.0]
    #[inline(always)]
    pub fn read_interpolated(&self, phase: f32, table_pos: f32) -> f32 {
        let pos_clamped = table_pos.clamp(0.0, 1.0);
        let frame_float = pos_clamped * (NUM_FRAMES - 1) as f32;
        let frame_idx = frame_float.floor() as usize;
        let frame_frac = frame_float - frame_idx as f32;
        let next_frame_idx = (frame_idx + 1).min(NUM_FRAMES - 1);

        let p = phase.fract();
        let p_norm = if p < 0.0 { p + 1.0 } else { p };
        let sample_float = p_norm * TABLE_SIZE as f32;
        let sample_idx = (sample_float.floor() as usize) % TABLE_SIZE;
        let sample_frac = sample_float - sample_idx as f32;
        let next_sample_idx = (sample_idx + 1) % TABLE_SIZE;

        // Bilinear interpolation between frame and next sample
        let s00 = self.frames[frame_idx][sample_idx];
        let s01 = self.frames[frame_idx][next_sample_idx];
        let s10 = self.frames[next_frame_idx][sample_idx];
        let s11 = self.frames[next_frame_idx][next_sample_idx];

        let interp0 = s00 + sample_frac * (s01 - s00);
        let interp1 = s10 + sample_frac * (s11 - s10);

        interp0 + frame_frac * (interp1 - interp0)
    }
}
