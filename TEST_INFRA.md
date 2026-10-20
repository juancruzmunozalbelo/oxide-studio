# TEST_INFRA.md: E2E Test Suite Infrastructure & Verification Architecture

## 1. Test Philosophy & Design Principles

### 1.1 Opaque-Box, Requirement-Driven Testing
The testing strategy for `ableton-rust` is strictly **opaque-box** and **requirement-driven**, derived directly from `ORIGINAL_REQUEST.md` and the reverse-engineered reference baselines from Ableton Live 12.4.6 Suite and Ableton Learning Synths (`/Users/juancruz/ableton-rea`).
- **Opaque-Box Boundary**: Tests interact exclusively with public module interfaces, API contracts, audio buffers, JSON schemas, and process boundaries. No test relies on private fields, implementation epicycles, or volatile internal refactorings.
- **Authoritative Oracles**: Expected outputs and numerical tolerances are derived directly from the decompiled DSP engine (`/Users/juancruz/ableton-rea/src/engine/dsp-core.ts`), the verified test baselines (`/Users/juancruz/ableton-rea/tests/dsp-core.test.ts`), the factory catalogs (`preset-catalog.json`, `mapping-catalog.json`), and the official Ableton Live 12 architectural specifications (`/Users/juancruz/ableton-rea/docs/live12-suite/`).
- **Deterministic & Isolated**: Every test case constructs its own fixtures, executes deterministically without ordering dependencies, and cleans up any temporary disk artifacts upon completion.

### 1.2 Multi-Tier Coverage Hierarchy
Verification is organized into five rigorous tiers across every functional component:
- **Tier 1 (Smoke & Instantiation)**: Validates default constructors, initial state validity, zero immediate panic, and memory sanity.
- **Tier 2 (Component & Subsystem Logic)**: Tests isolated mathematical and algorithmic properties (Padé [4/4] rational saturation, PolyBLEP edge corrections, ADSR stage transitions, parameter curve scalings, choke group voice termination, serialization fidelity).
- **Tier 3 (End-to-End Workflow Integration)**: Exercises complete cross-crate signal and data flows (MIDI note stream through voice stack, filter, envelopes, and mixer into audio buffers; 16-pad drum triggers; timeline arrangement clip rendering into multi-track WAV; project file save and restore).
- **Tier 4 (System Invariants & Real-Time Guarantees)**: Verifies non-functional and real-time guarantees: bounded signal amplitudes ($|y[n]| < 2.0$), zero `NaN` or `Inf` generation, zero audio-thread dynamic allocations during steady-state rendering, and clickless choke damping ($< 5\text{ ms}$).
- **Tier 5 (Adversarial & Fault Hardening)**: Stresses boundary conditions: self-oscillation at maximum resonance ($r = 1.0$), rapid legato note thrashing, corrupted JSON documents, and out-of-process crash recovery under fatal signals (`SIGSEGV` / `SIGBUS`).

---

## 2. Feature Inventory & Coverage Matrix (Features 1–22)

The 22 core features identified in `PROJECT.md` are mapped across all testing tiers:

| # | Feature | Milestone | Source Requirement | Tier 1 (Smoke) | Tier 2 (Subsystem) | Tier 3 (E2E Integration) | Tier 4 (Invariants / Hardening) |
|---|---------|-----------|--------------------|----------------|---------------------|--------------------------|---------------------------------|
| 1 | **Mystran ZDF Ladder Filter** | M1 | `ORIGINAL_REQUEST` §R1.1 | Default instantiation at 44.1/48kHz | Prewarping $\tan(w)$ polynomial, Padé [4/4] $\frac{\tanh(x)}{x}$ accuracy | 880Hz saw sweep through 180Hz vs 15kHz cutoff (>20dB roll-off) | Self-oscillation bounded $\|y[n]\| < 2.0$, zero NaN/Inf at $r=1.0$ |
| 2 | **PolyBLEP Oscillators** | M1 | `ORIGINAL_REQUEST` §R1.1 | Instantiate Saw, Square/PWM, Noise | PolyBLEP residual function at discontinuity $t < dt$ and $t > 1-dt$ | Oscillator mixer balance with 5-95% pulse width modulation | Band-limited harmonic suppression, no DC drift, XorShift PRNG uniform distribution |
| 3 | **ADSR Envelopes & 4-Shape LFO** | M1 | `ORIGINAL_REQUEST` §R1.1 | Instantiate Amp/Mod ADSR and LFO | RNBO convex attack curve, exponential decay/release coefficients | Mod envelope modulating filter cutoff; LFO pitch vibrato & PWM | Continuous transitions, monotonic attack, zero gate clicks, LFO phase wrap S&H updates |
| 4 | **Monophonic Voicing & Portamento** | M1 | `ORIGINAL_REQUEST` §R1.1 | Initialize NoteStack, empty check | NoteStack push/remove/top deduplication order | Note trill: 60 $\to$ 64 $\to$ release 64 restores 60 without gate re-trigger | Exponential one-pole glide smoothing, zero stack overflow on 16+ notes |
| 5 | **Presets & Macro XY Catalog** | M1 | `ORIGINAL_REQUEST` §R1.1 | Parse 38 presets & 17 mapping JSONs | 4 curve scalings: linear, exponential, log2 freq, stepped | Macro pad coordinate $(X, Y) \in [0, 1]^2$ breakpoint interpolation across all 17 maps | Parameter clamp to declared valid domains, identical acoustic response to `ableton-rea` |
| 6 | **909 Drum Rack (Sample + Synth)** | M2 | `ORIGINAL_REQUEST` §R1.2 | Instantiate 16-pad drum rack | Pad volume/pan/pitch/decay parameter scaling | Multi-pad playback combining 909 sample assets and synthesized percussion | Choke group instant fade-out (<5ms), 32-voice pool dynamic voice stealing without allocation |
| 7 | **Insert & Master FX Chain** | M2 | `ORIGINAL_REQUEST` §R1.3 | Instantiate Saturator, Delay, Reverb, Limiter | Padé tanh overdrive curve, delay ring buffer write/read pointer wrap | Audio signal routed through entire insert chain into master bus | Limiter -0.3 dBFS strict ceiling, 128-sample lookahead, zero clipping under +12dB overdrive |
| 8 | **Multi-Track Mixer & Metering** | M2 | `ORIGINAL_REQUEST` §R1.3 | Initialize 16 tracks + master bus | Constant-power sine/cosine panning law, dB-to-linear conversion | Multi-track summing with independent mute and solo logic | Peak hold and 300ms sliding RMS window accuracy, zero inter-channel bleed |
| 9 | **Zero-Allocation Audio Engine** | M2 | `ORIGINAL_REQUEST` §R1 | Instantiate CoreAudio engine struct | SPSC `rtrb` queue push/pop latency | Steady-state audio rendering loop processing 1024-frame blocks | Strict zero heap allocation in audio thread (`stats_alloc` assertion), lock-free wait freedom |
| 10 | **Headless WAV Bounce Engine** | M2 | `ORIGINAL_REQUEST` §R1.3 | Instantiate WAV writer struct | RIFF 44-byte WAVE header validation (`fmt ` and `data` chunks) | Headless CLI bounce (`--render-demo`) exporting 4.0s multi-track arrangement | Valid 16-bit / 32-bit float PCM, duration $\ge 4.0\text{s}$, RMS $> 0.02$, zero NaN/Inf samples |
| 11 | **Session Clip Matrix** | M3 | `ORIGINAL_REQUEST` §R2.1 | Initialize grid of clip slots | Quantization math for Immediate, 1/16, 1/4, 1 Bar, 2 Bars | Scene launch triggering simultaneous clip starts across tracks | Slot state machine integrity (Empty, Stopped, QueuedLaunch, Playing, QueuedStop) |
| 12 | **Linear Arrangement Timeline** | M3 | `ORIGINAL_REQUEST` §R2.2 | Create arrangement timeline container | Linear and exponential/power automation curve interpolation | Multi-track timeline playback tracking playhead beat position | Loop boundary wrapping, $C^0$ continuity on automation joins |
| 13 | **Session-to-Arrangement Recording** | M3 | `ORIGINAL_REQUEST` §R2.2 | Initialize track arrangement state | `ArrangementControlState` transition (Arrangement vs SessionOverride) | Real-time capture of Session clip triggers and automation into arrangement | "Back to Arrangement" resynchronization resets all track overrides without glitch |
| 14 | **Project Persistence** | M3 | `ORIGINAL_REQUEST` §R2.3 | Serialize default `DAWProject` to JSON | Schema completeness: tracks, clips, presets, macros, mixer | Save project to disk, reload from disk, verify structural equivalence | Roundtrip serialization fidelity, graceful failure and error reporting on malformed JSON |
| 15 | **Out-of-Process Plugin Scanner** | M4 | `ORIGINAL_REQUEST` §R3.1 | Spawn scanner worker CLI subprocess | Parse emitted JSON `PluginDescriptor` metadata | Enumerate `/Library/Audio/Plug-Ins/` AU/VST3 plugins into manifest | Master process survives simulated `SIGSEGV` / `SIGBUS` worker crash with quarantine flag |
| 16 | **AU / VST3 Plugin Hosting Basics** | M4 | `ORIGINAL_REQUEST` §R3.2 | Instantiate plugin slot descriptor | Audio/MIDI buffer mapping and parameter index enumeration | Route track audio buffers through plugin processing entrypoint | Clean error handling on missing bundle or unsupported channel layout |
| 17 | **Single-Window GPU Studio UI** | M5 | `ORIGINAL_REQUEST` §R4.1 | Initialize `eframe` / `egui` Metal context | Zero-menu layout decomposition: Transport, Center, Bottom Deck | Single-key toggle (`Tab`) between Session Matrix and Arrangement | Sustained 60 FPS repaint on macOS Retina displays |
| 18 | **Studio Visual Craft & Telemetry** | M5 | `ORIGINAL_REQUEST` §R4.2 | Validate carbon theme color palette | 64-band FFT log binning, 256-sample oscilloscope buffer | Real-time audio telemetry rendered on UI with modulation ghost indicators | Device Pixel Ratio clamped at 1.5 to eliminate GPU fill-rate throttling |
| 19 | **Piano Roll & 16-Step Drum Sequencer** | M5 | `ORIGINAL_REQUEST` §R4.1 | Instantiate step grid and piano roll models | Step toggle, velocity edit, accent and tie flags | Step sequencer triggers 909 pads; piano roll triggers synth notes | Rational musical beat precision, zero timing jitter across tempo changes |
| 20 | **Interactive XY Perform Pad** | M5 | `ORIGINAL_REQUEST` §R4.1 | Initialize XY pad state | Coordinate drag $(X, Y) \in [0.0, 1.0]^2$ to parameter dispatch | Real-time modulation sweep of Cutoff and Resonance | Phosphor decay ring trail rendering, audio-reactive RMS bloom |
| 21 | **Playable Computer Keyboards** | M5 | `ORIGINAL_REQUEST` §R4.2 | Key event dispatcher instantiation | Piano layout mapping (A-S-D-F) & Wicki-Hayden Isomorphic mapping | Octave shift hotkeys ($Z / X$), active polyphonic key state tracking | Zero stuck notes on rapid chord changes and focus loss |
| 22 | **Comprehensive E2E Verification** | M6 | Acceptance Criteria | Verify all test targets compile | Automated test runner execution across full test inventory | Complete end-to-end multi-track playback, bounce, and GUI verification | 100% pass rate across all test suites, zero regressions |

---

## 3. Test Suite Architecture & File Organization

The integration test suite is located in `/Users/juancruz/ableton-rust/tests/` and comprises six dedicated E2E test files:

### 3.1 `tests/e2e_dsp_filter_test.rs`
- **Domain**: Pure DSP, Mystran 2x ZDF 4-Pole Ladder Filter, PolyBLEP Oscillators, ADSR Envelopes, and LFO.
- **Key Test Cases**:
  - `test_mystran_zdf_rolloff_attenuation`: Renders an 880 Hz sawtooth wave through cutoff 180 Hz versus cutoff 15,000 Hz at 44.1 kHz. Computes steady-state RMS energy (samples 512–2048) and verifies that attenuation exceeds 20 dB ($E_{180\text{Hz}} < 0.1 \times E_{15\text{kHz}}$).
  - `test_mystran_zdf_self_oscillation_stability`: Drives the filter with maximum resonance ($resonance = 1.0$) and impulse excitation. Asserts all 4096 output samples are finite, non-NaN, and strictly bounded within $|y[n]| < 2.0$.
  - `test_polyblep_sawtooth_bandlimited_harmonics`: Verifies that PolyBLEP saw wave suppresses Nyquist discontinuities compared to naive saw, maintaining zero DC bias.
  - `test_polyblep_square_pwm_duty_cycle`: Verifies duty cycle modulation from 5% to 95% with accurate mean values.
  - `test_adsr_convex_attack_and_exponential_decay`: Tests RNBO convex attack curve curvature ($value + step \times (1.25 - 0.25 \times value)$) and exponential decay/release asymptotes.
  - `test_lfo_shapes_and_phase`: Validates all 4 LFO shapes (Sine, SawDown, Square, Sample & Hold) and envelope-controlled frequency modulation.

### 3.2 `tests/e2e_monophonic_presets_test.rs`
- **Domain**: Monophonic Last-Note Priority Voice Stack, Legato Phrasing, Portamento Glide, 38 Factory Presets, and 17 XY Macro Mappings.
- **Key Test Cases**:
  - `test_monophonic_last_note_priority_legato`: Triggers NoteOn(60) $\to$ NoteOn(64) $\to$ NoteOff(64). Verifies that releasing 64 restores note 60 as active pitch without closing the envelope gate or re-triggering attack.
  - `test_exponential_glide_slew`: Verifies smooth one-pole exponential glide between pitch transitions over specified glide times.
  - `test_38_factory_presets_evaluation`: Loads all 38 factory presets from `preset-catalog.json`, parses their 27–30 parameters, and verifies parameter domains and DSP instantiation.
  - `test_17_xy_macro_breakpoint_interpolation`: Evaluates all 17 XY macro mappings from `mapping-catalog.json` across breakpoints, confirming exact target parameter values and scaling curve mappings.

### 3.3 `tests/e2e_drum_rack_test.rs`
- **Domain**: 16-Pad Electronic Drum Machine, Sample Playback, 909 Algorithmic Synthesis, Voice Pooling, and Choke Groups.
- **Key Test Cases**:
  - `test_drum_rack_16_pad_initialization`: Verifies pad assignments (MIDI 36–51) for standard 909 mapping (Kick, Snare, HiHats, Toms, Cymbals).
  - `test_sample_playback_interpolation`: Verifies fractional phase accumulator with linear/cubic interpolation for pitch-shifted sample playback.
  - `test_909_choke_group_hihat_damping`: Triggers Open Hi-Hat (Pad 10) on Choke Group 1, then triggers Closed Hi-Hat (Pad 6) on Choke Group 1. Asserts that open hi-hat resonance is damped below $-60\text{ dBFS}$ within 240 samples ($< 5\text{ ms}$).
  - `test_voice_pool_stealing_without_allocation`: Triggers >32 simultaneous drum voices. Asserts that the oldest or quietest voice is stolen cleanly without heap allocation.
  - `test_per_pad_volume_pan_pitch_decay`: Validates constant-power panning, semitone pitch transposition, and decay envelope attenuation per pad.

### 3.4 `tests/e2e_project_roundtrip_test.rs`
- **Domain**: Universal JSON Project Persistence (`DAWProject`), Serialization, Deserialization, and Structural Fidelity.
- **Key Test Cases**:
  - `test_daw_project_serialization_roundtrip`: Instantiates a comprehensive project containing transport configuration, master track limiter, MIDI synth tracks, 909 Drum Rack tracks, Session clip matrices, Arrangement timeline clips, and automation lanes. Serializes to JSON, deserializes back, and asserts structural equality.
  - `test_persistence_with_all_38_presets_and_17_macros`: Verifies that a project file embedding factory synth patches and XY macro mappings serializes and deserializes without data loss.
  - `test_malformed_json_graceful_error_handling`: Verifies that syntax errors or missing required fields in project files produce clear `Err(DAWProjectError)` messages rather than panics.

### 3.5 `tests/e2e_plugin_scanner_test.rs`
- **Domain**: Out-of-Process AU/VST3 Plugin Discovery, Process Supervisor, Crash Isolation, and Quarantine Architecture.
- **Key Test Cases**:
  - `test_plugin_scanner_standard_directory_discovery`: Scans `/Library/Audio/Plug-Ins/` and user directories, producing a valid `PluginManifest`.
  - `test_crash_isolation_fatal_signal_survival`: Spawns a worker subprocess configured to simulate a fatal plugin crash (`SIGSEGV` / `SIGBUS`). Verifies that the host supervisor process catches the crash, records the plugin as quarantined, and continues normal execution without terminating.
  - `test_scanner_timeout_on_hung_plugin`: Tests worker timeout handling when a misbehaving plugin hangs during initialization.
  - `test_plugin_manifest_caching`: Verifies that unchanged plugin bundles are loaded from disk cache in $< 5\text{ ms}$ based on bundle `mtime`.

### 3.6 `tests/e2e_bounce_render_test.rs`
- **Domain**: Multi-Track Offline Render Engine, WAV File Writer, and Headless CLI Export.
- **Key Test Cases**:
  - `test_multitrack_offline_bounce_wav_export`: Renders an electronic music arrangement combining Synth melody and 909 drum patterns into a 16-bit or 32-bit float stereo `.wav` file.
  - `test_wav_riff_header_and_chunk_integrity`: Inspects the exported file's binary header: validates standard 4-byte `RIFF` magic, `WAVE` format marker, `fmt ` sub-chunk (16 or 32-bit, 2 channels, 44.1/48 kHz), and `data` chunk size.
  - `test_bounce_audio_duration_and_energy`: Asserts exported audio duration is $\ge 4.0\text{ seconds}$, RMS signal energy is $> 0.02$ (confirming non-silent, audible output), and scans all sample frames to verify zero `NaN` or `Inf` samples.

---

## 4. Test Execution & Toolchain Commands

### 4.1 Prerequisites
The tests require a standard Rust toolchain (Rust 2021 edition, `rustc 1.80+`). Ensure the toolchain is accessible in `$PATH`:
```bash
export PATH="$HOME/.cargo/bin:/opt/homebrew/bin:$PATH"
```

### 4.2 Running the E2E Test Suite
To run all integration tests across the workspace:
```bash
cargo test --workspace
```

To run individual E2E test suites with detailed output:
```bash
# 1. DSP & ZDF Ladder Filter
cargo test --test e2e_dsp_filter_test -- --nocapture

# 2. Monophonic Voicing, Presets & Macro Mappings
cargo test --test e2e_monophonic_presets_test -- --nocapture

# 3. 16-Pad Drum Rack & Choke Groups
cargo test --test e2e_drum_rack_test -- --nocapture

# 4. Project Persistence Roundtrip
cargo test --test e2e_project_roundtrip_test -- --nocapture

# 5. Out-of-Process Plugin Scanner Crash Isolation
cargo test --test e2e_plugin_scanner_test -- --nocapture

# 6. Headless WAV Bounce Export
cargo test --test e2e_bounce_render_test -- --nocapture
```

### 4.3 Running the Headless CLI Subcommands
```bash
# Render multi-track demo arrangement to stereo WAV
cargo run --release -- --render-demo /tmp/ableton_rust_demo.wav

# Run out-of-process AU/VST3 plugin scanner
cargo run --release -- --scan-plugins
```

---

## 5. Acceptance Criteria & Pass/Fail Thresholds

| Test Suite | Acceptance Metric | Strict Passing Threshold |
| :--- | :--- | :--- |
| **ZDF Ladder Filter** | 880 Hz Saw Filter Roll-off Attenuation | $> 20.0\text{ dB}$ ($E_{180\text{Hz}} < 0.1 \times E_{15\text{kHz}}$) |
| **ZDF Ladder Filter** | Self-Oscillation Stability at $r = 1.0$ | $\|y[n]\| < 2.0$, strictly finite, zero `NaN` or `Inf` |
| **Voice Stack & Glide** | Monophonic Last-Note Legato Trill | Gate stays open on upper release; pitch restored smoothly |
| **Preset & Macro Catalog** | Preset & Macro Breakpoint Precision | 38 presets loaded; 17 XY maps within $\pm 1.0\text{ Hz}$ / $\pm 0.01$ |
| **Drum Rack** | Hi-Hat Choke Group Ring Damping | Damped below $-60\text{ dBFS}$ in $< 240$ samples ($< 5\text{ ms}$) |
| **Drum Rack** | Polyphonic Voice Stealing | Oldest/quietest voice reused; zero heap allocations |
| **Project Persistence** | `DAWProject` JSON Roundtrip | $100\%$ structural equivalence: `original == deserialized` |
| **Plugin Scanner** | Subprocess Crash Isolation | Master survives `SIGSEGV` / `SIGBUS`; marks candidate quarantined |
| **WAV Bounce Engine** | RIFF Header & Sample Format | Valid 44-byte RIFF/WAVE header, 2 channels, 44.1/48 kHz |
| **WAV Bounce Engine** | Audio Signal Duration & Energy | Duration $\ge 4.0\text{ s}$, RMS $> 0.02$, zero `NaN` or `Inf` |
| **Full Workspace** | Cargo Compilation & Test Pass Rate | $0$ compiler errors, $0$ warnings with `-D warnings`, $100\%$ tests pass |
