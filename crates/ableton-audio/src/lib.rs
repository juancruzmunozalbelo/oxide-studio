//! Real-time CoreAudio audio engine for ableton-rust.
//!
//! Provides:
//! - CoreAudio driver bridge via `cpal`
//! - Zero heap allocation steady-state audio callback (`AudioEngineProcessor`)
//! - Lock-free SPSC command and telemetry queues via `rtrb`
//! - Parameter smoothing with `portable-atomic::AtomicF32`

pub mod parameters;
pub mod processor;
pub mod queues;
pub mod stream;

pub use parameters::{
    AtomicParameters, ParameterSmoother, MAX_PARAMS, PARAM_BPM, PARAM_FX_DELAY_WET,
    PARAM_FX_DRIVE, PARAM_FX_REVERB_WET, PARAM_MASTER_VOL, PARAM_SYNTH_CUTOFF,
    PARAM_SYNTH_MACRO_X, PARAM_SYNTH_MACRO_Y, PARAM_SYNTH_RESONANCE, PARAM_TRACK_PAN_BASE,
    PARAM_TRACK_VOL_BASE,
};
pub use processor::{AudioEngineProcessor, MAX_BLOCK_FRAMES};
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
