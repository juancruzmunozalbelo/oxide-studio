# r/LocalLLaMA & r/ClaudeAI — Instead of black-box MP3 generators, I built an open-source Rust DAW with a 76-tool MCP server so local LLMs can program real synths, 909 drums, and DSP live

Every "AI music" project I've tried treats music like Suno or Udio: you type a prompt into a black box and get a flat, uneditable MP3 full of phase artifacts. If you actually produce electronic music, you don't want an MP3—you want symbolic control over MIDI notes, filter cutoffs, FM operator ratios, drum racks, and automation lanes.

So I built **Oxide Studio**, an original open-source (MIT/Apache-2.0) electronic music workstation in native Rust with a built-in **76-tool MCP (Model Context Protocol) server** and a live TCP RPC bridge (`127.0.0.1:19384`).

* **GitHub & v1.0.0 Releases:** https://github.com/juancruzmunozalbelo/oxide-studio
* **Interactive Web DAW (Browser):** https://juancruzmunozalbelo.github.io/oxide-studio/web/
* **Web / Architecture:** https://juancruzmunozalbelo.github.io/oxide-studio/

### Architecture Across 11 Modular Crates

The workstation is partitioned across 11 workspace crates to guarantee strict isolation between the real-time audio thread and asynchronous agent control:
- `oxide-core`: Fundamental audio buffers, timing clocks, scales, and lock-free SPSC queues.
- `oxide-dsp`: Real-time polyphonic synthesis (4-op FM, Wavetable, 303 acid bass, ZDF filters).
- `oxide-drums`: TR-909 drum machine physical modeling, 16-pad drum rack, choke groups.
- `oxide-fx`: Dynamic multi-device chain (ADAA multiband saturator, OTT, 8-band EQ, Auto Filter, Glue).
- `oxide-timeline`: Session clip launcher, arrangement timeline, automation envelopes, and comping.
- `oxide-audio`: Low-latency CoreAudio `cpal` streaming with zero heap allocations on the audio callback.
- `oxide-bounce`: Deterministic offline multi-track bounce and 4-band stem remix separation.
- `oxide-project`: Native project state and open `.als` (Ableton Live Set) Gzip XML import/export.
- `oxide-plugin-scanner`: Out-of-process crash-isolated supervisor for AUv2, VST3, and CLAP plugins.
- `oxide-gui`: Single-window 60 FPS Metal/egui workstation UI with piano roll and perform pad.
- `oxide-mcp`: 76-tool Model Context Protocol server exposing comprehensive DSP control over stdio.

### How LLM-to-DSP Control Works in Real Time:

1. **76 Native MCP Tools:** You can point Claude Desktop, Gemini, or any local tool-calling model (Qwen-2.5-Coder, Llama-3.3, MiniMax) at the Oxide MCP server.
2. **Real Synthesis & Zero-Allocation DSP:** When the model calls a tool (`generate_euclidean_drum_clip`, `synth_set_parameter`, `fx_set_saturator`, `timeline_add_automation`), it doesn't render offline audio. It pushes lock-free SPSC commands directly into a 60 FPS Rust audio engine running a 4-op FM synth, Wavetable synth, 303 acid bass, TR-909 drum machine, Roar multiband saturator, and OTT.
3. **Human + Agent Co-Pilot:** Because the UI (`egui` at 60 FPS) and the RPC bridge share the same state machine, the LLM can lay down a 132 BPM Euclidean 909 groove and automate an EQ notch while you grab the 8-macro XY pad with your mouse and jam over it.
4. **Open `.als` Project Interop:** Once you and the agent build a loop that grooves, you hit Export `.als` and open the exact MIDI clips, tracks, and structure natively in your primary DAW.

If you're experimenting with local tool-calling models beyond boring CRUD/file tasks, watching a local Qwen model dial in a 303 acid line and tweak multiband distortion in real time is wildly fun. Happy to answer any questions about the MCP schema or the low-latency RPC bridge!
