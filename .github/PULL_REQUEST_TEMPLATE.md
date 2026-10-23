## Summary

<!-- Briefly describe the purpose of this change, linked issue, and what problem it solves. -->
Fixes #

## Changes Made

- 

## 🛡️ Contributor Quality & Invariant Checklist

Please verify that your pull request complies with Oxide Studio's core architectural constraints:

- [ ] **Audio Thread Safety**: Zero heap allocations (`Vec::push`, `Box::new`, `String::from`, `format!`), zero blocking synchronization primitives (`Mutex`, `RwLock`), and zero filesystem or network I/O inside `process()` and audio callback routines.
- [ ] **DSP Numerical Stability**: Bounded signal values ($|y[n]| < 2.0$), zero `NaN` or `Inf` samples, and unconditional filter stability up to resonance $1.0$.
- [ ] **Clean-Room Originality**: 100% original **Oxide Studio** identity; zero references to third-party trademarked crate/module/test names, reverse engineering, or proprietary manual chapters.
- [ ] **Workspace Test Suite**: `cargo test --workspace` passes with 100% green tests.
- [ ] **Layout Invariant Preservation**: Responsive layout budget preserved across standard resolutions (`960x600`, `1280x800`, `1920x1080`, `2560x1440`) with 0 text clipping, 0 overflow, 0 collisions, and 0 tofu (`□`) Unicode glyph errors.

## 🔒 Mandatory Pre-Closure Verification & Evidence Gate (DoD)

All pull requests modifying audio processing, DSP devices, GUI layouts, or persistence MUST satisfy the three verification gates before merge:

- [ ] **Gate 1 — Workstation Architecture & OKF Doc Sync**:
  - `docs/okf/OXIDE_STUDIO_MANUAL_OKF.md` updated with relevant operational guidance.
  - `crates/oxide-gui/src/okf_knowledge.rs` updated with bilingual copilot intent keywords.
  - Passes: `cargo test -p oxide-gui okf -- --nocapture`
- [ ] **Gate 2 — End-to-End Workstation & Real-Time DSP Audio Verification**:
  - GUI actions mutate real audio buffer state with verified acoustic telemetry.
  - Passes: `cargo test -p oxide-gui --test e2e_workstation_verification_test -- --nocapture`
  - Passes: `cargo test -p oxide-drums -p oxide-audio -- --nocapture`
- [ ] **Gate 3 — Anti-Breakage Layout Guard (576-Frame Matrix)**:
  - 0 overflow, 0 clipping, 0 collision, and 0 tofu (`□`) Unicode errors across 960x600, 1280x800, 1920x1080, and 2560x1440.
  - Passes: `cargo test -p oxide-gui --test e2e_workstation_layout_audit_test -- --nocapture`
  - Passes: `cargo test -p oxide-gui --test adversarial_acoustic_layout_stress_test -- --nocapture`
- [ ] **Automated Evidence Report Attached**:
  - Executed: `./scripts/verify_issue_dod.sh <issue_number> [--post-comment]`
  - Evidence file generated and committed: `docs/evidence/issue_<issue_number>_evidence.md`

## Formatting & Static Analysis

- [ ] `cargo fmt --all -- --check`
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`
