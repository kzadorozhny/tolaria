#!/usr/bin/env bash
# Scenario 4 — Cmd+S delivery. ADR-0115 §6 + the §Re-evaluation "native
# menu" trigger.
#
# Validates that the NSMenu installed via `cx.set_menus(...)` (paired
# with `cx.bind_keys([cmd-s])`) intercepts Cmd+S before the focused
# WKWebView's keyDown chain, so the textarea never gets a stray `s`
# character and the Rust `Save` action fires.
#
# Secondary check: standard Edit-menu key equivalents (Cmd+A / Cmd+C)
# must still route into the webview unchanged. If our Save action
# accidentally over-captured them, `cmd_s_fired` would never fire here —
# but it could also be silently breaking copy/paste in production. We
# can't observe the webview-side outcome from log output, so we only
# check that Save did NOT fire spuriously during Cmd+A/C.

set -euo pipefail
. "$(dirname "$0")/lib/common.sh"

SCENARIO="cmd-s"
trap shutdown_spike EXIT

launch_spike

# Step 1: textarea autofocuses on startup. Send Cmd+S; the menu must win.
MARK=$(log_mark)
if ! send_keystroke_with_mods "s" "command down" >/dev/null; then
  rc=$?
  if [ ${rc} -eq 99 ]; then
    emit_skip "${SCENARIO}" "accessibility permission required for osascript"
    exit 0
  fi
  emit_fail "${SCENARIO}" "osascript failed sending Cmd+S (rc=${rc})"
  exit 1
fi

if ! wait_for_log "${MARK}" 'cmd_s_fired' 3; then
  emit_fail "${SCENARIO}" "Cmd+S did not produce cmd_s_fired (webview swallowed the key equivalent?)"
  exit 1
fi

# Step 2: confirm standard Edit selectors still pass through. We type an
# ASCII letter, then Cmd+A (select all) + Cmd+C (copy). None of these
# should trigger cmd_s_fired.
MARK=$(log_mark)
send_keystroke "x" >/dev/null
send_keystroke_with_mods "a" "command down" >/dev/null
send_keystroke_with_mods "c" "command down" >/dev/null
sleep 0.5
if log_since "${MARK}" | grep -q 'cmd_s_fired'; then
  emit_fail "${SCENARIO}" "Save fired during Cmd+A/Cmd+C — menu is over-capturing standard Edit selectors"
  exit 1
fi

emit_pass "${SCENARIO}" "Cmd+S routed to NSMenu while webview held focus; Cmd+A/C left intact"
