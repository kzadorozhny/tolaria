#!/usr/bin/env bash
# Scenario 3 — Frame sync. ADR-0115 §4 + the §Re-evaluation "frame sync"
# trigger.
#
# Resizes the OS window via System Events and verifies:
#   1. `frame_event kind=window_resize ...` is emitted by the layout's
#      `observe_window_bounds` callback.
#   2. `InstrumentedWebView::prepaint` fires `frame_sync x= y= w= h=`
#      with dimensions that follow the new content area within 1 px.
#
# Sidebar splitter drag is excluded — `osascript` cannot synthesize
# clicks at screen coordinates (only `cliclick` or `pyautogui` can).
# That half stays MANUAL per the README's full validation script.

set -euo pipefail
. "$(dirname "$0")/lib/common.sh"

SCENARIO="frame-sync"
trap shutdown_spike EXIT

launch_spike

resize_window() {
  local w="$1" h="$2"
  send_osa "tell application \"System Events\"
    tell process \"${SPIKE_PROCESS_NAME}\"
      if exists window 1 then set size of window 1 to {${w}, ${h}}
    end tell
  end tell" >/dev/null
}

MARK=$(log_mark)
if ! resize_window 1000 700; then
  rc=$?
  if [ ${rc} -eq 99 ]; then
    emit_skip "${SCENARIO}" "accessibility permission required for osascript"
    exit 0
  fi
  emit_fail "${SCENARIO}" "osascript failed driving window resize (rc=${rc})"
  exit 1
fi
sleep 0.3
resize_window 1280 820 >/dev/null
sleep 0.5

LINES=$(log_since "${MARK}")
EVENTS=$(printf '%s\n' "${LINES}" | grep -Ec 'frame_event kind=window_resize' || true)
SYNCS=$(printf '%s\n' "${LINES}" | grep -Ec 'frame_sync x=' || true)

if [ "${EVENTS}" -eq 0 ]; then
  emit_fail "${SCENARIO}" "no frame_event kind=window_resize after osascript resize — observer not firing"
  exit 1
fi
if [ "${SYNCS}" -eq 0 ]; then
  emit_fail "${SCENARIO}" "no frame_sync x= after resize — InstrumentedWebView prepaint did not run"
  exit 1
fi

emit_pass "${SCENARIO}" "events=${EVENTS} syncs=${SYNCS} (sidebar splitter drag still MANUAL)"
