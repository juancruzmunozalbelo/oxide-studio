# Contributing to Oxide Studio

Thank you for your interest in contributing to **Oxide Studio** (`oxide-studio`), the clean-room native Rust digital audio workstation, professional studio workstation, and 76-tool Model Context Protocol (MCP) server.

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

When contributing UI code in `crates/oxide-gui`:
- All components must respect the `ViewportBudget` in `crates/oxide-gui/src/layout.rs`.
- Views must never overflow, clip the bottom mixer footer, or break at standard resolutions:
  - Compact Laptop ($960 \times 600$)
  - Standard MacBook ($1280 \times 800$)
  - Studio Display ($1920 \times 1080$)
  - Ultra-Wide / 2K Display ($2560 \times 1440$)
- Must pass the 576-frame layout matrix audit (`adversarial_acoustic_layout_stress_test.rs`).

### D. 🔒 Mandatory Pre-Closure Verification & Definition of Done (DoD)

To ensure that no code modification regresses real-time audio thread safety, acoustic DSP fidelity, or viewport geometry, **every Pull Request must satisfy the 3-Gate Definition of Done**.

Verification is executed via [`scripts/verify_issue_dod.sh`](scripts/verify_issue_dod.sh):

```bash
# Run local verification:
./scripts/verify_issue_dod.sh <issue_number>

# Run verification and post evidence directly to GitHub Issue:
./scripts/verify_issue_dod.sh <issue_number> --post-comment
```

#### The 3 Verification Gates
1. **Gate 1 — Workstation Architecture & OKF Documentation Sync**:
   - Validates that `docs/okf/OXIDE_STUDIO_MANUAL_OKF.md` and `crates/oxide-gui/src/okf_knowledge.rs` contain all 10 operational sections (`OKF-01` through `OKF-10`).
   - Executes `cargo test -p oxide-gui okf -- --nocapture` to ensure bilingual English & Argentine Spanish query roundtrips.
2. **Gate 2 — End-to-End Workstation & Real-Time DSP Audio Verification**:
   - Executes `cargo test -p oxide-gui --test e2e_workstation_verification_test -- --nocapture` and `cargo test -p oxide-drums -p oxide-audio -- --nocapture`.
   - Asserts mathematical $-\infty\text{ dBFS}$ null silence under track fader zero gain or mute, dynamic TR-909 pattern rendering, and distinct spectral centroids for all 16 drum voices.
3. **Gate 3 — Anti-Breakage Layout Guard (576-Frame Matrix)**:
   - Executes `cargo test -p oxide-gui --test e2e_workstation_layout_audit_test -- --nocapture` and `cargo test -p oxide-gui --test adversarial_acoustic_layout_stress_test -- --nocapture`.
   - Audits 576 frames across 4 viewport resolutions (`960x600`, `1280x800`, `1920x1080`, `2560x1440`) and 12 vertical splitter heights (`220px` to `540px`).
   - Asserts **0 viewport overflows, 0 clipped texts, 0 text collisions, and 0 tofu (`□`) Unicode glyph errors**.

---

## 3. Development Workflow

### Prerequisites
- Stable Rust (1.80+ recommended)
- macOS 13+ with Xcode Command Line Tools (CoreAudio & Metal), Linux (ALSA / Jack / X11 dev headers), or Windows 11 (MSVC)

### Building the Workspace
```bash
cargo build --workspace --release
```

### Local Test Pipeline Before Opening a PR

Before opening a pull request, run the following local verification pipeline to ensure your changes pass all CI checks:

1. **Full Workspace Compilation & Test Suite**:
   ```bash
   cargo check --workspace --all-targets
   cargo test --workspace
   ```
   Ensures all 11 `oxide-*` crates compile cleanly and pass their unit, integration, and DSP numerical stability tests.

2. **End-to-End Workstation Verification**:
   ```bash
   cargo test -p oxide-gui --test e2e_workstation_verification_test
   ```
   Executes the 27-test end-to-end workstation suite, validating multi-resolution layout rendering, TR-909 step sequencing, DSP fader attenuation, and bilingual copilot intent queries.

3. **Adversarial Acoustic & Layout Stress Testing**:
   ```bash
   cargo test -p oxide-gui --test adversarial_acoustic_layout_stress_test
   cargo test -p oxide-gui --test e2e_workstation_layout_audit_test
   ```
   Audits the 576-frame responsive layout matrix and asserts bit-exact silence on fader cutoffs.

4. **Issue Definition-of-Done (DoD) Verification**:
   If your PR addresses or closes a specific issue, run the DoD verification script:
   ```bash
   ./scripts/verify_issue_dod.sh <issue_number>
   ```

5. **Clean-Room Regression Check**:
   Confirm that zero legacy trademarked names or reverse-engineering references exist in the workspace:
   ```bash
   grep -rEI "ableton-(core|dsp|drums|fx|audio|bounce|timeline|project|plugin-scanner|gui|mcp)" --exclude-dir=target --exclude-dir=.git .
   grep -rEI "ableton_(core|dsp|drums|fx|audio|bounce|timeline|project|plugin_scanner|gui|mcp)" --exclude-dir=target --exclude-dir=.git .
   find . -maxdepth 4 -not -path "*/target/*" -not -path "*/.git/*" -iname "*ableton*"
   ```
   All checks must return zero matches.

6. **Checking Lints and Formatting**:
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
   - `docs(spec): update workstation architecture specification`
3. **Tests**: Include automated unit and integration tests covering your changes.
4. **PR Template**: Complete the checklist in [.github/PULL_REQUEST_TEMPLATE.md](.github/PULL_REQUEST_TEMPLATE.md), specifically verifying audio-thread compliance, DSP stability, clean-room originality, and layout invariance.
