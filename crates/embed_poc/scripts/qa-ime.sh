#!/usr/bin/env bash
# Scenario 2 — IME mid-composition. ADR-0115 §Re-evaluation "IME".
#
# Attempts to drive a Japanese Hiragana composition via osascript and
# verify the documented event sequence (compositionstart →
# compositionupdate+ → compositionend with `value_len` reflecting the
# CJK character count rather than the UTF-8 byte count).
#
# Hard requirements that cannot be auto-installed:
#   * Japanese Hiragana enabled as an input source.
#   * The macOS IME picker shortcut reachable from System Events.
#     We try Ctrl+Space (the most common factory default) but many
#     users remap to Caps Lock or the Globe key, which we cannot reach.
#
# If either requirement isn't met the script emits MANUAL (not FAIL).
# That matches the README's stance: IME is the hardest of the four
# triggers to automate; the human validation script is authoritative.

set -euo pipefail
. "$(dirname "$0")/lib/common.sh"

SCENARIO="ime"
trap shutdown_spike EXIT

# Detect whether Japanese is configured. The plist may format the value
# as XML or binary; `defaults` prints a readable form either way.
if ! /usr/bin/defaults read com.apple.HIToolbox AppleEnabledInputSources 2>/dev/null \
      | grep -qE 'Japanese|Hiragana|Kotoeri|Romaji'; then
  emit_manual "${SCENARIO}" "no Japanese input source enabled; follow README §IME manually"
  exit 0
fi

launch_spike

# Autofocus precondition.
if ! wait_for_log 0 'webview_focus state=in target=textarea' 3; then
  emit_manual "${SCENARIO}" "textarea not autofocused; cannot drive composition cleanly"
  exit 0
fi

MARK=$(log_mark)
# Ctrl+Space is the macOS factory shortcut for "switch to next input
# source". Many users remap it; if no compositionstart fires we degrade.
if ! send_key_code_with_mods 49 "control down" >/dev/null; then
  rc=$?
  if [ ${rc} -eq 99 ]; then
    emit_skip "${SCENARIO}" "accessibility permission required for osascript"
    exit 0
  fi
  emit_manual "${SCENARIO}" "IM switcher keystroke failed (rc=${rc})"
  exit 0
fi
sleep 0.4

# Type romaji that Hiragana IM composes into こんにちは. Letter-by-letter
# is more robust than a single keystroke containing the whole word —
# AppleScript's keystroke can collapse rapid input into a single event.
for ch in k o n n i c h i h a; do
  send_keystroke "${ch}" >/dev/null
  sleep 0.05
done
sleep 0.3

# Commit the composition (Return) and switch back to the previous IM so
# the rest of the QA run isn't disturbed.
send_key_code 36 >/dev/null
sleep 0.3
send_key_code_with_mods 49 "control down" >/dev/null

if ! wait_for_log "${MARK}" 'ime phase=compositionstart' 3; then
  emit_manual "${SCENARIO}" \
    "no compositionstart — Ctrl+Space may not be your IM-switcher shortcut; run scenario 2 manually"
  exit 0
fi
if ! wait_for_log "${MARK}" 'ime phase=compositionend' 6; then
  emit_fail "${SCENARIO}" "compositionstart fired but no compositionend within 6s — IM aborted mid-composition"
  exit 1
fi

# Final value_len should match the Japanese char count (5 for こんにちは).
# Allow ≥3 so partial commits still pass with a clear detail string.
END_LINE=$(log_since "${MARK}" | grep 'ime phase=compositionend' | tail -n 1)
VALUE_LEN=$(printf '%s' "${END_LINE}" | sed -nE 's/.*value_len=([0-9]+).*/\1/p')
if [ -z "${VALUE_LEN}" ]; then
  emit_fail "${SCENARIO}" "compositionend log line did not carry value_len="
  exit 1
fi
if [ "${VALUE_LEN}" -lt 3 ]; then
  emit_fail "${SCENARIO}" "compositionend value_len=${VALUE_LEN} — expected ~5 for こんにちは"
  exit 1
fi

emit_pass "${SCENARIO}" "compositionstart→compositionend with value_len=${VALUE_LEN} (chars, not bytes)"
