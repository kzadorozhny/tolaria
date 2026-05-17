#!/usr/bin/env bash
# Shared helpers for the embed_poc QA scripts. Sourced by qa.sh and the
# four per-scenario qa-*.sh scripts. Provides launch / shutdown,
# stdout-log mark/grep helpers, and a `send_keystroke` wrapper that
# surfaces the accessibility-permission failure with a useful hint
# instead of letting `osascript` error opaquely.

set -euo pipefail

# Resolve paths relative to *this* file so the scripts work from any cwd.
_COMMON_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
SCRIPTS_DIR="$( cd "${_COMMON_DIR}/.." && pwd )"
CRATE_DIR="$( cd "${SCRIPTS_DIR}/.." && pwd )"
REPO_ROOT="$( cd "${CRATE_DIR}/../.." && pwd )"

QA_LOG="${QA_LOG:-/tmp/embed_poc-qa.log}"
SPIKE_PROCESS_NAME="${SPIKE_PROCESS_NAME:-embed_poc}"
SPIKE_BIN="${SPIKE_BIN:-${REPO_ROOT}/target/debug/embed_poc}"
SPIKE_WAIT_READY_SECS="${SPIKE_WAIT_READY_SECS:-15}"

if [ -t 1 ]; then
  C_PASS=$'\033[32m'; C_FAIL=$'\033[31m'; C_SKIP=$'\033[33m'
  C_DIM=$'\033[2m';  C_OFF=$'\033[0m'
else
  C_PASS=''; C_FAIL=''; C_SKIP=''; C_DIM=''; C_OFF=''
fi

log_mark() {
  if [ -f "${QA_LOG}" ]; then wc -l <"${QA_LOG}" | tr -d ' '; else echo 0; fi
}

log_since() {
  local mark="$1"
  if [ -f "${QA_LOG}" ]; then
    awk -v from="$((mark + 1))" 'NR>=from' "${QA_LOG}"
  fi
}

# Poll `log_since "$MARK"` for $PATTERN every 100 ms up to $TIMEOUT seconds.
wait_for_log() {
  local mark="$1" pattern="$2" timeout="${3:-3}"
  local start now
  start=$(date +%s)
  while :; do
    if log_since "${mark}" | grep -Eq "${pattern}"; then return 0; fi
    now=$(date +%s)
    if (( now - start >= timeout )); then return 1; fi
    sleep 0.1
  done
}

# Wrap osascript so accessibility-blocked keystrokes return a clear hint
# instead of dumping the raw `(-1002)` error.
send_osa() {
  local script="$1" err rc
  set +e
  err=$(/usr/bin/osascript -e "${script}" 2>&1)
  rc=$?
  set -e
  if [ ${rc} -ne 0 ]; then
    if [[ "${err}" == *"not allowed"* ]] || [[ "${err}" == *"-1002"* ]] || [[ "${err}" == *"-25211"* ]]; then
      cat >&2 <<EOF
${C_FAIL}osascript blocked by macOS Accessibility/Automation permission.${C_OFF}
Grant Accessibility *and* Automation access to the terminal running this script:
  System Settings → Privacy & Security → Accessibility → enable your terminal
  System Settings → Privacy & Security → Automation → enable "System Events"
EOF
      return 99
    fi
    printf '%s\n' "${err}" >&2
    return ${rc}
  fi
  printf '%s' "${err}"
}

send_keystroke() {
  send_osa "tell application \"System Events\" to keystroke \"$1\""
}

send_keystroke_with_mods() {
  send_osa "tell application \"System Events\" to keystroke \"$1\" using $2"
}

send_key_code() {
  send_osa "tell application \"System Events\" to key code $1"
}

send_key_code_with_mods() {
  send_osa "tell application \"System Events\" to key code $1 using $2"
}

activate_spike() {
  send_osa "tell application \"System Events\"
    if exists (process \"${SPIKE_PROCESS_NAME}\") then
      set frontmost of process \"${SPIKE_PROCESS_NAME}\" to true
    end if
  end tell" >/dev/null
}

ensure_built() {
  if [ ! -x "${SPIKE_BIN}" ]; then
    (cd "${REPO_ROOT}" && cargo build -p embed_poc >&2)
  fi
}

launch_spike() {
  # If the caller already started the spike (qa.sh exports
  # EMBED_POC_PID into the child scripts), don't manage its lifecycle.
  if [ -n "${EMBED_POC_PID:-}" ] && kill -0 "${EMBED_POC_PID}" 2>/dev/null; then
    return 0
  fi
  ensure_built
  : >"${QA_LOG}"
  RUST_LOG=embed_poc=debug "${SPIKE_BIN}" >>"${QA_LOG}" 2>&1 &
  EMBED_POC_PID=$!
  export EMBED_POC_PID
  STARTED_SPIKE=1
  export STARTED_SPIKE
  if ! wait_for_log 0 '(embed_poc starting|embed_poc::macos)' "${SPIKE_WAIT_READY_SECS}"; then
    echo "${C_FAIL}spike did not log a startup banner within ${SPIKE_WAIT_READY_SECS}s${C_OFF}" >&2
    sed 's/^/  /' "${QA_LOG}" >&2 || true
    return 1
  fi
  sleep 0.5
  activate_spike
  sleep 0.3
}

shutdown_spike() {
  if [ "${STARTED_SPIKE:-0}" = "1" ] && [ -n "${EMBED_POC_PID:-}" ] \
     && kill -0 "${EMBED_POC_PID}" 2>/dev/null; then
    kill "${EMBED_POC_PID}" 2>/dev/null || true
    wait "${EMBED_POC_PID}" 2>/dev/null || true
  fi
}

# Scenario scripts emit exactly one tab-separated result line. The
# driver (qa.sh) `tail -n 1`s the output to extract it.
emit_pass()   { printf '%sPASS%s\t%s\t%s\n'   "${C_PASS}" "${C_OFF}" "$1" "${2:-}"; }
emit_fail()   { printf '%sFAIL%s\t%s\t%s\n'   "${C_FAIL}" "${C_OFF}" "$1" "${2:-}"; }
emit_skip()   { printf '%sSKIP%s\t%s\t%s\n'   "${C_SKIP}" "${C_OFF}" "$1" "${2:-}"; }
emit_manual() { printf '%sMANUAL%s\t%s\t%s\n' "${C_SKIP}" "${C_OFF}" "$1" "${2:-}"; }
