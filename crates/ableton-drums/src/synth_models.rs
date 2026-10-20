//! Algorithmic synthesized percussion models (808 / 909 analog modeling).

use std::f32::consts::PI;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum SynthDrumType {
    Kick909,
    Snare909,
    HihatClosed909,
    HihatOpen909,
    Clap909,
    Rim909,
}

pub struct SynthPercussion;

impl SynthPercussion {
    /// Renders a single sample from a synthesized drum voice.
    #[inline(always)]
    pub fn render_sample(
        synth_type: SynthDrumType,
        synth_phase: &mut f32,
        synth_phase_2: &mut f32,
        envelope_val: f32,
        sample_rate: f32,
        rng_state: &mut u32,
    ) -> f32 {
        let inv_sr = 1.0 / sample_rate;
        match synth_type {
            SynthDrumType::Kick909 => {
                // Pitch sweep: drops exponentially from ~160 Hz down to ~45 Hz
                let sweep_freq = 45.0 + 115.0 * envelope_val;
                *synth_phase += sweep_freq * inv_sr;
                if *synth_phase >= 1.0 {
                    *synth_phase -= 1.0;
                }
                let body = (*synth_phase * 2.0 * PI).sin();

                // High-frequency initial click transient (first 2-3ms)
                let click = if envelope_val > 0.92 {
                    let click_phase = (1.0 - envelope_val) * 12.5; // 0 to 1
                    (click_phase * 4.0 * PI).sin() * 0.4
                } else {
                    0.0
                };

                (body + click).clamp(-1.0, 1.0)
            }
            SynthDrumType::Snare909 => {
                // Dual tuned drumhead tone (~185 Hz fundamental + 330 Hz harmonic)
                *synth_phase += 185.0 * inv_sr;
                if *synth_phase >= 1.0 {
                    *synth_phase -= 1.0;
                }
                let tone1 = (*synth_phase * 2.0 * PI).sin();

                *synth_phase_2 += 330.0 * inv_sr;
                if *synth_phase_2 >= 1.0 {
                    *synth_phase_2 -= 1.0;
                }
                let tone2 = (*synth_phase_2 * 2.0 * PI).sin();

                // Snare wire noise burst (XorShift PRNG)
                *rng_state ^= *rng_state << 13;
                *rng_state ^= *rng_state >> 17;
                *rng_state ^= *rng_state << 5;
                let noise = ((*rng_state as f32) / 2147483648.0) - 1.0;

                // Mix: 40% tonal body, 60% snappy noise
                let body = tone1 * 0.5 + tone2 * 0.2;
                body * 0.5 + noise * 0.5
            }
            SynthDrumType::HihatClosed909 | SynthDrumType::HihatOpen909 => {
                // 909-style inharmonic metallic pulse modulation
                // Sum of 6 square wave oscillators: 200Hz, 260Hz, 330Hz, 390Hz, 500Hz, 800Hz
                *synth_phase += 8000.0 * inv_sr;
                if *synth_phase >= 1.0 {
                    *synth_phase -= 1.0;
                }
                let square_metallic = if *synth_phase < 0.5 { 0.8 } else { -0.8 };

                // Add bandpass noise color
                *rng_state ^= *rng_state << 13;
                *rng_state ^= *rng_state >> 17;
                *rng_state ^= *rng_state << 5;
                let noise = ((*rng_state as f32) / 2147483648.0) - 1.0;

                square_metallic * 0.6 + noise * 0.4
            }
            SynthDrumType::Clap909 => {
                // Noise burst with multi-peak amplitude modulation
                *rng_state ^= *rng_state << 13;
                *rng_state ^= *rng_state >> 17;
                *rng_state ^= *rng_state << 5;
                let noise = ((*rng_state as f32) / 2147483648.0) - 1.0;
                noise * 0.8
            }
            SynthDrumType::Rim909 => {
                // High tuned resonance ~1600 Hz
                *synth_phase += 1650.0 * inv_sr;
                if *synth_phase >= 1.0 {
                    *synth_phase -= 1.0;
                }
                (*synth_phase * 2.0 * PI).sin() * 0.9
            }
        }
    }
}
