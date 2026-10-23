//! Tempo-synced ping-pong delay with feedback lowpass/highpass filter.


pub const MAX_DELAY_SAMPLES: usize = 131072; // ~2.97 seconds at 44.1 kHz

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DelaySubdivision {
    Sixteenth,     // 1/16 (0.25 beats)
    SixteenthDotted, // 3/16 (0.375 beats)
    EighthTriplet, // 1/12 (0.3333 beats)
    Eighth,        // 1/8 (0.5 beats)
    EighthDotted,  // 3/8 (0.75 beats)
    Quarter,       // 1/4 (1.0 beats)
}

impl DelaySubdivision {
    pub fn beats(&self) -> f64 {
        match self {
            Self::Sixteenth => 0.25,
            Self::SixteenthDotted => 0.375,
            Self::EighthTriplet => 1.0 / 3.0,
            Self::Eighth => 0.5,
            Self::EighthDotted => 0.75,
            Self::Quarter => 1.0,
        }
    }
}

pub struct TempoDelay {
    buffer_left: Box<[f32; MAX_DELAY_SAMPLES]>,
    buffer_right: Box<[f32; MAX_DELAY_SAMPLES]>,
    write_pos: usize,
    sample_rate: f32,
    bpm: f64,
    pub subdivision: DelaySubdivision,
    pub delay_samples_l: usize,
    pub delay_samples_r: usize,
    pub feedback: f32, // [0.0, 0.98]
    pub wet: f32,      // [0.0, 1.0]
    pub ping_pong: bool,
    pub damping: f32,  // [0.0, 1.0]
    // Filter states
    filter_state_l: f32,
    filter_state_r: f32,
}

impl Default for TempoDelay {
    fn default() -> Self {
        Self::new(44100.0, 120.0)
    }
}

impl TempoDelay {
    pub fn new(sample_rate: f32, bpm: f64) -> Self {
        let sr = if sample_rate <= 0.0 { 44100.0 } else { sample_rate };
        let mut delay = Self {
            buffer_left: Box::new([0.0; MAX_DELAY_SAMPLES]),
            buffer_right: Box::new([0.0; MAX_DELAY_SAMPLES]),
            write_pos: 0,
            sample_rate: sr,
            bpm: if bpm <= 0.0 { 120.0 } else { bpm },
            subdivision: DelaySubdivision::EighthDotted,
            delay_samples_l: 0,
            delay_samples_r: 0,
            feedback: 0.45,
            wet: 0.35,
            ping_pong: true,
            damping: 0.3,
            filter_state_l: 0.0,
            filter_state_r: 0.0,
        };
        delay.recompute_delay_times();
        delay
    }

    pub fn set_tempo(&mut self, bpm: f64) {
        if bpm > 20.0 && (self.bpm - bpm).abs() > 0.01 {
            self.bpm = bpm;
            self.recompute_delay_times();
        }
    }

    pub fn set_subdivision(&mut self, sub: DelaySubdivision) {
        self.subdivision = sub;
        self.recompute_delay_times();
    }

    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        if sample_rate > 0.0 {
            self.sample_rate = sample_rate;
            self.recompute_delay_times();
        }
    }

    fn recompute_delay_times(&mut self) {
        let beats = self.subdivision.beats();
        let seconds_per_beat = 60.0 / self.bpm;
        let base_samples = (beats * seconds_per_beat * self.sample_rate as f64).round() as usize;
        let clamped = base_samples.clamp(1, MAX_DELAY_SAMPLES - 1);

        self.delay_samples_l = clamped;
        // In ping-pong mode, slightly offset R or match base
        self.delay_samples_r = if self.ping_pong {
            clamped
        } else {
            clamped
        };
    }

    pub fn reset(&mut self) {
        self.buffer_left.fill(0.0);
        self.buffer_right.fill(0.0);
        self.write_pos = 0;
        self.filter_state_l = 0.0;
        self.filter_state_r = 0.0;
    }

    /// Process a block of stereo planar samples with zero allocations.
    pub fn process_block(&mut self, left: &mut [f32], right: &mut [f32]) {
        let len = left.len().min(right.len());
        let delay_l = self.delay_samples_l;
        let delay_r = self.delay_samples_r;
        let fb = self.feedback.clamp(0.0, 0.98);
        let alpha = (1.0 - self.damping.clamp(0.0, 0.95)).clamp(0.05, 1.0);

        for i in 0..len {
            let in_l = left[i];
            let in_r = right[i];

            // Read delayed samples
            let read_idx_l = (self.write_pos + MAX_DELAY_SAMPLES - delay_l) % MAX_DELAY_SAMPLES;
            let read_idx_r = (self.write_pos + MAX_DELAY_SAMPLES - delay_r) % MAX_DELAY_SAMPLES;

            let delayed_l = self.buffer_left[read_idx_l];
            let delayed_r = self.buffer_right[read_idx_r];

            // One-pole lowpass damping filter on feedback
            self.filter_state_l += alpha * (delayed_l - self.filter_state_l);
            self.filter_state_r += alpha * (delayed_r - self.filter_state_r);

            let filtered_fb_l = self.filter_state_l * fb;
            let filtered_fb_r = self.filter_state_r * fb;

            if self.ping_pong {
                // Cross-feedback ping-pong
                self.buffer_left[self.write_pos] = in_l + filtered_fb_r;
                self.buffer_right[self.write_pos] = in_r + filtered_fb_l;
            } else {
                // Independent stereo feedback
                self.buffer_left[self.write_pos] = in_l + filtered_fb_l;
                self.buffer_right[self.write_pos] = in_r + filtered_fb_r;
            }

            self.write_pos = (self.write_pos + 1) % MAX_DELAY_SAMPLES;

            // Wet/dry mix
            left[i] = in_l * (1.0 - self.wet) + delayed_l * self.wet;
            right[i] = in_r * (1.0 - self.wet) + delayed_r * self.wet;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tempo_delay_timing() {
        let mut delay = TempoDelay::new(44100.0, 120.0);
        delay.set_subdivision(DelaySubdivision::Quarter); // 1 beat at 120 BPM = 0.5s = 22050 samples
        assert_eq!(delay.delay_samples_l, 22050);

        delay.set_subdivision(DelaySubdivision::Eighth); // 0.5 beat = 0.25s = 11025 samples
        assert_eq!(delay.delay_samples_l, 11025);
    }

    #[test]
    fn test_delay_processing() {
        let mut delay = TempoDelay::new(44100.0, 120.0);
        delay.wet = 1.0;
        delay.feedback = 0.5;
        delay.delay_samples_l = 10;
        delay.delay_samples_r = 10;

        let mut left = [0.0f32; 20];
        let mut right = [0.0f32; 20];
        left[0] = 1.0;
        right[0] = 1.0;

        delay.process_block(&mut left, &mut right);

        // At sample index 10, the delayed pulse should arrive
        assert_eq!(left[0], 0.0);
        assert!((left[10] - 1.0).abs() < 1e-3);
    }
}
