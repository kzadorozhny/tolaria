Phase 10 — Behavioral layers.  **Opened 2026-05-22 from
`7beb12f1`.**  Scope set in
[`../../roadmap.md`](../../roadmap.md) §Phase 10; row-level ledger
in [`worklist.md`](worklist.md).

Phase 10 extracts the cross-cutting glue that Phase 8 inlined as
ad-hoc closures into named, `mock_fixtures`-compatible GPUI crates
so Phase 11 service expansion and Phase 12 modal chrome both
consume a stable layer instead of re-deriving slices of it.  Each
crate lands as its own commit.

**Scope adjustment vs. the original Phase 10 plan (made before this
phase opened):**

- `auto_git` and `telemetry_pipeline` moved to Phase 11 (rows 11.13
  + 11.14).  Rationale: both are behavioral wrappers around Phase 11
  services (`git_provider`, `telemetry`) — landing them adjacent to
  their underlying service is cleaner than ratcheting the wrapper
  ahead of the service it wraps.
- `10.1.1 WKWebView z-order fix` filed as the first blocker.
  Rationale: GPUI popovers / dropdowns / dialog stack currently
  render *behind* the embedded WKWebView editor body, which makes
  `dialog_stack` (10.4) impossible to deliver as a usable modal
  surface.  This row lands before any of the 10.1–10.5 crate work.

**In-scope rows:** 1 blocker (`10.1.1`) + 5 high-priority crate
extractions (`10.1`–`10.5`).  See [`worklist.md`](worklist.md) for
the bucketed ledger and per-row annotations.

**Out-of-scope (moved out before opening):** `auto_git`,
`telemetry_pipeline` — now Phase 11.13 / 11.14.
