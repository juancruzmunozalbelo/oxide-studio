# KVR Audio & Elektronauts — Oxide Studio: Open-source (MIT) native Rust DAW with 4-op FM, 909, Euclidean sequencer & open .als interop

Hi everyone,

Wanted to share **Oxide Studio**, an original open-source (MIT/Apache-2.0) electronic music workstation and groovebox DAW written from scratch in native Rust (`cpal` + `egui` over Metal/wgpu at 60 FPS).

* **GitHub Repository:** https://github.com/juancruzmunozalbelo/oxide-studio
* **Overview & Architecture:** https://juancruzmunozalbelo.github.io/oxide-studio/
* **Web Edition (In-Browser):** https://juancruzmunozalbelo.github.io/oxide-studio/web/

### Modular 11-Crate Architecture

The codebase is engineered across 11 decoupled crates to maintain zero heap allocations on the audio callback and sub-millisecond telemetry feedback:
- `oxide-core`: Audio buffers, lock-free SPSC queues, tempo map, scales, and tuning models.
- `oxide-dsp`: Polyphonic synthesis (4-operator FM, Wavetable, 303 acid bass, Mystran ZDF ladder filters).
- `oxide-drums`: TR-909 physical drum synthesis models, 16-pad drum rack, transient slicing, and choke groups.
- `oxide-fx`: ADAA multiband saturator, 3-band OTT dynamics, 8-band EQ, Auto Filter, Glue compressor.
- `oxide-timeline`: Dual Session matrix and Arrangement timeline, automation curves, and take-lane comping.
- `oxide-audio`: Low-latency CoreAudio `cpal` streaming with zero heap allocations on the audio callback.
- `oxide-bounce`: Deterministic offline multi-track bounce and 4-band stem remix separation.
- `oxide-project`: Native project state and open `.als` (Ableton Live Set) Gzip XML project import and export.
- `oxide-plugin-scanner`: Out-of-process crash-isolated supervisor for AUv2, VST3, and CLAP plugins.
- `oxide-gui`: Single-window 60 FPS Metal/egui workstation UI with piano roll and 8-macro XY perform pad.
- `oxide-mcp`: 76-tool Model Context Protocol server and JSON-RPC bridge for scripting and local LLM workflows.

The goal was to combine the immediacy of a hardware-style step/Euclidean groovebox (4-op FM, Wavetable, 303, 909 Drum Rack, 8-macro XY pad with Orbit LFO) with two capabilities rarely found in open-source audio tools:

1. **Open Ableton Live (.als) Project Interop:** Oxide Studio reads and writes gzipped XML `.als` files directly, so you can sketch polymetric FM/909 patterns in a tiny native binary and open the resulting `.als` straight in your primary DAW.
2. **Deterministic 76-tool MCP / TCP RPC Bridge (`127.0.0.1:19384`):** The entire lock-free DSP graph and sequencer state can be scripted or driven in real time via JSON-RPC or Model Context Protocol tools.

The DSP chain runs with zero heap allocations on the audio callback (`rtrb` SPSC queues) and includes Mystran ZDF 4-pole ladder filters, Padé tanh/ADAA multiband saturation, 3-band OTT, 8-band parametric EQ, Glue bus compression, and 4-band stem separation (`Kick+Sub`, `Bass`, `Synths/Mids`, `Hats/Air`).

Source code is up on GitHub under dual MIT/Apache-2.0 license. Feedback on both the DSP implementation and workflow is very welcome!
