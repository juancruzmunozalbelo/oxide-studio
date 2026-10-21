---
name: "Ableton Live 12 Parity Epic"
about: "Track and implement an Ableton Live 12 Suite commercial parity epic"
title: "[Epic]: "
labels: ["epic", "ready-for-agent"]
---

## Ableton Live 12 Reference Manual / REA Ground Truth

- **Ableton Live 12 Reference Manual:**
  - Chapter X: *...*
- **REA Binary Forensic Evidence:**
  - Target Binaries & Subsystems:
    - `Live_arm64` / `OEngine` / `AEngineStructure` / ...

---

## Scope & Technical Architecture

### Target Crates
- `crates/...`

### Zero-Allocation Real-Time Constraints
- Audio thread safety (0 heap allocations, 0 blocking mutexes)
- Lock-free SPSC command queues and ring buffers

---

## Task Checklist

- [ ] Task 1
- [ ] Task 2

---

## Objective Acceptance Criteria

1. ...
2. ...

---

## 🔒 Mandatory Pre-Closure Verification & Evidence Gate (DoD)

Before this issue can be closed or merged into main, all 3 gates must pass and evidence must be posted:

- [ ] **1. OKF Documentation & Copilot Sync**:
  - `docs/okf/OXIDE_STUDIO_MANUAL_OKF.md` and `crates/ableton-gui/src/okf_knowledge.rs` updated with `OKF-01..08` sections.
  - Verification: `cargo test -p ableton-gui okf -- --nocapture`
- [ ] **2. End-to-End Manual & Real-Time DSP Audio Verification**:
  - Real-time DSP audio buffer mutations and acoustic telemetry verified.
  - Verification: `cargo test -p ableton-gui --test e2e_ableton_manual_verification_test -- --nocapture`
  - Verification: `cargo test -p ableton-drums -p ableton-audio -- --nocapture`
- [ ] **3. Anti-Breakage Layout Guard (576-Frame Matrix)**:
  - 0 overflow, 0 clipping, 0 collision, 0 tofu (`□`) Unicode errors across 960x600, 1280x800, 1920x1080, 2560x1440.
  - Verification: `cargo test -p ableton-gui --test e2e_live12_layout_audit_test -- --nocapture`
  - Verification: `cargo test -p ableton-gui --test adversarial_acoustic_layout_stress_test -- --nocapture`
- [ ] **4. Automated Evidence Report Posted on Issue**:
  - Run: `./scripts/verify_issue_dod.sh <issue_number> --post-comment`
  - Evidence report generated and committed: `docs/evidence/issue_<N>_evidence.md`
