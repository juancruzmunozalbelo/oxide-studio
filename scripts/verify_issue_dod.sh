#!/usr/bin/env bash
# ==============================================================================
# Oxide Studio — Mandatory Pre-Closure DoD Verification & Evidence Script
# Enforces the 3 Definition-of-Done (DoD) verification gates before closing
# any GitHub Issue on juancruzmunozalbelo/oxide-studio.
#
# Usage:
#   ./scripts/verify_issue_dod.sh <issue_number> [--post-comment]
#
# Example:
#   ./scripts/verify_issue_dod.sh 17
#   ./scripts/verify_issue_dod.sh 17 --post-comment
# ==============================================================================

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

if [[ $# -lt 1 ]]; then
    echo "❌ Error: Missing issue number."
    echo "Usage: $0 <issue_number> [--post-comment]"
    exit 1
fi

RAW_ISSUE_NUM="$1"
ISSUE_NUM="${RAW_ISSUE_NUM#"#"}"
POST_COMMENT=false

if [[ "${2:-}" == "--post-comment" ]] || [[ "${3:-}" == "--post-comment" ]]; then
    POST_COMMENT=true
fi

EVIDENCE_DIR="$REPO_ROOT/docs/evidence"
mkdir -p "$EVIDENCE_DIR"
EVIDENCE_FILE="$EVIDENCE_DIR/issue_${ISSUE_NUM}_evidence.md"

TIMESTAMP=$(date -u +"%Y-%m-%dT%H:%M:%SZ")
GIT_SHA=$(git rev-parse HEAD 2>/dev/null || echo "unknown")
RUSTC_VER=$(rustc --version 2>/dev/null || echo "unknown")

echo "======================================================================"
echo "🔒 Oxide Studio Pre-Closure DoD Verification Gate: Issue #${ISSUE_NUM}"
echo "• Timestamp: $TIMESTAMP"
echo "• Git Commit: $GIT_SHA"
echo "• Compiler: $RUSTC_VER"
echo "======================================================================"

GATE1_STATUS="FAIL"
GATE1_TESTS=0
GATE2_STATUS="FAIL"
GATE2_TESTS=0
GATE3_STATUS="FAIL"
GATE3_TESTS=0

TMP_DIR=$(mktemp -d)
trap 'rm -rf "$TMP_DIR"' EXIT

# ------------------------------------------------------------------------------
# Gate 1 — Ableton Manual & OKF Documentation Sync
# ------------------------------------------------------------------------------
echo ""
echo "▶ Running Gate 1: Ableton Manual & OKF Documentation Sync..."

OKF_DOC="$REPO_ROOT/docs/okf/OXIDE_STUDIO_MANUAL_OKF.md"
OKF_RUST="$REPO_ROOT/crates/ableton-gui/src/okf_knowledge.rs"

if [[ ! -f "$OKF_DOC" ]]; then
    echo "❌ Gate 1 Fail: Missing $OKF_DOC"
    exit 1
fi

if [[ ! -f "$OKF_RUST" ]]; then
    echo "❌ Gate 1 Fail: Missing $OKF_RUST"
    exit 1
fi

for section in OKF-01 OKF-02 OKF-03 OKF-04 OKF-05 OKF-06 OKF-07 OKF-08 OKF-09 OKF-10; do
    if ! grep -q "$section" "$OKF_DOC"; then
        echo "❌ Gate 1 Fail: Section $section missing in $OKF_DOC"
        exit 1
    fi
    if ! grep -q "$section" "$OKF_RUST"; then
        echo "❌ Gate 1 Fail: Section $section missing in $OKF_RUST"
        exit 1
    fi
done

GATE1_LOG="$TMP_DIR/gate1.log"
cargo test -p ableton-gui okf -- --nocapture > "$GATE1_LOG" 2>&1 || {
    cat "$GATE1_LOG"
    echo "❌ Gate 1 Fail: cargo test -p ableton-gui okf failed."
    exit 1
}

GATE1_TESTS=$(grep -E 'test result: ok\. [0-9]+ passed' "$GATE1_LOG" | awk '{sum += $4} END {print sum}')
GATE1_STATUS="PASS"
echo "✓ Gate 1 Passed ($GATE1_TESTS OKF doc/copilot tests passed)"

# ------------------------------------------------------------------------------
# Gate 2 — End-to-End Manual & DSP Audio Verification
# ------------------------------------------------------------------------------
echo ""
echo "▶ Running Gate 2: End-to-End Manual & Real-Time DSP Audio Verification..."

GATE2_LOG="$TMP_DIR/gate2.log"
cargo test -p ableton-gui --test e2e_ableton_manual_verification_test -- --nocapture > "$GATE2_LOG" 2>&1 || {
    cat "$GATE2_LOG"
    echo "❌ Gate 2 Fail: e2e_ableton_manual_verification_test failed."
    exit 1
}

GATE2_LOG2="$TMP_DIR/gate2_drums_audio.log"
cargo test -p ableton-drums -p ableton-audio -- --nocapture > "$GATE2_LOG2" 2>&1 || {
    cat "$GATE2_LOG2"
    echo "❌ Gate 2 Fail: ableton-drums or ableton-audio tests failed."
    exit 1
}

G2_COUNT1=$(grep -E 'test result: ok\. [0-9]+ passed' "$GATE2_LOG" | awk '{sum += $4} END {print sum}')
G2_COUNT2=$(grep -E 'test result: ok\. [0-9]+ passed' "$GATE2_LOG2" | awk '{sum += $4} END {print sum}')
GATE2_TESTS=$(( ${G2_COUNT1:-0} + ${G2_COUNT2:-0} ))
GATE2_STATUS="PASS"
echo "✓ Gate 2 Passed ($GATE2_TESTS E2E manual & acoustic DSP tests passed)"

# ------------------------------------------------------------------------------
# Gate 3 — Anti-Breakage Layout Guard (576-Frame Matrix)
# ------------------------------------------------------------------------------
echo ""
echo "▶ Running Gate 3: Anti-Breakage Layout Guard (576-Frame Matrix)..."

GATE3_LOG="$TMP_DIR/gate3_audit.log"
cargo test -p ableton-gui --test e2e_live12_layout_audit_test -- --nocapture > "$GATE3_LOG" 2>&1 || {
    cat "$GATE3_LOG"
    echo "❌ Gate 3 Fail: e2e_live12_layout_audit_test failed."
    exit 1
}

GATE3_LOG2="$TMP_DIR/gate3_stress.log"
cargo test -p ableton-gui --test adversarial_acoustic_layout_stress_test -- --nocapture > "$GATE3_LOG2" 2>&1 || {
    cat "$GATE3_LOG2"
    echo "❌ Gate 3 Fail: adversarial_acoustic_layout_stress_test failed."
    exit 1
}

G3_COUNT1=$(grep -E 'test result: ok\. [0-9]+ passed' "$GATE3_LOG" | awk '{sum += $4} END {print sum}')
G3_COUNT2=$(grep -E 'test result: ok\. [0-9]+ passed' "$GATE3_LOG2" | awk '{sum += $4} END {print sum}')
GATE3_TESTS=$(( ${G3_COUNT1:-0} + ${G3_COUNT2:-0} ))
GATE3_STATUS="PASS"
echo "✓ Gate 3 Passed ($GATE3_TESTS layout audit & adversarial stress tests passed)"

# ------------------------------------------------------------------------------
# Generate Structured Markdown Evidence Report
# ------------------------------------------------------------------------------
echo ""
echo "📝 Writing Evidence Report to $EVIDENCE_FILE..."

TOTAL_PASSED=$(( GATE1_TESTS + GATE2_TESTS + GATE3_TESTS ))

cat <<EOF > "$EVIDENCE_FILE"
# 🔒 Pre-Closure DoD Verification & Evidence Report: Issue #${ISSUE_NUM}

- **Issue Reference:** [Issue #${ISSUE_NUM}](https://github.com/juancruzmunozalbelo/oxide-studio/issues/${ISSUE_NUM})
- **Timestamp (UTC):** \`${TIMESTAMP}\`
- **Git Commit SHA:** \`${GIT_SHA}\`
- **Compiler:** \`${RUSTC_VER}\`
- **Final Verdict:** **APPROVED FOR CLOSURE** (All 3 Gates Passed)

---

## 1. Verification Gates Summary Table

| Gate | Verification Domain | Test Target | Status | Passing Tests |
| :--- | :--- | :--- | :---: | :---: |
| **Gate 1** | **Ableton Manual & OKF Doc Sync** | \`docs/okf/OXIDE_STUDIO_MANUAL_OKF.md\` & \`okf_knowledge.rs\` (\`OKF-01..10\`) | **${GATE1_STATUS}** | ${GATE1_TESTS} tests |
| **Gate 2** | **End-to-End Manual & Real-Time DSP Audio** | \`e2e_ableton_manual_verification_test\` & \`ableton-drums\` / \`ableton-audio\` | **${GATE2_STATUS}** | ${GATE2_TESTS} tests |
| **Gate 3** | **Anti-Breakage Layout Guard (576-Frame Matrix)** | \`e2e_live12_layout_audit_test\` & \`adversarial_acoustic_layout_stress_test\` | **${GATE3_STATUS}** | ${GATE3_TESTS} tests |
| **Total** | **Comprehensive Full Suite DoD Gate** | **All 3 Verification Gates Combined** | **PASS** | **${TOTAL_PASSED} tests** |

---

## 2. Gate 1 Evidence: Ableton Manual & OKF Copilot Sync

- **Census Verification:** All 10 OKF Knowledge Sections (\`OKF-01\` through \`OKF-10\`) verified present in both documentation markdown and native Rust copilot registry.
- **Bilingual Intent Querying:** Tested and verified bilingual query roundtrip across English and Argentine Spanish producer terminology.
- **Test Output Summary:**
\`\`\`text
$(cat "$GATE1_LOG" | grep -E 'test okf_knowledge|test.*okf' | head -n 15)
test result: ok. ${GATE1_TESTS} passed; 0 failed
\`\`\`

---

## 3. Gate 2 Evidence: Real-Time DSP Audio & Acoustic Verification

- **Fader & Mute Attenuation:** Verified $-\\infty\\text{ dBFS}$ exact mathematical silence when track faders are set to 0.0 or muted.
- **TR-909 Step Sequencer:** Verified dynamic sample rendering, swing grooves, and distinct frequency centroids across all 16 drum pads.
- **Test Output Summary:**
\`\`\`text
$(cat "$GATE2_LOG" "$GATE2_LOG2" 2>/dev/null | grep -E 'test test_e2e_|test drum_rack|test mixer|test processor' | head -n 20)
\`\`\`

---

## 4. Gate 3 Evidence: Anti-Breakage Layout Guard (576-Frame Matrix)

- **Layout Invariants:**
  - Multi-resolution responsive layout verified across \`960x600\`, \`1280x800\`, \`1920x1080\`, and \`2560x1440\`.
  - Continuous vertical splitters audited from \`220px\` to \`540px\` heights without clipping.
  - Zero overflow, collision, or tofu (\`□\`) Unicode rendering errors.
- **Test Output Summary:**
\`\`\`text
$(cat "$GATE3_LOG" "$GATE3_LOG2" 2>/dev/null | grep -E 'test test_f|test test_adv' | head -n 15)
\`\`\`

---

## 5. Closure Authorization

The Definition-of-Done (DoD) requirements for Issue #${ISSUE_NUM} are fully satisfied. The implementation has passed all documentation, real-time acoustic/DSP, and layout verification audits.
EOF

echo "✓ Evidence report generated: $EVIDENCE_FILE"

# ------------------------------------------------------------------------------
# Optional: Post Comment to GitHub Issue
# ------------------------------------------------------------------------------
if [[ "$POST_COMMENT" == true ]]; then
    echo ""
    echo "🚀 Posting evidence report to GitHub Issue #${ISSUE_NUM}..."
    gh issue comment "$ISSUE_NUM" --repo juancruzmunozalbelo/oxide-studio --body-file "$EVIDENCE_FILE"
    echo "✓ Successfully posted evidence to https://github.com/juancruzmunozalbelo/oxide-studio/issues/${ISSUE_NUM}"
fi

echo ""
echo "======================================================================"
echo "🎉 All 3 Pre-Closure DoD Gates PASSED (${TOTAL_PASSED} tests total)!"
echo "======================================================================"
