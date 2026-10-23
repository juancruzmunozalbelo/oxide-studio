# ADR 0001: Dual-Engine Native Rust DSP and Live 12 LOM MCP Architecture

- **Status**: Accepted
- **Date**: 2026-10-20

## Context

Electronic music producers and autonomous AI agents need a studio workstation that combines:
1. Ultra-fast, deterministic, headlessly renderable native synthesis, drum synthesis, multi-FX chains, warping, and arrangement in Rust (`oxide-studio`) with a simplified, zero-clutter native macOS GPU UI (`eframe`/`egui`).
2. Full bidirectional interoperability with the commercial **Ableton Live 12.4.6 Suite** macOS application (`Live.app`) uncovered during our Reverse-Engineer-Anything (REA) audit of `MacOS/Live_arm64` and `App-Resources/Python/`.

## Decision

1. **Multi-Crate Lock-Free Rust Workspace**:
   - Separate deterministic DSP (`oxide-dsp`, `oxide-drums`, `oxide-fx`) and timeline/project state (`oxide-timeline`, `oxide-project`) from the `cpal` CoreAudio host (`oxide-audio`), offline renderer (`oxide-bounce`), GPU UI (`oxide-gui`), and JSON-RPC 2.0 MCP server (`oxide-mcp`).
   - Preserve exact mathematical parity with the decompiled Cycling '74 Max RNBO `0.14.0` Mystran 2x-oversampled ZDF 4-pole ladder filter (`code[817]`) and Ableton Live 12's Cytomic SVF (`entity::DFM1_ZD_24dB`).

2. **Dual-Target Model Context Protocol (`oxide-mcp`)**:
   - **Native Engine Target**: Every MCP tool mutates `ProjectState`, renders deterministic 24-bit WAV audio via `oxide-bounce`, and computes spectral/loudness metrics (`RMS`, `Peak dBFS`, `Crest Factor`, `Spectral Centroid`, `3-Band Energy`, `Zero-Crossing Rate`, `Clipping Count`) in milliseconds without requiring a soundcard.
   - **Live 12 LOM Target (`lom_*` + `AbletonMCP` Remote Script)**: A companion Python 3.11 MIDI Remote Script (`AbletonMCP`) runs inside Ableton Live 12.4.6 Suite's embedded `CPython 3.11.6` interpreter and bridges the `Live.*` Boost.Python Object Model over localhost TCP (`127.0.0.1:9877`) to `oxide-mcp`.

## Consequences

- Autonomous agents can compose, synthesize, mix, master, and spectrally verify entire tracks headlessly in Rust (`oxide-studio --mcp`), and optionally mirror or drive sessions live inside Ableton Live 12.4.6 Suite via `lom_*` tools.
- All new features must cut vertically across DSP/state, native GUI panel, MCP tool schema, and deterministic integration tests.
