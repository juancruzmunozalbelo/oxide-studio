# r/rust — I wrote a clean-room electronic music DAW in 100% Rust (cpal + egui, lock-free DSP, open .als interop, and an MCP server)

Over the past few months I've been building **Oxide Studio**, an original open-source (MIT/Apache-2.0) electronic music workstation and session/arrangement DAW written from scratch in Rust.

* **Repo & v1.0.0 Releases:** https://github.com/juancruzmunozalbelo/oxide-studio
* **Try the Web Edition in Browser (WebAudio):** https://juancruzmunozalbelo.github.io/oxide-studio/web/
* **Architecture & Showcase:** https://juancruzmunozalbelo.github.io/oxide-studio/

I wanted a fast, single-binary electronic music workstation that boots in under a second, runs a lock-free synth/FX engine, reads and writes `.als` project files for open interop, and exposes its entire state machine over a local TCP RPC bridge (`127.0.0.1:19384`) and a 76-tool MCP (Model Context Protocol) server.

### Workspace Architecture (11 Crates)

To keep compilation times sane and enforce strict boundaries between the real-time audio thread and the UI/network layers, the project is cleanly separated into 11 specialized workspace crates:

* `oxide-core`: Core domain models, lock-free single-producer single-consumer (SPSC) ring buffers (`rtrb`), audio buffer slices, sample clock, GlobalScale, and microtonal tuning systems.
* `oxide-dsp`: Real-time polyphonic synthesis engines (Wavetable, 4-operator FM, 303 acid bass, Drift, Meld bi-timbral), Mystran 2x-oversampled zero-delay feedback (ZDF) 4-pole transistor ladder filter with Padé saturator, and Cytomic state-variable filters.
* `oxide-drums`: TR-909 physical drum synthesis models, 128-pad Drum Rack bank view, sample slicing, and choke groups.
* `oxide-fx`: Dynamic multi-device chain hosting EQ Eight, Glue bus compressor, Roar multi-stage saturator with 1st-order ADAA antialiasing, Auto Filter, 3-band OTT dynamics, and Hybrid Reverb.
* `oxide-timeline`: Dual-paradigm Session Matrix and Arrangement Timeline, automation curves, comping take lanes with equal-power micro-crossfades, and 6-mode warping engine.
* `oxide-audio`: Low-latency native CoreAudio integration via `cpal` with real-time thread priority scheduling and zero heap allocations on the audio callback.
* `oxide-bounce`: Deterministic offline 24-bit WAV bounce renderer, sample-rate conversion, and 4-stem harmonic remix separation (`Kick+Sub`, `Bass`, `Synths/Mids`, `Hats/Air`).
* `oxide-project`: Native project state serialization and bidirectional open `.als` (Ableton Live 12 Set) Gzip XML parser/serializer.
* `oxide-plugin-scanner`: Out-of-process crash-isolated supervisor for enumerating and hosting AUv2, VST3, and CLAP plugins without crashing the main host.
* `oxide-gui`: Single-window 60 FPS workstation interface rendered natively via Metal/egui, with interactive Piano Roll, Session/Arrangement dual views, and an 8-macro XY perform pad with Orbit LFO.
* `oxide-mcp`: Dual-target 76-tool Model Context Protocol (MCP) server and JSON-RPC bridge allowing autonomous local LLMs to manipulate the DAW state machine in real time.

### Real-Time Audio Constraints & `egui` Trade-offs

The hardest part of writing a DAW in Rust isn't writing biquad filters—it's keeping the audio callback strictly `zero-allocation` and lock-free while both a 60 FPS immediate-mode GUI and an asynchronous TCP RPC server mutate the project state simultaneously.

1. **No `Mutex` in the `cpal` callback:** All parameter tweaks (whether dragged with a mouse in `egui` or fired in bursts by an LLM over TCP) are pushed as small `Copy` command structs through bounded SPSC lock-free ring buffers. The audio thread drains the command queue at the start of each buffer block before processing DSP frames.
2. **Immediate-mode UI for audio:** People often warn against `egui` for DAWs. Honestly, for custom widgets like an XY morphing pad, step sequencers, or real-time spectrum/meter bars, `egui::Painter` is a joy to work with. The bottleneck only hits when rendering thousands of MIDI notes or uncached waveform peaks every frame—pre-bucketing min/max peak mipmaps per zoom level solved the frame-time spikes.

If you're working on Rust audio, `cpal`, or `egui` desktop apps, I'd really appreciate any feedback or critique on how the workspace crates and the audio-to-UI state synchronization are structured.
