## Summary

<!-- Briefly describe the purpose of this change, linked issue, and what problem it solves. -->
Fixes #

## Changes Made

- 

## 🔒 Mandatory Pre-Closure Verification & Evidence Gate (DoD)

All pull requests and issues closing parity milestones MUST satisfy the three verification gates before merge:

- [ ] **Gate 1 — Ableton Manual & OKF Doc Sync**:
  - `docs/okf/OXIDE_STUDIO_MANUAL_OKF.md` updated with relevant user manual guidance.
  - `crates/ableton-gui/src/okf_knowledge.rs` updated with bilingual copilot intent keywords.
  - Passes: `cargo test -p ableton-gui okf -- --nocapture`
- [ ] **Gate 2 — End-to-End Manual & Real-Time DSP Audio Verification**:
  - GUI actions mutate real audio buffer state with verified acoustic telemetry.
  - Passes: `cargo test -p ableton-gui --test e2e_ableton_manual_verification_test -- --nocapture`
  - Passes: `cargo test -p ableton-drums -p ableton-audio -- --nocapture`
- [ ] **Gate 3 — Anti-Breakage Layout Guard (576-Frame Matrix)**:
  - 0 overflow, 0 clipping, 0 collision, and 0 tofu (`□`) Unicode errors across 960x600, 1280x800, 1920x1080, and 2560x1440.
  - Passes: `cargo test -p ableton-gui --test e2e_live12_layout_audit_test -- --nocapture`
  - Passes: `cargo test -p ableton-gui --test adversarial_acoustic_layout_stress_test -- --nocapture`
- [ ] **Automated Evidence Report Attached**:
  - Executed: `./scripts/verify_issue_dod.sh <issue_number> --post-comment`
  - Evidence file generated and committed: `docs/evidence/issue_<issue_number>_evidence.md`

## Audio & Real-Time Invariants Checklist

- [ ] **Audio Thread Safety**: Zero heap allocations (`Vec::push`, `Box::new`, `format!`), zero blocking locks (`Mutex`, `RwLock`), and zero disk/network I/O inside `process()` callbacks.
- [ ] **DSP Numerical Stability**: Bounded signal values ($|y[n]| < 2.0$), zero `NaN` / `Inf` samples, and unconditional stability up to resonance $1.0$.
- [ ] **Full Workspace Cleanliness**: `cargo test --workspace` passes with 100% green tests.
- [ ] **Formatting & Lints**: `cargo fmt --all -- --check` and `cargo clippy --workspace --all-targets -- -D warnings`.
