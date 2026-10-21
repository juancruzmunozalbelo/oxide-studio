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
    LowTom909,
    MidTom909,
    HiTom909,
    Cowbell909,
    Crash909,
    Ride909,
    PercLow,
    PercHigh,
    Shaker,
    Tambourine,
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

        // XorShift PRNG for noise components
        let next_noise = |state: &mut u32| -> f32 {
            if *state == 0 {
                *state = 0x98765432;
            }
            *state ^= *state << 13;
            *state ^= *state >> 17;
            *state ^= *state << 5;
            ((*state as f32) / 2147483648.0) - 1.0
        };

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

                let noise = next_noise(rng_state);

                // Mix: 40% tonal body, 60% snappy noise
                let body = tone1 * 0.5 + tone2 * 0.2;
                body * 0.5 + noise * 0.5
            }
            SynthDrumType::LowTom909 => {
                // Pitched membrane sweep ~110 Hz -> 58 Hz
                let sweep_freq = 58.0 + 52.0 * envelope_val;
                *synth_phase += sweep_freq * inv_sr;
                if *synth_phase >= 1.0 {
                    *synth_phase -= 1.0;
                }
                let body = (*synth_phase * 2.0 * PI).sin();

                *synth_phase_2 += (sweep_freq * 1.48) * inv_sr;
                if *synth_phase_2 >= 1.0 {
                    *synth_phase_2 -= 1.0;
                }
                let skin = (*synth_phase_2 * 2.0 * PI).sin() * 0.25;

                let click = if envelope_val > 0.88 {
                    let click_phase = (1.0 - envelope_val) * 8.33;
                    (click_phase * 4.0 * PI).sin() * 0.3
                } else {
                    0.0
                };

                (body * 0.75 + skin + click).clamp(-1.0, 1.0)
            }
            SynthDrumType::MidTom909 => {
                // Pitched membrane sweep ~155 Hz -> 88 Hz
                let sweep_freq = 88.0 + 67.0 * envelope_val;
                *synth_phase += sweep_freq * inv_sr;
                if *synth_phase >= 1.0 {
                    *synth_phase -= 1.0;
                }
                let body = (*synth_phase * 2.0 * PI).sin();

                *synth_phase_2 += (sweep_freq * 1.48) * inv_sr;
                if *synth_phase_2 >= 1.0 {
                    *synth_phase_2 -= 1.0;
                }
                let skin = (*synth_phase_2 * 2.0 * PI).sin() * 0.25;

                let click = if envelope_val > 0.88 {
                    let click_phase = (1.0 - envelope_val) * 8.33;
                    (click_phase * 4.0 * PI).sin() * 0.3
                } else {
                    0.0
                };

                (body * 0.75 + skin + click).clamp(-1.0, 1.0)
            }
            SynthDrumType::HiTom909 => {
                // Pitched membrane sweep ~220 Hz -> 125 Hz
                let sweep_freq = 125.0 + 95.0 * envelope_val;
                *synth_phase += sweep_freq * inv_sr;
                if *synth_phase >= 1.0 {
                    *synth_phase -= 1.0;
                }
                let body = (*synth_phase * 2.0 * PI).sin();

                *synth_phase_2 += (sweep_freq * 1.48) * inv_sr;
                if *synth_phase_2 >= 1.0 {
                    *synth_phase_2 -= 1.0;
                }
                let skin = (*synth_phase_2 * 2.0 * PI).sin() * 0.25;

                let click = if envelope_val > 0.88 {
                    let click_phase = (1.0 - envelope_val) * 8.33;
                    (click_phase * 4.0 * PI).sin() * 0.3
                } else {
                    0.0
                };

                (body * 0.75 + skin + click).clamp(-1.0, 1.0)
            }
            SynthDrumType::Rim909 => {
                // High tuned resonance ~1600 Hz
                *synth_phase += 1650.0 * inv_sr;
                if *synth_phase >= 1.0 {
                    *synth_phase -= 1.0;
                }
                (*synth_phase * 2.0 * PI).sin() * 0.9
            }
            SynthDrumType::Clap909 => {
                // Noise burst with multi-peak amplitude modulation
                let noise = next_noise(rng_state);
                let clap_pulse = if envelope_val > 0.6 {
                    let step = ((1.0 - envelope_val) * 20.0) as usize % 2;
                    if step == 0 { 0.9 } else { 0.4 }
                } else {
                    0.8
                };
                noise * clap_pulse
            }
            SynthDrumType::Cowbell909 => {
                // Classic 808/909 dual square/bandpass metallic resonance at 540 Hz + 800 Hz
                *synth_phase += 540.0 * inv_sr;
                if *synth_phase >= 1.0 {
                    *synth_phase -= 1.0;
                }
                *synth_phase_2 += 800.0 * inv_sr;
                if *synth_phase_2 >= 1.0 {
                    *synth_phase_2 -= 1.0;
                }
                let sq1 = if *synth_phase < 0.5 { 0.5 } else { -0.5 };
                let sq2 = if *synth_phase_2 < 0.5 { 0.5 } else { -0.5 };
                let sine1 = (*synth_phase * 2.0 * PI).sin();
                let sine2 = (*synth_phase_2 * 2.0 * PI).sin();
                (sq1 * 0.3 + sq2 * 0.3 + sine1 * 0.25 + sine2 * 0.25).clamp(-1.0, 1.0)
            }
            SynthDrumType::HihatClosed909 => {
                // 909-style inharmonic metallic pulse modulation
                *synth_phase += 7800.0 * inv_sr;
                if *synth_phase >= 1.0 {
                    *synth_phase -= 1.0;
                }
                let sq = if *synth_phase < 0.5 { 0.6 } else { -0.6 };
                let noise = next_noise(rng_state);
                (sq * 0.5 + noise * 0.5).clamp(-1.0, 1.0)
            }
            SynthDrumType::HihatOpen909 => {
                *synth_phase += 7200.0 * inv_sr;
                if *synth_phase >= 1.0 {
                    *synth_phase -= 1.0;
                }
                *synth_phase_2 += 9600.0 * inv_sr;
                if *synth_phase_2 >= 1.0 {
                    *synth_phase_2 -= 1.0;
                }
                let sq1 = if *synth_phase < 0.5 { 0.5 } else { -0.5 };
                let sq2 = if *synth_phase_2 < 0.5 { 0.3 } else { -0.3 };
                let noise = next_noise(rng_state);
                (sq1 * 0.4 + sq2 * 0.3 + noise * 0.5).clamp(-1.0, 1.0)
            }
            SynthDrumType::Crash909 => {
                // Bright multi-oscillator metallic shimmer + high-frequency noise burst
                *synth_phase += 3120.0 * inv_sr;
                if *synth_phase >= 1.0 {
                    *synth_phase -= 1.0;
                }
                *synth_phase_2 += 4380.0 * inv_sr;
                if *synth_phase_2 >= 1.0 {
                    *synth_phase_2 -= 1.0;
                }
                let m1 = if *synth_phase < 0.5 { 0.35 } else { -0.35 };
                let m2 = if *synth_phase_2 < 0.5 { 0.35 } else { -0.35 };
                let noise = next_noise(rng_state);
                (m1 * 0.3 + m2 * 0.3 + noise * 0.6).clamp(-1.0, 1.0)
            }
            SynthDrumType::Ride909 => {
                // Bell ping at 1050 Hz + 1680 Hz + metallic shimmer
                *synth_phase += 1050.0 * inv_sr;
                if *synth_phase >= 1.0 {
                    *synth_phase -= 1.0;
                }
                *synth_phase_2 += 1680.0 * inv_sr;
                if *synth_phase_2 >= 1.0 {
                    *synth_phase_2 -= 1.0;
                }
                let bell1 = (*synth_phase * 2.0 * PI).sin();
                let bell2 = (*synth_phase_2 * 2.0 * PI).sin();
                let noise = next_noise(rng_state);
                (bell1 * 0.45 + bell2 * 0.35 + noise * 0.2).clamp(-1.0, 1.0)
            }
            SynthDrumType::PercLow => {
                // Woodblock / conga resonant ping ~290 Hz -> 210 Hz
                let sweep_freq = 210.0 + 80.0 * envelope_val;
                *synth_phase += sweep_freq * inv_sr;
                if *synth_phase >= 1.0 {
                    *synth_phase -= 1.0;
                }
                *synth_phase_2 += (sweep_freq * 2.76) * inv_sr;
                if *synth_phase_2 >= 1.0 {
                    *synth_phase_2 -= 1.0;
                }
                let tone1 = (*synth_phase * 2.0 * PI).sin();
                let tone2 = (*synth_phase_2 * 2.0 * PI).sin() * 0.3;
                (tone1 * 0.75 + tone2).clamp(-1.0, 1.0)
            }
            SynthDrumType::PercHigh => {
                // Agogo / high bongo ping ~620 Hz -> 480 Hz
                let sweep_freq = 480.0 + 140.0 * envelope_val;
                *synth_phase += sweep_freq * inv_sr;
                if *synth_phase >= 1.0 {
                    *synth_phase -= 1.0;
                }
                *synth_phase_2 += (sweep_freq * 1.62) * inv_sr;
                if *synth_phase_2 >= 1.0 {
                    *synth_phase_2 -= 1.0;
                }
                let tone1 = (*synth_phase * 2.0 * PI).sin();
                let tone2 = (*synth_phase_2 * 2.0 * PI).sin() * 0.35;
                (tone1 * 0.7 + tone2).clamp(-1.0, 1.0)
            }
            SynthDrumType::Shaker => {
                // High-passed shaped noise envelope burst
                let raw_noise = next_noise(rng_state);
                let hp = raw_noise - *synth_phase * 0.85;
                *synth_phase = raw_noise;
                (hp * 0.8).clamp(-1.0, 1.0)
            }
            SynthDrumType::Tambourine => {
                // Metallic jingle ring 3200 Hz + 4900 Hz + crisp noise
                *synth_phase += 3200.0 * inv_sr;
                if *synth_phase >= 1.0 {
                    *synth_phase -= 1.0;
                }
                *synth_phase_2 += 4900.0 * inv_sr;
                if *synth_phase_2 >= 1.0 {
                    *synth_phase_2 -= 1.0;
                }
                let ring1 = (*synth_phase * 2.0 * PI).sin();
                let ring2 = (*synth_phase_2 * 2.0 * PI).sin();
                let noise = next_noise(rng_state);
                (ring1 * 0.3 + ring2 * 0.3 + noise * 0.45).clamp(-1.0, 1.0)
            }
        }
    }
}
