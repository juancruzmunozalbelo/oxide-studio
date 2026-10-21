# KVR Audio & Elektronauts — Oxide Studio: Open-source (MIT) native Rust DAW with 4-op FM, 909, Euclidean sequencer & Ableton .als interop

Hi everyone,

Wanted to share **Oxide Studio**, an open-source (MIT/Apache-2.0) electronic music DAW and groovebox workstation written from scratch in native Rust (`cpal` + `egui` over Metal/wgpu at 60 FPS).

* **GitHub Repository:** https://github.com/juancruzmunozalbelo/oxide-studio
* **Overview & Architecture:** https://juancruzmunozalbelo.github.io/oxide-studio/

The goal was to combine the immediacy of a hardware-style step/Euclidean groovebox (4-op FM, Wavetable, 303, 909 Drum Rack, 8-macro XY pad with Orbit LFO) with two capabilities rarely found in open-source audio tools:

1. **Bidirectional Ableton Live 12 (`.als`) project exchange:** Oxide Studio reads and writes gzipped XML `.als` files directly, so you can sketch polymetric FM/909 patterns in a tiny native binary and open the resulting `.als` straight in Live 12.
2. **Deterministic 76-tool MCP / TCP RPC bridge (`127.0.0.1:19384`):** The entire lock-free DSP graph and sequencer state can be scripted or driven in real time via JSON-RPC or Model Context Protocol tools.

The DSP chain runs with zero heap allocations on the audio callback (`rtrb` SPSC queues) and includes Mystran ZDF 4-pole ladder filters, Padé tanh/ADAA multiband saturation (Roar-style), 3-band OTT, 8-band parametric EQ, Glue bus compression, and 4-band stem separation (`Kick+Sub`, `Bass`, `Synths/Mids`, `Hats/Air`).

Source code is up on GitHub under dual MIT/Apache-2.0 license. Feedback on both the DSP implementation and workflow is very welcome!
