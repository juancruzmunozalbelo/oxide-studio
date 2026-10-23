//! Drum voice state machine and interpolation algorithms.


#[derive(Clone, Copy, Debug)]
pub struct DrumVoice {
    pub active: bool,
    pub pad_idx: u8,
    pub choke_group: Option<u8>,
    pub phase: f64,
    pub phase_step: f64,
    pub velocity: f32,
    pub envelope_val: f32,
    pub envelope_decay: f32,
    pub is_fading_out: bool,
    pub fade_out_coeff: f32,
    pub synth_phase: f32,
    pub synth_phase_2: f32,
    pub synth_pitch: f32,
    pub rng_state: u32,
}

impl Default for DrumVoice {
    fn default() -> Self {
        Self {
            active: false,
            pad_idx: 0,
            choke_group: None,
            phase: 0.0,
            phase_step: 1.0,
            velocity: 1.0,
            envelope_val: 0.0,
            envelope_decay: 0.999,
            is_fading_out: false,
            fade_out_coeff: 0.95,
            synth_phase: 0.0,
            synth_phase_2: 0.0,
            synth_pitch: 440.0,
            rng_state: 0x98765432,
        }
    }
}

impl DrumVoice {
    /// Read sample with linear interpolation.
    #[inline(always)]
    pub fn interpolate_linear(sample_data: &[f32], channels: usize, phase: f64) -> f32 {
        let total_frames = sample_data.len() / channels;
        let frame_idx = phase as usize;
        if frame_idx >= total_frames {
            return 0.0;
        }
        let frac = (phase - frame_idx as f64) as f32;
        let s0 = sample_data[frame_idx * channels];
        let s1 = if frame_idx + 1 < total_frames {
            sample_data[(frame_idx + 1) * channels]
        } else {
            s0
        };
        s0 + frac * (s1 - s0)
    }

    /// Read sample with 4-point Hermite cubic interpolation for higher fidelity.
    #[inline(always)]
    pub fn interpolate_cubic(sample_data: &[f32], channels: usize, phase: f64) -> f32 {
        let total_frames = sample_data.len() / channels;
        let idx = phase as usize;
        if idx >= total_frames {
            return 0.0;
        }
        let frac = (phase - idx as f64) as f32;

        let s_prev = if idx > 0 {
            sample_data[(idx - 1) * channels]
        } else {
            sample_data[0]
        };
        let s0 = sample_data[idx * channels];
        let s1 = if idx + 1 < total_frames {
            sample_data[(idx + 1) * channels]
        } else {
            s0
        };
        let s2 = if idx + 2 < total_frames {
            sample_data[(idx + 2) * channels]
        } else {
            s1
        };

        let c0 = s0;
        let c1 = 0.5 * (s1 - s_prev);
        let c2 = s_prev - 2.5 * s0 + 2.0 * s1 - 0.5 * s2;
        let c3 = 0.5 * (s2 - s_prev) + 1.5 * (s0 - s1);

        ((c3 * frac + c2) * frac + c1) * frac + c0
    }
}
