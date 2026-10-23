//! Multi-track mixer with volume, pan, mute, solo, and peak/RMS ballistics metering.

use std::f32::consts::PI;
use crate::limiter::MasterLimiter;

pub const NUM_TRACKS: usize = 16;
pub const METER_WINDOW_SECONDS: f32 = 0.300; // 300ms sliding RMS window

#[derive(Debug, Clone)]
pub struct MixerTrack {
    pub name: String,
    pub volume_db: f32,
    pub volume_linear: f32,
    pub pan: f32, // [-1.0, 1.0]
    pub mute: bool,
    pub solo: bool,
    pub peak_l: f32,
    pub peak_r: f32,
    pub rms_l: f32,
    pub rms_r: f32,
    // Metering ballistics state
    sum_sq_l: f32,
    sum_sq_r: f32,
}

impl Default for MixerTrack {
    fn default() -> Self {
        Self::new("Track")
    }
}

impl MixerTrack {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            volume_db: 0.0,
            volume_linear: 1.0,
            pan: 0.0,
            mute: false,
            solo: false,
            peak_l: 0.0,
            peak_r: 0.0,
            rms_l: 0.0,
            rms_r: 0.0,
            sum_sq_l: 0.0,
            sum_sq_r: 0.0,
        }
    }

    pub fn set_volume_db(&mut self, db: f32) {
        self.volume_db = db.clamp(-60.0, 6.0);
        if self.volume_db <= -59.5 {
            self.volume_linear = 0.0;
        } else {
            self.volume_linear = 10.0_f32.powf(self.volume_db / 20.0);
        }
    }

    pub fn set_pan(&mut self, pan: f32) {
        self.pan = pan.clamp(-1.0, 1.0);
    }
}

pub struct Mixer {
    pub tracks: [MixerTrack; NUM_TRACKS],
    pub master_volume_linear: f32,
    pub master_pan: f32,
    pub master_limiter: MasterLimiter,
    pub master_peak_l: f32,
    pub master_peak_r: f32,
    pub master_rms_l: f32,
    pub master_rms_r: f32,
    master_sum_sq_l: f32,
    master_sum_sq_r: f32,
    sample_rate: f32,
    meter_alpha: f32,
    peak_decay_coeff: f32,
}

impl Default for Mixer {
    fn default() -> Self {
        Self::new(44100.0)
    }
}

impl Mixer {
    pub fn new(sample_rate: f32) -> Self {
        let sr = if sample_rate <= 0.0 { 44100.0 } else { sample_rate };
        // 300ms time constant for sliding RMS window: alpha = 1 - exp(-1 / (0.3 * sr))
        let meter_alpha = 1.0 - (-1.0 / (METER_WINDOW_SECONDS * sr)).exp();
        // ~1.5s decay for peak hold meter
        let peak_decay_coeff = (-1.0 / (1.5 * sr)).exp();

        let tracks = std::array::from_fn(|i| MixerTrack::new(&format!("Track {}", i + 1)));

        Self {
            tracks,
            master_volume_linear: 1.0,
            master_pan: 0.0,
            master_limiter: MasterLimiter::new(sr),
            master_peak_l: 0.0,
            master_peak_r: 0.0,
            master_rms_l: 0.0,
            master_rms_r: 0.0,
            master_sum_sq_l: 0.0,
            master_sum_sq_r: 0.0,
            sample_rate: sr,
            meter_alpha,
            peak_decay_coeff,
        }
    }

    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        if sample_rate > 0.0 {
            self.sample_rate = sample_rate;
            self.meter_alpha = 1.0 - (-1.0 / (METER_WINDOW_SECONDS * sample_rate)).exp();
            self.peak_decay_coeff = (-1.0 / (1.5 * sample_rate)).exp();
            self.master_limiter.set_sample_rate(sample_rate);
        }
    }

    /// Check if any track has solo engaged.
    pub fn is_any_solo(&self) -> bool {
        self.tracks.iter().any(|t| t.solo)
    }

    /// Process a track's audio block, updating meters and applying volume/pan.
    /// Returns true if the track is audible. Zero allocations.
    pub fn process_track(
        &mut self,
        track_idx: usize,
        left: &mut [f32],
        right: &mut [f32],
    ) -> bool {
        if track_idx >= NUM_TRACKS {
            return false;
        }

        let any_solo = self.is_any_solo();
        let track = &mut self.tracks[track_idx];

        // Solo / Mute logic:
        // If any track is soloed, this track must be soloed to play.
        // If track is muted, it is silenced.
        let is_audible = if any_solo {
            track.solo && !track.mute
        } else {
            !track.mute
        };

        let len = left.len().min(right.len());
        let theta = (track.pan + 1.0) * 0.25 * PI;
        let pan_l = theta.cos();
        let pan_r = theta.sin();
        let gain = if is_audible { track.volume_linear } else { 0.0 };

        for i in 0..len {
            let l = left[i] * gain * pan_l;
            let r = right[i] * gain * pan_r;

            // Metering ballistics
            let peak_cur_l = l.abs();
            let peak_cur_r = r.abs();
            track.peak_l = peak_cur_l.max(track.peak_l * self.peak_decay_coeff);
            track.peak_r = peak_cur_r.max(track.peak_r * self.peak_decay_coeff);

            track.sum_sq_l += self.meter_alpha * (l * l - track.sum_sq_l);
            track.sum_sq_r += self.meter_alpha * (r * r - track.sum_sq_r);

            left[i] = l;
            right[i] = r;
        }

        track.rms_l = track.sum_sq_l.max(0.0).sqrt();
        track.rms_r = track.sum_sq_r.max(0.0).sqrt();

        is_audible
    }

    /// Process master bus: sums to master, updates master peak/RMS meters, runs master limiter.
    pub fn process_master(&mut self, left: &mut [f32], right: &mut [f32]) {
        let len = left.len().min(right.len());

        let theta = (self.master_pan + 1.0) * 0.25 * PI;
        let pan_l = theta.cos();
        let pan_r = theta.sin();
        let gain = self.master_volume_linear;

        for i in 0..len {
            left[i] *= gain * pan_l;
            right[i] *= gain * pan_r;
        }

        // Run master lookahead brickwall limiter (-0.3 dBFS ceiling)
        self.master_limiter.process_block(left, right);

        // Update master meters
        for i in 0..len {
            let l = left[i];
            let r = right[i];
            let pl = l.abs();
            let pr = r.abs();

            self.master_peak_l = pl.max(self.master_peak_l * self.peak_decay_coeff);
            self.master_peak_r = pr.max(self.master_peak_r * self.peak_decay_coeff);

            self.master_sum_sq_l += self.meter_alpha * (l * l - self.master_sum_sq_l);
            self.master_sum_sq_r += self.meter_alpha * (r * r - self.master_sum_sq_r);
        }

        self.master_rms_l = self.master_sum_sq_l.max(0.0).sqrt();
        self.master_rms_r = self.master_sum_sq_r.max(0.0).sqrt();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mixer_solo_logic() {
        let mut mixer = Mixer::new(44100.0);
        mixer.tracks[0].solo = true;

        let mut l0 = [1.0f32; 64];
        let mut r0 = [1.0f32; 64];
        let audible0 = mixer.process_track(0, &mut l0, &mut r0);
        assert!(audible0);
        assert!(l0[0] > 0.0);

        let mut l1 = [1.0f32; 64];
        let mut r1 = [1.0f32; 64];
        let audible1 = mixer.process_track(1, &mut l1, &mut r1);
        assert!(!audible1);
        assert_eq!(l1[0], 0.0);
    }

    #[test]
    fn test_mixer_mute_logic() {
        let mut mixer = Mixer::new(44100.0);
        mixer.tracks[0].mute = true;

        let mut l0 = [1.0f32; 64];
        let mut r0 = [1.0f32; 64];
        let audible0 = mixer.process_track(0, &mut l0, &mut r0);
        assert!(!audible0);
        assert_eq!(l0[0], 0.0);
    }
}
