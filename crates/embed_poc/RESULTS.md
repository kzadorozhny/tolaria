# `embed_poc` validation results

Fill this in after running the validation script in `README.md`. One row
per ADR-0115 §9 goal; mark PASS / FAIL / N-A and attach a short note
with the relevant stdout snippet on FAIL.

## Environment

- macOS version:
- Xcode / Command Line Tools version:
- Spike commit (`git rev-parse HEAD`):
- Input source used for IME test:
- Tester:
- Date:

## Results

| # | Goal                                                              | Verdict | Notes / stdout excerpt |
| - | ----------------------------------------------------------------- | ------- | ---------------------- |
| 1 | WKWebView focus handoff (ADR-0115 §4 / §9)                        |         |                        |
| 2 | IME mid-composition survives chrome activity (ADR-0115 §4 / §9)   |         |                        |
| 3 | Frame sync during sidebar + window drag (ADR-0115 §4 / §9)        |         |                        |
| 4 | Cmd+S delivery while WKWebView holds focus (ADR-0115 §6 / §9)     |         |                        |

## Findings flagged for ADR re-evaluation

(Reserved for any FAIL that would re-open the native-GPUI-editor
alternative in ADR-0115's *Alternatives considered*.)

## Notes on known limitations encountered

(See "Known limitations" in `README.md`; record any you hit during this
pass so the next tester knows what to expect.)

## Automated QA run — 2026-05-17 16:04:03 PDT

Driver: `crates/embed_poc/scripts/qa.sh` · log: `/tmp/embed_poc-qa.log`

| Status | Scenario | Detail |
| --- | --- | --- |
| FAIL | focus | osascript failed sending Tab (rc=0) |
| MANUAL | ime | no Japanese input source enabled; follow README §IME manually |
| FAIL | frame-sync | osascript failed driving window resize (rc=0) |
| FAIL | cmd-s | osascript failed sending Cmd+S (rc=0) |

Aggregate: 0 PASS, 3 FAIL, 0 SKIP, 1 MANUAL

Manual checks the README still requires regardless of this run:
* Sidebar↔webview focus boundary clicks (scenario 1)
* Sidebar splitter drag while a composition is in flight (scenario 2 + 3 combined)

## Automated QA run — 2026-05-17 16:04:49 PDT

Driver: `crates/embed_poc/scripts/qa.sh` · log: `/tmp/embed_poc-qa.log`

| Status | Scenario | Detail |
| --- | --- | --- |
| FAIL | focus | no blur(textarea) after Tab — focus stuck on textarea |
| MANUAL | ime | no Japanese input source enabled; follow README §IME manually |
| FAIL | frame-sync | no frame_event kind=window_resize after osascript resize — observer not firing |
| PASS | cmd-s | Cmd+S routed to NSMenu while webview held focus; Cmd+A/C left intact |

Aggregate: 1 PASS, 2 FAIL, 0 SKIP, 1 MANUAL

Manual checks the README still requires regardless of this run:
* Sidebar↔webview focus boundary clicks (scenario 1)
* Sidebar splitter drag while a composition is in flight (scenario 2 + 3 combined)
