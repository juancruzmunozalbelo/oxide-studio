//! CoreAudio driver management via cpal and AudioEngine handle.

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{BufferSize, Stream, StreamConfig};

use crate::parameters::AtomicParameters;
use crate::processor::AudioEngineProcessor;
use crate::queues::{AudioQueues, UiAudioHandles};
use oxide_core::{AudioCommand, AudioTelemetry};

pub struct AudioStreamHandle {
    pub stream: Option<Stream>,
    pub ui_handles: UiAudioHandles,
    pub params: AtomicParameters,
    pub sample_rate: f32,
}

impl AudioStreamHandle {
    pub fn play(&mut self) {
        self.ui_handles.send_command(AudioCommand::Play);
    }

    pub fn stop(&mut self) {
        self.ui_handles.send_command(AudioCommand::Stop);
    }

    pub fn set_tempo(&mut self, bpm: f64) {
        self.ui_handles.send_command(AudioCommand::SetTempo(bpm));
    }

    pub fn send_note_on(&mut self, track_id: usize, note: u8, velocity: u8) {
        self.ui_handles.send_command(AudioCommand::NoteOn {
            track_id,
            note,
            velocity,
        });
    }

    pub fn send_note_off(&mut self, track_id: usize, note: u8) {
        self.ui_handles.send_command(AudioCommand::NoteOff {
            track_id,
            note,
        });
    }

    pub fn poll_telemetry(&mut self) -> Option<AudioTelemetry> {
        self.ui_handles.poll_telemetry()
    }
}

pub struct AudioEngine;

impl AudioEngine {
    /// Initialize audio engine and start real-time CoreAudio stream via cpal.
    pub fn start() -> Result<AudioStreamHandle, String> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or_else(|| "No default audio output device available".to_string())?;

        let default_config = device
            .default_output_config()
            .map_err(|e| format!("Failed to query default output config: {}", e))?;

        let sample_rate = default_config.sample_rate() as f32;
        let channels = default_config.channels();

        let config = StreamConfig {
            channels,
            sample_rate: default_config.sample_rate(),
            buffer_size: BufferSize::Fixed(128),
        };

        let queues = AudioQueues::new();
        let (ui_handles, thread_handles) = queues.split();
        let params = AtomicParameters::new();

        let mut processor = AudioEngineProcessor::new(sample_rate, thread_handles, params.clone());

        let err_fn = |err| eprintln!("CoreAudio stream error: {}", err);

        let stream = device
            .build_output_stream(
                config,
                move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                    processor.process_interleaved(data);
                },
                err_fn,
                None,
            )
            .map_err(|e| format!("Failed to build CoreAudio stream: {}", e))?;

        stream
            .play()
            .map_err(|e| format!("Failed to play CoreAudio stream: {}", e))?;

        Ok(AudioStreamHandle {
            stream: Some(stream),
            ui_handles,
            params,
            sample_rate,
        })
    }

    /// Creates a headless/mock audio stream handle for automated headless verification.
    pub fn start_mock(sample_rate: f32) -> (AudioStreamHandle, AudioEngineProcessor) {
        let queues = AudioQueues::new();
        let (ui_handles, thread_handles) = queues.split();
        let params = AtomicParameters::new();
        let processor = AudioEngineProcessor::new(sample_rate, thread_handles, params.clone());

        (
            AudioStreamHandle {
                stream: None,
                ui_handles,
                params,
                sample_rate,
            },
            processor,
        )
    }
}
