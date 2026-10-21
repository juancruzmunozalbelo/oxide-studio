# r/rust — I wrote a clean-room electronic music DAW in 100% Rust (cpal + egui, lock-free DSP, Ableton .als interop, and an MCP server)

Over the past few months I've been building **Oxide Studio**, an open-source (MIT/Apache-2.0) electronic music DAW and session/arrangement workstation written from scratch in Rust.

* **Repo:** https://github.com/juancruzmunozalbelo/oxide-studio
* **Web / Architecture overview:** https://juancruzmunozalbelo.github.io/oxide-studio/

I wanted a fast, single-binary workstation inspired by Ableton Live 12 that boots in under a second, runs a lock-free synth/FX engine, reads and writes `.als` project files, and exposes its entire state machine over a local TCP RPC bridge (`127.0.0.1:19384`) and an MCP (Model Context Protocol) server.

### Workspace Architecture (11 Crates)

To keep compilation times sane and enforce strict boundaries between the real-time audio thread and the UI/network layers, the project is split into 11 crates:

* `ableton-core` & `ableton-timeline`: Transport state, tempo map, session clip matrix, arrangement timeline, and MIDI clip structures (with scale-folding, Euclidean generators, and chord stacks).
* `ableton-dsp`, `ableton-drums`, `ableton-fx`: The synthesis and effects engines. Includes a 4-operator FM synth, Wavetable synth, 303 bass synth, TR-909 drum rack, multiband saturation, OTT multiband upward/downward compression, 8-band biquad EQ, Auto Filter, Glue bus compressor, and a 4-band stem separator (`Kick+Sub`, `Bass`, `Synths/Mids`, `Hats/Air`).
* `ableton-audio` & `ableton-bounce`: `cpal` stream management and offline deterministic rendering (WAV/AIFF stem export).
* `ableton-project`: Gzip + XML parser and serializer for native `.als` (Ableton Live 12) bidirectional project exchange.
* `ableton-gui`: Immediate-mode 60 FPS desktop interface built on `eframe` / `egui`, including an interactive Piano Roll, Session/Arrangement dual views, and an 8-macro XY pad with an Orbit LFO.
* `ableton-mcp`: A 76-tool JSON-RPC / MCP server that translates external commands into lock-free messages sent to the running audio engine.

### Real-Time Audio Constraints & `egui` Trade-offs

The hardest part of writing a DAW in Rust isn't writing biquad filters—it's keeping the audio callback strictly `zero-allocation` and lock-free while both a 60 FPS immediate-mode GUI and an asynchronous TCP RPC server mutate the project state simultaneously.

1. **No `Mutex` in the `cpal` callback:** All parameter tweaks (whether dragged with a mouse in `egui` or fired in bursts by an LLM over TCP) are pushed as small `Copy` command structs through bounded SPSC lock-free ring buffers. The audio thread drains the command queue at the start of each buffer block before processing DSP frames.
2. **Immediate-mode UI for audio:** People often warn against `egui` for DAWs. Honestly, for custom widgets like an XY morphing pad, step sequencers, or real-time spectrum/meter bars, `egui::Painter` is a joy to work with. The bottleneck only hits when rendering thousands of MIDI notes or uncached waveform peaks every frame—pre-bucketing min/max peak mipmaps per zoom level solved the frame-time spikes.

If you're working on Rust audio, `cpal`, or `egui` desktop apps, I'd really appreciate any feedback or critique on how the workspace crates and the audio-to-UI state synchronization are structured.
