# Show HN: Oxide Studio – Native Rust DAW with Ableton .als interop and 70 MCP tools

**Target URL:** https://github.com/juancruzmunozalbelo/oxide-studio

---

### First Comment (Post immediately after submitting):

Hi HN, I built Oxide Studio, an open-source (MIT/Apache-2.0) electronic music DAW written entirely in native Rust using cpal for real-time audio and egui for a 60 FPS immediate-mode interface.

Web overview / demo: https://juancruzmunozalbelo.github.io/oxide-studio/

I started building this out of two specific frustrations. First, every "AI music" tool right now treats music production as a black-box MP3 generator instead of giving an agent symbolic, deterministic control over actual synthesizers, drum machines, and DSP routing. Second, open-source DAWs usually trap you in their own project format, which makes them dead ends if you finish your tracks in Ableton Live.

What actually works today across the 11-crate workspace:

1. Bidirectional Ableton Live 12 (.als) bridge: Ableton project files are gzipped XML under the hood. Oxide parses .als files to pull tracks, clips, MIDI notes, tempo, and device parameters into its own engine, and exports your Oxide session back to a valid .als file so you can open it in Live 12 without bouncing everything to flat audio first.

2. Lock-free real-time DSP engine: The audio callback runs on cpal with zero heap allocations on the audio thread. Parameter changes and note events cross from the UI/RPC threads via lock-free SPSC ring buffers. Built-in devices include a 4-op FM synth (Operator style), Wavetable synth, TB-303 bassline, TR-909 drum rack, multiband saturation (Roar style), 3-band OTT dynamics, 8-band parametric EQ, Auto Filter, Glue Compressor, and a 4-band stem splitter (Kick+Sub, Bass, Synths/Mids, Hats/Air).

3. 70+ tool MCP server & Live RPC bridge (127.0.0.1:19384): You can connect Claude, Gemini, or a local LLM via the Model Context Protocol (or hit the local TCP RPC port directly with your own API keys). Instead of generating fake audio, the agent calls tools to program Euclidean drum patterns, dial in FM operator ratios, draw filter automation curves, or split a loop into stems while the transport is running.

4. Generative MIDI & Perform tools: Piano roll with scale-folding, chord stacks, Euclidean rhythm generator, humanize/strum, and an 8-macro XY pad driven by an Orbit LFO.

What is still rough:
- VST3/CLAP/AU hosting is early; the main focus so far has been the internal zero-allocation Rust DSP graph and out-of-process crash-isolated scanning.
- Very complex .als files with third-party plugins or Max for Live patches will only import the arrangement/session structure, MIDI, audio clips, and mapped native equivalents.
- egui kept development fast for a single developer, though drawing dense waveform overviews and piano roll grids at 60 FPS took careful peak mipmap caching to avoid chewing CPU.

I'd love technical feedback on the crate architecture, the lock-free audio graph, or the .als serializer if anyone here hacks on audio in Rust.
