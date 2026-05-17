#!/usr/bin/env bash
# Scenario 1 — WKWebView focus handoff. ADR-0115 §Re-evaluation "focus".
#
# The test page autofocuses the textarea on load, so the very first
# `webview_focus state=in target=textarea` line proves the JS bridge is
# reporting focus events. We then Tab through the form controls and
# verify the documented blur/focus pair on each hop.
#
# The README's "click sidebar → click textarea → click input → click
# sidebar" walk requires real mouse coordinates — `osascript` cannot
# synthesize them. Boundary clicks between the GPUI sidebar and the
# WKWebView therefore stay MANUAL; this script covers the
# webview-internal half deterministically.

set -euo pipefail
. "$(dirname "$0")/lib/common.sh"

SCENARIO="focus"
trap shutdown_spike EXIT

launch_spike

# Autofocus precondition.
if ! wait_for_log 0 'webview_focus state=in target=textarea' 3; then
  emit_fail "${SCENARIO}" "no autofocus webview_focus state=in target=textarea after startup"
  exit 1
fi

# Tab off the textarea → expect blur(textarea) + focus(single-line).
MARK=$(log_mark)
if ! send_key_code 48 >/dev/null; then  # 48 = Tab
  rc=$?
  if [ ${rc} -eq 99 ]; then
    emit_skip "${SCENARIO}" "accessibility permission required for osascript"
    exit 0
  fi
  emit_fail "${SCENARIO}" "osascript failed sending Tab (rc=${rc})"
  exit 1
fi
if ! wait_for_log "${MARK}" 'webview_focus state=out target=textarea' 2; then
  emit_fail "${SCENARIO}" "no blur(textarea) after Tab — focus stuck on textarea"
  exit 1
fi
if ! wait_for_log "${MARK}" 'webview_focus state=in target=single-line' 2; then
  emit_fail "${SCENARIO}" "no focus(single-line) after Tab from textarea"
  exit 1
fi

# Tab again → blur(single-line). Per README known-limitations the button
# does not emit a focus(in) event, only the blur of the previous control.
MARK=$(log_mark)
send_key_code 48 >/dev/null
if ! wait_for_log "${MARK}" 'webview_focus state=out target=single-line' 2; then
  emit_fail "${SCENARIO}" "no blur(single-line) after Tab — focus stuck on single-line"
  exit 1
fi

# Shift+Tab back to the textarea → expect focus(in target=textarea) again.
MARK=$(log_mark)
send_key_code_with_mods 48 "shift down" >/dev/null
send_key_code_with_mods 48 "shift down" >/dev/null
if ! wait_for_log "${MARK}" 'webview_focus state=in target=textarea' 2; then
  emit_fail "${SCENARIO}" "Shift+Tab back to textarea did not refocus it"
  exit 1
fi

emit_pass "${SCENARIO}" "webview-internal Tab traversal ok (sidebar↔webview clicks still MANUAL)"
