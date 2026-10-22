# Oxide Studio (`oxide-studio`)

[![CI](https://github.com/juancruzmunozalbelo/oxide-studio/actions/workflows/release.yml/badge.svg)](https://github.com/juancruzmunozalbelo/oxide-studio/actions)
[![GitHub Pages](https://img.shields.io/badge/GitHub%20Pages-Live%20Workstation-00f0ff?logo=githubpages&logoColor=black)](https://juancruzmunozalbelo.github.io/oxide-studio/web/)
[![Release](https://img.shields.io/github/v/release/juancruzmunozalbelo/oxide-studio?color=ffbe00&label=release)](https://github.com/juancruzmunozalbelo/oxide-studio/releases/tag/v1.0.0)
[![License](https://img.shields.io/badge/license-MIT%20%7C%20Apache--2.0-blue.svg)](LICENSE-MIT)
[![Tests](https://img.shields.io/badge/tests-674%20passing-00f758.svg)](https://github.com/juancruzmunozalbelo/oxide-studio/actions)
[![Layout Guard](https://img.shields.io/badge/layout%20guard-576%20frames%20clean-954eb2.svg)](crates/ableton-gui/tests/adversarial_acoustic_layout_stress_test.rs)

> **Native Rust Electronic Music DAW, 100% Commercial Ableton Live 12.4.6 Suite Workstation & 76-Tool Dual-Target Model Context Protocol (MCP) Server.**  
> *Repository:* [https://github.com/juancruzmunozalbelo/oxide-studio](https://github.com/juancruzmunozalbelo/oxide-studio)  
> *Interactive Web Workstation:* [https://juancruzmunozalbelo.github.io/oxide-studio/web/](https://juancruzmunozalbelo.github.io/oxide-studio/web/)  
> *Showcase & Download Hub:* [https://juancruzmunozalbelo.github.io/oxide-studio/](https://juancruzmunozalbelo.github.io/oxide-studio/)  
> *Dual License:* [MIT](LICENSE-MIT) OR [Apache-2.0](LICENSE-APACHE) | [Legal Notice](NOTICE.md)

---

## 🔒 Clean-Room Engineering & Trademark Disclaimer

> **IMPORTANT LEGAL & CLEAN-ROOM DISCLOSURE**:
> - **100% Clean-Room Implementation**: Oxide Studio is an independent, clean-room native Rust digital audio workstation (DAW) and audio engine developed entirely from mathematical, algorithmic, and digital signal processing (DSP) first principles (including Andrew Simper's Cytomic ZDF linear state-variable filters, Mystran's Padé $[4/4]$ 4-pole transistor ladder filter, Robert Bristow-Johnson's biquad cookbook, PolyBLEP band-limited oscillators, and granular SOLA / phase-vocoder time-stretching).
> - **Zero Proprietary Assets**: Oxide Studio contains **zero proprietary source code, decompiled C++ assemblies, SDKs, binary assets, copyrighted presets, or intellectual property from Ableton AG or Cycling '74**.
> - **Trademark Non-Affiliation**: "Ableton", "Ableton Live", "Live Suite", "Push", "Operator", "Simpler", "Wavetable", and "Max for Live" are registered trademarks or service marks of **Ableton AG** in Germany, the United States, and other countries. "RNBO" and "Max" are registered trademarks of **Cycling '74**. Oxide Studio is an independent open-source project and is **NOT affiliated with, sponsored by, endorsed by, or connected with Ableton AG or Cycling '74**.
> - **Nominative Fair Use**: All references to Ableton marks and products in this repository, documentation, or code comments are strictly for **nominative, comparative, interoperability, and architectural educational purposes** under applicable fair use doctrine to accurately describe workflow compatibility, format interchange, and structural architecture.
> - **Open Source Licensing**: Oxide Studio is dual-licensed under [MIT](LICENSE-MIT) OR [Apache-2.0](LICENSE-APACHE). For full provenance and citations, see [NOTICE.md](NOTICE.md).

---

## 1. Vision & Reverse-Engineering Provenance

**Oxide Studio** is a production-grade, real-time digital audio workstation (DAW) written in modern, memory-safe Rust. It is architected from a clean-room **Reverse-Engineer-Anything (REA)** structural and algorithmic audit of two foundational audio systems:

1. **Ableton Learning Synths Playground** (`rnbomatic.wasm`, Cycling '74 Max RNBO `0.14.0` runtime, WebAssembly `code[817]`, Loop #84): Complete extraction and mathematical reconstruction of the 8-voice polyphonic synthesizer engine, 23 normalized RNBO parameter curves, 38 factory sound presets, 17 XY macro breakpoint modulations, and Mystran's 2x-oversampled zero-delay feedback (ZDF) 4-pole transistor ladder filter featuring a rational $[4/4]$ Padé approximation for $\tanh(x)/x$.
2. **Ableton Live 12.4.6 Suite** (`OEngine`, `ableton::dsp`, `ableton::devices`, and the Live Object Model): Systematic reconstruction of Live 12's single-window modular workstation UX, dynamic per-track device chains, 10 flagship instruments, 6-mode audio warping engine (`OWarper`), global groove pool with `.agr` timing extraction, multi-lane comping with automatic micro-crossfades, generative and transformative MIDI algorithms, microtonal tuning systems, and a bidirectional dual-target Model Context Protocol (MCP) server.

Oxide Studio operates in two distinct execution modes:
- **Autonomous Native DAW**: In-process real-time CoreAudio streaming (`cpal`) or headless, deterministic 24-bit WAV rendering with lock-free parameter ring buffers and 60 FPS Metal GPU UI.
- **Live Studio Copilot & Autonomous Assistant**: A 76-tool Model Context Protocol server exposing comprehensive composition, mixing, synthesis, and analysis capabilities across both the internal native Rust engine and running workstation sessions via an internal JSON-RPC 2.0 TCP bridge (`127.0.0.1:9877`).

---

## 🌐 In-Browser Studio Playground (Zero Installation)

Experience Oxide Studio directly in your web browser with zero installation:

### [▶ Launch Full Oxide Studio Web Workstation](https://juancruzmunozalbelo.github.io/oxide-studio/web/)

- **Dual-View Workspace**: Switch between Session Matrix and Arrangement Timeline via `Tab`.
- **16-Voice TR-909 Sequencer**: Full Bank A & B drum synthesis with 16-step grid, dynamic velocity shading, and groove shuffle.
- **Polyphonic Suite Synthesizer**: Real-time `Drift`, `Meld`, `Operator`, and `Wavetable` engine modes with ADSR envelopes.
- **Audio FX Rack**: Glue Compressor, OTT Multiband Dynamics, Roar waveshaper, and 60 FPS HTML5 Canvas visualizers.
- **Interactive Mixer**: Channel strips with post-fader stereo VU meters, Pan, Mute/Solo, and Aux Sends A/B.
- **Bilingual Copilot & Export**: Ask questions in English or Argentine Spanish (`OKF-01`..`OKF-10`) and export your track directly to WAV.

---

## 2. Workspace Architecture

Oxide Studio is structured as a modular workspace containing 11 specialized Rust crates alongside the native Live GUI RPC bridge:

```mermaid
flowchart TD
    subgraph Host["Host Environment & AI Agents"]
        Agent["Autonomous Agent / LLM Client<br/>(Claude Desktop / Antigravity / Cursor)"]
        User["Human Musician / Sound Designer"]
    end

    subgraph MCP["MCP Control Layer"]
        StdioMCP["crates/ableton-mcp<br/>(76 Tools, JSON-RPC 2.0 stdio)"]
    end

    subgraph NativeEngine["Native Rust DAW Engine"]
        Core["crates/ableton-core<br/>(Buffers, SPSC Command Ring, Telemetry, Scales, Tunings)"]
        DSP["crates/ableton-dsp<br/>(Playground, Wavetable, Operator, Drift, Meld, Physical Suite)"]
        Drums["crates/ableton-drums<br/>(Simpler Sampler, DrumRack128, 808/909 Physical Synth)"]
        FX["crates/ableton-fx<br/>(DeviceChain, EQ Eight, Roar ADAA, Glue, Auto Filter ZDF, OTT)"]
        Timeline["crates/ableton-timeline<br/>(Session Matrix, Timeline, 6-Mode OWarper, Comping, Grooves)"]
        Audio["crates/ableton-audio<br/>(Real-Time cpal CoreAudio Engine, Zero-Alloc Callbacks)"]
        Bounce["crates/ableton-bounce<br/>(Offline 24-bit WAV Bounce, FFT Spectral Analyzer, Remix Stems)"]
        Scanner["crates/ableton-plugin-scanner<br/>(Out-of-Process macOS AUv2 / VST3 / CLAP Supervisor)"]
        Project["crates/ableton-project<br/>(.oxide.json / .als Ableton Live Set I/O)"]
        GUI["crates/ableton-gui<br/>(5-Zone 60 FPS eframe/egui Studio UI on Metal & AI Copilot)"]
    end

    subgraph LiveRpcBridge["Native Live GUI RPC Bridge"]
        LOMSocket["TCP JSON-RPC 2.0 Server<br/>127.0.0.1:9877"]
        LiveGui["crates/ableton-gui/src/live_rpc.rs<br/>(Native Live GUI RPC Server)"]
    end

    Agent <-->|stdio JSON-RPC| StdioMCP
    User <-->|GUI / Metal| GUI

    StdioMCP <-->|In-Process Dispatch| Core
    StdioMCP <-->|TCP JSON-RPC 2.0 (127.0.0.1:9877)| LOMSocket
    LOMSocket <--> LiveGui
    LiveGui <--> GUI

    GUI <-->|Lock-Free Command Ring / SPSC Telemetry| Audio
    Audio <--> Core
    Audio --> DSP
    Audio --> Drums
    Audio --> FX
    Audio --> Timeline
    Audio --> Scanner

    Bounce --> DSP
    Bounce --> Drums
    Bounce --> FX
    Bounce --> Timeline
```

### Workspace Crates Summary

- `crates/ableton-core`: Lock-free single-producer single-consumer (SPSC) command rings, real-time `AudioTelemetry` snapshots (RMS, peak dBFS, 64-bin FFT, 256-sample oscilloscope), `AudioBuffer` fixed-size stereo audio structures, 14 `ScaleMode` definitions, and 4 microtonal `TuningSystem` models.
- `crates/ableton-dsp`: Polyphonic synthesis engines (Learning Synths Playground, Wavetable, Operator FM, Drift, Meld bi-timbral, Analog, Collision, Tension, Electric), Mystran 2x-oversampled ZDF ladder filter with Padé saturator, and Cytomic state-variable filters.
- `crates/ableton-drums`: Physical modeling percussion (808 kick/snare, 909 open/closed hi-hats, FM cowbell/rimshot, Karplus-Strong claps), 3-mode `SimplerSampler`, and 128-pad `DrumRack128` with choke groups and return sends.
- `crates/ableton-fx`: Dynamic serial/parallel `DeviceChain` pipeline, `EQ Eight` with real-time Bode response, `Roar` multi-stage saturator with 1st-order ADAA antialiasing, `Glue Compressor`, Cytomic `Auto Filter` with 5 circuit drive models, `OTT` multiband dynamics, `Hybrid Reverb`, `Spectral Resonator`, and standard Live 12 MIDI effects.
- `crates/ableton-timeline`: Dual-paradigm Session Matrix and Arrangement Timeline, 6-mode `OWarper` audio warping engine, multi-lane comping with equal-power micro-crossfades, `.agr` Groove Pool, and Live 12 algorithmic MIDI Generators and Transformers.
- `crates/ableton-audio`: Zero-allocation CoreAudio integration via `cpal`, running bounded real-time audio threads with priority scheduling and sub-millisecond telemetry feedback.
- `crates/ableton-bounce`: Deterministic headless offline 24-bit WAV bounce renderer and 8D spectral audio analysis engine.
- `crates/ableton-plugin-scanner`: Out-of-process isolation worker and supervisor scanning AUv2, VST3, and CLAP plugins without crashing the workstation host.
- `crates/ableton-project`: JSON `.oxide.json` and Ableton Live Gzip XML `.als` project parser, stem exporter, and cosine similarity browser index.
- `crates/ableton-gui`: Single-window 60 FPS workstation interface rendered natively on macOS Metal using `egui` and `eframe`, themed in Live 12 Dark Carbon (`#07080a` / `#111318` / `#ffbe00` / `#00d2ff`).
- `crates/ableton-mcp`: Dual-target Model Context Protocol server exposing 76 JSON-RPC 2.0 tools over `stdio`.

---

## 3. Flagship Instruments (`ableton-dsp` & `ableton-drums`)

Oxide Studio implements 10 full-featured instrument devices directly inside the native Rust DSP layer:

### 1. Learning Synths Playground Synth (`SynthEngine`)
- **Origin**: Exact clean-room decompilation of Cycling '74 Max RNBO `0.14.0` (`rnbomatic.wasm`).
- **Polyphony**: 8-voice polyphonic architecture with dynamic voice allocation and voice-stealing algorithms.
- **Architecture**: Dual multi-shape oscillators with variable pulse width, saw detuning (coarse $\pm 48$ semitones, fine $\pm 100$ cents), sub-oscillator, and analog white noise generator.
- **Parameters**: 23 canonical parameters across pitch, filter, amplitude ADSR, modulation ADSR, and multi-shape LFO.
- **Presets & Macros**: 38 factory presets and 17 performance XY macro pairs with non-linear piece-wise breakpoint interpolation.
- **Filter DSP**: Mystran 2x-oversampled zero-delay feedback (ZDF) 4-pole low-pass transistor ladder filter. It uses a rational $[4/4]$ Padé approximation for $\tanh(x)/x$ to guarantee unconditional filter stability at self-oscillation without expensive iterative Newton-Raphson solvers:

$$\frac{\tanh(x)}{x} \approx \frac{a^2 + 105a + 945}{15a^2 + 420a + 945}, \quad \text{where } a = x^2$$

$$\omega = \frac{\pi \cdot f_c}{2 \cdot f_s}, \quad f(\omega) = \frac{\omega \cdot (0.99999949 - 0.096524608 \omega^2)}{1.0 - 0.42986726 \omega^2 + 0.009981878 \omega^4}$$

### 2. Wavetable (`WavetableDevice`)
- Dual-oscillator wavetable synthesizer with continuous 2D wavetable position frame morphing ($0.0 \dots 1.0$).
- Integrated oscillator warp algorithms: `None`, `Sync`, `Bend`, `PWM`, `Asym`, and `FM`.
- Stereo unison engine: `Classic`, `Shimmer`, and `Noise` unison modes with up to 8 detuned voice clusters.
- 14-target modulation matrix coupling 2 multi-stage ADSR envelopes, 2 tempo-synced LFOs, and MIDI velocity/aftertouch.

### 3. Operator (`OperatorDevice`)
- 4-operator FM and additive synthesis workstation (Operators A, B, C, D).
- 11 topological FM routing algorithms ranging from pure 4-op cascade FM to full 4-op parallel additive synthesis.
- Per-operator frequency multiplier modes (`Coarse` $0.5 \dots 32.0\times$, `Fine` $\pm 1000$), variable feedback, and multi-segment loopable envelopes.

### 4. Drift (`DriftDevice`)
- Subtractive virtual analog synthesizer featuring simulated analog thermal pitch and phase instability (`Drift` parameter).
- Shape-morphing main oscillator transitioning continuously through Sine, Triangle, Saw, and Pulse waveforms.
- Polyphonic MIDI Polyphonic Expression (MPE) support with per-voice pitch bend, pressure, and slide tracking.

### 5. Meld (`MeldDevice`)
- Bi-timbral macro-oscillator synthesizer featuring dual independent sound engines (Engine A & Engine B).
- 12 algorithmic oscillator engines: `Swarm`, `Harmonic`, `Sub`, `NoiseLoop`, `FM`, `Fold`, `Chip`, `Bitcrush`, `Formant`, `Chord`, `Resonator`, and `Shepard Tone`.
- Scale-aware modal plate resonators quantized directly to the active project scale.

### 6. Analog (`AnalogDevice`)
- Dual virtual analog oscillator architecture with variable sub-oscillator and sync capabilities.
- Dual multi-mode resonant filters routable in serial or parallel with selectable saturation stages.

### 7. Physical Modeling Suite
- **Collision (`CollisionDevice`)**: Mallet physical model with membrane and plate modal resonators for acoustic percussion, bells, and marimbas.
- **Tension (`TensionDevice`)**: Waveguide physical modeling string synthesizer with bow, pick, and hammer exciters and acoustic soundboard resonances.
- **Electric (`ElectricDevice`)**: Physical modeling of classic electro-mechanical pianos (Rhodes and Wurlitzer) simulating tine elasticity, hammer strike hardness, and electromagnetic pickup geometries.

### 8. Simpler (`SimplerSampler`)
- Dedicated sample playback instrument supporting 3 operational modes:
  - `Classic`: Polyphonic sample playback with loop points, crossfades, and pitch-tracking envelopes.
  - `1-Shot`: Monophonic trigger-to-completion playback with attack and release gate smoothing.
  - `Slicing`: Transient-based and beat-grid slice detection, mapping slices across chromatic MIDI keys starting at $C_1$.

### 9. Drum Rack 128 (`DrumRack128`)
- 128-pad percussion workstation arranged into 8 banks of 16 pads (`MIDI 0..127`).
- Hybrid sound generation per pad: synthesized 808/909 physical models or instantiated `Simpler` sample cells.
- 16 polyphonic choke groups for authentic open/closed hi-hat cutoffs and dedicated per-pad Return A/B send buses.

---

## 4. Device Chain & Audio/MIDI FX (`ableton-fx`)

Every track in Oxide Studio utilizes a dynamic, arbitrary-length `DeviceChain` processing signals in strict logical order:

$$\text{MIDI In} \longrightarrow [\text{MIDI Effects}]^* \longrightarrow [\text{Instrument}]^? \longrightarrow [\text{Audio Effects}]^* \longrightarrow \text{Audio Out}$$

### Flagship Audio Processors
- **Roar (`RoarSaturator`)**: Live 12 multi-stage coloration processor featuring 6 routing topologies (`Single`, `Serial`, `Parallel`, `Mid/Side`, `Multiband`, `Feedback`), 10 non-linear shaper curves (`Soft Sine`, `Digital Clip`, `Bitcrusher`, `Polynomial`, `Tube`, `Half-Wave`, `Diode`, `Hard Clip`, `Sine Wavefold`, `Noise Shaper`), and 1st-order Antiderivative Antialiasing (ADAA) to eradicate digital aliasing:

$$y(n) \approx \frac{F_1(x(n)) - F_1(x(n-1))}{x(n) - x(n-1)}, \quad \text{where } F_1(x) = \int f(x)\,dx$$

- **EQ Eight (`Eq8Device`)**: 8-band parametric equalizer supporting 48 dB/oct high-pass, low-shelf, 4 parametric bells, high-shelf, and low-pass filters with interactive Bode curve plotting and 64-bin FFT spectrum visualization.
- **Glue Compressor (`GlueCompressor`)**: Cytomic SSL 4000 G-Bus master compressor with discrete attack times, release settings ($0.1\text{s} \dots 1.2\text{s}$ + `Auto`), ratios ($2:1, 4:1, 10:1$), soft-clip transistor toggle, and sidechain high-pass filtering.
- **Auto Filter (`AutoFilterDevice`)**: Cytomic zero-delay feedback (ZDF) state-variable filter (SVF) featuring 5 analog circuit drive simulations: `Clean`, `OSR`, `MS2`, `SMP`, and `PRD`.
- **OTT Multiband Dynamics (`MultibandDynamicsDevice`)**: 3-band upward and downward dynamics processor splitting audio into Low, Mid, and High frequency bands with variable crossover points.
- **Hybrid Reverb (`HybridReverbDevice`)**: Dual-stage reverberator combining low-latency finite impulse response (FIR) convolution with algorithmic late-diffusion decay.
- **Spectral Resonator (`SpectralResonatorDevice`)**: FFT-based spectral bank tuning audio partials to incoming MIDI notes or harmonic pitch intervals.
- **Creative Suite**: `Auto Shift` (pitch correction & formant shifting), `Echo` (dual tape/digital delay), `Beat Repeat` (quantized buffer glitching), `Corpus` (acoustic membrane resonator), `Redux` (bit-depth and sample-rate decimation), `Erosion` (noise modulation), and `Limiter` (brickwall peak limiter with lookahead).

### Live 12 MIDI Effects
- `Arpeggiator`: Polyphonic note arpeggiation with 16 rhythmic patterns, octave transposition, and gate length.
- `Chord`: Instant 6-note chord voicing generation from single key strikes.
- `Scale`: Real-time note quantization constraining incoming pitches to any project scale.
- `Velocity`: Velocity curve compression, expansion, and random humanization.
- `Note Length`: Fixed and tempo-synced gate duration enforcement.

### Plugin Hosting (`HostedPluginDevice`)
- Out-of-process isolation architecture hosting macOS `AUv2`, `VST3`, and `CLAP` instruments and audio effects via `ableton-plugin-scanner`. Plugin crashes never compromise the main DAW process.

---

## 5. Timeline, Warping, Comping & Generative MIDI (`ableton-timeline`)

### 6-Mode Audio Warping Engine (`OWarper`)
Oxide Studio implements all 6 Ableton Live warping algorithms:
1. `Beats`: Transient slice detection with envelope decay control and 3 slice loop modes (`Off`, `Forward`, `Alternating`).
2. `Tones`: Granular Synchronized Overlap-Add (SOLA) algorithm optimized for monophonic instruments and vocal leads.
3. `Texture`: Granular time-stretching with adjustable grain size ($2 \dots 260\text{ ms}$) and stochastic grain flux modulation.
4. `RePitch`: Classical vari-speed resampling where playback speed and musical pitch remain strictly proportional.
5. `Complex`: Polyphonic phase-vocoder time-stretching with phase-locking across spectrum bins.
6. `ComplexPro`: Formant-preserving phase-vocoder with independent spectral envelope control for vocal polyphony.

### Multi-Lane Comping & Micro-Crossfades
- Multi-take recording lane architecture enabling seamless comp selection across multiple takes on Arrangement tracks.
- Automatic equal-power $4 \dots 10\text{ ms}$ micro-crossfades generated at slice splice boundaries to prevent DC pops:

$$\text{Gain}_{\text{in}}(n) = \sin\left(\frac{\pi n}{2L}\right), \quad \text{Gain}_{\text{out}}(n) = \cos\left(\frac{\pi n}{2L}\right)$$

### Global Groove Pool (`GroovePool`)
- Non-destructive rhythmic swing and velocity dynamics derived from classic groove templates:
  - `MPC 16th Swing (54% .. 75%)`
  - `Roland TR-909 Shuffle`
  - `E-mu SP-1200 Funk`
  - `Latin Triplet`
- Adjustable parameters: `Base` ($1/8, 1/16, 1/32$), `Quantize %`, `Timing %`, `Random %`, `Velocity %`, and destructive "Commit Groove" baking.

### Live 12 Generative & Transformative MIDI Tools
- **Generators**:
  - `Seed`: Probabilistic note, pitch, velocity, and duration cloud generation.
  - `Shape`: Pitch contour sculpting (`Linear Rise/Fall`, `Triangle`, `Arch`, `Dip`, `Wave`).
  - `Stacks`: Algorithmic chord stacking, inversions, and drop-2/drop-4 voicings.
  - `Rhythm`: Syncopated density pattern generator.
  - `Euclidean`: Bjorklund distribution algorithm.
- **Transformers**:
  - `Arpeggiate`, `Connect` (scale-locked passing tones), `Ornament` (flams and grace notes), `Quantize`, `Recombine` (parameter permutation), `Span` (staccato/legato shaping), `Strum` (chord micro-delay), and `Time Warp` (non-linear accelerando/ritardando curves).

### Global Scale & Microtonal Tuning
- **14 Global Scales**: Major, Minor, Dorian, Phrygian, Lydian, Mixolydian, Aeolian, Locrian, Harmonic Minor, Melodic Minor, Pentatonic, Blues, Diminished, and Whole Tone.
- **4 Microtonal Tuning Systems**: Standard `12-TET`, `Just Intonation (5-limit)`, `24-EDO (Quarter-Tone)`, and custom Scala `.scl` file import.

---

## 6. Unified 5-Zone 60 FPS Workstation GUI (`ableton-gui`)

Oxide Studio provides a native single-window workstation interface rendered directly on macOS Metal at 60 FPS without layout jitter:

```
+----------------------------------------------------------------------------------------------------------------------+
| [1] TOP TRANSPORT & SCALE BAR (BPM, Time Sig, Metronome, Scale, Tuning, Transport Controls, Oscilloscope & FFT)       |
+--------------------------+---------------------------------------------------------------+---------------------------+
| [2] LEFT BROWSER PANEL   | [3] CENTER WORKSPACE (Toggle with 'Tab')                      | [4] RIGHT MODULAR MIXER   |
| • Sounds / Drums / Inst  |                                                               | • Track Volume / Faders   |
| • Audio / MIDI Effects   |   A. SESSION MATRIX (Track-by-Scene Clip Launching)           | • Pan / Solo / Mute / Arm |
| • Live 12 Tag Filter     |                          - OR -                               | • Return A / Return B     |
| • 8D Spectral Similarity |   B. ARRANGEMENT TIMELINE (Linear Tracks, Automation, Comping)| • DJ Crossfader (A/B/Off) |
+--------------------------+---------------------------------------------------------------+---------------------------+
| [5] BOTTOM DETAIL DECK (Toggle with 'Shift+Tab')                                                                     |
| • Clip Detail View: Piano Roll (Scale Fold, Velocity, Chance, Live 12 Transform/Generate) & Audio Warping Display   |
| • Device Chain View: 9 Custom Vector Visualizers (Wavetable, Operator, Roar, EQ Eight, Glue, Auto Filter, etc.)    |
+----------------------------------------------------------------------------------------------------------------------+
```

### 9 Custom Interactive Vector Visualizers
1. **EQ Eight Visualizer**: Real-time 8-node Bode magnitude curve overlaying a 64-bin FFT spectrum analyzer.
2. **Roar Saturation Curve**: Interactive transfer function visualizer with dynamic waveshaper curve updates.
3. **Wavetable Visualizer**: 3D waterfall and 2D frame display showing real-time wavetable position and morphing.
4. **Operator FM Algorithm Diagram**: Interactive block diagram showing operator routing topologies and modulation depth.
5. **Auto Filter ZDF Curve**: Cutoff, resonance, and circuit saturation curve visualization.
6. **Glue Compressor VU Meter**: Physical needle ballistics showing gain reduction in dB.
7. **OTT Dynamics Display**: 3-band input, upward/downward compression threshold, and output energy bars.
8. **Simpler Waveform Visualizer**: Waveform slice points, loop markers, and transient thresholds.
9. **Drum Rack 16-Pad Bank**: Active velocity-sensitive pad triggers and choke grouping indicators.

---

## 7. 76-Tool Dual-Target MCP Server (`ableton-mcp`)

The Model Context Protocol (MCP) server enables autonomous AI agents to inspect, control, compose, synthesize, and analyze music programmatically. It operates over standard `stdio` JSON-RPC 2.0.

### Tool Catalogue by Functional Domain

#### Domain 1: Project & Transport (8 Tools)
- `project_new`: Initialize a new project with default lead synthesizer and 909 drum kit.
- `project_load`: Load complete project from a JSON state file.
- `project_save`: Save complete project state to disk.
- `transport_play`: Start sequencer playback.
- `transport_stop`: Stop playback and reset playhead.
- `transport_set_bpm`: Set project tempo ($20.0 \dots 999.0\text{ BPM}$).
- `transport_set_swing`: Adjust 16th-note swing ratio ($0.0 \dots 0.75$).
- `transport_get_telemetry`: Query real-time audio telemetry (RMS, peak dBFS, playhead beat, CPU load).

#### Domain 2: Tracks, Synthesizer & Drums (9 Tools)
- `track_create_synth`: Add a new polyphonic synthesizer track.
- `track_create_drums`: Add a new 128-pad drum rack track.
- `track_delete`: Remove a track by index.
- `synth_list_presets`: Enumerate the 38 factory Learning Synths presets.
- `synth_load_preset`: Apply a factory preset to a target synth track.
- `synth_set_parameter`: Mutate an individual synth parameter by name or index.
- `synth_set_macro`: Adjust performance XY macro axes ($0.0 \dots 1.0$).
- `drums_configure_pad`: Configure drum pad synthesis model or sample file.
- `drums_trigger_pad`: Fire a drum pad hit with specified velocity.

#### Domain 3: Session Matrix & Timeline (8 Tools)
- `clip_create_midi`: Create a MIDI clip with programmatic note data.
- `clip_create_drums`: Create a drum pattern clip with step velocities.
- `clip_create_lilypond`: Compile standard LilyPond music notation strings directly into MIDI clips.
- `clip_launch`: Fire a clip slot with musical quantization.
- `clip_stop`: Stop playback on a track clip slot.
- `scene_launch`: Launch an entire horizontal scene row across all tracks.
- `timeline_place_clip`: Position a clip on the Arrangement timeline with start/end beat bounds.
- `timeline_add_automation`: Draw multi-breakpoint automation curves for track and device parameters.

#### Domain 4: Mixer & Audio FX (5 Tools)
- `mixer_set_channel`: Set track volume (linear or dB), pan, mute, solo, and crossfader assignment.
- `fx_set_saturator`: Configure the Roar/Saturator parameters on a track.
- `fx_set_delay`: Adjust delay time, feedback, and dry/wet ratio.
- `fx_set_reverb`: Configure room size, decay time, damping, and mix.
- `fx_set_limiter`: Set master brickwall limiter ceiling and lookahead.

#### Domain 5: Plugin Scanner & Offline Bounce (3 Tools)
- `plugin_scan`: Scan system AUv2, VST3, and CLAP plugins out-of-process.
- `bounce_wav`: Render the current arrangement to an offline 24-bit stereo WAV file.
- `render_bounce_wav`: Render arrangement with specified sample rate, bit depth, and duration.

#### Domain 6: High-Level Agentic Music Composition Generators (5 Tools)
- `generate_chord_progression_clip`: Synthesize functional diatonic chord progressions from Roman numeral degrees (e.g. `["ii7", "V7", "Imaj7"]`).
- `generate_bassline_clip`: Synthesize rhythmically driving basslines locked to a root scale.
- `generate_arpeggio_clip`: Generate multi-octave rhythmic arpeggios.
- `generate_euclidean_drum_clip`: Calculate Bjorklund Euclidean drum patterns for polyrhythmic textures.
- `generate_drum_groove_clip`: Generate complete genre-specific drum kits (House, Techno, Hip-Hop, Drum & Bass).

#### Domain 7: Arrangement & "Agent Ears" Audio Analysis (2 Tools)
- `build_song_arrangement`: Construct an entire multi-section song arrangement (Intro, Verse, Buildup, Drop, Outro).
- `analyze_rendered_audio`: Extract 8D spectral audio metrics from a rendered WAV file (peak dBFS, RMS, spectral centroid, spectral flux, zero-crossing rate, attack sharpness) to enable autonomous closed-loop mixing decisions.

#### Domain 8: Interactive Sound Design (1 Tool)
- `preview_synth_patch`: Generate a rapid 1-second audio preview of a patch and return instant spectral metrics.

#### Domain 9: Devices & Device Chain Management (5 Tools)
- `device_add_to_track`: Insert any instrument or audio effect into a track's `DeviceChain`.
- `device_remove_from_track`: Delete a device from a track chain.
- `device_reorder`: Move a device to a new index within the chain.
- `device_set_param`: Set any device parameter by ID or name.
- `device_get_chain`: Inspect all devices and parameter trees on a track.

#### Domain 10: Simpler & Drum Slicing (1 Tool)
- `simpler_slice_to_drum_rack`: Slice an audio sample in Simpler into a 16-pad Drum Rack across transient markers.

#### Domain 11: Browser & Neural Sound Similarity (2 Tools)
- `browser_search`: Search browser library items by category, tags, or name.
- `browser_find_similar_sounds`: Perform 8D cosine similarity vector search across sound libraries to find acoustically matching samples.

#### Domain 12: Live 12 MIDI Generators & Transformers (2 Tools)
- `midi_transform_clip`: Apply Live 12 transformations (`Arpeggiate`, `Connect`, `Ornament`, `Quantize`, `Recombine`, `Span`, `Strum`, `Time Warp`).
- `midi_generate_clip`: Execute Live 12 generative algorithms (`Seed`, `Shape`, `Stacks`, `Rhythm`, `Euclidean`).

#### Domain 13: Audio Clips & 6-Mode Warping (3 Tools)
- `audio_clip_create_warped`: Create an audio clip with explicit warping parameters.
- `audio_clip_set_warp_mode`: Switch clip warp mode (`Beats`, `Tones`, `Texture`, `RePitch`, `Complex`, `ComplexPro`).
- `audio_clip_add_warp_marker`: Anchor audio sample timestamps to musical beat positions.

#### Domain 14: Groove Pool & Comping Take Lanes (4 Tools)
- `groove_pool_list`: Enumerate available `.agr` groove templates.
- `groove_pool_apply_to_clip`: Apply groove swing, timing, and velocity to a MIDI or audio clip.
- `take_lane_create`: Create an alternate recording take lane on an Arrangement track.
- `take_lane_comp_region`: Promote a selected region from a take lane to the master track with micro-crossfades.

#### Domain 15: Modular Mixer, Return Sends & Global Scale (3 Tools)
- `return_track_create`: Add an auxiliary return track (e.g. Return A Reverb, Return B Delay).
- `mixer_set_send`: Set the send level from an audio/MIDI track to a return bus.
- `scale_set_global`: Set the global musical root and scale mode for the entire project.

#### Domain 16: Live GUI RPC Server Bridge (15 Tools)
*Directly controls Oxide Studio GUI & Audio Engine via `127.0.0.1:9877` TCP bridge.*
- `live12_status`: Ping the native Oxide Studio GUI RPC server and verify connection.
- `live12_get_song_info`: Query active song tempo, time signature, track counts, and playhead position.
- `live12_create_midi_track`: Insert a new MIDI track in the live session matrix.
- `live12_create_audio_track`: Insert a new Audio track in the live session matrix.
- `live12_create_return_track`: Insert a new Return track in the live mixer.
- `live12_load_browser_item`: Load an instrument or effect from the library browser onto a track.
- `live12_create_clip`: Create a clip slot region in the Session view.
- `live12_add_notes`: Write MIDI notes with micro-timing and velocity into a session clip.
- `live12_fire_clip`: Launch a clip in the live session matrix.
- `live12_fire_scene`: Launch a horizontal scene row across all tracks.
- `live12_set_device_param`: Mutate any active device parameter (e.g. Operator, Wavetable, Roar).
- `live12_get_track_devices`: Enumerate all devices and parameter ranges on a track.
- `live12_set_tempo`: Update transport tempo in BPM.
- `live12_set_scale`: Update global scale and root note.
- `live12_exec_lom_query`: Execute safe queries against the Live Object Model (`song`, `app`).

---

## 8. Beyond Stock Ableton: Purpose-Built for Electronic Producers

While official Ableton Live 12 introduced landmark updates, electronic producers across Techno, Melodic House, DnB, and RKT remain heavily reliant on expensive third-party utility plugins (Kickstart 2, ShaperBox 3, StandardCLIP, Psyscope, Vision 4X, Metric AB).

Oxide Studio directly integrates the **top 5 community-requested electronic production features** natively into the zero-allocation Rust engine:

### 1. One-Click Bounce-in-Place (`Freeze & Flatten in 1 Click` / `Cmd+B`)
- **Producer Problem in Stock Live**: Ableton's "Freeze & Flatten" is destructive—it permanently overwrites MIDI note sequences, synth patch states, and automation envelopes unless manually cloned beforehand. Furthermore, Live cannot freeze Group tracks or tracks with sidechain routings.
- **Oxide Studio Solution**: Pressing `Cmd+B` renders selected clips or time selections offline via `ableton-bounce` in-process. The original MIDI/synth chain is automatically preserved in an underlying, zero-CPU "Source Take Lane" with an interactive header button: `[Audio Bounced] -> (Click to Revert to MIDI)`. Non-destructive, instant, and supports Group stem bouncing.

### 2. Native Ducking / Sidechain Envelope Shaper (`Pump / LFO Shaper` / `DuckShaper`)
- **Producer Problem in Stock Live**: Traditional compressor sidechain routing suffers from detector lag, non-linear release tails, and clicky transient distortion under fast attack times (<1 ms). Electronic producers universally buy Cableguys ShaperBox ($99) or Kickstart 2 ($19).
- **Oxide Studio Solution**: Native `DuckShaper` in `ableton-fx` evaluates a sample-accurate 2048-point bezier ducking curve per sample on the audio thread with zero latency. Triggered by tempo sync ($1/4, 1/8, 1/16$, dotted/triplet), ghost MIDI note re-triggers, or audio transient peak detection. Features a 4th-order Linkwitz-Riley $120\text{ Hz}$ crossover to duck low-end rumble while preserving transient attack grit.

### 3. Hard/Soft Clipper with Oversampling (`Club Clipper` / `MasterClipper`)
- **Producer Problem in Stock Live**: Achieving modern club loudness ($-6$ to $-4\text{ LUFS}$) in Techno, Bass House, and DnB requires shaving transient crest factor before the master limiter. Stock Saturator lacks adjustable polynomial knees and ceiling gain staging; non-oversampled clipping generates harsh high-frequency aliasing foldback across hi-hats.
- **Oxide Studio Solution**: Dedicated `MasterClipper` with continuously adjustable polynomial knee ($K \in [0.0, 1.0]$) and $2\times, 4\times, 8\times$ linear-phase half-band FIR polyphase oversampling. A live shave-meter highlights clipped peaks in bright crimson red and displays instantaneous crest factor (Peak-to-RMS ratio in dB).

### 4. Kick + Sub-Bass Phase & Collision Scope (`PhaseScope`)
- **Producer Problem in Stock Live**: Kick fundamentals and sub-bass share the critical $30\text{ Hz} \dots 100\text{ Hz}$ band. When phase is opposed, acoustic low-end cancels out completely on club sound systems (Funktion-One, Void). Ableton provides only single-track tuners or FFT spectrum analyzers—no dual-waveform time-domain phase comparison.
- **Oxide Studio Solution**: `PhaseScope` captures 512 samples from Track A (Bass) and Sidechain Track B (Kick) via dual ring buffers. It computes real-time Pearson correlation index $\rho \in [-1.0, +1.0]$, superimposes phosphor waveforms (Kick in neon pink `#ff007f`, Bass in laser cyan `#00d2ff`), issues visual cancellation warnings (`LOW-END PHASE CANCELLATION: -X.X dB`), and provides an on-device $0^\circ..360^\circ$ Hilbert phase rotator and $\pm 25\text{ ms}$ micro-delay slider.

### 5. Club Readiness & Target LUFS / Crest Factor Analyzer (`ClubDeck`)
- **Producer Problem in Stock Live**: Live's master channel provides only basic peak and RMS meters. Measuring ITU-R BS.1770 / EBU R128 integrated LUFS, short-term drop impact, true-peak inter-sample overs, and A/B reference switching requires third-party suites like ADPTR Metric AB ($199) or iZotope Insight ($199).
- **Oxide Studio Solution**: Master `ClubDeck` provides real-time integrated and short-term (3s) LUFS with selectable target markers ($-8\text{ LUFS}$ Club, $-14\text{ LUFS}$ Streaming), $4\times$ oversampled sinc true-peak detection, a sub-bass mono audition switch ($<90\text{ Hz}$), sum-to-mono phase cancellation check, and a 1-click A/B reference track player with loudness auto-matching.

---

## 📥 Download & Release Hub (v1.0.0)

Oxide Studio v1.0.0 is available as pre-compiled standalone native release archives with cryptographically verified checksums, or directly via Cargo.

### Native Binary Release Downloads

| Platform | Architecture | Archive Package | Checksums |
| :--- | :--- | :--- | :--- |
| **macOS** (Sonoma / Ventura / Sequoia) | **Apple Silicon (arm64)** | [oxide-studio-v1.0.0-macos-arm64.tar.gz](https://github.com/juancruzmunozalbelo/oxide-studio/releases/download/v1.0.0/oxide-studio-v1.0.0-macos-arm64.tar.gz)<br/>[oxide-studio-v1.0.0-macos-arm64.zip](https://github.com/juancruzmunozalbelo/oxide-studio/releases/download/v1.0.0/oxide-studio-v1.0.0-macos-arm64.zip) | [SHA256SUMS.txt](https://github.com/juancruzmunozalbelo/oxide-studio/releases/download/v1.0.0/SHA256SUMS.txt) |
| **macOS** | **Intel (x86_64)** | [GitHub Releases v1.0.0](https://github.com/juancruzmunozalbelo/oxide-studio/releases/tag/v1.0.0) | [SHA256SUMS.txt](https://github.com/juancruzmunozalbelo/oxide-studio/releases/download/v1.0.0/SHA256SUMS.txt) |
| **Linux** | **x86_64 (PipeWire / ALSA)** | [GitHub Releases v1.0.0](https://github.com/juancruzmunozalbelo/oxide-studio/releases/tag/v1.0.0) | [SHA256SUMS.txt](https://github.com/juancruzmunozalbelo/oxide-studio/releases/download/v1.0.0/SHA256SUMS.txt) |
| **Windows** | **x86_64 (WASAPI / ASIO)** | [GitHub Releases v1.0.0](https://github.com/juancruzmunozalbelo/oxide-studio/releases/tag/v1.0.0) | [SHA256SUMS.txt](https://github.com/juancruzmunozalbelo/oxide-studio/releases/download/v1.0.0/SHA256SUMS.txt) |

### Install via Cargo

```bash
cargo install --git https://github.com/juancruzmunozalbelo/oxide-studio.git ableton-gui --bin ableton-rust
```

All release artifacts include the executable workstation binary, the 36-chapter Operational Knowledge Framework manual (`OXIDE_STUDIO_MANUAL_OKF.md`), licenses, and documentation.

---

## 9. Quickstart & Installation

### Prerequisites
- macOS 13+ (Ventura, Sonoma, Sequoia)
- Rust toolchain (`rustup default stable`, Rust 1.80+)
- *Optional (for Live 12 LOM bridging)*: Official Ableton Live 12.4.6 Suite

### 1. Build from Source
```bash
git clone https://github.com/juancruzmunozalbelo/oxide-studio.git
cd oxide-studio

# Compile release binary
cargo build --release
```

### 2. Launch Native Workstation GUI
Launch the 60 FPS single-window studio interface:
```bash
./target/release/ableton-rust
```
*Keyboard Shortcuts*:
- `Tab`: Toggle between **Session Matrix View** and **Arrangement Timeline View**.
- `Shift+Tab`: Toggle Bottom Deck between **Clip Detail View** and **Device Chain View**.
- `Space`: Transport Play / Stop.
- `Z` / `X`: Octave Down / Octave Up for computer keyboard playing.

### 3. Run Headless Demo Bounce & Verification
Render a 4.5-second multi-track demo arrangement and verify signal integrity:
```bash
./target/release/ableton-rust --render-demo /tmp/oxide_studio_demo.wav
```

### 4. Run Out-of-Process Plugin Scanner
Scan installed AUv2 and VST3 plugins:
```bash
./target/release/ableton-rust --scan-plugins --output plugins.json
```

---

## 10. Model Context Protocol (MCP) Setup

### Running Standalone MCP Server
To run the MCP server over standard input/output (`stdio`):
```bash
./target/release/ableton-rust --mcp
```

### Configuration for Claude Desktop / Antigravity / Cursor
Add the following to your `claude_desktop_config.json` or `mcp_config.json`:

```json
{
  "mcpServers": {
    "oxide-studio": {
      "command": "/absolute/path/to/oxide-studio/target/release/ableton-rust",
      "args": ["--mcp"],
      "env": {}
    }
  }
}
```

---

## 11. Native Live GUI RPC Bridge, In-App Copilot & Interoperability

Oxide Studio eliminates external Python remote script dependencies with an all-Rust, native architecture bridging GUI, DSP, autonomous agents, and DAW interoperability.

### 1. 100% Native Rust Live GUI <-> MCP TCP Bridge (`127.0.0.1:9877`)
- **Module**: [`crates/ableton-gui/src/live_rpc.rs`](crates/ableton-gui/src/live_rpc.rs)
- **Architecture**: Embedded non-blocking TCP socket server (`live_rpc.rs`) listening on `127.0.0.1:9877`.
- **Live Bidirectional Synchronization**: External MCP tool invocations (`live12_*`) communicate directly with the running Oxide Studio GUI window:
  - Dispatches line-delimited JSON-RPC 2.0 requests with zero external runtime dependencies.
  - Dynamically mutates session matrix tracks, piano roll note events, device chains, transport state, BPM, and global scale modes.
  - Propagates corresponding lock-free `AudioCommand` messages into the real-time audio thread ring buffer without allocation or jitter.

### 2. In-App `🤖 AI Producer Copilot`
- **Module**: [`crates/ableton-gui/src/panels/ai_chat.rs`](crates/ableton-gui/src/panels/ai_chat.rs)
- **Features**:
  - **Context-Aware Studio State**: Inspects selected track names, kick/synth parameter state (frequency, drive, cutoff), and real-time 4-band spectral FFT telemetry ("Agent Ears").
  - **Producer Slang Intent Router**: Understands English and Argentine Spanish (Rioplatense) music production terminology (`"hace este kick mas gordo"`, `"dale mas acido al bajo"`, `"arma un drop de peak techno"`, `"mete un patron de rkt / turreo"`, `"limpia el barro con ott"`).
  - **Real DAW Mutation**: Dispatches live engine changes, configures 909 drum rack pads, adjusts ZDF filters, and triggers clip playback.
  - **Clickable MCP Badges**: Visual indicators (`⚡ drums_configure_pad`, `⚡ fx_set_saturator`) with 1-click clipboard copying.

### 3. Bidirectional Ableton `.als` Live Set Interoperability
- **Module**: [`crates/ableton-project/src/als.rs`](crates/ableton-project/src/als.rs)
- **CLI Commands**:
  ```bash
  # Import native Ableton Live 12 XML Gzip project (.als) into Oxide Studio
  ./target/release/ableton-rust --import-als session.als

  # Export current Oxide Studio session directly to Ableton Live 12 Set (.als)
  ./target/release/ableton-rust --export-als exported_session.als
  ```
- **MCP Tools**: `project_import_als`, `project_export_als`.
- **Fidelity**: Directly parses and serializes Gzip magic bytes (`\x1f\x8b`), reconstructing Ableton Live 12 tracks, mixer channel faders, send routings, tempo markers, and MIDI clips.

### 4. 1-Click 4-Stem Remix Extractor
- **Module**: [`crates/ableton-bounce/src/stems.rs`](crates/ableton-bounce/src/stems.rs)
- **CLI Command**:
  ```bash
  # Extract isolated 4-stem remix package from rendered audio
  ./target/release/ableton-rust --split-stems master.wav --output-dir ./stems/
  ```
- **MCP Tools**: `stems_separate_remix`, `bounce_export_stems`.
- **DSP Separation**:
  - `drums.wav`: Transient/percussive signal decomposition via envelope attack differential.
  - `bass.wav`: Phase-coherent low-frequency isolation below 140 Hz (4th-order Linkwitz-Riley crossover).
  - `vocals.wav`: Center-channel mid-frequency extraction (220 Hz – 4.5 kHz) using Mid/Side matrixing.
  - `synths_other.wav`: Stereo width harmonic and high-frequency residual.
  - **Near-Exact Energy Conservation**: $\sum \text{stems} \approx \text{original}$ with reconstruction error $< 10^{-4}$.

---

## 12. Verification & Testing

Run the comprehensive workspace test suite:
```bash
cargo test --workspace
```

All 11 crates include unit and integration tests verifying:
- Mystran ZDF filter stability and Padé polynomial accuracy.
- Cytomic SVF filter slopes and saturation circuit models.
- Lock-free command and telemetry buffer thread safety.
- Wavetable frame interpolation and Operator FM carrier/modulator matrices.
- 6-mode warping transient detection and comping equal-power micro-crossfades.
- Deterministic 24-bit WAV file rendering with zero NaN/infinite samples.
- Native Live GUI RPC TCP bridge command routing and state synchronization.

---

## 13. License & Governance

Oxide Studio is free and open-source software, dual-licensed under either:
- **MIT License** ([LICENSE-MIT](LICENSE-MIT))
- **Apache License, Version 2.0** ([LICENSE-APACHE](LICENSE-APACHE))

at your option.

### Community & Legal Documents
- [CHANGELOG.md](CHANGELOG.md) — Comprehensive v1.0.0 release notes & Ableton Live 12.4.6 Suite parity changelog.
- [docs/ROADMAP_100_PARITY.md](docs/ROADMAP_100_PARITY.md) — 100% Commercial Ableton Live 12.4.6 Suite Parity Roadmap & verification evidence.
- [NOTICE.md](NOTICE.md) — Clean-room architectural provenance, mathematical citations & trademark non-affiliation disclaimer.
- [CONTRIBUTING.md](CONTRIBUTING.md) — Contribution workflow, 3-Gate Definition of Done, and real-time audio thread safety invariants.
- [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md) — Contributor Covenant v2.1.
- [SECURITY.md](SECURITY.md) — Security policy and plugin sandbox isolation threat model.
- [GLOSSARY.md](GLOSSARY.md) — Canonical domain glossary and DSP architecture terms.

