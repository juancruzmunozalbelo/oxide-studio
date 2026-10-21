## Summary

<!-- Briefly describe the purpose of this change and what problem it solves. -->

## Changes Made

- 

## Verification & Audio Invariants Checklist

- [ ] **Audio Thread Safety**: Verified zero heap allocations (`Vec::push`, `Box::new`, `format!`), zero blocking locks (`Mutex`, `RwLock`), and zero disk/network I/O inside `process()` callbacks.
- [ ] **DSP Numerical Stability**: Verified filters and non-linear saturators remain bounded ($|y[n]| < 2.0$), produce zero `NaN` / `Inf` samples, and maintain stability up to self-oscillation ($resonance = 1.0$).
- [ ] **Multi-Resolution Layout Invariance**: If modifying `crates/ableton-gui`, verified layout adherence to `ViewportBudget` across 960x600, 1280x800, and 1920x1080 without clipped mixer footers or scrolling bugs.
- [ ] **Automated Tests**: Added or updated unit/integration tests covering this behavior.
- [ ] **Workspace Tests**: Ran `cargo test --workspace` with 100% passing results.
- [ ] **Lint & Formatting**: Ran `cargo fmt --all -- --check` and `cargo clippy --workspace --all-targets -- -D warnings`.
