# ADR-0115 MVP scope

> **The MVP cut delivers an app that can open a local vault on disk,
> navigate its notes, and render a note in the editor.**  Everything
> not on this list is post-MVP.

## In scope

1. **Launch the app** — single window, native menu (already works
   after Phase 1).
2. **Point at a local vault** — `tolaria --vault <path>` CLI arg, OR
   a "Open Vault…" file-dialog action (whichever is cheaper to wire).
3. **List notes in the sidebar / center pane** — `sidebar_panel` and
   `note_list_pane` already exist (Phase 2d); MVP-swap their
   data source from `mock_fixtures::MockVault` to the real `vault`
   service.
4. **Open a note** — clicking a note in the list creates a
   `note_item` in the center `Pane` that spawns a per-note
   `WKWebView` and loads its content (Phase 4 work).
5. **Render the note** — editor-host Vite project (BlockNote +
   CodeMirror per ADR-0022) loads the note via the bridge.
6. **Save the note** — `Cmd+S` → JS bridge → `vault::save_note(id,
   content)` → file written to disk.

That's it.  Everything below is deferred.

## Out of scope for MVP

| Feature | Why deferred |
|---------|--------------|
| Git operations (autogit, status, commit, push, conflict resolver, history) | Adds a whole `git_provider` service + `inspector_panel` GitHistory wiring + `commit` dialog.  Users can run `git` outside the app for MVP. |
| Full-text search | `search_panel` exists with mock data; real `vault_search` is a separate service.  Sidebar/list filtering covers basic find-in-vault. |
| AI panel | Optional for read/write/navigate.  `ai_panel` renders mock data already; real `cli_agents` + `mcp_bridge` are a substantial service constellation. |
| Settings UI persistence beyond defaults | `settings_panel` renders the 5 tabs but writes nothing to disk; defer the wiring. |
| Multi-tab `Pane` UX | Single open note is enough for MVP.  Phase 4 ships single-tab; multi-tab is a UX-spec follow-up. |
| Command palette / quick open / dialogs / wikilink combobox / image lightbox / emoji picker / startup screen | Phase 2e chrome — useful but not needed for the navigate-and-render flow. |
| `gpui-component` removal eval | Independent decision; runs once MVP is shipped, before cross-platform expansion. |
| Telemetry, app updater, localization | Production-readiness, not MVP-readiness. |
| Windows / Linux | macOS-only per ADR-0115 §8.  Cross-platform after MVP stabilizes. |
| Native-GPUI editor body | ADR-0115 commits to WKWebView for the editor; native editor R&D is post-cutover R&D. |

## What "MVP shipped" looks like

- `cargo run -p tolaria -- --vault ~/Documents/MyNotes` opens the app.
- The sidebar shows note types + folder tree derived from
  `~/Documents/MyNotes/`.
- The note list shows real notes; clicking one opens it in the
  center pane.
- The note renders in the embedded WKWebView, edits flow into the
  buffer, `Cmd+S` writes the file back to disk.
- The Tauri `src-tauri/` tree is **still untouched**.  MVP runs in
  parallel with the legacy app and does not replace it yet.

The cut-over (delete `src-tauri/`, prune `src/`, rewire signing,
flip superseded ADRs) is post-MVP.

## What MVP needs that we don't have yet

| Need | New crate / change | Effort |
|------|---------------------|--------|
| Real vault service | `vault` crate (open dir, list, read, save, basic watcher).  Replaces `mock_fixtures::MockVault` shape for shape. | medium |
| Editor host | `editor_host/` Vite project (carry-over from `src/` per ADR-0115 §5) | medium |
| Editor bridge | `editor_bridge` crate (JSON envelope, `wry::with_ipc_handler` + `evaluate_script`) | medium |
| Per-note WKWebView pane | `note_item` crate implementing `workspace::Item`, hosting `gpui-wry::WebView` | medium-large (Phase 4 §4 of ADR-0115) |
| Vault-on-launch wiring | tolaria binary: `--vault <path>` arg, install `vault::Vault` global when provided | small |
| Sidebar / note_list service swap | Re-point `sidebar_panel::from_or_empty` and `note_list_pane::from_or_empty` at the new `vault::Vault` global instead of `MockVault` | small |

See `roadmap.md` for how those line up into phases.
