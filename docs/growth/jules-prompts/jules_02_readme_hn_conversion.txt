Optimize the top of `README.md` and `docs/index.html` in `juancruzmunozalbelo/oxide-studio` for high-traffic Hacker News (`Show HN`) and Reddit (`r/rust`, `r/ableton`, `r/LocalLLaMA`) developer/producer conversion.

Requirements:
1. In `README.md`, ensure the very top fold (first 25 lines) immediately shows:
   - Badges for License (`MIT OR Apache-2.0`), Rust Edition, CI status, and Live Demo (`https://juancruzmunozalbelo.github.io/oxide-studio/`).
   - A clean, zero-hype 3-bullet summary of the 3 technical differentiators:
     a) Bidirectional Ableton Live 12 (`.als`) native import/export.
     b) Lock-free, zero-allocation Rust DSP engine (`cpal` + `rtrb` + 60 FPS `egui` GUI) with 4-op FM, Wavetable, 303, TR-909, Roar saturation, OTT, and 4-band stem splitter.
     c) 76-tool MCP Server & Live TCP RPC Bridge (`127.0.0.1:19384`) for deterministic real-time synth/MIDI control from Claude, Gemini, or local LLMs.
   - A copy-pasteable 30-second Quickstart (`git clone`, `cargo run --release`, and MCP server config snippet for `claude_desktop_config.json` / `.mcp.json`).
2. In `docs/index.html`, verify OpenGraph (`og:title`, `og:description`, `og:url`, `twitter:card`) meta tags are present and sharp so links shared on Hacker News, Reddit, Discord, and X render a clean technical preview.
3. Keep all existing architecture documentation and tests intact (`cargo test --workspace` must pass).
