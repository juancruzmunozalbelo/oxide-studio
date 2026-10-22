//! Real-time CoreAudio audio engine for ableton-rust.
//!
//! Provides:
//! - CoreAudio driver bridge via `cpal`
//! - Zero heap allocation steady-state audio callback (`AudioEngineProcessor`)
//! - Lock-free SPSC command and telemetry queues via `rtrb`
//! - Parameter smoothing with `portable-atomic::AtomicF32`

pub mod engine;
pub mod fact_sheet;
pub mod mixer;
pub mod parameters;
pub mod preferences;
pub mod processor;
pub mod queues;
pub mod recording;
pub mod stream;

pub use fact_sheet::{
    compute_null_test, verify_64bit_summing_neutrality, verify_mixer_fader_mute_silence,
    verify_phase_inversion_null, verify_sample_rate_conversion_neutrality,
    verify_undithered_rendering_neutrality, NullTestReport,
};

pub use engine::{
    CpuLoadMeter, EngineArchitectureState, MultiCoreDspDispatcher, WorkerScratchBuffer,
    ZeroAllocAudit, CPU_LOAD_EMA_ALPHA, CPU_PEAK_DECAY,
};

pub use preferences::{
    AudioHardwarePreferences, AudioLatencySummary, MAX_BUFFER_SIZE, MIN_BUFFER_SIZE,
    SUPPORTED_BUFFER_SIZES, SUPPORTED_SAMPLE_RATES,
};
pub use recording::{
    CommittedTake, DelayCompLine, DriverLatencyCompensator, HardwareInputBus, HardwareIoMatrix,
    HardwareOutputBus, InputChannelMode, IoRoutingError, MultiTrackRecorder, RecordingMode,
    TrackIoRoute, TrackRecorderState, DEFAULT_TAKE_CAPACITY_FRAMES, MAX_DELAY_COMP_FRAMES,
    MAX_HARDWARE_INPUT_BUSES, MAX_HARDWARE_OUTPUT_BUSES, MAX_RECORDING_TRACKS,
};

pub use mixer::{
    AudioReturnChannel, AudioSendState, AudioTrackChannel, ModularAudioMixer, MAX_DELAY_MS,
    METER_WINDOW_SECONDS,
};
pub use parameters::{
    AtomicParameters, ParameterSmoother, MAX_PARAMS, PARAM_BPM, PARAM_FX_DELAY_WET,
    PARAM_FX_DRIVE, PARAM_FX_REVERB_WET, PARAM_MASTER_VOL, PARAM_SYNTH_CUTOFF,
    PARAM_SYNTH_MACRO_X, PARAM_SYNTH_MACRO_Y, PARAM_SYNTH_RESONANCE, PARAM_TRACK_PAN_BASE,
    PARAM_TRACK_VOL_BASE,
};
pub use processor::{
    AudioEngineProcessor, MAX_BLOCK_FRAMES, PARAM_ID_CROSSFADER, PARAM_ID_MASTER_VOL,
    PARAM_ID_TEST_TONE_ENABLE, PARAM_ID_TEST_TONE_FREQ, PARAM_ID_TEST_TONE_VOL,
    PARAM_ID_TRACK_MUTE_BASE, PARAM_ID_TRACK_PAN_BASE, PARAM_ID_TRACK_SEND_A_BASE,
    PARAM_ID_TRACK_SEND_B_BASE, PARAM_ID_TRACK_SOLO_BASE, PARAM_ID_TRACK_VOL_BASE,
};
pub use queues::{
    AudioQueues, AudioThreadHandles, UiAudioHandles, DEFAULT_COMMAND_QUEUE_CAPACITY,
    DEFAULT_TELEMETRY_QUEUE_CAPACITY,
};
pub use stream::{AudioEngine, AudioStreamHandle};

#[cfg(test)]
mod tests {
    use super::*;
    use ableton_core::AudioCommand;

    #[test]
    fn test_spsc_queue_latency_and_freedom() {
        let queues = AudioQueues::new();
        let (mut ui, mut audio) = queues.split();

        // Push commands from UI
        assert!(ui.send_command(AudioCommand::Play));
        assert!(ui.send_command(AudioCommand::SetTempo(128.0)));
        assert!(ui.send_command(AudioCommand::NoteOn {
            track_id: 0,
            note: 60,
            velocity: 100,
        }));

        let mut received = Vec::new();
        audio.drain_commands(|cmd| {
            received.push(cmd);
        });

        assert_eq!(received.len(), 3);
        assert_eq!(received[0], AudioCommand::Play);
        assert_eq!(received[1], AudioCommand::SetTempo(128.0));
    }

    #[test]
    fn test_mock_audio_engine_steady_state_loop() {
        let (mut handle, mut processor) = AudioEngine::start_mock(44100.0);

        handle.play();
        handle.send_note_on(0, 60, 110);
        handle.send_note_on(1, 36, 127); // Kick drum

        let mut output = [0.0f32; 1024];
        processor.process_interleaved(&mut output);

        let energy: f32 = output.iter().map(|s| s * s).sum();
        assert!(energy > 0.001, "Audio loop must render non-zero energy");

        let telem = handle.poll_telemetry();
        assert!(telem.is_some());
        let t = telem.unwrap();
        assert!(t.master_peak_l > 0.0 || t.master_peak_r > 0.0);
    }
}
