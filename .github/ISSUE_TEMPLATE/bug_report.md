---
name: "Bug Report"
about: "Report a bug or unexpected behavior in Oxide Studio audio engine, GUI, DSP devices, or MCP server"
title: "[BUG]: "
labels: ["bug", "triage"]
---

### Affected Subsystem

Select the affected subsystem:
- [ ] Audio Engine (`oxide-audio`: CoreAudio / cpal / latency / buffer underruns)
- [ ] DSP Devices & Synthesis (`oxide-dsp`, `oxide-fx`: Oscillators, Filters, Roar, EQ8, Glue Compressor)
- [ ] Drum Machine & Samplers (`oxide-drums`: TR-909, Simpler, DrumRack)
- [ ] Session Matrix & Timeline (`oxide-timeline`: Clips, Scenes, Warping, Automation)
- [ ] Studio GUI & Visualizers (`oxide-gui`: Viewport, Piano Roll, Mixer, Spectrum, Vectorscope)
- [ ] Model Context Protocol Server (`oxide-mcp`: stdio JSON-RPC, 76 tools, copilot bridge)
- [ ] Plugin Scanner & Hosting (`oxide-plugin-scanner`: AUv2, VST3, CLAP supervisor)
- [ ] Project Persistence (`oxide-project`: `.oxs` native state, open `.als` import/export)
- [ ] Build & Test Infrastructure (Cargo workspaces, CI, platforms)

### Environment Details

- **OS & Version:** (e.g. macOS 14.5 Sonoma, Ubuntu 24.04 LTS, Windows 11)
- **Architecture / CPU:** (e.g. Apple Silicon M2 Max, x86_64 AMD Ryzen 9)
- **Rust Toolchain:** (e.g. rustc 1.81.0 stable)
- **Audio Interface / Driver:** (e.g. Built-in Output, CoreAudio, ALSA, WASAPI)
- **Sample Rate & Buffer Size:** (e.g. 48000 Hz, 128 samples)

### Steps to Reproduce

1. Launch `oxide-studio` (or `cargo run -p oxide-gui --bin oxide-studio`)
2. ...
3. ...

### Expected Behavior

A clear and concise description of what you expected to happen.

### Actual Behavior / Crash Logs

What actually happened instead. Please attach terminal output, crash logs, or backtraces (`RUST_BACKTRACE=1`):

```text
<paste logs or backtraces here>
```

### Telemetry / Audio Readings (if applicable)

- Peak dBFS / RMS level:
- DSP CPU usage %:
- Underrun / XRun count:

### Additional Context

Any other context, screenshots, or sample project files (.als / .oxs) about the problem.
