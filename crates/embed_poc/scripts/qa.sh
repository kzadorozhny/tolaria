#!/usr/bin/env bash
# Phase 0 spike — automated QA driver. Runs all four ADR-0115
# re-evaluation triggers against a single spike instance and appends
# the verdict to crates/embed_poc/RESULTS.md.
#
# Pre-reqs:
#   * macOS with Accessibility *and* Automation (System Events) granted
#     to the terminal you run this from.
#   * For scenario 2 (IME): Japanese Hiragana enabled as an input source,
#     and Ctrl+Space mapped to "switch to next input source" (the macOS
#     factory default). Otherwise scenario 2 degrades to MANUAL.
#
# Non-pre-reqs that the README still requires you to walk through:
#   * Sidebar↔webview boundary clicks (focus scenario half).
#   * Sidebar splitter drag (frame-sync scenario half).
# These need real mouse coordinates that osascript cannot synthesize.

set -uo pipefail
. "$(dirname "$0")/lib/common.sh"

trap shutdown_spike EXIT

launch_spike || exit $?

SCENARIOS=(
  qa-focus.sh
  qa-ime.sh
  qa-frame-sync.sh
  qa-cmd-s.sh
)

RESULTS=()
for script in "${SCENARIOS[@]}"; do
  printf '%s--- %s ---%s\n' "${C_DIM}" "${script}" "${C_OFF}"
  set +e
  output=$(EMBED_POC_PID="${EMBED_POC_PID}" STARTED_SPIKE="" \
           "${SCRIPTS_DIR}/${script}" 2>&1)
  rc=$?
  set -e
  printf '%s\n' "${output}"
  # The result line is the last non-blank line of the script's output.
  line=$(printf '%s\n' "${output}" | awk 'NF{last=$0} END{print last}')
  RESULTS+=("${line}")
  if [ "${rc}" -ne 0 ] && ! grep -qE '^(\x1b\[[0-9;]+m)?(PASS|FAIL|SKIP|MANUAL)' <<<"${line}"; then
    # Script crashed before emitting a verdict — record an explicit FAIL.
    RESULTS[-1]=$(printf 'FAIL\t%s\tscript exited non-zero without verdict (rc=%s)' \
                  "${script%.sh}" "${rc}")
  fi
  sleep 0.3
done

printf '\n%s=== summary ===%s\n' "${C_DIM}" "${C_OFF}"
for r in "${RESULTS[@]}"; do printf '%s\n' "${r}"; done

# Compute aggregate verdict + tally.
PASS=0; FAIL=0; SKIP=0; MANUAL=0
for r in "${RESULTS[@]}"; do
  case "${r}" in
    *PASS*)   PASS=$((PASS+1));;
    *FAIL*)   FAIL=$((FAIL+1));;
    *SKIP*)   SKIP=$((SKIP+1));;
    *MANUAL*) MANUAL=$((MANUAL+1));;
  esac
done
printf '\npass=%d fail=%d skip=%d manual=%d\n' "${PASS}" "${FAIL}" "${SKIP}" "${MANUAL}"

# Append a section to RESULTS.md so the ADR re-eval has an audit trail.
RESULTS_FILE="${CRATE_DIR}/RESULTS.md"
{
  printf '\n## Automated QA run — %s\n\n' "$(date '+%Y-%m-%d %H:%M:%S %Z')"
  printf 'Driver: `crates/embed_poc/scripts/qa.sh` · log: `%s`\n\n' "${QA_LOG}"
  printf '| Status | Scenario | Detail |\n'
  printf '| --- | --- | --- |\n'
  for r in "${RESULTS[@]}"; do
    # Strip ANSI colors and turn tab-separated into table cells.
    clean=$(printf '%s' "${r}" | sed -E $'s/\x1b\\[[0-9;]+m//g')
    status=$(printf '%s' "${clean}" | awk -F'\t' '{print $1}')
    scen=$(printf '%s'   "${clean}" | awk -F'\t' '{print $2}')
    detail=$(printf '%s' "${clean}" | awk -F'\t' '{for(i=3;i<=NF;i++){printf "%s",$i; if(i<NF)printf " "}}')
    printf '| %s | %s | %s |\n' "${status:-?}" "${scen:-?}" "${detail:-—}"
  done
  printf '\nAggregate: %d PASS, %d FAIL, %d SKIP, %d MANUAL\n' \
         "${PASS}" "${FAIL}" "${SKIP}" "${MANUAL}"
  printf '\nManual checks the README still requires regardless of this run:\n'
  printf '* Sidebar↔webview focus boundary clicks (scenario 1)\n'
  printf '* Sidebar splitter drag while a composition is in flight (scenario 2 + 3 combined)\n'
} >>"${RESULTS_FILE}"

# Exit code mirrors fail count so CI / agents can branch on it.
if [ "${FAIL}" -gt 0 ]; then exit 1; fi
exit 0
