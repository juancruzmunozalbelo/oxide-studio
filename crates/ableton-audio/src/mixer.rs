//! Modular Real-Time Audio Mixer supporting Unlimited Tracks, Group Summing, Return Buses,
//! Pre/Post Sends, DJ Crossfader, Track Delay, and Peak/RMS Ballistics.

use std::f32::consts::PI;
use ableton_fx::MasterLimiter;
use ableton_timeline::{CrossfadeAssign, OutputDestination, SendRoutingMode, TrackType};

pub const METER_WINDOW_SECONDS: f32 = 0.300; // 300 ms sliding RMS window
pub const MAX_DELAY_MS: f32 = 100.0;

/// Auxiliary send level on an audio channel strip.
#[derive(Debug, Clone, PartialEq)]
pub struct AudioSendState {
    pub return_index: usize,
    pub level_db: f32,
    pub gain_linear: f32,
    pub mode: SendRoutingMode,
}

impl AudioSendState {
    pub fn new(return_index: usize, level_db: f32, mode: SendRoutingMode) -> Self {
        let db = level_db.clamp(-70.0, 6.0);
        let gain = if db <= -59.5 { 0.0 } else { 10.0_f32.powf(db / 20.0) };
        Self {
            return_index,
            level_db: db,
            gain_linear: gain,
            mode,
        }
    }

    pub fn set_level_db(&mut self, db: f32) {
        self.level_db = db.clamp(-70.0, 6.0);
        self.gain_linear = if self.level_db <= -59.5 {
            0.0
        } else {
            10.0_f32.powf(self.level_db / 20.0)
        };
    }
}

/// Dynamic channel strip for an individual Track (Audio, MIDI, or Group).
#[derive(Debug, Clone)]
pub struct AudioTrackChannel {
    pub id: u32,
    pub name: String,
    pub track_type: TrackType,
    pub volume_db: f32,
    pub volume_linear: f32,
    pub pan: f32, // -1.0 .. +1.0
    pub mute: bool,
    pub solo: bool,
    pub arm: bool,
    pub crossfade_assign: CrossfadeAssign,
    pub track_delay_ms: f32, // -100.0 .. +100.0 ms
    pub output_destination: OutputDestination,
    pub sends: Vec<AudioSendState>,

    // Delay ring buffers (capacity = 100 ms at 192 kHz = 19200 frames)
    delay_buf_l: Vec<f32>,
    delay_buf_r: Vec<f32>,
    delay_write_ptr: usize,
    delay_samples: usize,

    // Metering ballistics
    pub peak_l: f32,
    pub peak_r: f32,
    pub rms_l: f32,
    pub rms_r: f32,
    sum_sq_l: f32,
    sum_sq_r: f32,
}

impl AudioTrackChannel {
    pub fn new(id: u32, name: &str, track_type: TrackType, sample_rate: f32) -> Self {
        let max_delay_frames = ((MAX_DELAY_MS / 1000.0) * sample_rate.max(44100.0)).ceil() as usize + 64;
        Self {
            id,
            name: name.to_string(),
            track_type,
            volume_db: 0.0,
            volume_linear: 1.0,
            pan: 0.0,
            mute: false,
            solo: false,
            arm: false,
            crossfade_assign: CrossfadeAssign::Off,
            track_delay_ms: 0.0,
            output_destination: OutputDestination::Master,
            sends: Vec::new(),
            delay_buf_l: vec![0.0; max_delay_frames],
            delay_buf_r: vec![0.0; max_delay_frames],
            delay_write_ptr: 0,
            delay_samples: 0,
            peak_l: 0.0,
            peak_r: 0.0,
            rms_l: 0.0,
            rms_r: 0.0,
            sum_sq_l: 0.0,
            sum_sq_r: 0.0,
        }
    }

    pub fn set_volume_db(&mut self, db: f32) {
        self.volume_db = db.clamp(-70.0, 6.0);
        self.volume_linear = if self.volume_db <= -59.5 {
            0.0
        } else {
            10.0_f32.powf(self.volume_db / 20.0)
        };
    }

    pub fn set_pan(&mut self, pan: f32) {
        self.pan = pan.clamp(-1.0, 1.0);
    }

    pub fn set_track_delay(&mut self, ms: f32, sample_rate: f32) {
        self.track_delay_ms = ms.clamp(-MAX_DELAY_MS, MAX_DELAY_MS);
        if self.track_delay_ms > 0.0 {
            self.delay_samples = ((self.track_delay_ms / 1000.0) * sample_rate).round() as usize;
            let cap = self.delay_buf_l.len();
            if self.delay_samples >= cap {
                self.delay_samples = cap - 1;
            }
        } else {
            self.delay_samples = 0;
        }
    }

    pub fn set_send(&mut self, return_idx: usize, level_db: f32, mode: SendRoutingMode) {
        if let Some(send) = self.sends.iter_mut().find(|s| s.return_index == return_idx) {
            send.set_level_db(level_db);
            send.mode = mode;
        } else {
            self.sends.push(AudioSendState::new(return_idx, level_db, mode));
        }
    }
}

/// Auxiliary return channel strip (e.g. Return A Reverb, Return B Delay).
#[derive(Debug, Clone)]
pub struct AudioReturnChannel {
    pub id: u32,
    pub name: String,
    pub volume_db: f32,
    pub volume_linear: f32,
    pub pan: f32,
    pub mute: bool,
    pub solo: bool,
    pub buffer_l: Vec<f32>,
    pub buffer_r: Vec<f32>,

    // Metering ballistics
    pub peak_l: f32,
    pub peak_r: f32,
    pub rms_l: f32,
    pub rms_r: f32,
    sum_sq_l: f32,
    sum_sq_r: f32,
}

impl AudioReturnChannel {
    pub fn new(id: u32, name: &str) -> Self {
        Self {
            id,
            name: name.to_string(),
            volume_db: 0.0,
            volume_linear: 1.0,
            pan: 0.0,
            mute: false,
            solo: false,
            buffer_l: vec![0.0; 1024],
            buffer_r: vec![0.0; 1024],
            peak_l: 0.0,
            peak_r: 0.0,
            rms_l: 0.0,
            rms_r: 0.0,
            sum_sq_l: 0.0,
            sum_sq_r: 0.0,
        }
    }

    pub fn set_volume_db(&mut self, db: f32) {
        self.volume_db = db.clamp(-70.0, 6.0);
        self.volume_linear = if self.volume_db <= -59.5 {
            0.0
        } else {
            10.0_f32.powf(self.volume_db / 20.0)
        };
    }

    pub fn set_pan(&mut self, pan: f32) {
        self.pan = pan.clamp(-1.0, 1.0);
    }

    pub fn ensure_capacity(&mut self, frames: usize) {
        if self.buffer_l.len() < frames {
            self.buffer_l.resize(frames, 0.0);
            self.buffer_r.resize(frames, 0.0);
        }
    }

    pub fn clear(&mut self, frames: usize) {
        self.ensure_capacity(frames);
        for i in 0..frames {
            self.buffer_l[i] = 0.0;
            self.buffer_r[i] = 0.0;
        }
    }
}

/// Full Modular Audio Mixer managing unlimited tracks, return buses, DJ crossfader, and master limiting.
pub struct ModularAudioMixer {
    pub channels: Vec<AudioTrackChannel>,
    pub returns: Vec<AudioReturnChannel>,
    pub master_volume_db: f32,
    pub master_volume_linear: f32,
    pub master_pan: f32,
    pub crossfader_position: f32, // -1.0 (A) .. 0.0 (Center) .. +1.0 (B)
    pub master_limiter: MasterLimiter,

    // Master meters
    pub master_peak_l: f32,
    pub master_peak_r: f32,
    pub master_rms_l: f32,
    pub master_rms_r: f32,
    master_sum_sq_l: f32,
    master_sum_sq_r: f32,

    // Internal scratch summing buffers
    master_bus_l: Vec<f32>,
    master_bus_r: Vec<f32>,
    resampling_bus_l: Vec<f32>,
    resampling_bus_r: Vec<f32>,

    sample_rate: f32,
    meter_alpha: f32,
    peak_decay_coeff: f32,
    next_id: u32,
}

impl Default for ModularAudioMixer {
    fn default() -> Self {
        Self::new(44100.0)
    }
}

impl ModularAudioMixer {
    pub fn new(sample_rate: f32) -> Self {
        let sr = if sample_rate <= 0.0 { 44100.0 } else { sample_rate };
        let meter_alpha = 1.0 - (-1.0 / (METER_WINDOW_SECONDS * sr)).exp();
        let peak_decay_coeff = (-1.0 / (1.5 * sr)).exp();

        let mut mixer = Self {
            channels: Vec::new(),
            returns: Vec::new(),
            master_volume_db: 0.0,
            master_volume_linear: 1.0,
            master_pan: 0.0,
            crossfader_position: 0.0,
            master_limiter: MasterLimiter::new(sr),
            master_peak_l: 0.0,
            master_peak_r: 0.0,
            master_rms_l: 0.0,
            master_rms_r: 0.0,
            master_sum_sq_l: 0.0,
            master_sum_sq_r: 0.0,
            master_bus_l: vec![0.0; 1024],
            master_bus_r: vec![0.0; 1024],
            resampling_bus_l: vec![0.0; 1024],
            resampling_bus_r: vec![0.0; 1024],
            sample_rate: sr,
            meter_alpha,
            peak_decay_coeff,
            next_id: 1,
        };

        // Create standard default return tracks: A-Reverb and B-Delay
        mixer.add_return("A-Reverb");
        mixer.add_return("B-Delay");

        mixer
    }

    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        if sample_rate > 0.0 {
            self.sample_rate = sample_rate;
            self.meter_alpha = 1.0 - (-1.0 / (METER_WINDOW_SECONDS * sample_rate)).exp();
            self.peak_decay_coeff = (-1.0 / (1.5 * sample_rate)).exp();
            self.master_limiter.set_sample_rate(sample_rate);
        }
    }

    /// Add a track channel (MIDI, Audio, or Group) and return its index.
    pub fn add_channel(&mut self, name: &str, track_type: TrackType) -> usize {
        let id = self.next_id;
        self.next_id += 1;
        let mut ch = AudioTrackChannel::new(id, name, track_type, self.sample_rate);

        // Populate send slots matching return tracks
        for (ret_idx, _) in self.returns.iter().enumerate() {
            ch.sends.push(AudioSendState::new(ret_idx, -70.0, SendRoutingMode::PostFader));
        }

        self.channels.push(ch);
        self.channels.len() - 1
    }

    /// Add a return bus (e.g. Return A Reverb, Return B Delay) and return its index.
    pub fn add_return(&mut self, name: &str) -> usize {
        let id = self.next_id;
        self.next_id += 1;
        let ret_idx = self.returns.len();
        self.returns.push(AudioReturnChannel::new(id, name));

        // Add send slot to all existing channels
        for ch in &mut self.channels {
            if !ch.sends.iter().any(|s| s.return_index == ret_idx) {
                ch.sends.push(AudioSendState::new(ret_idx, -70.0, SendRoutingMode::PostFader));
            }
        }

        ret_idx
    }

    /// Set crossfader position in range [-1.0, 1.0].
    pub fn set_crossfader(&mut self, pos: f32) {
        self.crossfader_position = pos.clamp(-1.0, 1.0);
    }

    /// Set master volume in decibels.
    pub fn set_master_volume_db(&mut self, db: f32) {
        self.master_volume_db = db.clamp(-70.0, 6.0);
        self.master_volume_linear = if self.master_volume_db <= -59.5 {
            0.0
        } else {
            10.0_f32.powf(self.master_volume_db / 20.0)
        };
    }

    /// Set master pan [-1.0, 1.0].
    pub fn set_master_pan(&mut self, pan: f32) {
        self.master_pan = pan.clamp(-1.0, 1.0);
    }

    /// Check if any channel or return bus is soloed.
    pub fn is_any_solo(&self) -> bool {
        self.channels.iter().any(|c| c.solo) || self.returns.iter().any(|r| r.solo)
    }

    /// Equal-power DJ Crossfader curve calculation.
    ///
    /// For crossfader position X in [-1.0, 1.0]:
    /// - Assign A: g_A = 1.0 if X <= 0, cos(pi/2 * X) if X > 0.
    /// - Assign B: g_B = cos(pi/2 * |X|) if X < 0, 1.0 if X >= 0.
    /// - Assign Off: g_Off = 1.0.
    pub fn crossfade_gain(assign: CrossfadeAssign, x: f32) -> f32 {
        let x_clamped = x.clamp(-1.0, 1.0);
        match assign {
            CrossfadeAssign::Off => 1.0,
            CrossfadeAssign::A => {
                if x_clamped <= 0.0 {
                    1.0
                } else {
                    (0.5 * PI * x_clamped).cos()
                }
            }
            CrossfadeAssign::B => {
                if x_clamped >= 0.0 {
                    1.0
                } else {
                    (0.5 * PI * x_clamped.abs()).cos()
                }
            }
        }
    }

    /// Prepare a block for processing: clears master and return accumulator buffers.
    pub fn begin_block(&mut self, frames: usize) {
        if self.master_bus_l.len() < frames {
            self.master_bus_l.resize(frames, 0.0);
            self.master_bus_r.resize(frames, 0.0);
            self.resampling_bus_l.resize(frames, 0.0);
            self.resampling_bus_r.resize(frames, 0.0);
        }

        for i in 0..frames {
            self.master_bus_l[i] = 0.0;
            self.master_bus_r[i] = 0.0;
        }

        for ret in &mut self.returns {
            ret.clear(frames);
        }
    }

    /// Process a track's audio block through its channel strip:
    /// - Applies Solo / Mute gating
    /// - Taps Pre-fader sends into Return buffers
    /// - Applies Pan (constant-power -3dB) and Volume Fader
    /// - Applies Track Delay compensation
    /// - Taps Post-fader sends into Return buffers
    /// - Applies DJ Crossfader curve
    /// - Accumulates into Master bus or designated Group bus
    /// - Updates channel ballistics meters
    pub fn process_channel(
        &mut self,
        channel_idx: usize,
        left: &mut [f32],
        right: &mut [f32],
    ) -> bool {
        if channel_idx >= self.channels.len() {
            return false;
        }

        let any_solo = self.is_any_solo();
        let ch = &mut self.channels[channel_idx];
        let len = left.len().min(right.len());

        let is_audible = if any_solo {
            ch.solo && !ch.mute
        } else {
            !ch.mute
        };

        // 1. Pre-fader sends: tapped BEFORE volume fader, mute, and pan
        for send in &ch.sends {
            if send.mode == SendRoutingMode::PreFader && send.gain_linear > 0.0 {
                if let Some(ret) = self.returns.get_mut(send.return_index) {
                    ret.ensure_capacity(len);
                    for i in 0..len {
                        ret.buffer_l[i] += left[i] * send.gain_linear;
                        ret.buffer_r[i] += right[i] * send.gain_linear;
                    }
                }
            }
        }

        // 2. Pan & Volume Gain
        let theta = (ch.pan + 1.0) * 0.25 * PI;
        let pan_l = theta.cos();
        let pan_r = theta.sin();
        let fader_gain = if is_audible { ch.volume_linear } else { 0.0 };

        // 3. Track Delay application via circular buffer
        let delay_samps = ch.delay_samples;
        let cap = ch.delay_buf_l.len();
        let mut write_p = ch.delay_write_ptr;

        for i in 0..len {
            let in_l = left[i] * fader_gain * pan_l;
            let in_r = right[i] * fader_gain * pan_r;

            ch.delay_buf_l[write_p] = in_l;
            ch.delay_buf_r[write_p] = in_r;

            let (out_l, out_r) = if delay_samps > 0 {
                let read_p = (write_p + cap - delay_samps) % cap;
                (ch.delay_buf_l[read_p], ch.delay_buf_r[read_p])
            } else {
                (in_l, in_r)
            };

            write_p = (write_p + 1) % cap;

            left[i] = out_l;
            right[i] = out_r;
        }
        ch.delay_write_ptr = write_p;

        // 4. Post-fader sends: tapped AFTER volume fader, mute, and pan
        for send in &ch.sends {
            if send.mode == SendRoutingMode::PostFader && send.gain_linear > 0.0 {
                if let Some(ret) = self.returns.get_mut(send.return_index) {
                    ret.ensure_capacity(len);
                    for i in 0..len {
                        ret.buffer_l[i] += left[i] * send.gain_linear;
                        ret.buffer_r[i] += right[i] * send.gain_linear;
                    }
                }
            }
        }

        // 5. Crossfader weighting
        let xf_gain = Self::crossfade_gain(ch.crossfade_assign, self.crossfader_position);
        for i in 0..len {
            left[i] *= xf_gain;
            right[i] *= xf_gain;
        }

        // 6. Metering ballistics
        for i in 0..len {
            let l = left[i];
            let r = right[i];
            let pl = l.abs();
            let pr = r.abs();

            ch.peak_l = pl.max(ch.peak_l * self.peak_decay_coeff);
            ch.peak_r = pr.max(ch.peak_r * self.peak_decay_coeff);

            ch.sum_sq_l += self.meter_alpha * (l * l - ch.sum_sq_l);
            ch.sum_sq_r += self.meter_alpha * (r * r - ch.sum_sq_r);
        }

        ch.rms_l = ch.sum_sq_l.max(0.0).sqrt();
        ch.rms_r = ch.sum_sq_r.max(0.0).sqrt();

        let output_dest = ch.output_destination.clone();

        // 7. Routing accumulation (ch borrow is released)
        match output_dest {
            OutputDestination::Master => {
                for i in 0..len {
                    self.master_bus_l[i] += left[i];
                    self.master_bus_r[i] += right[i];
                }
            }
            OutputDestination::GroupTrack(grp_id) => {
                if let Some(grp_idx) = self.channels.iter().position(|c| c.id == grp_id) {
                    let grp = &mut self.channels[grp_idx];
                    let grp_cap = grp.delay_buf_l.len();
                    for i in 0..len {
                        let wp = (grp.delay_write_ptr + i) % grp_cap;
                        grp.delay_buf_l[wp] += left[i];
                        grp.delay_buf_r[wp] += right[i];
                    }
                } else {
                    for i in 0..len {
                        self.master_bus_l[i] += left[i];
                        self.master_bus_r[i] += right[i];
                    }
                }
            }
            OutputDestination::SendsOnly => {}
            OutputDestination::Resampling => {
                for i in 0..len {
                    left[i] = self.resampling_bus_l[i];
                    right[i] = self.resampling_bus_r[i];
                    self.master_bus_l[i] += left[i];
                    self.master_bus_r[i] += right[i];
                }
            }
            OutputDestination::External(_) => {}
        }

        is_audible
    }

    /// Process a return bus block with optional return FX callback, then accumulate into master bus.
    pub fn process_return<F>(
        &mut self,
        return_idx: usize,
        mut fx_processor: F,
        frames: usize,
    ) where
        F: FnMut(&mut [f32], &mut [f32]),
    {
        if return_idx >= self.returns.len() {
            return;
        }

        let any_solo = self.is_any_solo();
        let ret = &mut self.returns[return_idx];
        ret.ensure_capacity(frames);

        let is_audible = if any_solo {
            ret.solo && !ret.mute
        } else {
            !ret.mute
        };

        // Run return effect chain (e.g. Reverb, Delay)
        fx_processor(&mut ret.buffer_l[..frames], &mut ret.buffer_r[..frames]);

        let theta = (ret.pan + 1.0) * 0.25 * PI;
        let pan_l = theta.cos();
        let pan_r = theta.sin();
        let gain = if is_audible { ret.volume_linear } else { 0.0 };

        for i in 0..frames {
            let l = ret.buffer_l[i] * gain * pan_l;
            let r = ret.buffer_r[i] * gain * pan_r;

            ret.buffer_l[i] = l;
            ret.buffer_r[i] = r;

            self.master_bus_l[i] += l;
            self.master_bus_r[i] += r;

            let pl = l.abs();
            let pr = r.abs();
            ret.peak_l = pl.max(ret.peak_l * self.peak_decay_coeff);
            ret.peak_r = pr.max(ret.peak_r * self.peak_decay_coeff);

            ret.sum_sq_l += self.meter_alpha * (l * l - ret.sum_sq_l);
            ret.sum_sq_r += self.meter_alpha * (r * r - ret.sum_sq_r);
        }

        ret.rms_l = ret.sum_sq_l.max(0.0).sqrt();
        ret.rms_r = ret.sum_sq_r.max(0.0).sqrt();
    }

    /// Finalize block processing:
    /// Applies Master volume fader, pan, master lookahead limiter, and updates master meters.
    /// Copies master output into destination `out_left` and `out_right`.
    pub fn end_block(&mut self, out_left: &mut [f32], out_right: &mut [f32]) {
        let len = out_left.len().min(out_right.len()).min(self.master_bus_l.len());

        let theta = (self.master_pan + 1.0) * 0.25 * PI;
        let pan_l = theta.cos();
        let pan_r = theta.sin();
        let gain = self.master_volume_linear;

        for i in 0..len {
            self.master_bus_l[i] *= gain * pan_l;
            self.master_bus_r[i] *= gain * pan_r;
        }

        // Run master brickwall limiter (-0.3 dBFS ceiling)
        self.master_limiter
            .process_block(&mut self.master_bus_l[..len], &mut self.master_bus_r[..len]);

        // Copy to output and update master meters
        for i in 0..len {
            let l = self.master_bus_l[i];
            let r = self.master_bus_r[i];

            out_left[i] = l;
            out_right[i] = r;

            self.resampling_bus_l[i] = l;
            self.resampling_bus_r[i] = r;

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
    fn test_modular_mixer_sends_and_returns() {
        let mut mixer = ModularAudioMixer::new(44100.0);
        let ch0 = mixer.add_channel("Synth", TrackType::Audio);
        let ch1 = mixer.add_channel("Drum", TrackType::Audio);

        // Configure Channel 0: Post-fader send to Return 0 (A-Reverb) at 0 dB (1.0 gain)
        mixer.channels[ch0].set_send(0, 0.0, SendRoutingMode::PostFader);

        // Configure Channel 1: Pre-fader send to Return 1 (B-Delay) at 0 dB, but channel muted
        mixer.channels[ch1].set_send(1, 0.0, SendRoutingMode::PreFader);
        mixer.channels[ch1].mute = true;

        let frames = 256;
        mixer.begin_block(frames);

        let mut l0 = vec![1.0f32; frames];
        let mut r0 = vec![1.0f32; frames];
        let audible0 = mixer.process_channel(ch0, &mut l0, &mut r0);
        assert!(audible0);

        let mut l1 = vec![1.0f32; frames];
        let mut r1 = vec![1.0f32; frames];
        let audible1 = mixer.process_channel(ch1, &mut l1, &mut r1);
        assert!(!audible1, "Muted channel must report not audible");

        // Verify Return 0 received audio from ch0
        assert!(mixer.returns[0].buffer_l[0] > 0.0);

        // Verify Return 1 received audio from ch1 because send is PreFader
        assert!(mixer.returns[1].buffer_l[0] > 0.0, "Pre-fader send must pass audio even if track is muted");

        // Process Return 0 with pass-through
        mixer.process_return(0, |_l, _r| {}, frames);
        mixer.process_return(1, |_l, _r| {}, frames);

        let mut out_l = vec![0.0f32; frames];
        let mut out_r = vec![0.0f32; frames];
        mixer.end_block(&mut out_l, &mut out_r);

        // Limiter has 128 samples lookahead, so samples past 128 must be non-zero
        assert!(out_l[150] > 0.0, "Master out must contain summed channel and returns");
        assert!(mixer.master_peak_l > 0.0);
    }

    #[test]
    fn test_crossfader_equal_power_law() {
        // At center (0.0), both A and B are 1.0 (power sum = 2.0 = +3 dB power preservation)
        let ga_center = ModularAudioMixer::crossfade_gain(CrossfadeAssign::A, 0.0);
        let gb_center = ModularAudioMixer::crossfade_gain(CrossfadeAssign::B, 0.0);
        assert_eq!(ga_center, 1.0);
        assert_eq!(gb_center, 1.0);

        // At full A (-1.0): A is 1.0, B is 0.0
        let ga_full_a = ModularAudioMixer::crossfade_gain(CrossfadeAssign::A, -1.0);
        let gb_full_a = ModularAudioMixer::crossfade_gain(CrossfadeAssign::B, -1.0);
        assert_eq!(ga_full_a, 1.0);
        assert!((gb_full_a - 0.0).abs() < 1e-4);

        // At full B (+1.0): A is 0.0, B is 1.0
        let ga_full_b = ModularAudioMixer::crossfade_gain(CrossfadeAssign::A, 1.0);
        let gb_full_b = ModularAudioMixer::crossfade_gain(CrossfadeAssign::B, 1.0);
        assert!((ga_full_b - 0.0).abs() < 1e-4);
        assert_eq!(gb_full_b, 1.0);

        // Off ignores crossfader
        assert_eq!(ModularAudioMixer::crossfade_gain(CrossfadeAssign::Off, -1.0), 1.0);
        assert_eq!(ModularAudioMixer::crossfade_gain(CrossfadeAssign::Off, 1.0), 1.0);
    }

    #[test]
    fn test_track_delay_ring_buffer() {
        let mut mixer = ModularAudioMixer::new(44100.0);
        let ch = mixer.add_channel("DelayedTrack", TrackType::Audio);

        // Set 1 ms delay = 44.1 samples
        mixer.channels[ch].set_track_delay(1.0, 44100.0);
        let delay_s = mixer.channels[ch].delay_samples;
        assert_eq!(delay_s, 44);

        mixer.begin_block(64);
        let mut l = vec![1.0f32; 64];
        let mut r = vec![1.0f32; 64];

        mixer.process_channel(ch, &mut l, &mut r);

        // First 44 samples output must be silence (initial delay buffer)
        for i in 0..44 {
            assert_eq!(l[i], 0.0);
        }
        // Sample 44 onwards should have the delayed signal
        assert!(l[44] > 0.0);
    }
}
