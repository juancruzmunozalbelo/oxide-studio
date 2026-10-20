//! Lock-free SPSC communication channels for the real-time audio thread.

use ableton_core::{AudioCommand, AudioTelemetry};
use rtrb::{Consumer, Producer, RingBuffer};

pub const DEFAULT_COMMAND_QUEUE_CAPACITY: usize = 1024;
pub const DEFAULT_TELEMETRY_QUEUE_CAPACITY: usize = 64;

pub struct AudioQueues {
    pub cmd_producer: Producer<AudioCommand>,
    pub cmd_consumer: Consumer<AudioCommand>,
    pub telemetry_producer: Producer<AudioTelemetry>,
    pub telemetry_consumer: Consumer<AudioTelemetry>,
}

impl AudioQueues {
    pub fn new() -> Self {
        let (cmd_producer, cmd_consumer) = RingBuffer::new(DEFAULT_COMMAND_QUEUE_CAPACITY);
        let (telemetry_producer, telemetry_consumer) = RingBuffer::new(DEFAULT_TELEMETRY_QUEUE_CAPACITY);

        Self {
            cmd_producer,
            cmd_consumer,
            telemetry_producer,
            telemetry_consumer,
        }
    }

    /// Split queues into UI handles and Audio Thread handles.
    pub fn split(self) -> (UiAudioHandles, AudioThreadHandles) {
        (
            UiAudioHandles {
                cmd_sender: self.cmd_producer,
                telemetry_receiver: self.telemetry_consumer,
            },
            AudioThreadHandles {
                cmd_receiver: self.cmd_consumer,
                telemetry_sender: self.telemetry_producer,
            },
        )
    }
}

impl Default for AudioQueues {
    fn default() -> Self {
        Self::new()
    }
}

pub struct UiAudioHandles {
    pub cmd_sender: Producer<AudioCommand>,
    pub telemetry_receiver: Consumer<AudioTelemetry>,
}

impl UiAudioHandles {
    /// Send an audio command from UI thread. Returns true if queued, false if queue full.
    pub fn send_command(&mut self, cmd: AudioCommand) -> bool {
        self.cmd_sender.push(cmd).is_ok()
    }

    /// Drain the latest telemetry update on the UI thread.
    pub fn poll_telemetry(&mut self) -> Option<AudioTelemetry> {
        let mut latest = None;
        while let Ok(telem) = self.telemetry_receiver.pop() {
            latest = Some(telem);
        }
        latest
    }
}

pub struct AudioThreadHandles {
    pub cmd_receiver: Consumer<AudioCommand>,
    pub telemetry_sender: Producer<AudioTelemetry>,
}

impl AudioThreadHandles {
    /// Drain all pending commands in the audio thread. Zero allocations.
    #[inline(always)]
    pub fn drain_commands<F>(&mut self, mut handler: F)
    where
        F: FnMut(AudioCommand),
    {
        while let Ok(cmd) = self.cmd_receiver.pop() {
            handler(cmd);
        }
    }

    /// Non-blocking, allocation-free telemetry push. Drops frame if queue is full.
    #[inline(always)]
    pub fn push_telemetry(&mut self, telemetry: AudioTelemetry) {
        let _ = self.telemetry_sender.push(telemetry);
    }
}
