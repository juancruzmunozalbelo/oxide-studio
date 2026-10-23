# Show HN: Oxide Studio – Native Rust DAW with open .als interop and 76 MCP tools

**Target URL:** https://github.com/juancruzmunozalbelo/oxide-studio

---

### First Comment (Post immediately after submitting):

Hi HN, I built Oxide Studio, an original open-source (MIT/Apache-2.0) electronic music workstation written entirely in native Rust using cpal for real-time audio and egui for a 60 FPS immediate-mode interface.

Web overview / demo: https://juancruzmunozalbelo.github.io/oxide-studio/

I started building this out of two specific frustrations. First, every "AI music" tool right now treats music production as a black-box MP3 generator instead of giving an agent symbolic, deterministic control over actual synthesizers, drum machines, and DSP routing. Second, open-source DAWs usually trap you in their own project format, which makes them dead ends if you finish your tracks in other software with open .als project exchange.

The project is structured across 11 modular workspace crates:
- `oxide-core`: Core primitives, timing clock, scale/tuning systems, and lock-free SPSC command queues.
- `oxide-dsp`: Real-time polyphonic synthesis (4-operator FM, wavetable, 303 acid bass, Mystran ZDF ladder filter).
- `oxide-drums`: TR-909 physical drum synthesis models, 16-pad drum rack, transient slicing, and choke groups.
- `oxide-fx`: Device chain processing (Roar-style ADAA multiband saturator, OTT, 8-band EQ, Auto Filter, Glue compressor).
- `oxide-timeline`: Dual Session matrix & Arrangement timeline, automation curves, comping take lanes, and warping.
- `oxide-audio`: Low-latency CoreAudio I/O via `cpal` with zero heap allocations on the audio callback.
- `oxide-bounce`: Deterministic offline WAV/AIFF bounce rendering and 4-stem harmonic remix separation.
- `oxide-project`: Native project state and bidirectional open `.als` project format import/export.
- `oxide-plugin-scanner`: Out-of-process crash-isolated supervisor for hosting AUv2, VST3, and CLAP plugins.
- `oxide-gui`: 60 FPS immediate-mode desktop workstation UI rendered via Metal/egui, with scale-aware piano roll and XY perform pad.
- `oxide-mcp`: 76-tool Model Context Protocol (MCP) server & JSON-RPC bridge for local LLM orchestration.

What actually works today:

1. Open .als project interop: Ableton project files are gzipped XML under the hood. Oxide Studio parses open .als files to pull tracks, clips, MIDI notes, tempo, and device parameters into its own engine, and exports your Oxide session back to a valid .als file so you can open it seamlessly without bouncing everything to flat audio first.

2. Lock-free real-time DSP engine: The audio callback runs on cpal with zero heap allocations on the audio thread. Parameter changes and note events cross from the UI/RPC threads via lock-free SPSC ring buffers. Built-in devices include a 4-op FM synth, Wavetable synth, TB-303 bassline, TR-909 drum rack, multiband saturation, 3-band OTT dynamics, 8-band parametric EQ, Auto Filter, Glue Compressor, and a 4-band stem splitter (Kick+Sub, Bass, Synths/Mids, Hats/Air).

3. 76-tool MCP server & RPC bridge (127.0.0.1:19384): You can connect Claude, Gemini, or a local LLM via the Model Context Protocol (or hit the local TCP RPC port directly with your own API keys). Instead of generating fake audio, the agent calls tools to program Euclidean drum patterns, dial in FM operator ratios, draw filter automation curves, or split a loop into stems while the transport is running.

4. Generative MIDI & Perform tools: Piano roll with scale-folding, chord stacks, Euclidean rhythm generator, humanize/strum, and an 8-macro XY pad driven by an Orbit LFO.

What is still rough:
- VST3/CLAP/AU hosting is early; the main focus so far has been the internal zero-allocation Rust DSP graph and out-of-process crash-isolated scanning.
- Very complex .als files with third-party plugins or proprietary patches will only import the arrangement/session structure, MIDI, audio clips, and mapped native equivalents.
- egui kept development fast for a single developer, though drawing dense waveform overviews and piano roll grids at 60 FPS took careful peak mipmap caching to avoid chewing CPU.

I'd love technical feedback on the crate architecture, the lock-free audio graph, or the .als serializer if anyone here hacks on audio in Rust.
