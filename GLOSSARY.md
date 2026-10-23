# Oxide Studio Domain Glossary

Canonical domain terminology for **Oxide Studio** (`oxide-studio`), the native Rust electronic music workstation and dual-target Model Context Protocol (MCP) server adhering to the **Oxide Studio Workstation Architecture & DSP Specification** with open Ableton Live (.als) project format interop.14.0`).

## Core Audio & DSP Terms

- **Mystran ZDF Ladder Filter (`ZdfLadderFilter`)**: 2x-oversampled zero-delay feedback 4-pole transistor ladder filter (`code[817]` from `rnbomatic.wasm`) using a rational `[4/4]` Padé `tanh(x)/x` saturator (`(x^4 + 105x^2 + 945) / (15x^2 + 420)`) for self-oscillation stability without Newton-Raphson iteration.
- **Cytomic SVF (`CytomicSvf`)**: Andrew Simper's zero-delay feedback trapezoidal state-variable filter (`ableton::dsp::entity::DFM1_ZD_24dB`) providing simultaneous low-pass, band-pass, high-pass, notch, peak, and bell responses.
- **Lock-Free Command Ring (`EngineCommand`)**: Bounded SPSC/MPSC ring buffer transporting UI and MCP parameter mutations, note events, and clip triggers to the real-time `cpal` CoreAudio callback with zero heap allocations on the audio thread.
- **Telemetry Frame (`TelemetrySnapshot`)**: Lock-free snapshot emitted by the audio engine back to the UI and MCP server containing peak/RMS stereo levels (dBFS), 64-bin FFT spectrum, 256-sample oscilloscope ring, transport bar/beat/sixteenth position, and CPU load percentage.

## Synthesis & Instruments (`InstrumentRack`)

- **Playground Synth (`SynthEngine`)**: 8-voice polyphonic synthesizer implementing the exact 23 RNBO parameters (`osc1_*`, `osc2_*`, `sub_*`, `noise_*`, `filter_*`, `amp_*`, `mod_env_*`, `lfo_*`, `glide`, `unison_voices`, `unison_detune`), 38 factory presets, and 17 XY macro pairs from Ableton Learning Synths.
- **Wavetable (`WavetableSynth`)**: Dual-oscillator wavetable synthesizer with 2D frame morphing (`0.0..=1.0`), Warp modes (`None`, `Sync`, `Bend`, `PWM`, `Asym`, `FM`), 3 envelopes (`Amp`, `Env2`, `Env3`), 2 LFOs, and an assignable Modulation Matrix.
- **Operator (`OperatorSynth`)**: 4-operator FM/additive synthesizer (`Op A`, `Op B`, `Op C`, `Op D`) with 11 routing algorithms (`Alg 1..11`), per-operator coarse/fine ratios, feedback, and ADSR envelopes.
- **Drift (`DriftSynth`)**: MPE-capable subtractive synthesizer featuring per-voice analog oscillator phase/pitch drift (`Drift` parameter), shape morphing, and dual-slope cycling envelope.
- **Meld (`MeldSynth`)**: Bi-timbral macro-oscillator synthesizer with two independent engines (`Engine A`, `Engine B`), 12 macro-oscillator algorithms (`Swarm`, `Harmonic`, `Sub`, `NoiseLoop`, `FM`, `Fold`, `Chip`, `Bitcrush`, `Formant`, `Chord`, `Resonator`, `Shepard`), and scale-quantized resonators.
- **Physical Modeling Suite (`CollisionSynth`, `TensionSynth`, `ElectricSynth`)**: Mallet/membrane modal resonator synthesizer (`Collision`), waveguide string synthesizer (`Tension`), and Rhodes/Wurlitzer tine/reed electro-mechanical piano (`Electric`).
- **Simpler (`SimplerSampler`)**: Sample-based instrument operating in three playback modes: `Classic` (polyphonic pitch-transposed loop), `One-Shot` (trigger-to-completion with fade envelope), and `Slicing` (transient/beat-division slice mapping across MIDI notes `C1..`).
- **Drum Rack (`DrumRack128`)**: 128-pad (`MIDI 0..127`, viewed in 16-pad visible banks) percussion container supporting synthesized 808/909/FM/Karplus drums, `Simpler` sample cells, choke groups (`1..16`), and per-pad send levels.

## Audio Effects & Device Chain (`DeviceChain`)

- **Device Chain (`DeviceChain`)**: Ordered per-track signal-processing pipeline holding an instrument/source followed by an arbitrary sequence of serial or parallel `AudioEffectDevice` instances (`EQ Eight`, `Glue Compressor`, `Roar`, `Auto Filter`, `OTT`, `Hybrid Reverb`, `Spectral Resonator`, `Echo`, `Beat Repeat`, `Corpus`, `Redux`, `Utility`, `Limiter`).
- **Roar (`RoarSaturator`)**: Live 12 multi-stage saturator/waveshaper (`Single`, `Serial`, `Parallel`, `Mid/Side`, `Multiband`) with shaper curves (`Soft Sine`, `Tube`, `Tape`, `Diode`, `Bitcrush`, `Polynomial`) and feedback routing.
- **Glue Compressor (`GlueCompressor`)**: Cytomic SSL 4000 G-Bus master compressor with selectable attack, release (`0.1s..1.2s` + `Auto`), ratio (`2:1`, `4:1`, `10:1`), soft-clip toggle, and sidechain high-pass filter.

## Timeline, Warping & Groove (`ProjectState`)

- **Session Matrix (`SessionGrid`)**: Track-by-Scene 2D grid of `ClipSlot` cells supporting quantized launch (`1 Bar`, `1/2`, `1/4`, `1/8`, `1/16`, `None`), follow actions, and Scene launch.
- **Arrangement Timeline (`ArrangementView`)**: Linear bar-based timeline positioning `ArrangementClip` regions and multi-breakpoint `AutomationLane` curves (`Linear`, `Exponential`, `Logarithmic`, `S-Curve`, `Step`).
- **Warp Engine (`WarpMode`)**: Time-stretching and pitch-shifting processor supporting Ableton's 6 warp algorithms (`Beats`, `Tones`, `Texture`, `Re-Pitch`, `Complex`, `Complex Pro` with formant envelope preservation) anchored by `WarpMarker` pairs (`sample_frame <-> beat_time`).
- **MIDI Transformation & Generation (`MidiTool`)**: Live 12 algorithmic MIDI processors (`Arpeggiate`, `Span`, `Connect`, `Recombine`, `Strum`, `Time Warp`, `Velocity Shaper`, `Stacks`, `Seed`, `Rhythm`, `Shape`) constrained by active `ScaleLock`.
- **Groove Pool (`GroovePool`)**: Collection of micro-timing and velocity templates (`MPC 16th Swing 54..75%`, `SP-1200`, `Latin Percussion`, `909 Shuffle`) applied non-destructively to clips via `Timing %`, `Random %`, `Velocity %`, and `Quantize %`.

## Dual-Target MCP & LOM Bridge

- **Oxide MCP Server (`oxide-mcp`)**: JSON-RPC 2.0 `stdio` Model Context Protocol server (`oxide-studio --mcp`) exposing full programmatic control over synthesis, device chains, MIDI/audio clips, mixing, arrangement, offline WAV bounce, spectral audio analysis, and Live 12 LOM bridging.
- **Live Object Model Bridge (`AbletonMCP`)**: Python 3.11 MIDI Remote Script (`~/Music/Ableton/User Library/Remote Scripts/AbletonMCP/`) running inside official **Ableton Live 12.4.6 Suite** (`CPython 3.11.6`, Boost.Python `Live.*` API) that exposes a localhost JSON-RPC TCP socket (`127.0.0.1:9877`) bridged into `oxide-mcp` under the `lom_*` tool namespace.

## Top Community-Requested Beyond-Stock Electronic Features

- **Bounce-in-Place (`BounceInPlace` / `Cmd+B`)**: Non-destructive 1-click in-place audio rendering mechanism (`oxide-bounce`) replacing stock Ableton's destructive "Freeze & Flatten". Renders selected clips or Group tracks into audio on the main track while preserving underlying MIDI notes, synth patches, and automation in an inactive, zero-CPU source take lane with 1-click revert capability.
- **DuckShaper (`DuckShaper` / `Pump / LFO Shaper`)**: Native sample-accurate volume and filter ducking envelope processor (`oxide-fx`). Replaces sluggish compressor sidechain pumps with a 2048-point bezier curve evaluated per sample without detector lag. Supports tempo-sync divisions ($1/4, 1/8, 1/16$, dotted/triplet), ghost MIDI re-triggers, audio transient triggers, and a 4th-order Linkwitz-Riley $120\text{ Hz}$ crossover for transparent sub-only ducking.
- **Club Clipper (`MasterClipper` / `Club Clipper`)**: High-performance hard/soft clipping processor with selectable $2\times, 4\times, 8\times$ linear-phase half-band FIR polyphase oversampling to prevent high-frequency aliasing foldback. Features continuous polynomial knee adjustment ($K \in [0.0, 1.0]$), input drive, ceiling limiter, and real-time shave-meter showing instantaneous crest factor (Peak-to-RMS dB).
- **PhaseScope (`PhaseScope` / `Kick + Sub-Bass Phase Scope`)**: Real-time dual circular ring buffer oscilloscope and correlation visualizer (`oxide-core` / `oxide-gui`). Superimposes Kick (neon pink `#ff007f`) and Sub-Bass (cyan `#00d2ff`) waveforms in the time domain, computing continuous Pearson phase correlation $\rho \in [-1.0, +1.0]$, and alerts producers to destructive low-end acoustic phase cancellation with on-device phase rotation ($0^\circ..360^\circ$) and micro-delay alignment ($\pm 25\text{ ms}$).
- **ClubDeck (`ClubDeck` / `Club Readiness & Loudness Analyzer`)**: Master telemetry and reference suite integrating ITU-R BS.1770 / EBU R128 integrated and short-term LUFS metering (with -8 LUFS club and -14 LUFS streaming target markers), $4\times$ oversampled true-peak inter-sample detection, sub-bass mono compatibility audition ($<90\text{ Hz}$), sum-to-mono phase cancellation check, and a 1-click A/B reference track player with automatic loudness matching.

