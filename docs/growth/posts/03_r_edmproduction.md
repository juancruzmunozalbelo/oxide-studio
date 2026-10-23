# r/edmproduction — I built a free, open-source (MIT) lightweight DAW in Rust with open .als project interop

When traveling with just a laptop on battery or sketching a quick 909/FM/acid loop without waiting for heavy external plugin folders to scan, having a tiny, standalone, zero-latency scratchpad that doesn't lock ideas into an incompatible proprietary format is a game changer.

Since `.als` files are gzipped XML under the hood, I spent the last few months building **Oxide Studio**—a free, open-source (MIT/Apache-2.0 license) electronic music workstation written in native Rust that provides bidirectional open `.als` project exchange.

* **GitHub (Source & v1.0.0 Releases):** https://github.com/juancruzmunozalbelo/oxide-studio
* **Try the Web DAW in Browser (no install):** https://juancruzmunozalbelo.github.io/oxide-studio/web/
* **Overview / Demo:** https://juancruzmunozalbelo.github.io/oxide-studio/

### Architecture Across 11 Modular Crates

Oxide Studio is built with clean-room separation across 11 workspace crates:
- `oxide-core`: Audio buffers, lock-free SPSC queues, tempo map, scales, and tuning models.
- `oxide-dsp`: Polyphonic synthesis (4-operator FM, Wavetable, 303 acid bass, ZDF ladder filters).
- `oxide-drums`: TR-909 drum machine modeling, 16-pad drum rack, transient slicing, and choke groups.
- `oxide-fx`: Audio effects (multiband ADAA saturator, 3-band OTT, 8-band EQ, Auto Filter, Glue compressor).
- `oxide-timeline`: Dual Session matrix and Arrangement timeline, automation curves, and take-lane comping.
- `oxide-audio`: CoreAudio / `cpal` low-latency stream management with zero heap allocations on the audio thread.
- `oxide-bounce`: Deterministic offline WAV bounce rendering and 4-stem remix separation.
- `oxide-project`: Native project state and open `.als` Gzip XML project import and export.
- `oxide-plugin-scanner`: Out-of-process crash-isolated supervisor for AUv2, VST3, and CLAP plugins.
- `oxide-gui`: Single-window 60 FPS Metal/egui workstation interface with piano roll and 8-macro XY pad.
- `oxide-mcp`: 76-tool Model Context Protocol server and JSON-RPC bridge for scripting and local LLM workflows.

### What you can actually use it for:

1. **Open `.als` Interop:** You can sketch a drum groove, bassline, and chord progression in Oxide Studio, hit export to `.als`, and open the exact session/arrangement structure, MIDI clips, and tempo straight in your main DAW. You can also load an existing `.als` into Oxide to strip it down or rework MIDI ideas.
2. **Built-in Electronic Music DSP (zero external plugins needed):** Native Rust versions of the tools you need for electronic music that consume minimal CPU: a 4-op FM synth, Wavetable synth, TR-909 Drum Rack, 303 bassline, multiband saturation, OTT dynamics, 8-band EQ, Auto Filter, and a Glue bus compressor.
3. **Instant 4-band Stem Separation:** Built-in splitter that separates loops/audio into `Kick+Sub`, `Bass`, `Synths/Mids`, and `Hats/Air` inside the project for quick resampling or remixing.
4. **Piano Roll & Generative MIDI tools:** Scale-fold locking, Euclidean rhythm generator, Chord Stacks, Strum/Humanize, plus an 8-macro XY Perform Pad with an Orbit LFO for hands-on modulation.
5. **Programmable / Scriptable (Optional):** If you like tinkering with code or local AI agents, the entire DAW exposes a local bridge (`127.0.0.1:19384`) and 76 MCP tools, so you can script complex parameter modulations or generate MIDI variations programmatically while the audio is playing.

It's 100% free, no telemetry, no accounts, MIT licensed. Think of it as a fast companion groovebox and sketchpad with open project exchange.

Would love to hear how the `.als` import/export behaves with your own projects if anyone gives it a spin!
