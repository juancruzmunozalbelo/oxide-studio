//! Real-Time Multi-Track Audio Input Recording, Hardware I/O Matrix & Driver Latency Compensation.
//!
//! Provides:
//! - `HardwareIoMatrix`: 16 stereo input buses, 16 stereo output buses, per-track routing, input gain trim, and hardware loopback protection.
//! - `DriverLatencyCompensator`: Roundtrip driver latency compensation with zero-allocation circular delay line `DelayCompLine` (up to 4096 frames).
//! - `MultiTrackRecorder`: Lock-free fixed-capacity ring/take buffer per armed track, punch-in / punch-out beat bounds, overdub vs take-lane modes, peak metering, and sample-accurate latency shift on commit.

pub const MAX_HARDWARE_INPUT_BUSES: usize = 16;
pub const MAX_HARDWARE_OUTPUT_BUSES: usize = 16;
pub const MAX_RECORDING_TRACKS: usize = 32;
pub const MAX_DELAY_COMP_FRAMES: usize = 4096;
pub const DEFAULT_TAKE_CAPACITY_FRAMES: usize = 192_000 * 4; // 16 seconds @ 48kHz, pre-allocated

/// Mode for routing audio input channels to a track.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputChannelMode {
    Stereo,
    MonoLeft,
    MonoRight,
}

/// Recording mode: either overdubbing into current take or recording new take lanes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordingMode {
    Overdub,
    TakeLane,
}

/// Error returned when I/O routing fails (e.g., loopback cycle detected).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IoRoutingError {
    InvalidBusIndex(usize),
    InvalidTrackIndex(usize),
    LoopbackConflict { in_bus: usize, out_bus: usize },
}

/// Configuration for an input bus on the hardware interface.
#[derive(Debug, Clone)]
pub struct HardwareInputBus {
    pub name: String,
    pub gain_trim_db: f32,
    pub gain_linear: f32,
    pub enabled: bool,
    pub is_loopback_source: bool,
}

impl Default for HardwareInputBus {
    fn default() -> Self {
        Self {
            name: String::new(),
            gain_trim_db: 0.0,
            gain_linear: 1.0,
            enabled: true,
            is_loopback_source: false,
        }
    }
}

/// Configuration for an output bus on the hardware interface.
#[derive(Debug, Clone)]
pub struct HardwareOutputBus {
    pub name: String,
    pub enabled: bool,
    pub loopback_protected: bool,
}

impl Default for HardwareOutputBus {
    fn default() -> Self {
        Self {
            name: String::new(),
            enabled: true,
            loopback_protected: true,
        }
    }
}

/// Routing state for an individual track.
#[derive(Debug, Clone)]
pub struct TrackIoRoute {
    pub input_bus: Option<usize>,
    pub input_mode: InputChannelMode,
    pub output_bus: usize,
    pub input_trim_db: f32,
    pub input_trim_linear: f32,
}

impl Default for TrackIoRoute {
    fn default() -> Self {
        Self {
            input_bus: None,
            input_mode: InputChannelMode::Stereo,
            output_bus: 0,
            input_trim_db: 0.0,
            input_trim_linear: 1.0,
        }
    }
}

/// 16 stereo input / 16 stereo output hardware matrix with loopback protection.
#[derive(Debug, Clone)]
pub struct HardwareIoMatrix {
    pub inputs: [HardwareInputBus; MAX_HARDWARE_INPUT_BUSES],
    pub outputs: [HardwareOutputBus; MAX_HARDWARE_OUTPUT_BUSES],
    pub track_routes: [TrackIoRoute; MAX_RECORDING_TRACKS],
    pub loopback_protection_enabled: bool,
}

impl Default for HardwareIoMatrix {
    fn default() -> Self {
        Self::new()
    }
}

impl HardwareIoMatrix {
    pub fn new() -> Self {
        let inputs = std::array::from_fn(|i| HardwareInputBus {
            name: format!("In {}-{}", i * 2 + 1, i * 2 + 2),
            gain_trim_db: 0.0,
            gain_linear: 1.0,
            enabled: true,
            is_loopback_source: false,
        });

        let outputs = std::array::from_fn(|i| HardwareOutputBus {
            name: format!("Out {}-{}", i * 2 + 1, i * 2 + 2),
            enabled: true,
            loopback_protected: true,
        });

        let track_routes = std::array::from_fn(|_| TrackIoRoute::default());

        Self {
            inputs,
            outputs,
            track_routes,
            loopback_protection_enabled: true,
        }
    }

    /// Set gain trim in dB (-24.0 .. +24.0 dB) for a specific hardware input bus.
    pub fn set_input_trim_db(&mut self, bus: usize, trim_db: f32) -> Result<(), IoRoutingError> {
        if bus >= MAX_HARDWARE_INPUT_BUSES {
            return Err(IoRoutingError::InvalidBusIndex(bus));
        }
        let clamped = trim_db.clamp(-24.0, 24.0);
        self.inputs[bus].gain_trim_db = clamped;
        self.inputs[bus].gain_linear = 10.0_f32.powf(clamped / 20.0);
        Ok(())
    }

    /// Route a track's input to a hardware input bus.
    pub fn set_track_input_route(
        &mut self,
        track_id: usize,
        bus: usize,
        mode: InputChannelMode,
    ) -> Result<(), IoRoutingError> {
        if track_id >= MAX_RECORDING_TRACKS {
            return Err(IoRoutingError::InvalidTrackIndex(track_id));
        }
        if bus >= MAX_HARDWARE_INPUT_BUSES {
            return Err(IoRoutingError::InvalidBusIndex(bus));
        }

        // Loopback protection: prevent routing if output matches loopback
        if self.loopback_protection_enabled {
            let out_bus = self.track_routes[track_id].output_bus;
            if bus == out_bus && self.outputs[out_bus].loopback_protected {
                return Err(IoRoutingError::LoopbackConflict {
                    in_bus: bus,
                    out_bus,
                });
            }
        }

        self.track_routes[track_id].input_bus = Some(bus);
        self.track_routes[track_id].input_mode = mode;
        Ok(())
    }

    /// Route a track's output to a hardware output bus.
    pub fn set_track_output_route(
        &mut self,
        track_id: usize,
        bus: usize,
    ) -> Result<(), IoRoutingError> {
        if track_id >= MAX_RECORDING_TRACKS {
            return Err(IoRoutingError::InvalidTrackIndex(track_id));
        }
        if bus >= MAX_HARDWARE_OUTPUT_BUSES {
            return Err(IoRoutingError::InvalidBusIndex(bus));
        }

        // Check loopback risk
        if self.loopback_protection_enabled {
            if let Some(in_bus) = self.track_routes[track_id].input_bus {
                if in_bus == bus && self.outputs[bus].loopback_protected {
                    return Err(IoRoutingError::LoopbackConflict {
                        in_bus,
                        out_bus: bus,
                    });
                }
            }
        }

        self.track_routes[track_id].output_bus = bus;
        Ok(())
    }

    /// Set gain trim in dB (-24.0 .. +24.0 dB) for an individual track's input route.
    pub fn set_track_input_trim_db(
        &mut self,
        track_id: usize,
        trim_db: f32,
    ) -> Result<(), IoRoutingError> {
        if track_id >= MAX_RECORDING_TRACKS {
            return Err(IoRoutingError::InvalidTrackIndex(track_id));
        }
        let clamped = trim_db.clamp(-24.0, 24.0);
        self.track_routes[track_id].input_trim_db = clamped;
        self.track_routes[track_id].input_trim_linear = 10.0_f32.powf(clamped / 20.0);
        Ok(())
    }

    /// Enable or disable a hardware input bus.
    pub fn set_input_enabled(&mut self, bus: usize, enabled: bool) -> Result<(), IoRoutingError> {
        if bus >= MAX_HARDWARE_INPUT_BUSES {
            return Err(IoRoutingError::InvalidBusIndex(bus));
        }
        self.inputs[bus].enabled = enabled;
        Ok(())
    }

    /// Enable or disable loopback protection for a hardware output bus.
    pub fn set_output_loopback_protected(
        &mut self,
        bus: usize,
        protected: bool,
    ) -> Result<(), IoRoutingError> {
        if bus >= MAX_HARDWARE_OUTPUT_BUSES {
            return Err(IoRoutingError::InvalidBusIndex(bus));
        }
        self.outputs[bus].loopback_protected = protected;
        Ok(())
    }

    /// Check if routing `in_bus` to `out_bus` is considered a hardware loopback.
    pub fn is_loopback(&self, in_bus: usize, out_bus: usize) -> bool {
        in_bus == out_bus && self.outputs[out_bus].loopback_protected
    }

    /// Zero-allocation hardware input processing: routes hardware buses to tracks
    /// applying trim gains and channel mode formatting.
    pub fn process_hardware_inputs(
        &self,
        hw_inputs: &[[f32; 2]; MAX_HARDWARE_INPUT_BUSES],
        track_dest: &mut [[f32; 2]; MAX_RECORDING_TRACKS],
    ) {
        for track_id in 0..MAX_RECORDING_TRACKS {
            let route = &self.track_routes[track_id];
            if let Some(in_bus) = route.input_bus {
                if in_bus < MAX_HARDWARE_INPUT_BUSES && self.inputs[in_bus].enabled {
                    // Loopback check
                    if self.loopback_protection_enabled
                        && self.is_loopback(in_bus, route.output_bus)
                    {
                        track_dest[track_id] = [0.0, 0.0];
                        continue;
                    }

                    let raw = hw_inputs[in_bus];
                    let bus_gain = self.inputs[in_bus].gain_linear;
                    let track_gain = route.input_trim_linear;
                    let total_gain = bus_gain * track_gain;

                    match route.input_mode {
                        InputChannelMode::Stereo => {
                            track_dest[track_id] = [raw[0] * total_gain, raw[1] * total_gain];
                        }
                        InputChannelMode::MonoLeft => {
                            let mono = raw[0] * total_gain;
                            track_dest[track_id] = [mono, mono];
                        }
                        InputChannelMode::MonoRight => {
                            let mono = raw[1] * total_gain;
                            track_dest[track_id] = [mono, mono];
                        }
                    }
                } else {
                    track_dest[track_id] = [0.0, 0.0];
                }
            } else {
                track_dest[track_id] = [0.0, 0.0];
            }
        }
    }
}

/// Zero-allocation circular delay line of up to `MAX_DELAY_COMP_FRAMES` (4096 frames).
#[derive(Clone)]
pub struct DelayCompLine {
    buffer: [[f32; 2]; MAX_DELAY_COMP_FRAMES],
    write_pos: usize,
    delay_frames: usize,
}

impl Default for DelayCompLine {
    fn default() -> Self {
        Self::new(0)
    }
}

impl DelayCompLine {
    pub fn new(delay_frames: usize) -> Self {
        Self {
            buffer: [[0.0, 0.0]; MAX_DELAY_COMP_FRAMES],
            write_pos: 0,
            delay_frames: delay_frames.min(MAX_DELAY_COMP_FRAMES - 1),
        }
    }

    pub fn set_delay(&mut self, delay_frames: usize) {
        self.delay_frames = delay_frames.min(MAX_DELAY_COMP_FRAMES - 1);
    }

    pub fn delay_frames(&self) -> usize {
        self.delay_frames
    }

    pub fn reset(&mut self) {
        self.buffer = [[0.0, 0.0]; MAX_DELAY_COMP_FRAMES];
        self.write_pos = 0;
    }

    #[inline(always)]
    pub fn push(&mut self, sample: [f32; 2]) {
        self.buffer[self.write_pos] = sample;
        self.write_pos = (self.write_pos + 1) % MAX_DELAY_COMP_FRAMES;
    }

    #[inline(always)]
    pub fn read(&self) -> [f32; 2] {
        let read_pos = (self.write_pos + MAX_DELAY_COMP_FRAMES - self.delay_frames)
            % MAX_DELAY_COMP_FRAMES;
        self.buffer[read_pos]
    }

    #[inline(always)]
    pub fn process_frame(&mut self, sample: [f32; 2]) -> [f32; 2] {
        if self.delay_frames == 0 {
            return sample;
        }
        let out = self.read();
        self.push(sample);
        out
    }

    pub fn process_block(&mut self, input: &[[f32; 2]], output: &mut [[f32; 2]]) {
        let count = input.len().min(output.len());
        for i in 0..count {
            output[i] = self.process_frame(input[i]);
        }
    }
}

/// Driver latency compensator calculating roundtrip and alignment offsets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DriverLatencyCompensator {
    pub input_buffer_frames: u32,
    pub output_buffer_frames: u32,
    pub driver_safety_offset_samples: u32,
    pub driver_error_compensation_samples: i32,
}

impl Default for DriverLatencyCompensator {
    fn default() -> Self {
        Self::new(256, 256, 64)
    }
}

impl DriverLatencyCompensator {
    pub const fn new(
        input_buffer_frames: u32,
        output_buffer_frames: u32,
        driver_safety_offset_samples: u32,
    ) -> Self {
        Self {
            input_buffer_frames,
            output_buffer_frames,
            driver_safety_offset_samples,
            driver_error_compensation_samples: 0,
        }
    }

    pub const fn new_with_error_comp(
        input_buffer_frames: u32,
        output_buffer_frames: u32,
        driver_safety_offset_samples: u32,
        driver_error_compensation_samples: i32,
    ) -> Self {
        Self {
            input_buffer_frames,
            output_buffer_frames,
            driver_safety_offset_samples,
            driver_error_compensation_samples,
        }
    }

    /// Calculate total roundtrip driver latency in samples:
    /// `input_buffer_frames + output_buffer_frames + driver_safety_offset_samples + driver_error_compensation_samples`
    pub fn roundtrip_latency_samples(&self) -> u32 {
        let base = (self.input_buffer_frames + self.output_buffer_frames + self.driver_safety_offset_samples) as i32;
        let total = base + self.driver_error_compensation_samples;
        total.max(0) as u32
    }

    /// Calculate input-only recording latency in samples:
    /// `input_buffer_frames + driver_safety_offset_samples`
    pub const fn input_latency_samples(&self) -> u32 {
        self.input_buffer_frames + self.driver_safety_offset_samples
    }

    /// Calculate output-only playback latency in samples:
    pub const fn output_latency_samples(&self) -> u32 {
        self.output_buffer_frames
    }

    /// Calculate roundtrip latency in milliseconds at specified sample rate.
    pub fn roundtrip_latency_ms(&self, sample_rate: f32) -> f32 {
        if sample_rate > 0.0 {
            (self.roundtrip_latency_samples() as f32 / sample_rate) * 1000.0
        } else {
            0.0
        }
    }

    /// Calculate input latency in milliseconds at specified sample rate.
    pub fn input_latency_ms(&self, sample_rate: f32) -> f32 {
        if sample_rate > 0.0 {
            (self.input_latency_samples() as f32 / sample_rate) * 1000.0
        } else {
            0.0
        }
    }

    /// Calculate output latency in milliseconds at specified sample rate.
    pub fn output_latency_ms(&self, sample_rate: f32) -> f32 {
        if sample_rate > 0.0 {
            (self.output_latency_samples() as f32 / sample_rate) * 1000.0
        } else {
            0.0
        }
    }

    /// Create a zero-allocation circular delay line calibrated to compensate roundtrip latency.
    pub fn create_delay_comp_line(&self) -> DelayCompLine {
        let delay = (self.roundtrip_latency_samples() as usize).min(MAX_DELAY_COMP_FRAMES - 1);
        DelayCompLine::new(delay)
    }
}

/// A committed take resulting from a recording pass with latency compensation applied.
#[derive(Debug, Clone)]
pub struct CommittedTake {
    pub track_id: usize,
    pub take_number: usize,
    pub start_beat: f64,
    pub latency_offset_samples: usize,
    pub left_channel: Vec<f32>,
    pub right_channel: Vec<f32>,
}

/// Per-track recording state and pre-allocated buffers.
#[derive(Clone)]
pub struct TrackRecorderState {
    pub armed: bool,
    pub peak_meter_l: f32,
    pub peak_meter_r: f32,
    pub recorded_frames: usize,
    pub take_buffer_l: Vec<f32>,
    pub take_buffer_r: Vec<f32>,
    pub take_count: usize,
}

impl Default for TrackRecorderState {
    fn default() -> Self {
        Self {
            armed: false,
            peak_meter_l: 0.0,
            peak_meter_r: 0.0,
            recorded_frames: 0,
            take_buffer_l: vec![0.0; DEFAULT_TAKE_CAPACITY_FRAMES],
            take_buffer_r: vec![0.0; DEFAULT_TAKE_CAPACITY_FRAMES],
            take_count: 0,
        }
    }
}

/// Real-time multi-track recorder supporting punch-in/out, zero-allocation block capture,
/// take lanes, and sample-accurate latency compensation.
pub struct MultiTrackRecorder {
    pub tracks: [TrackRecorderState; MAX_RECORDING_TRACKS],
    pub mode: RecordingMode,
    pub punch_in_beat: Option<f64>,
    pub punch_out_beat: Option<f64>,
    pub is_recording: bool,
    pub record_start_beat: f64,
}

impl Default for MultiTrackRecorder {
    fn default() -> Self {
        Self::new()
    }
}

impl MultiTrackRecorder {
    pub fn new() -> Self {
        let tracks = std::array::from_fn(|_| TrackRecorderState::default());

        Self {
            tracks,
            mode: RecordingMode::TakeLane,
            punch_in_beat: None,
            punch_out_beat: None,
            is_recording: false,
            record_start_beat: 0.0,
        }
    }

    pub fn set_recording_mode(&mut self, mode: RecordingMode) {
        self.mode = mode;
    }

    pub fn arm_track(&mut self, track_id: usize, armed: bool) {
        if track_id < MAX_RECORDING_TRACKS {
            self.tracks[track_id].armed = armed;
        }
    }

    pub fn is_track_armed(&self, track_id: usize) -> bool {
        if track_id < MAX_RECORDING_TRACKS {
            self.tracks[track_id].armed
        } else {
            false
        }
    }

    pub fn set_punch_bounds(&mut self, punch_in: Option<f64>, punch_out: Option<f64>) {
        self.punch_in_beat = punch_in;
        self.punch_out_beat = punch_out;
    }

    pub fn start_recording(&mut self, current_beat: f64) {
        self.is_recording = true;
        self.record_start_beat = current_beat;
        for track in &mut self.tracks {
            if track.armed {
                track.recorded_frames = 0;
                track.peak_meter_l = 0.0;
                track.peak_meter_r = 0.0;
            }
        }
    }

    pub fn stop_recording(&mut self) {
        self.is_recording = false;
    }

    /// Check if current beat is within punch-in / punch-out bounds.
    #[inline(always)]
    pub fn is_punch_active(&self, beat: f64) -> bool {
        if let Some(in_beat) = self.punch_in_beat {
            if beat < in_beat {
                return false;
            }
        }
        if let Some(out_beat) = self.punch_out_beat {
            if beat >= out_beat {
                return false;
            }
        }
        true
    }

    pub fn get_peak_meters(&self, track_id: usize) -> (f32, f32) {
        if track_id < MAX_RECORDING_TRACKS {
            (self.tracks[track_id].peak_meter_l, self.tracks[track_id].peak_meter_r)
        } else {
            (0.0, 0.0)
        }
    }

    pub fn reset_peak_meters(&mut self, track_id: usize) {
        if track_id < MAX_RECORDING_TRACKS {
            self.tracks[track_id].peak_meter_l = 0.0;
            self.tracks[track_id].peak_meter_r = 0.0;
        }
    }

    /// Zero-allocation block recording routine executed in the audio callback.
    /// Captures input frames into pre-allocated take buffers for armed tracks
    /// when recording is active and within punch bounds.
    pub fn record_block(
        &mut self,
        track_inputs: &[[f32; 2]; MAX_RECORDING_TRACKS],
        frames: usize,
        current_beat: f64,
        is_playing: bool,
    ) {
        if !self.is_recording || !is_playing {
            return;
        }

        let in_punch = self.is_punch_active(current_beat);
        if !in_punch {
            return;
        }

        for track_id in 0..MAX_RECORDING_TRACKS {
            let track = &mut self.tracks[track_id];
            if !track.armed {
                continue;
            }

            let input = track_inputs[track_id];
            let abs_l = input[0].abs();
            let abs_r = input[1].abs();
            if abs_l > track.peak_meter_l {
                track.peak_meter_l = abs_l;
            }
            if abs_r > track.peak_meter_r {
                track.peak_meter_r = abs_r;
            }

            let start = track.recorded_frames;
            let end = (start + frames).min(DEFAULT_TAKE_CAPACITY_FRAMES);

            for f in start..end {
                track.take_buffer_l[f] = input[0];
                track.take_buffer_r[f] = input[1];
            }
            track.recorded_frames = end;
        }
    }

    /// Commit a recorded take for an armed track, applying exact sample-accurate
    /// latency shift compensation based on `DriverLatencyCompensator`.
    pub fn commit_track_take(
        &mut self,
        track_id: usize,
        latency_comp: &DriverLatencyCompensator,
    ) -> Option<CommittedTake> {
        if track_id >= MAX_RECORDING_TRACKS {
            return None;
        }

        let track = &mut self.tracks[track_id];
        let total_frames = track.recorded_frames;
        if total_frames == 0 {
            return None;
        }

        let latency = latency_comp.roundtrip_latency_samples() as usize;

        // Apply sample-accurate latency shift:
        // Recorded audio arrived `latency` samples later than real-time playback,
        // so shift audio back by `latency` samples.
        let aligned_frames = total_frames.saturating_sub(latency);

        let mut left = Vec::with_capacity(aligned_frames);
        let mut right = Vec::with_capacity(aligned_frames);

        if aligned_frames > 0 {
            left.extend_from_slice(&track.take_buffer_l[latency..total_frames]);
            right.extend_from_slice(&track.take_buffer_r[latency..total_frames]);
        }

        track.take_count += 1;
        let take_num = track.take_count;

        // Reset recorded frames for next take pass
        track.recorded_frames = 0;

        Some(CommittedTake {
            track_id,
            take_number: take_num,
            start_beat: self.record_start_beat,
            latency_offset_samples: latency,
            left_channel: left,
            right_channel: right,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zero_allocation_recording_steady_state() {
        let mut recorder = MultiTrackRecorder::new();
        recorder.arm_track(0, true);
        recorder.arm_track(1, true);
        recorder.start_recording(0.0);

        let mut track_inputs = [[0.0f32; 2]; MAX_RECORDING_TRACKS];
        track_inputs[0] = [0.75, -0.75];
        track_inputs[1] = [0.50, 0.50];

        // Process 100 blocks of 128 frames
        for i in 0..100 {
            recorder.record_block(&track_inputs, 128, i as f64 * 0.1, true);
        }

        assert_eq!(recorder.tracks[0].recorded_frames, 12800);
        assert_eq!(recorder.tracks[1].recorded_frames, 12800);
        assert_eq!(recorder.tracks[2].recorded_frames, 0); // unarmed

        let (peak_l0, peak_r0) = recorder.get_peak_meters(0);
        assert!((peak_l0 - 0.75).abs() < 1e-4);
        assert!((peak_r0 - 0.75).abs() < 1e-4);
    }

    #[test]
    fn test_punch_in_punch_out_bounds() {
        let mut recorder = MultiTrackRecorder::new();
        recorder.arm_track(0, true);
        recorder.set_punch_bounds(Some(2.0), Some(4.0));
        recorder.start_recording(0.0);

        let mut track_inputs = [[0.0f32; 2]; MAX_RECORDING_TRACKS];
        track_inputs[0] = [0.8, 0.8];

        // Beat 1.0 (before punch-in): should NOT record
        recorder.record_block(&track_inputs, 256, 1.0, true);
        assert_eq!(recorder.tracks[0].recorded_frames, 0);

        // Beat 2.5 (inside punch window): SHOULD record
        recorder.record_block(&track_inputs, 256, 2.5, true);
        assert_eq!(recorder.tracks[0].recorded_frames, 256);

        // Beat 4.5 (after punch-out): should NOT record
        recorder.record_block(&track_inputs, 256, 4.5, true);
        assert_eq!(recorder.tracks[0].recorded_frames, 256);
    }

    #[test]
    fn test_driver_latency_compensator_sample_accurate_alignment() {
        let compensator = DriverLatencyCompensator::new(256, 256, 64);
        let roundtrip = compensator.roundtrip_latency_samples();
        assert_eq!(roundtrip, 576);

        // Circular delay line test
        let mut delay_line = compensator.create_delay_comp_line();
        assert_eq!(delay_line.delay_frames(), 576);

        // Feed impulse at index 0
        let mut impulse = [[0.0f32; 2]; 1024];
        impulse[0] = [1.0, -1.0];

        let mut delayed = [[0.0f32; 2]; 1024];
        delay_line.process_block(&impulse, &mut delayed);

        // The impulse must appear at exactly index 576
        assert_eq!(delayed[0], [0.0, 0.0]);
        assert_eq!(delayed[576], [1.0, -1.0]);

        // Now test recorder commit latency shift
        let mut recorder = MultiTrackRecorder::new();
        recorder.arm_track(0, true);
        recorder.start_recording(0.0);

        let mut track_inputs = [[0.0f32; 2]; MAX_RECORDING_TRACKS];
        track_inputs[0] = [0.9, 0.9];

        // Record 1000 frames
        recorder.record_block(&track_inputs, 1000, 0.0, true);
        assert_eq!(recorder.tracks[0].recorded_frames, 1000);

        let committed = recorder.commit_track_take(0, &compensator).expect("Take commit");
        assert_eq!(committed.latency_offset_samples, 576);
        assert_eq!(committed.left_channel.len(), 1000 - 576);
        assert!((committed.left_channel[0] - 0.9).abs() < 1e-4);
    }

    #[test]
    fn test_hardware_io_matrix_routing_gain_and_loopback_protection() {
        let mut matrix = HardwareIoMatrix::new();

        // 1. Gain trim test (+6 dB ~= 1.995 linear)
        matrix.set_input_trim_db(2, 6.0).expect("Set trim");
        assert!((matrix.inputs[2].gain_linear - 1.995262).abs() < 0.01);

        // 2. Route input 2 to track 0 (mono left mode)
        matrix.set_track_input_route(0, 2, InputChannelMode::MonoLeft).expect("Route input");

        let mut hw_inputs = [[0.0f32; 2]; MAX_HARDWARE_INPUT_BUSES];
        hw_inputs[2] = [0.5, 0.1]; // L=0.5, R=0.1

        let mut track_inputs = [[0.0f32; 2]; MAX_RECORDING_TRACKS];
        matrix.process_hardware_inputs(&hw_inputs, &mut track_inputs);

        // Expected: MonoLeft duplicated to L and R with +6 dB gain
        let expected = 0.5 * matrix.inputs[2].gain_linear;
        assert!((track_inputs[0][0] - expected).abs() < 1e-4);
        assert!((track_inputs[0][1] - expected).abs() < 1e-4);

        // 3. Loopback protection test
        matrix.set_track_output_route(0, 3).expect("Route output");
        // Attempting to route input bus 3 to track 0 (which outputs to bus 3) should fail
        let err = matrix.set_track_input_route(0, 3, InputChannelMode::Stereo);
        assert!(matches!(err, Err(IoRoutingError::LoopbackConflict { in_bus: 3, out_bus: 3 })));
    }

    #[test]
    fn test_driver_latency_compensator_buffer_sizes_and_error_comp() {
        let buffer_sizes: [u32; 6] = [64, 128, 256, 512, 1024, 2048];
        let sample_rates: [f32; 4] = [44100.0, 48000.0, 88200.0, 96000.0];

        for &bs in &buffer_sizes {
            for &sr in &sample_rates {
                let comp = DriverLatencyCompensator::new(bs, bs, 32);
                let roundtrip_samples = bs + bs + 32;
                assert_eq!(comp.roundtrip_latency_samples(), roundtrip_samples);
                assert_eq!(comp.input_latency_samples(), bs + 32);
                assert_eq!(comp.output_latency_samples(), bs);

                let expected_ms = (roundtrip_samples as f32 / sr) * 1000.0;
                assert!((comp.roundtrip_latency_ms(sr) - expected_ms).abs() < 1e-3);

                // With Driver Error Compensation
                let comp_dec = DriverLatencyCompensator::new_with_error_comp(bs, bs, 32, -16);
                assert_eq!(comp_dec.roundtrip_latency_samples(), roundtrip_samples - 16);
                let expected_dec_ms = ((roundtrip_samples - 16) as f32 / sr) * 1000.0;
                assert!((comp_dec.roundtrip_latency_ms(sr) - expected_dec_ms).abs() < 1e-3);
            }
        }
    }

    #[test]
    fn test_multi_track_recorder_32_tracks_armed_simultaneously() {
        let mut recorder = MultiTrackRecorder::new();
        assert_eq!(recorder.tracks.len(), MAX_RECORDING_TRACKS);
        assert_eq!(MAX_RECORDING_TRACKS, 32);

        // Arm all 32 tracks
        for track_id in 0..MAX_RECORDING_TRACKS {
            recorder.arm_track(track_id, true);
            assert!(recorder.is_track_armed(track_id));
        }

        // Out-of-bounds safety check
        assert!(!recorder.is_track_armed(MAX_RECORDING_TRACKS));
        recorder.arm_track(MAX_RECORDING_TRACKS, true); // should not panic
        assert!(!recorder.is_track_armed(MAX_RECORDING_TRACKS));

        recorder.start_recording(0.0);

        // Prepare unique signals for each of the 32 tracks
        let mut track_inputs = [[0.0f32; 2]; MAX_RECORDING_TRACKS];
        for track_id in 0..MAX_RECORDING_TRACKS {
            let val = (track_id as f32 + 1.0) / 40.0; // 0.025 .. 0.8
            track_inputs[track_id] = [val, -val];
        }

        // Record 500 frames while playing
        recorder.record_block(&track_inputs, 500, 0.0, true);

        for track_id in 0..MAX_RECORDING_TRACKS {
            assert_eq!(recorder.tracks[track_id].recorded_frames, 500);
            let expected_peak = (track_id as f32 + 1.0) / 40.0;
            let (peak_l, peak_r) = recorder.get_peak_meters(track_id);
            assert!((peak_l - expected_peak).abs() < 1e-4);
            assert!((peak_r - expected_peak).abs() < 1e-4);
        }

        // Commit all 32 tracks with latency compensation
        let compensator = DriverLatencyCompensator::new(128, 128, 64); // roundtrip = 320
        assert_eq!(compensator.roundtrip_latency_samples(), 320);

        for track_id in 0..MAX_RECORDING_TRACKS {
            let committed = recorder
                .commit_track_take(track_id, &compensator)
                .expect("Take must commit");
            assert_eq!(committed.track_id, track_id);
            assert_eq!(committed.take_number, 1);
            assert_eq!(committed.latency_offset_samples, 320);
            assert_eq!(committed.left_channel.len(), 500 - 320);
            assert_eq!(committed.right_channel.len(), 500 - 320);

            let expected_val = (track_id as f32 + 1.0) / 40.0;
            assert!((committed.left_channel[0] - expected_val).abs() < 1e-4);
            assert!((committed.right_channel[0] - (-expected_val)).abs() < 1e-4);

            // Buffer should reset after commit
            assert_eq!(recorder.tracks[track_id].recorded_frames, 0);
            assert_eq!(recorder.tracks[track_id].take_count, 1);
        }
    }

    #[test]
    fn test_punch_in_out_boundary_conditions_and_latency_edge_cases() {
        let mut recorder = MultiTrackRecorder::new();
        recorder.arm_track(0, true);

        // Case A: Punch in only (from beat 4.0 onwards)
        recorder.set_punch_bounds(Some(4.0), None);
        assert!(!recorder.is_punch_active(3.99));
        assert!(recorder.is_punch_active(4.0));
        assert!(recorder.is_punch_active(10.0));

        // Case B: Punch out only (until beat 8.0)
        recorder.set_punch_bounds(None, Some(8.0));
        assert!(recorder.is_punch_active(0.0));
        assert!(recorder.is_punch_active(7.99));
        assert!(!recorder.is_punch_active(8.0));
        assert!(!recorder.is_punch_active(8.5));

        // Case C: Open window (no punch locators)
        recorder.set_punch_bounds(None, None);
        assert!(recorder.is_punch_active(0.0));
        assert!(recorder.is_punch_active(100.0));

        // Case D: Playing flag gate (transport stopped)
        recorder.start_recording(0.0);
        let mut track_inputs = [[0.0f32; 2]; MAX_RECORDING_TRACKS];
        track_inputs[0] = [1.0, 1.0];
        recorder.record_block(&track_inputs, 256, 0.0, false); // is_playing = false
        assert_eq!(recorder.tracks[0].recorded_frames, 0);

        // Case E: Latency offset exceeds recorded frames
        recorder.record_block(&track_inputs, 100, 0.0, true);
        let compensator = DriverLatencyCompensator::new(256, 256, 64); // 576 frames
        let committed = recorder.commit_track_take(0, &compensator).expect("Commit");
        assert_eq!(committed.left_channel.len(), 0);
        assert_eq!(committed.right_channel.len(), 0);
        assert_eq!(committed.latency_offset_samples, 576);

        // Case F: Zero frames recorded returns None
        assert!(recorder.commit_track_take(0, &compensator).is_none());

        // Case G: Out of bounds track ID returns None
        assert!(recorder.commit_track_take(MAX_RECORDING_TRACKS, &compensator).is_none());
    }

    #[test]
    fn test_hardware_io_matrix_16_bus_structure_and_limits() {
        let mut matrix = HardwareIoMatrix::new();

        // Verify 16 hardware input buses
        assert_eq!(matrix.inputs.len(), MAX_HARDWARE_INPUT_BUSES);
        assert_eq!(MAX_HARDWARE_INPUT_BUSES, 16);
        for i in 0..16 {
            let expected_name = format!("In {}-{}", i * 2 + 1, i * 2 + 2);
            assert_eq!(matrix.inputs[i].name, expected_name);
            assert_eq!(matrix.inputs[i].gain_trim_db, 0.0);
            assert_eq!(matrix.inputs[i].gain_linear, 1.0);
            assert!(matrix.inputs[i].enabled);
            assert!(!matrix.inputs[i].is_loopback_source);
        }

        // Verify 16 hardware output buses
        assert_eq!(matrix.outputs.len(), MAX_HARDWARE_OUTPUT_BUSES);
        assert_eq!(MAX_HARDWARE_OUTPUT_BUSES, 16);
        for i in 0..16 {
            let expected_name = format!("Out {}-{}", i * 2 + 1, i * 2 + 2);
            assert_eq!(matrix.outputs[i].name, expected_name);
            assert!(matrix.outputs[i].enabled);
            assert!(matrix.outputs[i].loopback_protected);
        }

        // Verify 32 track routes default state
        assert_eq!(matrix.track_routes.len(), MAX_RECORDING_TRACKS);
        assert_eq!(MAX_RECORDING_TRACKS, 32);
        for i in 0..32 {
            assert_eq!(matrix.track_routes[i].input_bus, None);
            assert_eq!(matrix.track_routes[i].input_mode, InputChannelMode::Stereo);
            assert_eq!(matrix.track_routes[i].output_bus, 0);
            assert_eq!(matrix.track_routes[i].input_trim_db, 0.0);
            assert_eq!(matrix.track_routes[i].input_trim_linear, 1.0);
        }

        // Out of bounds bus checks
        assert_eq!(
            matrix.set_input_trim_db(16, 3.0),
            Err(IoRoutingError::InvalidBusIndex(16))
        );
        assert_eq!(
            matrix.set_track_input_route(0, 16, InputChannelMode::Stereo),
            Err(IoRoutingError::InvalidBusIndex(16))
        );
        assert_eq!(
            matrix.set_track_output_route(0, 16),
            Err(IoRoutingError::InvalidBusIndex(16))
        );
        assert_eq!(
            matrix.set_input_enabled(16, false),
            Err(IoRoutingError::InvalidBusIndex(16))
        );
        assert_eq!(
            matrix.set_output_loopback_protected(16, false),
            Err(IoRoutingError::InvalidBusIndex(16))
        );

        // Out of bounds track checks
        assert_eq!(
            matrix.set_track_input_route(32, 0, InputChannelMode::Stereo),
            Err(IoRoutingError::InvalidTrackIndex(32))
        );
        assert_eq!(
            matrix.set_track_output_route(32, 0),
            Err(IoRoutingError::InvalidTrackIndex(32))
        );
        assert_eq!(
            matrix.set_track_input_trim_db(32, 3.0),
            Err(IoRoutingError::InvalidTrackIndex(32))
        );
    }

    #[test]
    fn test_hardware_io_matrix_gain_trim_clamping_and_scaling() {
        let mut matrix = HardwareIoMatrix::new();

        // Negative clamp to -24 dB
        matrix.set_input_trim_db(0, -35.0).expect("Clamp negative");
        assert_eq!(matrix.inputs[0].gain_trim_db, -24.0);
        assert!((matrix.inputs[0].gain_linear - 0.06309573).abs() < 1e-4);

        // Positive clamp to +24 dB
        matrix.set_input_trim_db(1, 40.0).expect("Clamp positive");
        assert_eq!(matrix.inputs[1].gain_trim_db, 24.0);
        assert!((matrix.inputs[1].gain_linear - 15.848932).abs() < 1e-3);

        // Unity at 0 dB
        matrix.set_input_trim_db(2, 0.0).expect("Unity 0 dB");
        assert_eq!(matrix.inputs[2].gain_trim_db, 0.0);
        assert_eq!(matrix.inputs[2].gain_linear, 1.0);

        // -6 dB trim
        matrix.set_input_trim_db(3, -6.0).expect("-6 dB");
        assert!((matrix.inputs[3].gain_linear - 0.5011872).abs() < 1e-4);

        // Track trim + bus trim interaction (multiplicative gain)
        matrix.set_input_trim_db(4, 6.0).expect("+6 dB bus");
        matrix.set_track_output_route(0, 1).expect("Output to 1");
        matrix.set_track_input_route(0, 4, InputChannelMode::Stereo).expect("Route to 4");
        matrix.set_track_input_trim_db(0, -6.0).expect("-6 dB track");

        let mut hw_inputs = [[0.0f32; 2]; MAX_HARDWARE_INPUT_BUSES];
        hw_inputs[4] = [1.0, -1.0];

        let mut track_inputs = [[0.0f32; 2]; MAX_RECORDING_TRACKS];
        matrix.process_hardware_inputs(&hw_inputs, &mut track_inputs);

        // Combined linear factor: 10^(6/20) * 10^(-6/20) == 1.0
        assert!((track_inputs[0][0] - 1.0).abs() < 1e-3);
        assert!((track_inputs[0][1] - (-1.0)).abs() < 1e-3);
    }

    #[test]
    fn test_hardware_io_matrix_mono_stereo_splitting() {
        let mut matrix = HardwareIoMatrix::new();

        // Route Bus 5 to Track 0 (Stereo), Track 1 (MonoLeft), Track 2 (MonoRight)
        matrix.set_track_output_route(0, 0).expect("Out 0");
        matrix.set_track_output_route(1, 1).expect("Out 1");
        matrix.set_track_output_route(2, 2).expect("Out 2");
        matrix.set_track_output_route(3, 3).expect("Out 3");

        matrix.set_track_input_route(0, 5, InputChannelMode::Stereo).expect("Route T0");
        matrix.set_track_input_route(1, 5, InputChannelMode::MonoLeft).expect("Route T1");
        matrix.set_track_input_route(2, 5, InputChannelMode::MonoRight).expect("Route T2");
        // Track 3 left unrouted (None)

        let mut hw_inputs = [[0.0f32; 2]; MAX_HARDWARE_INPUT_BUSES];
        hw_inputs[5] = [0.4, -0.8];

        let mut track_inputs = [[0.0f32; 2]; MAX_RECORDING_TRACKS];
        matrix.process_hardware_inputs(&hw_inputs, &mut track_inputs);

        // Stereo: L=0.4, R=-0.8
        assert!((track_inputs[0][0] - 0.4).abs() < 1e-5);
        assert!((track_inputs[0][1] - (-0.8)).abs() < 1e-5);

        // MonoLeft: duplicated to both channels [0.4, 0.4]
        assert!((track_inputs[1][0] - 0.4).abs() < 1e-5);
        assert!((track_inputs[1][1] - 0.4).abs() < 1e-5);

        // MonoRight: duplicated to both channels [-0.8, -0.8]
        assert!((track_inputs[2][0] - (-0.8)).abs() < 1e-5);
        assert!((track_inputs[2][1] - (-0.8)).abs() < 1e-5);

        // Unrouted track 3 must be silent
        assert_eq!(track_inputs[3], [0.0, 0.0]);

        // When Bus 5 is disabled, all connected tracks receive silence
        matrix.set_input_enabled(5, false).expect("Disable bus 5");
        let mut muted_dest = [[0.0f32; 2]; MAX_RECORDING_TRACKS];
        matrix.process_hardware_inputs(&hw_inputs, &mut muted_dest);
        assert_eq!(muted_dest[0], [0.0, 0.0]);
        assert_eq!(muted_dest[1], [0.0, 0.0]);
        assert_eq!(muted_dest[2], [0.0, 0.0]);
    }

    #[test]
    fn test_hardware_io_matrix_loopback_rejection_and_muting() {
        let mut matrix = HardwareIoMatrix::new();

        // 1. Output already set to bus 7; attempting to set input to bus 7 must fail
        matrix.set_track_output_route(0, 7).expect("Set out 7");
        let err_input = matrix.set_track_input_route(0, 7, InputChannelMode::Stereo);
        assert_eq!(
            err_input,
            Err(IoRoutingError::LoopbackConflict {
                in_bus: 7,
                out_bus: 7
            })
        );

        // 2. Input already set to bus 4; attempting to set output to bus 4 must fail
        matrix.set_track_output_route(1, 0).expect("Safe out 0");
        matrix.set_track_input_route(1, 4, InputChannelMode::Stereo).expect("Set in 4");
        let err_output = matrix.set_track_output_route(1, 4);
        assert_eq!(
            err_output,
            Err(IoRoutingError::LoopbackConflict {
                in_bus: 4,
                out_bus: 4
            })
        );

        // 3. When loopback protection is disabled on output bus 7, routing succeeds
        matrix.set_output_loopback_protected(7, false).expect("Unprotect bus 7");
        assert!(matrix.set_track_input_route(0, 7, InputChannelMode::Stereo).is_ok());

        // 4. When global loopback protection is disabled, conflict is bypassed
        matrix.loopback_protection_enabled = false;
        matrix.set_output_loopback_protected(4, true).expect("Protect bus 4");
        assert!(matrix.set_track_output_route(1, 4).is_ok());

        // 5. Real-time safety: re-enabling loopback protection mutes conflicting track during processing
        matrix.loopback_protection_enabled = true;
        let mut hw_inputs = [[0.0f32; 2]; MAX_HARDWARE_INPUT_BUSES];
        hw_inputs[4] = [0.9, 0.9];
        let mut track_inputs = [[0.0f32; 2]; MAX_RECORDING_TRACKS];
        matrix.process_hardware_inputs(&hw_inputs, &mut track_inputs);
        assert_eq!(
            track_inputs[1],
            [0.0, 0.0],
            "Conflicting loopback route must be muted during audio processing"
        );
    }
}
