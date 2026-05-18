# Evaluation — remove `gpui-component` dependency

**Status:** **scheduled** — runs after Phase 2e (remaining chrome
surfaces) completes and **before** Phase 3 (services migration).
The eval may produce follow-on implementation work (vendor / replace
some primitives) that happens in the same gate window before
services migration begins; this lets us lock the chrome's primitive
contract before we plumb live services through it.

The original trigger conditions are preserved as **escalation
triggers** that would move the eval earlier in the timeline if hit
mid-Phase-2: (a) `gpui-component` ships a 1.0 with stable API
contracts, OR (b) an upgrade breaks Tolaria and the cost-to-fix
exceeds the cost-to-replace, OR (c) the primitive-gap tally crosses
~50% (i.e., we're already building most primitives ourselves).

## Why consider this

ADR-0115 §7 pins `gpui-component` to upstream HEAD (`a5268cd`)
because v0.5.1 lacks the `crates/webview/` (`gpui-wry`) we need and
v0.5.2 isn't tagged yet.  The pin is the chief stability gamble:

- **Pre-1.0 API churn risk** is the #1 risk in ADR-0115 §Consequences.
- We don't control the release cadence; a refactor upstream forces a
  Tolaria-side migration on their timeline.
- 16/26 primitives are direct fits per ADR-0115 §7; the other 10 are
  partial-fit or gap, meaning we already build some primitives
  ourselves in `crates/ui`.  The math gets less favorable as gaps
  grow.

The opposite case — keep `gpui-component` — is also reasonable:
they're an active, well-maintained upstream with a strong shadcn-style
primitive set, and the alternative (building everything ourselves)
multiplies our chrome workload.

## Current usage snapshot

(Run `grep -rlE "gpui_component" crates/ --include='*.rs'` to refresh.)

- **17 source files** under `crates/` import or reference `gpui_component`.
- **16 Cargo.toml files** declare it as a dependency.
- **6 concrete primitive types** in active use:
  - `gpui_component::alert::Alert` — `banners`
  - `gpui_component::button::Button` — `breadcrumb_bar`, `status_bar`,
    `settings_panel`, `sidebar_panel`, `ai_panel`, `inspector_panel`,
    `workspace::mock_note_item`, `embed_poc::layout`
  - `gpui_component::input::InputState` — `ui::picker`,
    `search_panel`, `note_list_pane`, `ai_panel`
  - `gpui_component::notification::Notification` /
    `NotificationType` — `toasts`
  - `gpui_component::theme::Theme` — `theme` (the global wrapper)
- **Traits in use**: `ActiveTheme`, `ButtonVariants`, `FluentBuilder`,
  `Sizable` (variant), `Disableable`, etc.
- **Layout primitives**: `gpui_component::resizable::h_resizable` /
  `v_resizable` / `resizable_panel` — `workspace::workspace`,
  `embed_poc::layout`.
- **Init**: `gpui_component::init(cx)` is the Theme global bootstrap
  in `theme::init`.

## Replacement options

1. **In-house build under `crates/ui`** — port the 6 primitives + 2
   layout helpers as native gpui views.  Estimated 800–1500 LOC across
   8 small files.  We already vendor `Picker` from Zed (~495 LOC) and
   have a placeholder `ui` crate; the convention exists.
2. **Vendor a snapshot** — copy `gpui-component`'s used modules into
   `crates/ui/vendor/` under their Apache-2.0 license, sever the git
   dep.  Fast (~1 day), preserves behavior, decouples us from
   upstream releases.  Cost: we maintain the vendor on bugfixes.
3. **Replace selectively** — keep `gpui-component` for the rich
   primitives (Sidebar, Tree, Calendar, DatePicker, Tab, Accordion,
   Resizable) and migrate only the simple ones (Button, Input, Alert,
   Notification) to in-house.  Half-measure with the highest combined
   cost.

## Evaluation deliverable

When this work is scheduled, the eval pass produces:

1. **Refreshed usage inventory** — counts above, plus depth-of-use per
   primitive (how many call sites, how customized).
2. **Per-primitive replacement difficulty matrix** — Button (trivial),
   Input (medium), Alert (trivial), Notification (medium), Theme
   (involved — global + observer plumbing), Resizable (involved —
   drag state).
3. **Vendor-vs-rebuild recommendation** with a rough LOC + time
   estimate.
4. **Trigger criteria** — concrete conditions under which we'd
   execute (e.g., "next breaking gpui-component release that requires
   >4 hours of migration work, OR adoption of any primitive whose
   gpui-component implementation forces a workaround on our side").
5. **Migration order** if we proceed — which primitives flip first,
   how to keep the workspace building through the swap.

## Timeline & decision criteria

**Scheduled slot:** post-Phase-2e, pre-Phase-3.  The eval is timeboxed
to a single pass that produces a recommendation; any resulting
replacement work runs in the same window before services migration
starts.

**Decision matrix at the scheduled slot:**

| Outcome | Trigger | Follow-on work |
|---------|---------|----------------|
| Keep `gpui-component` as-is | All 6 primitives still fit, no upstream churn since Phase 2 kickoff, no breaking-change PRs queued | None — proceed to Phase 3 |
| Pin a sha snapshot (defer) | Upstream stable enough for v1 but we want isolation for the services migration's stability window | Re-pin Cargo.toml to current sha; document in ADR-0115 supplement |
| Vendor snapshot under `crates/ui/vendor/` | Active churn upstream + we want full control through services + cut-over | ~1 day port, license preservation, snapshot doc |
| In-house rewrite under `crates/ui` | At least one upstream incompatibility or our customizations have outgrown the primitives | 2–4 days port, deprecate `gpui-component` dep across 16 Cargo.tomls |

**Escalation (move earlier than scheduled slot)** — trigger immediately
if any of these fire during Phase 2d–2e:

- We're already going to refactor a primitive's call site for an
  unrelated reason (e.g., Phase 4 editor host might want a custom
  Button) — saves a double-touch.
- `gpui-component` ships an upgrade that requires us to either pin
  back or migrate >4 hours of code.
- The 26-primitive coverage from ADR-0115 §7 drops below 50% as we
  add more chrome (i.e., we're already building primitives
  ourselves).
- We need to ship on a platform where `gpui-component`'s test/dev
  support is weaker than upstream gpui itself.

## Out of scope for this eval

- Replacing `gpui` itself (we're committed via ADR-0115).
- Replacing `gpui-wry` (`webview` crate); that's the only thing
  forcing us onto upstream HEAD and a separate question (Phase 4
  embedding strategy).
- Theme system overhaul; the Theme global wrapper is the most
  load-bearing piece and any replacement plan must keep its
  observer/init contract.
