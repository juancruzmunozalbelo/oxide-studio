# r/LocalLLaMA & r/ClaudeAI — Instead of black-box MP3 generators, I built an open-source Rust DAW with a 76-tool MCP server so local LLMs can program real synths, 909 drums, and DSP live

Every "AI music" project I've tried treats music like Suno or Udio: you type a prompt into a black box and get a flat, uneditable MP3 full of phase artifacts. If you actually produce electronic music, you don't want an MP3—you want symbolic control over MIDI notes, filter cutoffs, FM operator ratios, drum racks, and automation lanes.

So I built **Oxide Studio**, an open-source (MIT) electronic music DAW in native Rust with a built-in **76-tool MCP (Model Context Protocol) server** and a live TCP RPC bridge (`127.0.0.1:19384`).

* **GitHub:** https://github.com/juancruzmunozalbelo/oxide-studio
* **Web / Architecture:** https://juancruzmunozalbelo.github.io/oxide-studio/

### How LLM-to-DSP Control Works in Real Time:

1. **76 Native MCP Tools:** You can point Claude Desktop, Gemini, or any local tool-calling model (Qwen-2.5-Coder, Llama-3.3, MiniMax) at the Oxide MCP server.
2. **Real Synthesis & Zero-Allocation DSP:** When the model calls a tool (`generate_euclidean_drum_clip`, `synth_set_parameter`, `fx_set_saturator`, `timeline_add_automation`), it doesn't render offline audio. It pushes lock-free SPSC commands directly into a 60 FPS Rust audio engine running a 4-op FM synth, Wavetable synth, 303 acid bass, TR-909 drum machine, Roar multiband saturator, and OTT.
3. **Human + Agent Co-Pilot:** Because the UI (`egui` at 60 FPS) and the RPC bridge share the same state machine, the LLM can lay down a 132 BPM Euclidean 909 groove and automate an EQ notch while you grab the 8-macro XY pad with your mouse and jam over it.
4. **Export straight to Ableton Live 12 (`.als`):** Once you and the agent build a loop that actually grooves, you hit Export `.als` and open the exact MIDI clips, tracks, and structure natively in Ableton Live 12.

If you're experimenting with local tool-calling models beyond boring CRUD/file tasks, watching a local Qwen model dial in a 303 acid line and tweak multiband distortion in real time is wildly fun. Happy to answer any questions about the MCP schema or the low-latency RPC bridge!
