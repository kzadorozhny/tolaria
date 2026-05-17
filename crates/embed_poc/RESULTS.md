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
