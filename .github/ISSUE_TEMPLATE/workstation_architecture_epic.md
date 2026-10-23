---
name: "Workstation Architecture Epic"
about: "Track and implement an Oxide Studio workstation architecture, DSP device, or subsystem epic"
title: "[Epic]: "
labels: ["epic", "architecture"]
---

## Oxide Studio Specification & Architectural Ground Truth

- **Workstation Architecture & DSP Specification:**
  - Section / Subsystem: *...*
  - Reference: `docs/okf/OXIDE_STUDIO_MANUAL_OKF.md`
- **Subsystems & Components:**
  - Target Engines: *Core Audio / DSP Devices / Timeline / Session Matrix / MCP Bridge*

---

## Scope & Technical Architecture

### Target Crates
- `crates/oxide-...`

### Zero-Allocation Real-Time Constraints
- Audio thread safety (zero heap allocations, zero blocking mutexes, zero I/O)
- Lock-free SPSC command queues and ring buffers (`rtrb`, `portable-atomic`)
- Strict numerical stability ($|y[n]| < 2.0$, zero `NaN` / `Inf` samples)

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

Before this issue can be closed or merged into main, all verification gates must pass and evidence must be posted:

- [ ] **1. OKF Documentation & Copilot Sync**:
  - `docs/okf/OXIDE_STUDIO_MANUAL_OKF.md` and `crates/oxide-gui/src/okf_knowledge.rs` updated with operational guidance.
  - Verification: `cargo test -p oxide-gui okf -- --nocapture`
- [ ] **2. End-to-End Workstation & Real-Time DSP Audio Verification**:
  - Real-time DSP audio buffer mutations and acoustic telemetry verified.
  - Verification: `cargo test -p oxide-gui --test e2e_workstation_verification_test -- --nocapture`
  - Verification: `cargo test -p oxide-drums -p oxide-audio -- --nocapture`
- [ ] **3. Anti-Breakage Layout Guard (576-Frame Matrix)**:
  - 0 overflow, 0 clipping, 0 collision, 0 tofu (`□`) Unicode errors across 960x600, 1280x800, 1920x1080, 2560x1440.
  - Verification: `cargo test -p oxide-gui --test e2e_workstation_layout_audit_test -- --nocapture`
  - Verification: `cargo test -p oxide-gui --test adversarial_acoustic_layout_stress_test -- --nocapture`
- [ ] **4. Automated Evidence Report Posted on Issue**:
  - Run: `./scripts/verify_issue_dod.sh <issue_number> [--post-comment]`
  - Evidence report generated and committed: `docs/evidence/issue_<N>_evidence.md`
