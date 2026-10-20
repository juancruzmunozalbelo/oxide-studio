# Project: ableton-rust (Native macOS Electronic Music DAW)

## Architecture Overview
`ableton-rust` is a native macOS electronic music DAW implemented in Rust, inspired by the reverse-engineered architecture of Ableton Live 12.4.6 Suite and Ableton Learning Synths (`/Users/juancruz/ableton-rea`). It delivers a high-performance, single-window GPU-accelerated electronic music production studio.

### Key Architectural Layers
1. **Core Domain & Models (`ableton-core`)**:
   - Audio buffer structures, musical time representations (rational beats, bars, ticks), MIDI events, parameter IDs, and shared atomic telemetry types.
2. **DSP & Built-in Synthesizer (`ableton-dsp`)**:
   - Mystran 2x-oversampled non-linear ZDF 4-pole low-pass ladder filter with Padé [4/4] tanh(x)/x saturation (20 Hz - 15 kHz, resonance <= 1.0).
   - Band-limited PolyBLEP Sawtooth and Square/PWM (5-95%) oscillators + 32-bit XorShift white noise generator.
   - Dual ADSR envelopes (Amp & Mod) with RNBO convex attack lockout and exponential decay/release.
   - 4-shape LFO (Sine, Saw Down, Square, Sample & Hold).
   - Monophonic last-note priority voice stack with exponential one-pole portamento glide.
   - 38 factory synth presets catalog & 17 XY macro mappings with piecewise breakpoint interpolation.
3. **Electronic Drum Rack & Sample Engine (`ableton-drums`)**:
   - 16-pad drum machine with 32-voice allocation pool.
   - 909 sample playback (`kick.wav`, `snare.wav`, `hihat.wav` from `/Users/juancruz/ableton-rea/public/sounds/`) + synthesized electronic percussion.
   - Per-pad controls (level, pan, pitch transpose, decay envelope) and choke/mute groups with 5 ms clickless fadeout.
4. **Mixer & FX Chain (`ableton-fx`)**:
   - Per-track volume, pan, mute, solo, peak/RMS ballistics metering.
   - Saturator / overdrive distortion (Padé [4/4] tanh approximation).
   - Tempo-synced ping-pong delay with feedback filter.
   - Reverb (algorithmic FDN / convolution with impulse response).
   - Master 128-sample lookahead brickwall limiter (-0.3 dBFS ceiling).
5. **Real-Time Audio Engine & CoreAudio HAL (`ableton-audio`)**:
   - Zero-allocation steady-state audio callback running on CoreAudio HAL thread (`com.apple.audio.IOThread.client`) via `cpal`.
   - Lock-free SPSC command queue (`AudioCommand`) and triple-buffered telemetry queue (`AudioTelemetry`) via `rtrb`.
6. **Headless & Real-Time WAV Bounce Engine (`ableton-bounce`)**:
   - Offline multi-track render engine exporting 16-bit PCM (TPDF dither) or 32-bit float stereo `.wav` files with verified RIFF headers.
   - Headless CLI integration (`--render-demo <path>`).
7. **Session Matrix, Arrangement Timeline & Persistence (`ableton-timeline` & `ableton-project`)**:
   - Session clip matrix with quantized launching (`1 bar`, `1/4`, `immediate`), clip slot state machine, and pattern looping.
   - Linear multi-track arrangement timeline with automation curve interpolation (linear & bezier/power curves).
   - Session-to-Arrangement recording with "Back to Arrangement" track override.
   - `serde`-driven JSON project serialization (`DAWProject`).
8. **Out-of-Process AU/VST3 Plugin Scanner (`ableton-plugin-scanner`)**:
   - Reverse-engineered from Live 12's `Ableton Plugin Scanner`.
   - Scans `/Library/Audio/Plug-Ins/VST3`, `~/Library/Audio/Plug-Ins/VST3`, `/Library/Audio/Plug-Ins/Components`, and `~/Library/Audio/Plug-Ins/Components`.
   - Out-of-process crash isolation (SIGSEGV/SIGBUS quarantine) via CLI `--scan-plugins`.
   - JSON manifest cache.
9. **Single-Window 60 FPS GPU Studio UI/UX (`ableton-gui` & main binary `ableton-rust`)**:
   - Native macOS single window using `egui` + `eframe` over `wgpu` (Metal).
   - Dark carbon studio theme (`#07080a`, `#111318`) with high-contrast accents (`#ffbe00`, `#00d2ff`, `#00e676`).
   - Top transport & telemetry bar, Center workspace (Session Matrix vs Linear Arrangement toggle/split), Bottom deck (Device controls, Piano Roll / 16-step drum sequencer, XY Perform Pad).
   - Real-time oscilloscope, 64-band FFT spectrum, interactive 4-pole ZDF filter curve, live modulation ghost indicators on sliders.
   - Computer keyboard input: Piano layout and Wicki-Hayden Isomorphic layout.

---

## Feature Inventory
Every feature from the original request and survey is assigned to a milestone:

| # | Feature | Description | Milestone | Source |
|---|---------|-------------|-----------|--------|
| 1 | Mystran ZDF Ladder Filter | 2x-oversampled non-linear ZDF 4-pole ladder filter (Padé [4/4] tanh, resonance <= 1.0, 20Hz-15kHz) | M1 | ORIGINAL_REQUEST §R1.1 |
| 2 | PolyBLEP Oscillators | Band-limited Sawtooth, Square/PWM (5-95%), and 32-bit XorShift White Noise generator | M1 | ORIGINAL_REQUEST §R1.1 |
| 3 | ADSR Envelopes & 4-Shape LFO | RNBO convex attack lockout, exponential decay/release, LFO (Sine, Saw Down, Square, S&H) | M1 | ORIGINAL_REQUEST §R1.1 |
| 4 | Monophonic Voicing & Portamento | Monophonic last-note priority stack, legato gate persistence, exponential one-pole glide | M1 | ORIGINAL_REQUEST §R1.1 |
| 5 | Presets & Macro XY Catalog | 38 factory synth presets & 17 XY macro breakpoint curves imported from ableton-rea | M1 | ORIGINAL_REQUEST §R1.1 |
| 6 | 909 Drum Rack (Sample + Synth) | 16-pad drum machine, 909 sample playback (kick, snare, hihat) + synth percussion, choke groups | M2 | ORIGINAL_REQUEST §R1.2 |
| 7 | Insert & Master FX Chain | Saturator (Padé tanh), tempo-synced delay, reverb (FDN/convolution), master lookahead limiter | M2 | ORIGINAL_REQUEST §R1.3 |
| 8 | Multi-Track Mixer & Metering | Volume, pan, mute, solo, and peak/RMS ballistics metering per track and master bus | M2 | ORIGINAL_REQUEST §R1.3 |
| 9 | Zero-Allocation Audio Engine & CoreAudio | Real-time audio engine via cpal, SPSC lock-free rtrb queues, zero heap allocation during playback | M2 | ORIGINAL_REQUEST §R1 |
| 10 | Headless WAV Bounce Engine | 16-bit PCM and 32-bit float stereo WAV export with verified RIFF headers; CLI `--render-demo` | M2 | ORIGINAL_REQUEST §R1.3 |
| 11 | Session Clip Matrix | Quantized clip/scene launch (1 bar, 1/4, immediate), slot state machine, pattern looping | M3 | ORIGINAL_REQUEST §R2.1 |
| 12 | Linear Arrangement Timeline | Continuous bar/beat timeline, automation curves (linear/bezier), playhead tracking | M3 | ORIGINAL_REQUEST §R2.2 |
| 13 | Session-to-Arrangement Recording | Real-time capture of Session clip launches and tweaks into Arrangement with override state | M3 | ORIGINAL_REQUEST §R2.2 |
| 14 | Project Persistence | Universal JSON/RON project serialization (`DAWProject`), roundtrip disk test | M3 | ORIGINAL_REQUEST §R2.3 |
| 15 | Out-of-Process Plugin Scanner | Scan macOS VST3 & AU plugins, crash-isolated worker subprocess, CLI `--scan-plugins` | M4 | ORIGINAL_REQUEST §R3.1 |
| 16 | AU / VST3 Plugin Hosting Basics | Track plugin slots, audio/MIDI buffer routing, parameter exposure | M4 | ORIGINAL_REQUEST §R3.2 |
| 17 | Single-Window GPU Studio UI | Zero-menu-diving layout (Transport, Center Workspace, Bottom Deck) via egui/eframe Metal | M5 | ORIGINAL_REQUEST §R4.1 |
| 18 | Studio Visual Craft & Telemetry | Carbon theme, real-time oscilloscope, 64-band FFT, interactive ZDF curve, modulation ghosts | M5 | ORIGINAL_REQUEST §R4.2 |
| 19 | Piano Roll & 16-Step Drum Sequencer | Visual step sequencer for 909 pads and piano roll editor for melodic clips | M5 | ORIGINAL_REQUEST §R4.1 |
| 20 | Interactive XY Perform Pad | Performance XY pad with phosphor decay trail, parameter cross-modulation | M5 | ORIGINAL_REQUEST §R4.1 |
| 21 | Playable Computer Keyboards | Piano layout (A-S-D-F... + W-E-T...) and Wicki-Hayden Isomorphic layout with octave shift | M5 | ORIGINAL_REQUEST §R4.2 |
| 22 | Comprehensive E2E Verification | 100% pass of Tiers 1-4 E2E tests, Tier 5 adversarial hardening, and full acceptance suite | M6 | Acceptance Criteria |
| 23 | Model Context Protocol Server | JSON-RPC 2.0 stdio MCP server (crates/ableton-mcp & --mcp) for full autonomous control | M7 | ORIGINAL_REQUEST §R5 |

---

## Milestones & Work Plan

| # | Milestone Name | Scope | Dependencies | Status |
|---|----------------|-------|-------------|--------|
| M0 | Workspace Setup & Core Crates | Workspace Cargo.toml, toolchain environment, `ableton-core` definitions | None | DONE |
| M1 | DSP Core & Synth Engine | `ableton-dsp`: Mystran ZDF, PolyBLEP, ADSR, LFO, Voicing, 38 Presets, 17 Mappings | M0 | DONE |
| M2 | Drums, FX, Audio Engine & Bounce | `ableton-drums`, `ableton-fx`, `ableton-audio`, `ableton-bounce`, CLI `--render-demo` | M1 | DONE |
| M3 | Session, Arrangement & Project | `ableton-timeline`, `ableton-project`, Session matrix, timeline, serde persistence | M2 | DONE |
| M4 | AU/VST3 Plugin Scanner | `ableton-plugin-scanner`, crash isolation, `--scan-plugins`, manifest cache | M0 | DONE |
| M5 | 60 FPS Single-Window UI/UX | `ableton-gui`, egui/eframe Metal, visualizers, step sequencer, XY pad, keyboards | M1, M2, M3, M4 | DONE |
| M6 | Final E2E Integration & Audit | E2E suite pass, headless bounce verify, roundtrip test, 60 FPS GUI check | M1, M2, M3, M4, M5 | DONE |
| M7 | Full-Control MCP Server | `crates/ableton-mcp`, `--mcp` CLI flag, `tests/e2e_mcp_server_test.rs` | M0..M6 | DONE |

---

## Code Layout
```
/Users/juancruz/ableton-rust/
├── Cargo.toml                      # Root workspace manifest
├── crates/
│   ├── ableton-core/               # Fundamental types: AudioBuffer, MusicalTime, MidiEvent, AtomicTelemetry
│   │   ├── Cargo.toml
│   │   └── src/
│   ├── ableton-dsp/                # Mystran ZDF ladder, PolyBLEP, ADSR, LFO, Synth, Presets, Mappings
│   │   ├── Cargo.toml
│   │   └── src/
│   ├── ableton-drums/              # 16-pad Drum Rack, 909 sample player, synth drums, choke groups
│   │   ├── Cargo.toml
│   │   └── src/
│   ├── ableton-fx/                 # Saturator, ping-pong delay, reverb, limiter, meters
│   │   ├── Cargo.toml
│   │   └── src/
│   ├── ableton-audio/              # CoreAudio HAL via cpal, zero-alloc audio loop, SPSC queues
│   │   ├── Cargo.toml
│   │   └── src/
│   ├── ableton-bounce/             # Headless multi-track bounce, 16/32-bit WAV RIFF writer
│   │   ├── Cargo.toml
│   │   └── src/
│   ├── ableton-timeline/           # Session matrix, arrangement timeline, automation curves
│   │   ├── Cargo.toml
│   │   └── src/
│   ├── ableton-project/            # DAWProject serialization (serde JSON/RON), schema
│   │   ├── Cargo.toml
│   │   └── src/
│   ├── ableton-plugin-scanner/     # Out-of-process AU/VST3 scanner, crash-isolated worker
│   │   ├── Cargo.toml
│   │   └── src/
│   ├── ableton-gui/                # egui/eframe single-window studio, visualizers, perform pad
│   │   ├── Cargo.toml
│   │   └── src/
│   └── ableton-mcp/                # Full-control Model Context Protocol server (JSON-RPC 2.0 stdio)
│       ├── Cargo.toml
│       └── src/
├── src/                            # Main application binary entrypoint
│   └── main.rs                     # CLI dispatcher (--render-demo, --scan-plugins, --mcp, GUI launch)
├── tests/                          # E2E integration test suite
│   ├── e2e_dsp_filter_test.rs
│   ├── e2e_monophonic_presets_test.rs
│   ├── e2e_drum_rack_test.rs
│   ├── e2e_bounce_render_test.rs
│   ├── e2e_project_roundtrip_test.rs
│   ├── e2e_plugin_scanner_test.rs
│   └── e2e_mcp_server_test.rs
└── assets/                         # Sound assets copied/linked from ableton-rea
    ├── kick.wav
    ├── snare.wav
    ├── hihat.wav
    └── impulseresponse.wav
```

---

## Interface Contracts

### 1. `ableton-core` ↔ Audio Thread & DSP
```rust
pub struct AudioBuffer {
    pub left: [f32; 1024],
    pub right: [f32; 1024],
    pub frame_count: usize,
}

pub struct MusicalTime {
    pub beat: f64,
    pub bpm: f64,
    pub signature_num: u8,
    pub signature_den: u8,
    pub is_playing: bool,
}

pub enum MidiEvent {
    NoteOn { channel: u8, note: u8, velocity: u8 },
    NoteOff { channel: u8, note: u8 },
    ControlChange { channel: u8, controller: u8, value: u8 },
    PitchBend { channel: u8, value: i16 },
}
```

### 2. UI ↔ Audio Engine Lock-Free Communication
```rust
pub enum AudioCommand {
    Play,
    Stop,
    SetTempo(f64),
    NoteOn { track_id: usize, note: u8, velocity: u8 },
    NoteOff { track_id: usize, note: u8 },
    SetParameter { track_id: usize, param_id: u32, value: f32 },
    TriggerClip { track_id: usize, slot_id: usize },
    StopClip { track_id: usize },
    TriggerScene(usize),
}

pub struct AudioTelemetry {
    pub playhead_beat: f64,
    pub cpu_load: f32,
    pub track_peaks_l: [f32; 16],
    pub track_peaks_r: [f32; 16],
    pub master_peak_l: f32,
    pub master_peak_r: f32,
    pub master_rms_l: f32,
    pub master_rms_r: f32,
    pub oscilloscope_slice: [f32; 256],
    pub spectrum_bins: [f32; 64],
}
```

### 3. Out-of-Process Plugin Scanner Interface
```rust
pub struct PluginDescriptor {
    pub name: String,
    pub vendor: String,
    pub category: PluginCategory, // Instrument, Effect
    pub format: PluginFormat,     // AudioUnit, VST3
    pub path: PathBuf,
    pub is_valid: bool,
    pub quarantine_reason: Option<String>,
}
```
