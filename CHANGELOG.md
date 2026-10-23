# Changelog

All notable changes to **Oxide Studio** (`oxide-studio`) are documented in this file.
The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [1.0.0] — 2026-10-22

### 🎉 Landmark 1.0.0 Production Release: Oxide Studio Workstation Architecture & DSP Specification

Oxide Studio v1.0.0 marks the official public open-source release of the clean-room native Rust digital audio workstation (DAW), Oxide Studio electronic music workstation, and 76-tool Model Context Protocol (MCP) server.

This release represents the culmination of all 12 Workstation Architecture & DSP Specification Epics (#17–#28), backed by 674 passing automated tests, 576 layout guard frames, and an interactive In-Browser Web Workstation.

---

### 🚀 Highlights & Flagship Features

#### 1. In-Browser Interactive Workstation (`docs/web/`)
- **100% Client-Side Web DAW**: Zero-install electronic music studio running directly on GitHub Pages via WebAudio and HTML5 Canvas.
- **Dual-View Paradigm**: Press `Tab` to switch seamlessly between the non-linear Session Matrix and linear Arrangement Timeline.
- **16-Voice Synthesized TR-909**: Complete Bank A (Core Drums) & Bank B (Cymbals & Latin Percussion) with 16-step sequencer, step velocity shading, accents, and Groove Pool shuffle.
- **Polyphonic Suite Synthesizer Engine**: Real-time mode switching between `Drift`, `Meld`, `Operator`, and `Wavetable` with interactive ADSR envelope shaping.
- **Audio FX Rack with Live 60 FPS Visualizers**: Glue Compressor with analog needle ballistics, OTT Multiband Dynamics, Roar ADAA waveshaper, and real-time 64-bin FFT spectrum and oscilloscope.
- **Bilingual OKF Copilot**: Interactive AI studio assistant answering in English and Argentine Spanish (`EN`/`ES`).
- **1-Click In-Browser WAV Bounce**: Render and export studio arrangements to 16/24-bit WAV directly in the browser.

#### 2. The 12 Workstation Architecture & DSP Specification Epics (#17–#28)
- **Epic #17 (DSP Engine)**: Real-time multi-track audio recording with hardware I/O matrix (up to 64 buses), zero-allocation circular recording ring buffers, and sample-accurate `DriverLatencyCompensator` ensuring loopback click transients align to $\le 1$ sample accuracy.
- **Epic #18 (Plugin Hosting)**: Out-of-process isolation architecture hosting `CLAP`, `VST3`, and macOS `AUv2` plugins via `oxide-plugin-scanner`. Features a POSIX shared-memory IPC audio ring buffer and crash watchdog with transparent zero-latency dry bypass (DAW survives `kill -SEGV` with zero dropouts).
- **Epic #19 (Audio Warping)**: 6-mode audio warping engine (`Beats`, `Tones`, `Texture`, `RePitch`, `Complex`, `ComplexPro`) powered by a 2048-point STFT phase-vocoder with Laroche-Dolson phase locking and LPC/cepstral formant preservation. Double-buffered disk streaming cache for multi-gigabyte audio assets.
- **Epic #20 (DSP Graph & Live Coding)**: Modular patch graph with embedded Wasmtime JIT compiler and Faust DSP backend for Max for Live (`.amxd`) equivalent visual patching and audio-rate parameter modulation.
- **Epic #21 (Suite Instruments)**: Complete 8-instrument suite (`Drift`, `Meld` bi-timbral macro-synth with 12 engines, `Collision` mallet resonator, `Tension` waveguide string model, `Electric` electro-mechanical piano, `Analog` dual-VCO synth, `Sampler`, and `Granulator III` MPE granular scrub engine) plus 16 TR-909 physical modeling percussion voices.
- **Epic #22 (Suite Audio FX)**: Complete 7-device studio effects suite (`Hybrid Reverb` partitioned zero-latency convolution + FDN algorithmic hall, `Spectral Resonator` STFT harmonic tracking, `Spectral Time` freeze, `Echo` tape/digital delay, 40-band `Vocoder`, `Corpus` membrane resonator, and 4x oversampled `TruePeakLimiter`).
- **Epic #23 (MIDI FX Racks)**: 8 native MIDI effect devices (`Arpeggiator` with 16 patterns, `Chord` 6-interval voicings, `Scale` with global project sync, `Velocity` compression/humanization, `Random`, `Note Length`, `Pitch`, and `MPE Control`).
- **Epic #24 (Hardware Control Surfaces)**: Native USB 60 FPS display driver for Ableton Push 2 (960x160 BGR565) and Push 3 64-pad MPE matrix, APC40 mkII, Launchpad Pro, and universal MIDI Learn with `Jump`, `Pickup`, and `Value Scaling` parameter takeover modes.
- **Epic #25 (Arrangement & Workflow)**: Up to 8-level nested Group Tracks, deterministic offline track freeze & flatten to 32-bit float audio, non-destructive Bounce-in-Place (`Cmd+B`), all 9 Session Follow Action types with dual A:B chance weighting, and Linked-Track multi-track editing.
- **Epic #26 (Machine Listening & Grooves)**: Polyphonic Audio-to-MIDI transcription (`Convert Drums to MIDI`, `Convert Melody to MIDI` via pYIN, `Convert Harmony to MIDI` via CQT chroma filterbank), and `.agr` Groove Pool template extraction and non-destructive swing application.
- **Epic #27 (Formats & Exchange)**: Official Ableton Live Pack (`.alp`) authoring bundler and unpacker, `LosslessFreezeCodec` (Delta-LPC Rice compression saving 50–60% disk space with $-\infty\text{ dBFS}$ bit-exact null test), and open `DAWproject` (`.dawproject`) XML interchange with Bitwig and Studio One.
- **Epic #28 (Cross-Platform Platform & CI)**: Cross-platform `wgpu` rendering backend (Metal on macOS, Vulkan on Linux, DirectX 12 on Windows), multi-platform audio driver abstraction (CoreAudio, PipeWire/JACK, WASAPI/ASIO), and automated golden-file telemetry verification.

#### 3. Beyond Stock Ableton Live: 5 Top Producer Tools
1. **1-Click Bounce-in-Place (`Cmd+B`)**: Renders selected arrangement tracks or clip selections in-process without manual track cloning; preserves the original MIDI and synth device chain in a zero-CPU "Source Take Lane" with 1-click reversion.
2. **DuckShaper LFO**: Sample-accurate 2048-point bezier ducking curve evaluator with 4th-order Linkwitz-Riley 120 Hz crossover to duck low-end rumble while preserving transient attack grit without third-party sidechain plugins.
3. **MasterClipper**: Continuously adjustable polynomial soft/hard knee with 2x, 4x, and 8x linear-phase half-band FIR polyphase oversampling, shave-meter, and crest factor (Peak-to-RMS) display.
4. **PhaseScope**: Dual-waveform time-domain correlation scope comparing Bass fundamentals against Kick sidechains with Pearson correlation coefficient $\rho \in [-1.0, +1.0]$, acoustic cancellation alerts, and 360° Hilbert phase rotator.
5. **ClubDeck**: Master channel loudness analyzer with integrated and short-term (3s) LUFS, target markers (-8 LUFS Club, -14 LUFS Streaming), 4x oversampled sinc true-peak detection, and 1-click A/B reference track player with automatic gain matching.

#### 4. 76-Tool Model Context Protocol (MCP) Server
- Complete stdio JSON-RPC 2.0 interface across 15 functional domains.
- Native Live GUI TCP bridge (`127.0.0.1:9877`) for live bidirectional workstation control from Claude Desktop, Antigravity, and Cursor.

---

### 🧪 Quality, Performance & Verification Invariants

- **674 Workspace Tests**: 100% green test execution across all 11 workspace crates (`cargo test --workspace`).
- **576 Layout Guard Frames**: Multi-resolution responsive layout audit (960x600, 1280x800, 1920x1080, 2560x1440) across 12 splitter heights, 2 workspace views, and 6 deck states asserting 0 overflows, 0 clipped text, 0 collisions, and 0 tofu glyphs.
- **Hard Real-Time Zero-Allocation Audio Thread**: Strict prohibition of heap allocations (`Box`, `Vec`, `String`), mutex locking, and file/socket I/O inside the audio rendering callback.
- **Mathematical Null Tests**: Verified $-\infty\text{ dBFS}$ exact mathematical null on 180° polarity inversion, fader minimum attenuation, and channel mute silence.
