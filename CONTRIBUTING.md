# Contributing to Oxide Studio

Thank you for your interest in contributing to **Oxide Studio** (`oxide-studio`), the clean-room native Rust digital audio workstation, Ableton Live 12 studio workstation, and 76-tool Model Context Protocol (MCP) server.

*Repository:* [https://github.com/juancruzmunozalbelo/oxide-studio](https://github.com/juancruzmunozalbelo/oxide-studio)  
*Showcase:* [https://juancruzmunozalbelo.github.io/oxide-studio/](https://juancruzmunozalbelo.github.io/oxide-studio/)

We welcome contributions from audio developers, DSP researchers, systems engineers, and electronic music producers.

---

## 1. Code of Conduct

All contributors and participants are expected to adhere to our [Code of Conduct](CODE_OF_CONDUCT.md) (Contributor Covenant v2.1). Please read it before participating.

---

## 2. Core Architectural Invariants

Oxide Studio is a hard real-time audio application. All code contributing to audio processing must respect the following strict engineering principles.

### A. Real-Time Audio Thread Safety (Zero Allocation Invariant)

The audio rendering callback (`process(&mut self, buffer: &mut AudioBuffer)`) executes on a high-priority, real-time operating system thread (via CoreAudio / `cpal`).

**On the Audio Thread, the following operations are STRICTLY PROHIBITED:**
1. **Zero Heap Allocations**:
   - Never call `Box::new()`, `Vec::push()`, `Vec::with_capacity()`, `String::from()`, `format!()`, or any method allocating memory.
   - All buffers, delay lines, voice pools, wavetable frames, and scratch memory must be pre-allocated during device initialization (`new()` / `prepare()`).
2. **Zero Synchronization Blocking**:
   - Never lock a `std::sync::Mutex` or `std::sync::RwLock` on the audio thread.
   - Never use blocking channels, semaphores, or condition variables.
3. **Zero File and Network I/O**:
   - Never open files, write logs, read disk assets, or make network/socket calls on the audio thread.
4. **Lock-Free Communication**:
   - Communication between the GUI / MCP server and the audio engine must exclusively use bounded Single-Producer Single-Consumer (SPSC) lock-free ring buffers (`rtrb`) and atomic primitives (`portable-atomic`).

### B. DSP Filter Stability & Bounded Output

1. **Self-Oscillation Stability**: All resonant filters (e.g., Mystran ZDF ladder, Cytomic SVF) must remain unconditionally stable under self-oscillation ($resonance = 1.0$) with strictly bounded output ($|y[n]| < 2.0$) and **zero `NaN` or `Inf` samples**.
2. **High-Frequency Attenuation**: Low-pass filters must exhibit proper Nyquist roll-off ($> 20\text{ dB}$ attenuation above cutoff frequency).
3. **Anti-Aliasing**: Nonlinear shaping and saturation stages must implement anti-aliasing techniques such as 1st-order Antiderivative Antialiasing (ADAA) or linear-phase half-band FIR polyphase oversampling ($2\times, 4\times, 8\times$).

### C. Studio Viewport & Layout Budget Invariants

When contributing UI code in `crates/ableton-gui`:
- All components must respect the `ViewportBudget` in `crates/ableton-gui/src/layout.rs`.
- Views must never overflow, clip the bottom mixer footer, or break at standard resolutions:
  - Compact Laptop ($960 \times 600$)
  - Standard MacBook ($1280 \times 800$)
  - Studio Display ($1920 \times 1080$)

---

## 3. Development Workflow

### Prerequisites
- Stable Rust (1.80+ recommended)
- macOS 13+ with Xcode Command Line Tools (CoreAudio & Metal)

### Building the Workspace
```bash
cargo build --workspace --release
```

### Running Tests
All pull requests must pass the complete automated test suite:
```bash
cargo test --workspace
```

### Checking Lints and Formatting
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
```

---

## 4. Submitting Pull Requests

1. **Fork & Branch**: Create a feature branch off `main` (e.g., `feat/club-clipper`, `fix/zdf-filter-denormals`).
2. **Commit Messages**: Follow standard conventional commits:
   - `feat(dsp): implement polyphase 4x oversampled clipper`
   - `fix(audio): eliminate heap allocation in voice stealing loop`
   - `docs(legal): update trademark disclaimer in NOTICE.md`
3. **Tests**: Include automated unit and integration tests covering your changes.
4. **PR Template**: Complete the checklist in [.github/PULL_REQUEST_TEMPLATE.md](.github/PULL_REQUEST_TEMPLATE.md), specifically verifying audio-thread compliance, DSP stability, and layout invariance.
