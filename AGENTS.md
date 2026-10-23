# Oxide Studio — Agent Instructions

Native Rust Electronic Music DAW, Workstation & Dual-Target Model Context Protocol (MCP) Server.

## Build & Verification Commands

- **Typecheck & compile**: `cargo check --workspace`
- **Run full test suite**: `cargo test --workspace`
- **Build release binary**: `cargo build --release`
- **Launch native macOS GUI**: `./target/release/oxide-studio`
- **Launch MCP Server (`stdio` JSON-RPC 2.0)**: `./target/release/oxide-studio --mcp`

## Architecture Overview

- `crates/oxide-core`: Lock-free SPSC ring buffers, `EngineCommand`, `TelemetrySnapshot`, transport clock, parameter & preset banks.
- `crates/oxide-dsp`: Polyphonic synthesizers (`SynthEngine`, `WavetableSynth`, `OperatorSynth`, `DriftSynth`, `MeldSynth`, `CollisionSynth`, `TensionSynth`, `ElectricSynth`, `SimplerSampler`), Mystran 2x-oversampled ZDF 4-pole ladder filter, Cytomic SVF.
- `crates/oxide-drums`: 808/909/FM/Karplus drum synthesis and 128-pad `DrumRack128` with choke groups and sends.
- `crates/oxide-fx`: Dynamic per-track `DeviceChain` (`EQ Eight`, `Glue Compressor`, `Roar`, `Auto Filter`, `OTT`, `Hybrid Reverb`, `Spectral Resonator`, `Echo`, `Beat Repeat`, `Corpus`, `Redux`, `Utility`, `Limiter`) and Return Track send buses.
- `crates/oxide-timeline`: Session Matrix (`SessionGrid`), Arrangement Timeline (`ArrangementView`), 6-mode Audio Warping (`WarpMode`), MIDI Generators & Transformers (`MidiTool`), Groove Pool (`GroovePool`), and Scale Lock (`ScaleLock`).
- `crates/oxide-project`: `.aruston.json` and Ableton `.als` (Gzip XML) import/export, stem export, and Neural Sound Similarity browser index.
- `crates/oxide-plugin-scanner`: Out-of-process macOS AUv2/VST3 plugin scanner with crash isolation.
- `crates/oxide-audio` & `crates/oxide-bounce`: Real-time `cpal` CoreAudio engine and deterministic offline 24-bit WAV bounce + spectral audio analyzer.
- `crates/oxide-gui`: Native 60fps `eframe`/`egui` Oxide Studio dark workstation UI (`#121316` / `#FF764D` / `#00E5FF`).
- `crates/oxide-mcp`: Full-control Model Context Protocol server (`stdio`) + `AbletonMCP` Python 3.11 LOM bridge for Ableton Live 12.4.6 Suite.

## Agent skills

When running any of the skills below, read the linked files first for repo-specific context:

- Issue tracker operations (`gh` / `glab` / local files, how to read/create/link issues, wayfinding conventions): [docs/agents/issue-tracker.md](docs/agents/issue-tracker.md)
- Triage label vocabulary (`needs-triage`, `ready-for-agent`, etc.): [docs/agents/triage-labels.md](docs/agents/triage-labels.md)
- Domain glossary & ADRs (where they live, how to use them): [docs/agents/domain.md](docs/agents/domain.md)

Applies to: `/wayfinder`, `/to-tickets`, `/to-spec`, `/triage`, `/write-a-prd`, `/grill-with-docs`, `/improve-codebase-architecture`, `/domain-modeling`, `/diagnose`, `/Interrogate`, `/blast-radius`, `/prototype`, `/research`, `/tdd`, `/pr`, `/code-review`, `/deslop`, `/wizard`.
