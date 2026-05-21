#![forbid(unsafe_code)]
//! Vault service for Tolaria.
//!
//! Provides read / list / save over a directory of Markdown files.  The
//! public API mirrors [`mock_fixtures::MockVault`] so chrome panels can be
//! swapped with minimal call-site changes (Phase 5-MVP wires the swap into
//! `sidebar_panel` / `note_list_pane` / the `tolaria` binary).
//!
//! # Phase 8.11 capabilities
//!
//! - **Background IO** — [`Vault::open_at_async`] runs the initial scan
//!   on `cx.background_executor()`; once an executor is installed via
//!   [`Vault::set_executor`] every [`Vault::note_content`] read and
//!   [`Vault::save`] write dispatches off the foreground thread.
//! - **Frontmatter** — every [`Note`] carries a parsed
//!   [`Frontmatter`] populated during the directory scan.  See
//!   [`frontmatter::parse`] for the shape.
//! - **Folders + assets** — [`Vault::folders`] / [`Vault::assets`]
//!   expose vault-root-relative paths discovered during the scan,
//!   sorted lexicographically.
//! - **fs-watcher** — [`Vault::watch_events`] hands out a
//!   `flume::Receiver<VaultChanged>` that fires after a 200 ms
//!   debounce window for any create / modify / delete under the
//!   vault root.  Phase 9.6 (`vault_lifecycle`) consumes the
//!   receiver and routes invalidation back into [`Vault::rescan`].
//!
//! # Known limitations
//!
//! - **No symlink traversal**.  Symlinked directories are skipped
//!   during `rescan` to avoid loops.
//! - **Best-effort watcher**.  When the OS layer can't install the
//!   watcher (inotify quota exhausted, exotic platform), the vault
//!   still opens — the receiver simply stays silent.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result};
use chrono::{DateTime, Utc};
use gpui::{App, BackgroundExecutor, Global, SharedString, Task};
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub mod frontmatter;
pub mod watcher;
pub use frontmatter::{Frontmatter, FrontmatterValue};
pub use watcher::VaultChanged;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Stable identifier for a note within a single [`Vault`] instance.
///
/// IDs are monotonically increasing for the lifetime of the `Vault` and are
/// **never reused** even when a note is dropped (deleted on disk + rescan).
/// They are also **not persisted** — reopening the same vault from disk
/// restarts ID assignment at `0`.  Treat `NoteId` as ephemeral, valid for
/// the current process's `Vault` instance only.
///
/// Serialises as a bare integer (`7`, not `{"NoteId":7}`) so the
/// [`editor_bridge`](https://docs.rs/editor_bridge) wire format stays
/// numeric.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct NoteId(u64);

impl NoteId {
    /// Raw numeric value (for logging / display only — do not depend on the
    /// numeric layout being stable across `Vault` instances).
    #[must_use]
    pub fn get(self) -> u64 {
        self.0
    }

    /// Fabricate a [`NoteId`] from a raw integer.
    ///
    /// Production code obtains `NoteId`s exclusively from [`Vault`]'s
    /// constructors and rescan paths.  This constructor exists for two
    /// legitimate callers:
    ///
    /// 1. **`mock_fixtures::MockVault`** — needs to seed deterministic IDs
    ///    at startup so chrome panels populated from mocks compare equal
    ///    to those populated from a real `Vault`.
    /// 2. **Downstream test fixtures** in `editor_bridge` and `note_item`
    ///    — building a `Note` for a unit test without a real on-disk
    ///    vault.
    #[must_use]
    pub fn from_raw(n: u64) -> Self {
        Self(n)
    }
}

/// Coarse category of a vault entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoteKind {
    /// `.md` file — the only kind surfaced through [`Vault::notes`] for MVP.
    Markdown,
    /// Any other file (images, PDFs, …).  Reserved for Phase 8 when the
    /// asset tree lands.
    Asset,
    /// A subdirectory.  Reserved for Phase 8 when the folder tree lands.
    Folder,
}

/// Metadata for a single note.  The body is fetched lazily via
/// [`Vault::note_content`]; cache management belongs to the caller (the
/// `editor_host` crate in Phase 4-MVP).
///
/// `frontmatter` carries the parsed YAML block (Phase 8.11).  Empty for
/// notes that don't begin with `---\n…\n---\n`; populated during the
/// initial scan + on rescan so chrome surfaces can render properties
/// without re-reading the file.
#[derive(Debug, Clone)]
pub struct Note {
    pub id: NoteId,
    pub title: SharedString,
    pub path: PathBuf,
    pub kind: NoteKind,
    pub modified: DateTime<Utc>,
    pub byte_size: u64,
    pub frontmatter: Frontmatter,
}

impl Note {
    /// Parsed YAML frontmatter for this note.  Empty `Frontmatter` when
    /// the note doesn't begin with a `---\n…\n---\n` block.
    #[must_use]
    pub fn frontmatter(&self) -> &Frontmatter {
        &self.frontmatter
    }

    /// Shorthand for `self.frontmatter().favorite()` — true iff the
    /// note's `_favorite` key is literal `true`.  Wired to the
    /// note-toolbar star cell and the sidebar Favorites section
    /// (worklist 9.2.1).
    #[must_use]
    pub fn is_favorite(&self) -> bool {
        self.frontmatter.favorite()
    }

    /// Shorthand for `self.frontmatter().organized()` — true iff the
    /// note's `_organized` key is literal `true`.  Wired to the
    /// note-toolbar organized cell (worklist 9.2.2).
    #[must_use]
    pub fn is_organized(&self) -> bool {
        self.frontmatter.organized()
    }
}

/// Recoverable errors from vault operations.
#[derive(Debug, Error)]
pub enum VaultError {
    #[error("note {0:?} not found in vault")]
    NotFound(NoteId),
    #[error("io error reading {path:?}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    /// Errors surfaced by the post-create / post-mutation rescan.
    /// Preserves the original `anyhow` chain so callers can walk it
    /// instead of receiving a flattened string-coded `io::Error`.
    /// Currently only surfaced by [`Vault::create_note`].
    #[error("rescan failed: {0:#}")]
    Rescan(#[source] anyhow::Error),
}

// ---------------------------------------------------------------------------
// Vault
// ---------------------------------------------------------------------------

/// The Tolaria vault service.  Installed as a GPUI `Global` by the `tolaria`
/// binary after `--vault <path>` is processed (Phase 5-MVP).
pub struct Vault {
    root: PathBuf,
    notes: HashMap<NoteId, Note>,
    next_id: u64,
    /// Vault-root-relative paths of every directory below `root`,
    /// sorted lexicographically.  Populated by `rescan_internal`.
    folders: Vec<PathBuf>,
    /// Vault-root-relative paths of every non-markdown file (images,
    /// PDFs, plain catch-all).  Sorted lexicographically.  Populated by
    /// `rescan_internal`.
    assets: Vec<PathBuf>,
    /// Background executor used by [`note_content`] and [`save`] to
    /// move file IO off the foreground thread.  `None` for vaults
    /// constructed through the legacy sync test path — those fall back
    /// to inline synchronous IO via `Task::ready`.  Production builds
    /// (`tolaria::main`) call [`set_executor`] before installing the
    /// vault as a `Global` so chrome surfaces get true async reads.
    background_executor: Option<BackgroundExecutor>,
    /// Optional fs-watcher.  When present, `notify` events arriving on
    /// the watcher's dispatch thread are coalesced (200 ms debounce)
    /// and forwarded through `watch_tx`.  Subscribers obtain a
    /// receiver via [`watch_events`].  Dropped together with the
    /// `Vault` so no watcher thread / OS handle leaks.
    watcher: Option<watcher::VaultWatcher>,
    /// Sender end of the `VaultChanged` channel — held so the watcher
    /// thread keeps a valid receiver target for the lifetime of the
    /// vault.  The receiver side is exposed via [`watch_events`].
    watch_tx: flume::Sender<VaultChanged>,
    /// Receiver template — `flume::Receiver` is `Clone`, so handing
    /// out a clone lets each subscriber consume independently without
    /// stealing events from the others.
    watch_rx: flume::Receiver<VaultChanged>,
}

impl Global for Vault {}

/// Maximum number of `{stem}-N.md` suffixes [`Vault::create_note`] will
/// try before giving up.  A vault that already has 1000 untitled notes
/// is a pathological case; failing loud beats spinning forever.
const CREATE_NOTE_SUFFIX_LIMIT: u32 = 1000;

impl Vault {
    /// Open a vault rooted at `root`.  Walks the directory tree (depth limit
    /// 32), indexes every `.md` file, and returns a ready `Vault`.
    ///
    /// # Errors
    ///
    /// Returns an error if `root` cannot be canonicalised or read.
    pub fn open_at(root: impl AsRef<Path>) -> Result<Self> {
        let root = root
            .as_ref()
            .canonicalize()
            .with_context(|| format!("canonicalising vault root {:?}", root.as_ref()))?;
        let (watch_tx, watch_rx) = flume::unbounded::<VaultChanged>();
        let mut vault = Self {
            root,
            notes: HashMap::new(),
            next_id: 0,
            folders: Vec::new(),
            assets: Vec::new(),
            background_executor: None,
            watcher: None,
            watch_tx,
            watch_rx,
        };
        vault.rescan_internal()?;
        // Best-effort: a watcher failure (inotify quota, exotic
        // platform) shouldn't keep the vault from opening.  Log and
        // continue with `watcher = None`; subscribers will receive
        // no events but the rest of the vault stays functional.
        match watcher::VaultWatcher::spawn(&vault.root, vault.watch_tx.clone()) {
            Ok(watcher) => vault.watcher = Some(watcher),
            Err(err) => log::warn!("vault watcher disabled: {err:#}"),
        }
        log::info!(
            "opened vault at {:?} with {} note(s)",
            vault.root,
            vault.notes.len()
        );
        Ok(vault)
    }

    /// Open a vault asynchronously, running the directory scan on
    /// `cx.background_executor()` so the foreground thread isn't
    /// blocked for large vaults.  The returned [`Task`] resolves to
    /// the same shape as [`open_at`] — a ready [`Vault`] (already
    /// equipped with the same background executor for subsequent
    /// reads / saves) or an `anyhow::Error`.
    ///
    /// # Errors
    ///
    /// Resolves to an error if `root` cannot be canonicalised or
    /// scanned.
    pub fn open_at_async(root: PathBuf, cx: &App) -> Task<Result<Self>> {
        let executor = cx.background_executor().clone();
        let executor_for_vault = executor.clone();
        executor.spawn(async move {
            let mut vault = Self::open_at(&root)?;
            vault.set_executor(executor_for_vault);
            Ok(vault)
        })
    }

    /// Vault root directory (canonicalised).
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Subscribe to vault filesystem change events.
    ///
    /// Returns a `flume::Receiver` cloned from the vault's internal
    /// channel.  **Note** — `flume::Receiver` clones share a single
    /// queue (work-stealing semantics), so each `VaultChanged` is
    /// delivered to exactly one cloned receiver.  At the present
    /// scale we have a single subscriber (`note_list_pane`, worklist
    /// 9.2.12) per vault; when a second subscriber lands we'll need
    /// a fan-out adapter (e.g. spawn a dispatch task that owns the
    /// receiver and forwards into a `broadcast::Sender`).
    ///
    /// Events arrive from two sources: (a) the OS-level fs watcher
    /// (`watcher::VaultWatcher`, 200ms debounce) and (b) chrome-side
    /// frontmatter mutators (e.g. [`set_frontmatter_bool`]) that emit
    /// synchronously after touching the in-memory map.  Subscribers
    /// don't need to distinguish — both shapes carry the touched
    /// paths and the recommended response is the same (rebuild the
    /// cached projection).
    ///
    /// When the OS watcher couldn't be installed (e.g. exotic
    /// platform, inotify quota), only the chrome-side emissions
    /// arrive; subscribers should still treat the channel as a
    /// best-effort signal.
    ///
    /// [`set_frontmatter_bool`]: Self::set_frontmatter_bool
    #[must_use]
    pub fn watch_events(&self) -> flume::Receiver<VaultChanged> {
        self.watch_rx.clone()
    }

    /// Install a [`BackgroundExecutor`] so subsequent reads / saves
    /// dispatch onto the background thread pool.
    ///
    /// Idempotent — overrides any executor set by a previous call.
    /// `tolaria::main` calls this once at startup; tests typically
    /// omit it so the legacy `Task::ready` shape keeps drive-by unit
    /// tests deterministic.
    pub fn set_executor(&mut self, executor: BackgroundExecutor) {
        self.background_executor = Some(executor);
    }

    /// All note IDs in the vault.  Order is unspecified.
    pub fn notes(&self) -> Task<Vec<NoteId>> {
        Task::ready(self.note_ids_vec())
    }

    /// Internal sync helper backing both [`notes`] and the test accessor.
    /// Kept here so the two call sites can't drift apart.
    fn note_ids_vec(&self) -> Vec<NoteId> {
        self.notes.keys().copied().collect()
    }

    /// Metadata for a single note, or `None` if the ID is unknown.
    pub fn note(&self, id: NoteId) -> Task<Option<Note>> {
        Task::ready(self.notes.get(&id).cloned())
    }

    /// Synchronous reference to the in-memory metadata for a single
    /// note.  Cheap HashMap lookup with no `Task` indirection — the
    /// render path in `note_toolbar` reads `favorite()` / `organized()`
    /// once per paint, and a `Task<Option<Note>>` (which clones the
    /// `Note`) would be wasteful there.  Prefer this accessor when the
    /// caller already holds the GPUI foreground lock via a `&Vault`
    /// borrow.
    #[must_use]
    pub fn note_sync(&self, id: NoteId) -> Option<&Note> {
        self.notes.get(&id)
    }

    /// Synchronous iterator over every note's metadata.  Order is
    /// unspecified (HashMap iteration order).  Used by chrome surfaces
    /// that need to compute a derived view of the vault on every
    /// render — see `sidebar_panel::compute_favorites` (worklist
    /// 9.2.1).
    pub fn iter_notes(&self) -> impl Iterator<Item = &Note> {
        self.notes.values()
    }

    /// Read the on-disk body of a note.  When a background executor is
    /// installed (see [`set_executor`]) the file read happens on the
    /// background thread pool; otherwise it runs inline so legacy test
    /// call sites continue to work without a `TestAppContext`.
    pub fn note_content(&self, id: NoteId) -> Task<Result<String, VaultError>> {
        let Some(note) = self.notes.get(&id) else {
            return Task::ready(Err(VaultError::NotFound(id)));
        };
        let path = note.path.clone();
        match self.background_executor.as_ref() {
            Some(executor) => executor.spawn(async move { read_to_string(&path) }),
            None => Task::ready(read_to_string(&path)),
        }
    }

    /// Persist `content` to a note's on-disk path and refresh its modified
    /// timestamp + byte size.  When a background executor is installed
    /// the disk write runs on the background thread pool; otherwise it
    /// runs inline so legacy test call sites continue to work.
    ///
    /// In the async path the in-memory metadata refresh is deferred to
    /// the next [`rescan`] / fs-watcher tick — the disk write still
    /// completes atomically before the returned [`Task`] resolves.
    pub fn save(&mut self, id: NoteId, content: &str) -> Task<Result<(), VaultError>> {
        match self.background_executor.clone() {
            Some(executor) => {
                // Async path: schedule the write on the background
                // pool.  Metadata refresh is deferred (the next
                // rescan or the Phase 9.6 fs-watcher will pick up the
                // new mtime / size).
                let Some(note) = self.notes.get(&id) else {
                    return Task::ready(Err(VaultError::NotFound(id)));
                };
                let path = note.path.clone();
                let body = content.to_owned();
                executor.spawn(async move { write_to_disk(&path, &body) })
            }
            None => {
                // Sync path: write inline and refresh metadata
                // immediately so tests that round-trip `save → read`
                // see the new byte size without an extra rescan.
                Task::ready(self.save_sync(id, content))
            }
        }
    }

    /// Synchronous body of [`save`] — exposed under `#[cfg(test)]` so unit
    /// tests can assert against the `Result` directly without needing a
    /// `TestAppContext` to drive the `Task`.
    fn save_sync(&mut self, id: NoteId, content: &str) -> Result<(), VaultError> {
        let Some(note) = self.notes.get(&id) else {
            return Err(VaultError::NotFound(id));
        };
        let path = note.path.clone();
        std::fs::write(&path, content).map_err(|source| VaultError::Io {
            path: path.clone(),
            source,
        })?;
        // Refresh in-memory metadata.  IO failure here is non-fatal — the
        // write itself succeeded — but we log so divergence between memory
        // and disk doesn't go silent.
        if let Some(note) = self.notes.get_mut(&id) {
            match std::fs::metadata(&note.path) {
                Ok(meta) => {
                    note.byte_size = meta.len();
                    match meta.modified() {
                        Ok(t) => note.modified = DateTime::<Utc>::from(t),
                        Err(err) => log::warn!(
                            "vault::save: modified() unavailable for {:?}: {err}",
                            note.path,
                        ),
                    }
                }
                Err(err) => log::warn!(
                    "vault::save: metadata refresh failed for {:?}: {err}",
                    note.path,
                ),
            }
        }
        Ok(())
    }

    /// Toggle a boolean key (`_favorite`, `_organized`, …) inside the
    /// note's YAML frontmatter, preserving every byte outside the
    /// affected line.
    ///
    /// Semantics (matching the React handler):
    ///
    /// - `value == true` writes `{key}: true` — inserts a fresh
    ///   frontmatter block if the note had none, replaces the existing
    ///   line if the key was present, or appends a new line just
    ///   before the closing `---` otherwise.
    /// - `value == false` **removes** the line — absence is the
    ///   canonical "off" representation, mirroring
    ///   `useEntryActions.handleToggleFavorite`.
    ///
    /// The in-memory [`Note::frontmatter`] is updated **before** the
    /// disk write is queued so a follow-up render of `vault.notes`
    /// observes the new state without waiting for an fs-watcher tick
    /// — important for the toolbar star / organized cells which
    /// re-render off the same frame as the click.
    ///
    /// Worklist 9.2.1 (star) and 9.2.2 (organized) share this path:
    /// landing them together amortises the YAML splitter +
    /// byte-identical rewrite cost.
    ///
    /// # Errors
    ///
    /// - `NotFound(id)` when the id is unknown.
    /// - `Io { path, source }` when reading or writing the file fails.
    pub fn set_frontmatter_bool(
        &mut self,
        id: NoteId,
        key: &str,
        value: bool,
    ) -> Task<Result<(), VaultError>> {
        // Read the current bytes on the foreground thread — we need
        // them synchronously to update the in-memory map.  At Tolaria's
        // note sizes (a few KiB) the read is negligible vs. the
        // scheduling overhead of bouncing onto the background pool
        // just for an open/read pair.
        let Some(note) = self.notes.get(&id) else {
            return Task::ready(Err(VaultError::NotFound(id)));
        };
        let path = note.path.clone();
        let raw = match read_to_string(&path) {
            Ok(raw) => raw,
            Err(err) => return Task::ready(Err(err)),
        };
        let new_contents = frontmatter::set_bool_in_raw(&raw, key, value);
        // Fast path: the on-disk bytes already match the requested
        // state.  Skip the write — but still re-sync the in-memory
        // `Note::frontmatter` (and `byte_size` / `modified`) from
        // the disk bytes we just read.  Without this refresh an
        // external edit can silently desync the toolbar (worklist
        // 9.2.9): the toolbar's render-time read returns the stale
        // in-memory state, the closure dispatches the same value
        // disk already has, this branch returns `Ok(())`, and the
        // next render still sees the stale state — the user
        // perceives the star / organized toggle as broken.  By
        // mirroring disk into memory here, the next render sees the
        // truth and the closure captures the right value on the
        // following click.
        if new_contents == raw {
            if let Some(note) = self.notes.get_mut(&id) {
                sync_in_memory_from_disk(note, &raw, &path);
            }
            // Even the fast path emits a `VaultChanged` — subscribers
            // (e.g. `note_list_pane` for the Inbox `_organized` filter,
            // worklist 9.2.12) treat it as an invalidation hint and
            // rebuild their cached projection.  Without this nudge a
            // toggle that round-trips the same value disk already had
            // would leave a stale subscriber view, exactly the bug the
            // reopened 9.2.12 annotation describes for the chrome path
            // when an external edit and a chrome click land in the
            // same frame.
            self.emit_frontmatter_changed(&path);
            return Task::ready(Ok(()));
        }
        // Update the in-memory frontmatter map BEFORE queueing the
        // disk write so the next render sees the new state without
        // racing an fs-watcher tick.  On disk-write failure the
        // in-memory state is briefly ahead of disk — acceptable for
        // an optimistic toggle (matches the React handler's
        // optimistic-update + rollback shape, minus the rollback).
        // TODO(9.2-followup): on write failure, revert the in-memory
        // mutation and surface a chrome-side toast.
        if let Some(note) = self.notes.get_mut(&id) {
            if value {
                note.frontmatter.insert_bool(key, true);
            } else {
                note.frontmatter.remove(key);
            }
        }
        // Notify subscribers as soon as the in-memory state changes —
        // they shouldn't have to wait for the background write to
        // settle before refreshing.  Mirrors the "in-memory ahead of
        // disk" optimistic-update shape: the chrome layer (e.g. the
        // inbox filter) sees the new frontmatter on the very next
        // render, not several debounced fs-events later.  Worklist
        // 9.2.12 (reopened) — without this signal the
        // `note_list_pane` cached `entries` slice stayed stale after
        // `toggle_frontmatter_flag` → the Inbox kept showing the
        // organized note until something else triggered a rebuild.
        self.emit_frontmatter_changed(&path);
        match self.background_executor.as_ref() {
            Some(executor) => executor
                .clone()
                .spawn(async move { write_to_disk(&path, &new_contents) }),
            None => {
                let result =
                    std::fs::write(&path, &new_contents).map_err(|source| VaultError::Io {
                        path: path.clone(),
                        source,
                    });
                if result.is_ok() {
                    // Mirror `save_sync`'s metadata refresh so tests
                    // that round-trip `toggle → read` see the new
                    // byte size without an extra rescan.
                    if let Some(note) = self.notes.get_mut(&id) {
                        if let Ok(meta) = std::fs::metadata(&note.path) {
                            note.byte_size = meta.len();
                            if let Ok(t) = meta.modified() {
                                note.modified = DateTime::<Utc>::from(t);
                            }
                        }
                    }
                }
                Task::ready(result)
            }
        }
    }

    /// Send a `VaultChanged { paths: [path] }` event on the watcher
    /// channel so chrome subscribers can refresh.  Used by frontmatter
    /// mutators that update the in-memory map directly — they need to
    /// surface the change immediately without waiting for the OS-level
    /// fs-watcher debounce to fire (which can lag 200ms+ behind the
    /// click).  Reusing the existing `watch_tx` keeps a single channel
    /// per vault instead of growing parallel event streams.
    ///
    /// Failure to send is logged at `debug` rather than `warn`: the
    /// only failure mode is "no receivers attached" (every receiver
    /// clone was dropped), which is the steady state in headless
    /// tests that don't observe vault events.
    fn emit_frontmatter_changed(&self, path: &Path) {
        if let Err(err) = self.watch_tx.send(VaultChanged {
            paths: vec![path.to_path_buf()],
        }) {
            log::debug!(
                target: "vault::frontmatter",
                "watch channel send dropped (no live receivers): {err}",
            );
        }
    }

    /// Create a new markdown note in the vault root with an empty body.
    ///
    /// Picks a unique filename derived from `stem`: starts with
    /// `{stem}.md`, falling back to `{stem}-1.md`, `{stem}-2.md`, … if
    /// a file with the candidate name already exists.  The write goes
    /// through `OpenOptions::create_new(true)` so concurrent
    /// `create_note` callers (or external filesystem races) can't
    /// stomp on each other — `create_new` returns `AlreadyExists` if
    /// the path materialised between the existence check and the open.
    ///
    /// After the empty file lands on disk, `rescan` rebuilds the
    /// in-memory index and the freshly-assigned [`NoteId`] is
    /// returned so callers can route it through `OpenNoteEvent` (or
    /// equivalent) to open the new note in the editor.
    ///
    /// Worklist 2.19 — wired to the notes-list `+` button and to
    /// `actions::NewNote` (Cmd+N) so both code paths share the same
    /// create-and-open flow.
    ///
    /// # Errors
    ///
    /// - The uniqueness loop exceeds [`CREATE_NOTE_SUFFIX_LIMIT`]
    ///   iterations (returned as [`VaultError::Io`] with `AlreadyExists`).
    ///   A vault with 1000+ `untitled-N.md` files is a pathological
    ///   case; surfacing it as an error beats spinning forever.
    /// - The disk write fails (permission denied, disk full, …) —
    ///   surfaced as [`VaultError::Io`] with the candidate path.
    /// - The post-create rescan fails (e.g. a subdirectory became
    ///   unreadable) — surfaced as [`VaultError::Rescan`] preserving
    ///   the underlying `anyhow` chain.
    /// - The post-rescan index does not contain the freshly-written
    ///   path — surfaced as [`VaultError::Io`] with `ErrorKind::NotFound`.
    ///   Should not happen in practice (the file was just written)
    ///   but defended against so the caller sees a clear diagnostic
    ///   instead of a missing entry.
    pub fn create_note(&mut self, stem: &str) -> Result<NoteId, VaultError> {
        let path = self.allocate_note_path(stem)?;
        // `create_new(true)` makes the open atomic w.r.t. the
        // existence check — if a parallel writer materialised the
        // file between `allocate_note_path` and here, the open errors
        // out instead of silently truncating.  Empty body matches the
        // React variant; users rename or populate the note next.
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|source| VaultError::Io {
                path: path.clone(),
                source,
            })?;
        // `rescan` errors out as `anyhow::Error`; surface through
        // [`VaultError::Rescan`] so the chain stays walkable instead
        // of being flattened into a string-coded `io::Error`.
        self.rescan_internal().map_err(VaultError::Rescan)?;
        // Find the id assigned to the freshly-created path.  The
        // rescan above just walked the directory, so the entry must
        // exist; surfacing `NotFound` here would mean the path
        // vanished between the write and the rescan (e.g. an
        // external delete), which is worth flagging instead of
        // silently swallowing.
        self.notes
            .iter()
            .find(|(_, note)| note.path == path)
            .map(|(id, _)| *id)
            .ok_or_else(|| {
                log::error!("vault::create_note: wrote {path:?} but rescan did not surface it",);
                VaultError::Io {
                    path,
                    source: std::io::Error::new(
                        std::io::ErrorKind::NotFound,
                        "freshly-created note not found in post-rescan index",
                    ),
                }
            })
    }

    /// Resolve a fresh, unique markdown path under [`Self::root`] for
    /// a new note.  Tries `{stem}.md` first; on conflict appends
    /// `-1`, `-2`, … up to [`CREATE_NOTE_SUFFIX_LIMIT`] before giving
    /// up.  Pure path arithmetic — does not touch the in-memory note
    /// index, so safe to call before any state mutation.
    fn allocate_note_path(&self, stem: &str) -> Result<PathBuf, VaultError> {
        let candidate = self.root.join(format!("{stem}.md"));
        if !candidate.exists() {
            return Ok(candidate);
        }
        for suffix in 1..=CREATE_NOTE_SUFFIX_LIMIT {
            let candidate = self.root.join(format!("{stem}-{suffix}.md"));
            if !candidate.exists() {
                return Ok(candidate);
            }
        }
        Err(VaultError::Io {
            path: self.root.clone(),
            source: std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                format!(
                    "exhausted {CREATE_NOTE_SUFFIX_LIMIT} suffixes searching for unique {stem}-N.md"
                ),
            ),
        })
    }

    /// Case-insensitive substring search over note titles.
    pub fn search_titles(&self, query: &str) -> Task<Vec<NoteId>> {
        let q = query.to_lowercase();
        let ids = self
            .notes
            .iter()
            .filter(|(_, n)| n.title.to_lowercase().contains(&q))
            .map(|(id, _)| *id)
            .collect();
        Task::ready(ids)
    }

    /// Rescan the on-disk vault, rebuilding the note index.  Reuses existing
    /// [`NoteId`]s for notes whose path is unchanged; assigns fresh IDs for
    /// newly-discovered notes and drops IDs for notes whose files vanished.
    ///
    /// # Errors
    ///
    /// Returns an error if any directory along the walk cannot be read.
    pub fn rescan(&mut self) -> Result<()> {
        self.rescan_internal()
    }

    /// Vault-root-relative directory paths discovered during the most
    /// recent scan, in lexicographic order.  Empty for a freshly-opened
    /// vault with no subdirectories.
    ///
    /// Cheap accessor over cached state — call freely.
    #[must_use]
    pub fn folders(&self) -> &[PathBuf] {
        &self.folders
    }

    /// Vault-root-relative paths of every non-markdown file discovered
    /// during the most recent scan, in lexicographic order.  Includes
    /// recognised assets (`.png`, `.jpg`, `.jpeg`, `.gif`, `.webp`,
    /// `.svg`, `.pdf`) and any other arbitrary file the user keeps
    /// alongside notes.
    ///
    /// Cheap accessor over cached state — call freely.
    #[must_use]
    pub fn assets(&self) -> &[PathBuf] {
        &self.assets
    }

    fn rescan_internal(&mut self) -> Result<()> {
        let mut scan = ScanResult::default();
        walk_vault(&self.root, 32, &mut scan)?;
        let ScanResult {
            mut notes,
            mut folders,
            mut assets,
        } = scan;
        notes.sort();
        folders.sort();
        assets.sort();
        let paths = notes;

        let mut old_by_path: HashMap<PathBuf, NoteId> = self
            .notes
            .iter()
            .map(|(id, n)| (n.path.clone(), *id))
            .collect();
        let mut new_notes = HashMap::new();

        for path in paths {
            let id = match old_by_path.remove(&path) {
                Some(id) => id,
                None => {
                    let id = NoteId(self.next_id);
                    self.next_id += 1;
                    id
                }
            };
            let meta = std::fs::metadata(&path).ok();
            let modified = meta
                .as_ref()
                .and_then(|m| m.modified().ok())
                .map(DateTime::<Utc>::from)
                .unwrap_or_else(Utc::now);
            let byte_size = meta.as_ref().map(std::fs::Metadata::len).unwrap_or(0);
            let title = path
                .file_stem()
                .and_then(|s| s.to_str())
                .map(|s| SharedString::from(s.to_owned()))
                .unwrap_or_default();
            let fm = std::fs::read_to_string(&path)
                .ok()
                .map(|raw| frontmatter::parse(&raw).0)
                .unwrap_or_default();
            new_notes.insert(
                id,
                Note {
                    id,
                    title,
                    path,
                    kind: NoteKind::Markdown,
                    modified,
                    byte_size,
                    frontmatter: fm,
                },
            );
        }

        self.notes = new_notes;
        // Make folder / asset paths vault-root-relative so consumers
        // (folder_tree, asset browsers) don't have to strip the prefix.
        self.folders = folders
            .into_iter()
            .filter_map(|p| p.strip_prefix(&self.root).ok().map(Path::to_path_buf))
            .collect();
        self.assets = assets
            .into_iter()
            .filter_map(|p| p.strip_prefix(&self.root).ok().map(Path::to_path_buf))
            .collect();
        Ok(())
    }

    /// Number of notes in the index (test-only accessor — production code
    /// should use [`Vault::notes`] and resolve the returned task).
    #[cfg(test)]
    pub fn note_count(&self) -> usize {
        self.notes.len()
    }

    /// All note IDs as a `Vec`, no `Task` wrapper (test-only accessor).
    #[cfg(test)]
    pub fn note_ids_sync(&self) -> Vec<NoteId> {
        self.note_ids_vec()
    }

    /// Notes whose body contains a `[[wikilink]]` pointing at `id` —
    /// the "inbound" half of the note's neighbourhood (Phase 9 worklist
    /// 9.2.3).  Walks every other note's on-disk body, parses each
    /// `[[target]]` / `[[target|alias]]` occurrence, and matches the
    /// target against the destination note's filename stem (the same
    /// value [`Note::title`] carries).  Match is **exact, case
    /// sensitive** — mirrors the rest of the codebase, which stores
    /// titles as `SharedString` derived directly from `Path::file_stem`
    /// without any case-folding.
    ///
    /// On-demand parse rather than a cached index: at Tolaria-scale
    /// vaults the call site (neighbourhood-mode entry from the toolbar
    /// `Map` cell) fires rarely, so the cost of one pass over every
    /// note body per click is far smaller than the bookkeeping cost of
    /// maintaining an inbound-link index across [`set_frontmatter_bool`]
    /// / [`save`] / [`rescan`] / fs-watcher ticks.  The result is
    /// **sorted ascending by [`NoteId`]** so callers don't observe
    /// HashMap iteration jitter — and the inspector_panel (worklist
    /// 9.2.8) can rely on a stable list.
    ///
    /// Returns an empty `Vec` when `id` is unknown.  Skips the active
    /// note itself (a note that links to its own title doesn't count
    /// as its own backlink).
    ///
    /// IO failures while reading a candidate note's body degrade
    /// gracefully — the offending note is logged and skipped, the rest
    /// of the result stays valid.
    ///
    /// # Performance
    ///
    /// O(N) synchronous file reads on the calling thread (one per
    /// other note in the vault).  At Tolaria-scale vaults (≤ a few
    /// thousand notes) and the rare call cadence (mouse click on the
    /// `Map` toolbar cell, or one inspector_panel render), this stays
    /// well under one frame.  If a future workflow calls this on a
    /// hot path, route the reads onto [`Vault::set_executor`]'s
    /// background pool the way [`Vault::note_content`] does.
    #[must_use]
    pub fn backlinks(&self, id: NoteId) -> Vec<NoteId> {
        let Some(target) = self.notes.get(&id) else {
            return Vec::new();
        };
        let target_title = target.title.as_ref();
        let mut hits: Vec<NoteId> = Vec::new();
        for note in self.notes.values() {
            if note.id == id {
                continue;
            }
            let raw = match std::fs::read_to_string(&note.path) {
                Ok(s) => s,
                Err(err) => {
                    log::debug!(
                        "vault::backlinks: skipping {:?} (read failed: {err})",
                        note.path,
                    );
                    continue;
                }
            };
            if wikilink_targets(&raw).any(|t| t == target_title) {
                hits.push(note.id);
            }
        }
        hits.sort();
        hits
    }

    /// Notes that the body at `id` links TO via `[[wikilink]]` syntax —
    /// the "outbound" half of the neighbourhood (Phase 9 worklist
    /// 9.2.3).  Mirror of [`backlinks`]: parses the active note's body
    /// once, resolves every `[[target]]` to a [`NoteId`] by exact
    /// title (`file_stem`) match, and returns the de-duplicated set.
    ///
    /// Targets that don't resolve to any note in the vault (broken
    /// links, drafts that point at a yet-to-be-created title) are
    /// silently dropped — the neighbourhood-mode predicate only needs
    /// the resolvable subset.
    ///
    /// Result is **sorted ascending by [`NoteId`]**.  Returns an empty
    /// `Vec` when `id` is unknown or the note's body cannot be read.
    ///
    /// # Performance
    ///
    /// One synchronous file read (the active note's body) plus an
    /// O(N) HashMap build (one entry per other note's title).  Cheap
    /// at Tolaria-scale; see [`Vault::backlinks`] for the same
    /// caveats around hot-path use.
    #[must_use]
    pub fn outbound_links(&self, id: NoteId) -> Vec<NoteId> {
        let Some(note) = self.notes.get(&id) else {
            return Vec::new();
        };
        let raw = match std::fs::read_to_string(&note.path) {
            Ok(s) => s,
            Err(err) => {
                log::debug!(
                    "vault::outbound_links: read failed for {:?}: {err}",
                    note.path,
                );
                return Vec::new();
            }
        };
        // Build a title → id map so every parsed target is one HashMap
        // lookup, not an O(n) scan per occurrence.  Skip the source
        // note so a self-link doesn't surface as its own neighbour.
        let mut title_to_id: HashMap<&str, NoteId> = HashMap::with_capacity(self.notes.len());
        for n in self.notes.values() {
            if n.id == id {
                continue;
            }
            title_to_id.insert(n.title.as_ref(), n.id);
        }
        let mut hits: Vec<NoteId> = Vec::new();
        let mut seen: std::collections::HashSet<NoteId> = std::collections::HashSet::new();
        for target in wikilink_targets(&raw) {
            if let Some(&nid) = title_to_id.get(target) {
                if seen.insert(nid) {
                    hits.push(nid);
                }
            }
        }
        hits.sort();
        hits
    }
}

// ---------------------------------------------------------------------------
// Internal: directory walker
// ---------------------------------------------------------------------------

/// Read a file's contents to a `String`, mapping IO failures into
/// `VaultError::Io` with the path preserved for diagnostics.  Shared
/// by the sync and async paths so both produce identical errors.
fn read_to_string(path: &Path) -> Result<String, VaultError> {
    std::fs::read_to_string(path).map_err(|source| VaultError::Io {
        path: path.to_path_buf(),
        source,
    })
}

/// Write a byte slice to `path`, mapping IO failures into
/// `VaultError::Io` with the path preserved for diagnostics.  Used
/// exclusively by the async [`Vault::save`] path; the sync path
/// continues to call [`Vault::save_sync`] so it can also refresh
/// in-memory metadata.
fn write_to_disk(path: &Path, body: &str) -> Result<(), VaultError> {
    std::fs::write(path, body).map_err(|source| VaultError::Io {
        path: path.to_path_buf(),
        source,
    })
}

/// Re-parse `raw` (the bytes just read from disk) into the given
/// note's [`Frontmatter`] and refresh its `byte_size` / `modified`
/// fields from `path`.
///
/// Extracted from [`Vault::set_frontmatter_bool`] so its fast-path
/// branch can re-sync the in-memory map without duplicating the
/// metadata-refresh boilerplate from `save_sync` (worklist 9.2.9).
/// Pure free function — takes a borrowed `&mut Note` so the caller
/// owns the `notes.get_mut(&id)` lookup and we don't allocate a
/// fresh `Option` borrow inside the helper.
///
/// `std::fs::metadata` failures are non-fatal: the frontmatter
/// refresh is the load-bearing invariant (it drives the toolbar's
/// `is_favorite` / `is_organized` glyph), while `byte_size` /
/// `modified` are decoration that the next `rescan` or `save` will
/// refresh.
fn sync_in_memory_from_disk(note: &mut Note, raw: &str, path: &Path) {
    note.frontmatter = frontmatter::parse(raw).0;
    if let Ok(meta) = std::fs::metadata(path) {
        note.byte_size = meta.len();
        if let Ok(t) = meta.modified() {
            note.modified = DateTime::<Utc>::from(t);
        }
    }
}

/// Iterator over every `[[target]]` occurrence in `body`, yielding the
/// target portion only (`[[Alpha|alias]]` → `"Alpha"`, `[[Beta]]` →
/// `"Beta"`).  Mirrors the React-side semantics implemented by
/// `src-tauri/src/vault/parsing.rs::extract_outgoing_links` so the
/// inspector_panel row (worklist 9.2.8) and neighbourhood-mode filter
/// (worklist 9.2.3) reach the same targets the Tauri-era UI did.
///
/// Phase 9 MVP: doesn't filter out `[[…]]` occurrences that fall inside
/// fenced code blocks.  Bringing fenced-code awareness over from the
/// React `wikilinks.ts` parser is a follow-up — see `TODO(9.2.3-fence)`
/// below.  At Tolaria's scale (no synthetic vaults with hostile bodies)
/// the false positive rate is small enough to ship without it.
///
/// Returns an `Iterator` borrowing from `body` so callers don't pay an
/// allocation per occurrence.  `body` outlives the iterator by
/// construction — every caller holds it on the stack for the duration
/// of the parse.
fn wikilink_targets(body: &str) -> impl Iterator<Item = &str> {
    // TODO(9.2.3-fence): respect fenced-code blocks the way
    // `src/utils/wikilinks.ts::iterateWikilinks` does, so a backtick
    // block containing `[[Foo]]` doesn't produce a phantom outbound
    // link.  Tracked alongside the same gap in the React parser tests.
    WikilinkTargets {
        body,
        search_from: 0,
    }
}

/// State machine backing [`wikilink_targets`] — exists as a named
/// struct rather than an inline closure so the iterator type can be
/// `impl Iterator<Item = &'a str>` without `'static` bounds on the
/// callback shape.
struct WikilinkTargets<'a> {
    body: &'a str,
    search_from: usize,
}

impl<'a> Iterator for WikilinkTargets<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let rel_start = self.body[self.search_from..].find("[[")?;
            let abs_inner = self.search_from + rel_start + 2;
            let rel_end = self.body[abs_inner..].find("]]")?;
            let inner = &self.body[abs_inner..abs_inner + rel_end];
            self.search_from = abs_inner + rel_end + 2;
            // `[[Alpha|alias]]` → take the part before `|`.  Strip
            // surrounding whitespace so `[[  Alpha  ]]` still resolves
            // (mirrors the React parser's `.trim()`).
            let target = match inner.find('|') {
                Some(idx) => inner[..idx].trim(),
                None => inner.trim(),
            };
            if target.is_empty() {
                // `[[]]` and `[[|alias]]` carry no target — skip.
                continue;
            }
            return Some(target);
        }
    }
}

/// Result of one [`walk_vault`] pass — keeps the three sibling lists
/// in a single allocation so callers can destructure them cleanly.
#[derive(Default)]
struct ScanResult {
    notes: Vec<PathBuf>,
    folders: Vec<PathBuf>,
    assets: Vec<PathBuf>,
}

/// Visit every entry under `root` (depth-limited).  Markdown files
/// land in `out.notes`; subdirectories in `out.folders`; everything
/// else in `out.assets`.  Hidden directories (`.git`, `.obsidian`, …)
/// are skipped to avoid indexing tool metadata.
fn walk_vault(root: &Path, max_depth: usize, out: &mut ScanResult) -> Result<()> {
    fn recurse(dir: &Path, depth: usize, max: usize, out: &mut ScanResult) -> Result<()> {
        if depth > max {
            return Ok(());
        }
        let entries = std::fs::read_dir(dir).with_context(|| format!("reading {dir:?}"))?;
        for entry in entries {
            let entry = entry.with_context(|| format!("iterating {dir:?}"))?;
            let file_type = entry.file_type()?;
            let path = entry.path();
            if file_type.is_dir() {
                if let Some(name) = path.file_name().and_then(|s| s.to_str()) {
                    if name.starts_with('.') {
                        continue;
                    }
                }
                out.folders.push(path.clone());
                recurse(&path, depth + 1, max, out)?;
            } else if file_type.is_file() {
                if is_markdown_extension(&path) {
                    out.notes.push(path);
                } else {
                    out.assets.push(path);
                }
            }
        }
        Ok(())
    }
    recurse(root, 0, max_depth, out)
}

/// Markdown extensions Tolaria treats as notes.  `.md` is the canonical
/// form, `.markdown` is accepted so vaults imported from other editors
/// don't lose entries.
fn is_markdown_extension(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|e| e.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref(),
        Some("md") | Some("markdown")
    )
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    fn write(dir: &Path, rel: &str, body: &str) -> PathBuf {
        let p = dir.join(rel);
        if let Some(parent) = p.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&p, body).unwrap();
        p
    }

    #[test]
    fn opens_empty_vault() {
        let dir = tempdir().unwrap();
        let v = Vault::open_at(dir.path()).unwrap();
        assert_eq!(v.note_count(), 0);
        assert!(v.root().is_absolute(), "root must be canonicalised");
    }

    #[test]
    fn indexes_markdown_files() {
        let dir = tempdir().unwrap();
        write(dir.path(), "a.md", "alpha");
        write(dir.path(), "b.md", "beta");
        write(dir.path(), "sub/c.md", "gamma");
        let v = Vault::open_at(dir.path()).unwrap();
        assert_eq!(v.note_count(), 3);
    }

    #[test]
    fn skips_hidden_directories() {
        let dir = tempdir().unwrap();
        write(dir.path(), ".git/HEAD", "ref: refs/heads/main");
        write(dir.path(), ".obsidian/x.md", "should not appear");
        write(dir.path(), "visible.md", "ok");
        let v = Vault::open_at(dir.path()).unwrap();
        assert_eq!(v.note_count(), 1, "only visible.md should index");
    }

    #[test]
    fn skips_non_markdown_files() {
        let dir = tempdir().unwrap();
        write(dir.path(), "n.md", "x");
        write(dir.path(), "image.png", "fake");
        write(dir.path(), "doc.txt", "fake");
        let v = Vault::open_at(dir.path()).unwrap();
        assert_eq!(v.note_count(), 1);
    }

    #[test]
    fn save_writes_to_disk_and_updates_byte_size() {
        let dir = tempdir().unwrap();
        let path = write(dir.path(), "n.md", "old");
        let mut v = Vault::open_at(dir.path()).unwrap();
        let id = v.note_ids_sync()[0];
        let original_size = std::fs::metadata(&path).unwrap().len();
        v.save_sync(id, "much longer content body")
            .expect("save_sync must succeed");
        let after = std::fs::read_to_string(&path).unwrap();
        assert_eq!(after, "much longer content body");
        let new_size = std::fs::metadata(&path).unwrap().len();
        assert!(new_size > original_size);
    }

    #[test]
    fn save_unknown_id_returns_not_found() {
        let dir = tempdir().unwrap();
        let mut v = Vault::open_at(dir.path()).unwrap();
        let err = v
            .save_sync(NoteId(99), "x")
            .expect_err("save_sync on unknown id must error");
        assert!(
            matches!(err, VaultError::NotFound(NoteId(99))),
            "expected NotFound(99), got {err:?}",
        );
    }

    #[test]
    fn create_note_returns_new_id_for_fresh_filename() {
        let dir = tempdir().unwrap();
        let mut v = Vault::open_at(dir.path()).unwrap();
        assert_eq!(v.note_count(), 0);
        let id = v
            .create_note("untitled")
            .expect("create_note must succeed for fresh stem");
        let path = dir.path().join("untitled.md");
        assert!(path.exists(), "create_note must write the file to disk");
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "",
            "freshly-created notes start with an empty body",
        );
        let note = v.notes.get(&id).expect("freshly-created id must resolve");
        // Path stored on the Note must match the on-disk write path
        // (modulo symlink canonicalisation on macOS — `tempdir()` uses
        // `/var/folders/...` which `canonicalize` resolves to
        // `/private/var/...`).
        assert_eq!(
            note.path.canonicalize().unwrap(),
            path.canonicalize().unwrap()
        );
    }

    #[test]
    fn create_note_appends_suffix_on_conflict() {
        let dir = tempdir().unwrap();
        write(dir.path(), "untitled.md", "pre-existing");
        let mut v = Vault::open_at(dir.path()).unwrap();
        let id = v
            .create_note("untitled")
            .expect("create_note must succeed when {stem}.md exists");
        let new_path = dir.path().join("untitled-1.md");
        assert!(
            new_path.exists(),
            "create_note must fall back to -1 suffix on conflict",
        );
        let note = v.notes.get(&id).expect("freshly-created id must resolve");
        assert_eq!(
            note.path.canonicalize().unwrap(),
            new_path.canonicalize().unwrap()
        );
    }

    #[test]
    fn create_note_handles_existing_suffixed_files() {
        let dir = tempdir().unwrap();
        write(dir.path(), "untitled.md", "a");
        write(dir.path(), "untitled-1.md", "b");
        let mut v = Vault::open_at(dir.path()).unwrap();
        let id = v
            .create_note("untitled")
            .expect("create_note must skip past -1 to -2");
        let new_path = dir.path().join("untitled-2.md");
        assert!(
            new_path.exists(),
            "create_note must skip past existing suffixes",
        );
        let note = v.notes.get(&id).expect("freshly-created id must resolve");
        assert_eq!(
            note.path.canonicalize().unwrap(),
            new_path.canonicalize().unwrap()
        );
    }

    #[test]
    fn rescan_preserves_ids_for_unchanged_paths() {
        let dir = tempdir().unwrap();
        write(dir.path(), "a.md", "x");
        let mut v = Vault::open_at(dir.path()).unwrap();
        let original_id = v.note_ids_sync()[0];
        // Add another file.
        write(dir.path(), "b.md", "y");
        v.rescan().unwrap();
        assert_eq!(v.note_count(), 2);
        // The id for a.md must still be in the index.
        assert!(v.note_ids_sync().contains(&original_id));
    }

    #[test]
    fn rescan_drops_vanished_notes() {
        let dir = tempdir().unwrap();
        let path = write(dir.path(), "a.md", "x");
        write(dir.path(), "b.md", "y");
        let mut v = Vault::open_at(dir.path()).unwrap();
        assert_eq!(v.note_count(), 2);
        fs::remove_file(&path).unwrap();
        v.rescan().unwrap();
        assert_eq!(v.note_count(), 1);
    }

    #[test]
    fn note_frontmatter_populated_on_open() {
        let dir = tempdir().unwrap();
        write(
            dir.path(),
            "n.md",
            "---\ntype: Note\nstatus: Done\n---\n\n# body\n",
        );
        let v = Vault::open_at(dir.path()).unwrap();
        let id = v.note_ids_sync()[0];
        let note = v.notes.get(&id).expect("note exists");
        assert_eq!(note.frontmatter().len(), 2);
        assert!(note.frontmatter().get("type").is_some());
        assert!(note.frontmatter().get("status").is_some());
    }

    #[test]
    fn note_frontmatter_empty_when_absent() {
        let dir = tempdir().unwrap();
        write(dir.path(), "n.md", "no frontmatter here\n");
        let v = Vault::open_at(dir.path()).unwrap();
        let id = v.note_ids_sync()[0];
        let note = v.notes.get(&id).expect("note exists");
        assert!(note.frontmatter().is_empty());
    }

    #[test]
    fn open_nonexistent_dir_errors() {
        let dir = tempdir().unwrap();
        let bogus = dir.path().join("does-not-exist");
        assert!(Vault::open_at(&bogus).is_err());
    }

    #[test]
    fn surfaces_folders_and_assets() {
        let dir = tempdir().unwrap();
        write(dir.path(), "notes/a.md", "alpha");
        write(dir.path(), "notes/sub/b.md", "beta");
        write(dir.path(), "images/c.png", "fake-png");
        write(dir.path(), "d.pdf", "fake-pdf");
        write(dir.path(), "random.bin", "blob");
        let v = Vault::open_at(dir.path()).unwrap();

        let folders: Vec<&str> = v
            .folders()
            .iter()
            .map(|p| p.to_str().unwrap_or_default())
            .collect();
        assert!(folders.contains(&"notes"), "folders: {folders:?}");
        assert!(folders.contains(&"notes/sub"), "folders: {folders:?}");
        assert!(folders.contains(&"images"), "folders: {folders:?}");

        let assets: Vec<&str> = v
            .assets()
            .iter()
            .map(|p| p.to_str().unwrap_or_default())
            .collect();
        assert!(assets.contains(&"images/c.png"), "assets: {assets:?}");
        assert!(assets.contains(&"d.pdf"), "assets: {assets:?}");
        assert!(assets.contains(&"random.bin"), "assets: {assets:?}");
    }

    #[gpui::test]
    async fn open_at_async_runs_scan_off_thread(cx: &mut gpui::TestAppContext) {
        let dir = tempdir().unwrap();
        write(dir.path(), "a.md", "---\ntype: Note\n---\n# a\n");
        write(dir.path(), "b.md", "no frontmatter");
        let root = dir.path().to_path_buf();
        let vault = cx
            .update(|cx| Vault::open_at_async(root, cx))
            .await
            .expect("async open must succeed");
        assert_eq!(vault.note_count(), 2);
        // Async open installs the executor so subsequent reads go
        // through the background pool.
        assert!(
            vault.background_executor.is_some(),
            "open_at_async must set the background executor on the resolved vault"
        );
    }

    #[gpui::test]
    async fn async_note_content_round_trips(cx: &mut gpui::TestAppContext) {
        let dir = tempdir().unwrap();
        write(dir.path(), "n.md", "body content");
        let root = dir.path().to_path_buf();
        let vault = cx
            .update(|cx| Vault::open_at_async(root, cx))
            .await
            .expect("async open must succeed");
        let id = vault.note_ids_sync()[0];
        let body = vault
            .note_content(id)
            .await
            .expect("async read must succeed");
        assert_eq!(body, "body content");
    }

    #[gpui::test]
    async fn async_save_round_trips_through_disk(cx: &mut gpui::TestAppContext) {
        let dir = tempdir().unwrap();
        let path = write(dir.path(), "n.md", "old");
        let root = dir.path().to_path_buf();
        let mut vault = cx
            .update(|cx| Vault::open_at_async(root, cx))
            .await
            .expect("async open must succeed");
        let id = vault.note_ids_sync()[0];
        vault
            .save(id, "new content via async path")
            .await
            .expect("async save must succeed");
        // Re-read directly off disk so we're not just observing the
        // in-memory cache.
        let on_disk = fs::read_to_string(&path).unwrap();
        assert_eq!(on_disk, "new content via async path");
    }

    #[test]
    fn watch_events_receives_external_changes() {
        use std::time::{Duration, Instant};

        let dir = tempdir().unwrap();
        write(dir.path(), "seed.md", "seed");
        let v = Vault::open_at(dir.path()).unwrap();
        let rx = v.watch_events();

        // Give the OS watcher time to attach before kicking events.
        std::thread::sleep(Duration::from_millis(100));

        // Create -> modify -> delete a file inside the vault.
        let p = v.root().join("touched.md");
        std::fs::write(&p, "create").unwrap();
        std::thread::sleep(Duration::from_millis(60));
        std::fs::write(&p, "modify").unwrap();
        std::thread::sleep(Duration::from_millis(60));
        std::fs::remove_file(&p).unwrap();

        // Drain the channel for up to 2 s; timing-tolerant.
        let deadline = Instant::now() + Duration::from_secs(2);
        let mut seen_touched = false;
        while Instant::now() < deadline {
            match rx.recv_timeout(Duration::from_millis(50)) {
                Ok(VaultChanged { paths }) => {
                    if paths
                        .iter()
                        .any(|p| p.file_name().is_some_and(|n| n == "touched.md"))
                    {
                        seen_touched = true;
                        break;
                    }
                }
                Err(_) => continue,
            }
        }
        assert!(
            seen_touched,
            "watch_events() must surface events for touched.md within 2 s"
        );
        // Drop the vault explicitly so we can assert the watcher
        // thread is cleaned up on the test path too.
        drop(v);
    }

    // -----------------------------------------------------------------
    // set_frontmatter_bool — worklist 9.2.1 / 9.2.2
    // -----------------------------------------------------------------

    #[gpui::test]
    async fn set_frontmatter_bool_toggle_on_preserves_existing_keys(cx: &mut gpui::TestAppContext) {
        let dir = tempdir().unwrap();
        let path = write(
            dir.path(),
            "n.md",
            "---\ntype: Note\nstatus: Done\n---\n\n# Heading\n\nA body paragraph.\n",
        );
        let root = dir.path().to_path_buf();
        let mut vault = cx
            .update(|cx| Vault::open_at_async(root, cx))
            .await
            .expect("async open must succeed");
        let id = vault.note_ids_sync()[0];
        vault
            .set_frontmatter_bool(id, "_favorite", true)
            .await
            .expect("toggle-on must succeed");
        // On-disk bytes must include the new line and leave the rest
        // (including the body) verbatim.
        let on_disk = fs::read_to_string(&path).unwrap();
        assert_eq!(
            on_disk,
            "---\ntype: Note\nstatus: Done\n_favorite: true\n---\n\n# Heading\n\nA body paragraph.\n",
            "toggle-on must append the key without touching surrounding bytes",
        );
        // In-memory frontmatter mirrors the new state.
        let note = vault.note(id).await.expect("note exists");
        assert!(
            note.is_favorite(),
            "in-memory frontmatter must reflect the toggle"
        );
        assert_eq!(
            note.frontmatter()
                .get("type")
                .map(|v| matches!(v, FrontmatterValue::Text(_))),
            Some(true),
            "other keys must survive the rewrite",
        );
    }

    #[gpui::test]
    async fn set_frontmatter_bool_toggle_on_inserts_block_when_absent(
        cx: &mut gpui::TestAppContext,
    ) {
        let dir = tempdir().unwrap();
        let path = write(
            dir.path(),
            "n.md",
            "# Just a heading\n\nNo frontmatter here.\n",
        );
        let root = dir.path().to_path_buf();
        let mut vault = cx
            .update(|cx| Vault::open_at_async(root, cx))
            .await
            .expect("async open must succeed");
        let id = vault.note_ids_sync()[0];
        vault
            .set_frontmatter_bool(id, "_organized", true)
            .await
            .expect("toggle-on must succeed even with no existing block");
        let on_disk = fs::read_to_string(&path).unwrap();
        assert_eq!(
            on_disk, "---\n_organized: true\n---\n# Just a heading\n\nNo frontmatter here.\n",
            "toggle-on must prepend a fresh frontmatter block",
        );
    }

    #[gpui::test]
    async fn set_frontmatter_bool_toggle_off_removes_line(cx: &mut gpui::TestAppContext) {
        let dir = tempdir().unwrap();
        let path = write(
            dir.path(),
            "n.md",
            "---\ntype: Note\n_favorite: true\nstatus: Done\n---\n\nbody\n",
        );
        let root = dir.path().to_path_buf();
        let mut vault = cx
            .update(|cx| Vault::open_at_async(root, cx))
            .await
            .expect("async open must succeed");
        let id = vault.note_ids_sync()[0];
        vault
            .set_frontmatter_bool(id, "_favorite", false)
            .await
            .expect("toggle-off must succeed");
        let on_disk = fs::read_to_string(&path).unwrap();
        assert_eq!(
            on_disk, "---\ntype: Note\nstatus: Done\n---\n\nbody\n",
            "toggle-off must remove the line (absent ⇔ false)",
        );
        let note = vault.note(id).await.expect("note exists");
        assert!(!note.is_favorite());
    }

    #[gpui::test]
    async fn set_frontmatter_bool_round_trip_preserves_body_bytes(cx: &mut gpui::TestAppContext) {
        // Fixture body lifted from the React reference shape: tabs,
        // mixed paragraphs, a wikilink — all of these must survive a
        // toggle-on + toggle-off cycle byte-for-byte.
        let body = "\n# Sponsor Onboarding\n\nTurn a signed sponsor into a smooth first placement.\n\n- Confirm the publication date.\n- Hand off recurring communication to [[person-matteo-cellini]].\n";
        let initial = format!(
            "---\ntype: Procedure\naliases:\n  - \"[[Sponsor Onboarding]]\"\nbelongs_to: \"[[responsibility-sponsorships]]\"\nowner: \"[[person-luca-rossi]]\"\ncadence: \"As needed\"\n---\n{body}"
        );
        let dir = tempdir().unwrap();
        let path = write(dir.path(), "procedure.md", &initial);
        let root = dir.path().to_path_buf();
        let mut vault = cx
            .update(|cx| Vault::open_at_async(root, cx))
            .await
            .expect("async open must succeed");
        let id = vault.note_ids_sync()[0];
        vault
            .set_frontmatter_bool(id, "_favorite", true)
            .await
            .expect("toggle-on must succeed");
        vault
            .set_frontmatter_bool(id, "_favorite", false)
            .await
            .expect("toggle-off must succeed");
        let after_round_trip = fs::read_to_string(&path).unwrap();
        assert_eq!(
            after_round_trip, initial,
            "toggle-on then toggle-off must restore the original bytes exactly",
        );
    }

    #[test]
    fn set_frontmatter_bool_sync_path_writes_and_updates_metadata() {
        // Sync path mirrors `save_sync`: useful for unit tests that
        // don't want a `TestAppContext` spin-up.
        let dir = tempdir().unwrap();
        let path = write(dir.path(), "n.md", "---\ntype: Note\n---\nbody\n");
        let mut v = Vault::open_at(dir.path()).unwrap();
        let id = v.note_ids_sync()[0];
        let original_size = std::fs::metadata(&path).unwrap().len();
        // No background executor installed → the write runs inline
        // before `Task::ready` wraps the result; dropping the task is
        // enough to assert the disk-side effect occurred.
        drop(v.set_frontmatter_bool(id, "_favorite", true));
        let on_disk = fs::read_to_string(&path).unwrap();
        assert_eq!(on_disk, "---\ntype: Note\n_favorite: true\n---\nbody\n");
        let new_size = std::fs::metadata(&path).unwrap().len();
        assert!(
            new_size > original_size,
            "byte_size on disk grew after toggle-on"
        );
        let cached_size = v.notes.get(&id).map(|n| n.byte_size).unwrap_or(0);
        assert_eq!(
            cached_size, new_size,
            "sync path must refresh the cached byte_size to match disk",
        );
    }

    #[gpui::test]
    async fn set_frontmatter_bool_unknown_id_returns_not_found(cx: &mut gpui::TestAppContext) {
        let dir = tempdir().unwrap();
        let root = dir.path().to_path_buf();
        let mut vault = cx
            .update(|cx| Vault::open_at_async(root, cx))
            .await
            .expect("async open must succeed");
        let err = vault
            .set_frontmatter_bool(NoteId(99), "_favorite", true)
            .await
            .expect_err("unknown id must error");
        assert!(
            matches!(err, VaultError::NotFound(NoteId(99))),
            "expected NotFound(99), got {err:?}",
        );
    }

    /// Worklist 9.2.9 — the in-memory `Note::frontmatter` must
    /// re-sync from disk whenever `set_frontmatter_bool` hits its
    /// "disk already matches" fast path.  Without that refresh the
    /// toolbar's star/organized glyph stays stale after an external
    /// edit, every subsequent click trips the same fast path, and the
    /// button looks broken to the user.
    ///
    /// Production scenario (matching the user's report):
    /// 1. Toolbar shows the star UN-set.  In-memory + disk = absent.
    /// 2. Some external write (editor save, `git checkout`, …)
    ///    flips `_favorite` to `true` on disk.  Nothing reloads the
    ///    vault — the chrome doesn't yet subscribe to fs-watcher
    ///    events (Phase 9.6).
    /// 3. The user clicks the star expecting a toggle (visually: "OFF
    ///    → ON", because the toolbar still shows the stale UN-set
    ///    glyph).  The closure dispatches `set_frontmatter_bool(id,
    ///    "_favorite", true)`.
    /// 4. Disk fresh = `_favorite: true` already → fast path returns
    ///    `Ok(())` and skips the in-memory mutation.
    /// 5. The toolbar re-reads `is_favorite()` on the next render and
    ///    *still* sees the stale `false` in memory, because the fast
    ///    path didn't refresh it.  Visually the click does nothing.
    /// 6. Subsequent clicks repeat steps 3-5 forever.
    ///
    /// The fix: the fast path must mirror the disk state back into
    /// the in-memory `Note::frontmatter` (and `byte_size` /
    /// `modified`) before returning, so the next render sees the
    /// truth and the closure captures the right value.
    #[gpui::test]
    async fn star_toggle_resyncs_in_memory_when_disk_already_matches(
        cx: &mut gpui::TestAppContext,
    ) {
        let dir = tempdir().unwrap();
        let path = write(dir.path(), "n.md", "---\ntype: Note\n---\nbody\n");
        let root = dir.path().to_path_buf();
        let mut vault = cx
            .update(|cx| Vault::open_at_async(root, cx))
            .await
            .expect("async open must succeed");
        let id = vault.note_ids_sync()[0];

        // Sanity: in-memory + disk both say `false` at the start.
        assert!(!vault.note_sync(id).unwrap().is_favorite());

        // External write — disk now says `true`, in-memory still says
        // `false`.  Nothing has called `rescan` (production chrome
        // doesn't subscribe to fs-watcher events yet).
        fs::write(&path, "---\ntype: Note\n_favorite: true\n---\nbody\n").unwrap();
        assert!(
            !vault.note_sync(id).unwrap().is_favorite(),
            "sanity: external write must not magically update in-memory state",
        );

        // The toolbar's render-time read still saw `false`, so the
        // click closure dispatches `set_frontmatter_bool(id,
        // "_favorite", !false)` = `true` — the same value disk
        // already has.  Without the 9.2.9 fix this is the
        // fast-path no-op that silently desyncs the UI.
        vault
            .set_frontmatter_bool(id, "_favorite", true)
            .await
            .expect("fast-path call must still return Ok");

        // Load-bearing assertion: after the call, in-memory must
        // reflect the disk state.  Before the fix this fails because
        // the fast path returned early without touching
        // `notes[id].frontmatter`.
        assert!(
            vault.note_sync(id).unwrap().is_favorite(),
            "fast path must re-sync in-memory frontmatter from disk so the next \
             render of the toolbar sees the truth",
        );
    }

    /// Worklist 9.2.9 — `set_frontmatter_bool` must keep working after
    /// the note is rewritten outside the UI (external editor, `git
    /// checkout`, anything that drives a [`Vault::rescan`]).  The user
    /// scenario:
    ///
    /// 1. Toggle `_favorite` on through the toolbar (vault writes
    ///    `_favorite: true` to disk + in-memory).
    /// 2. An external editor rewrites the file on disk — `_favorite`
    ///    flips back to absent (= false).
    /// 3. The chrome calls `Vault::rescan` (Phase 9.6 will do this
    ///    automatically from the fs-watcher; this test drives it
    ///    directly).
    /// 4. The user clicks the star a second time, expecting an
    ///    advance from disk's `false` state to `true`.
    ///
    /// The second toggle must (a) succeed (no `NotFound`), (b) update
    /// the on-disk bytes to reflect the new value, AND (c) leave the
    /// in-memory `Note::is_favorite()` accessor returning the new
    /// value so the next render of the toolbar shows the right glyph.
    #[gpui::test]
    async fn star_toggle_survives_external_edit_plus_rescan(cx: &mut gpui::TestAppContext) {
        let dir = tempdir().unwrap();
        let path = write(dir.path(), "n.md", "---\ntype: Note\n---\nbody\n");
        let root = dir.path().to_path_buf();
        let mut vault = cx
            .update(|cx| Vault::open_at_async(root, cx))
            .await
            .expect("async open must succeed");
        let id = vault.note_ids_sync()[0];

        // Step 1 — toolbar-equivalent: toggle ON via the public API.
        vault
            .set_frontmatter_bool(id, "_favorite", true)
            .await
            .expect("initial toggle-on must succeed");
        assert!(
            fs::read_to_string(&path)
                .unwrap()
                .contains("_favorite: true"),
            "step 1: disk must reflect the toggle"
        );
        assert!(
            vault.note_sync(id).unwrap().is_favorite(),
            "step 1: in-memory must reflect the toggle",
        );

        // Step 2 — external write.  Simulates an editor's atomic-save
        // (write tempfile + rename), or a `git checkout` that drops the
        // `_favorite` line.  Bypasses every vault API on purpose.
        fs::write(&path, "---\ntype: Note\n---\nbody-was-edited\n").unwrap();

        // Step 3 — `Vault::rescan` (in production driven by the
        // fs-watcher, Phase 9.6).  Must preserve the existing
        // `NoteId` for this path and refresh in-memory state from
        // disk.
        vault.rescan().expect("rescan must succeed");
        assert!(
            !vault.note_sync(id).unwrap().is_favorite(),
            "step 3: rescan must pull `_favorite` back to false from disk",
        );

        // Step 4 — second toggle.  This is the load-bearing assertion
        // — without the 9.2.9 fix, the toolbar's captured
        // `is_favorite` was stale (still `true`), so the click
        // dispatched `set_frontmatter_bool(id, "_favorite", false)`
        // and the fast path short-circuited because disk already said
        // `false`.  The fix routes the click through whatever it has
        // to so the second toggle observes the up-to-date state.
        let current = vault.note_sync(id).unwrap().is_favorite();
        let want = !current;
        vault
            .set_frontmatter_bool(id, "_favorite", want)
            .await
            .expect("second toggle must succeed (NotFound = id-stability regression)");
        let on_disk = fs::read_to_string(&path).unwrap();
        assert_eq!(
            on_disk.contains("_favorite: true"),
            want,
            "step 4: disk must reflect the second toggle (want={want}, got={on_disk:?})",
        );
        assert_eq!(
            vault.note_sync(id).unwrap().is_favorite(),
            want,
            "step 4: in-memory frontmatter must mirror disk after the second toggle",
        );
    }

    /// Worklist 9.2.12 (reopened) — `set_frontmatter_bool` must emit a
    /// `VaultChanged` event on the `watch_events` channel so chrome
    /// subscribers (notably `note_list_pane` for the Inbox `_organized`
    /// filter) can refresh their cached projections without waiting
    /// for the OS-level fs-watcher debounce.  The exact shape of the
    /// payload is "paths includes the touched file"; subscribers
    /// rebuild from the vault on any signal rather than mapping
    /// individual paths to ids.
    #[test]
    fn set_frontmatter_bool_emits_watch_event_on_write() {
        let dir = tempdir().unwrap();
        write(dir.path(), "n.md", "---\ntype: Note\n---\nbody\n");
        let mut v = Vault::open_at(dir.path()).unwrap();
        let rx = v.watch_events();
        let id = v.note_ids_sync()[0];
        // Vault canonicalises its root; the in-memory `Note.path` mirrors
        // the canonical form, so the emitted event must match against the
        // canonical path the vault stores.
        let canonical = v.notes.get(&id).unwrap().path.clone();
        // Drop the task immediately — the synchronous fallback runs
        // the write inline before `Task::ready` wraps the result.
        drop(v.set_frontmatter_bool(id, "_organized", true));
        // The OS watcher itself may also fire (the file was just
        // written), but the chrome-emitted event must arrive
        // synchronously so the very next `recv_timeout` sees it.
        let event = rx
            .recv_timeout(std::time::Duration::from_millis(100))
            .expect("write path must emit a synchronous VaultChanged event");
        assert!(
            event.paths.iter().any(|p| p == &canonical),
            "emitted VaultChanged must include the toggled note's canonical path; got {:?}",
            event.paths,
        );
    }

    /// Worklist 9.2.12 (reopened) — the fast path (disk already
    /// matches the requested value) must also emit a `VaultChanged`
    /// event, because the chrome's cached projection may still be
    /// stale even when disk happens to match the click's target.
    /// The 9.2.9 fix re-syncs in-memory from disk on this branch;
    /// the chrome can't see that re-sync without an explicit signal
    /// because the `Vault` is a GPUI `Global`, not an `Entity`.
    #[test]
    fn set_frontmatter_bool_emits_watch_event_on_fast_path() {
        let dir = tempdir().unwrap();
        write(
            dir.path(),
            "n.md",
            "---\ntype: Note\n_organized: true\n---\nbody\n",
        );
        let mut v = Vault::open_at(dir.path()).unwrap();
        let rx = v.watch_events();
        let id = v.note_ids_sync()[0];
        let canonical = v.notes.get(&id).unwrap().path.clone();
        // Disk already says `_organized: true`, so this call takes
        // the fast path (no write).
        drop(v.set_frontmatter_bool(id, "_organized", true));
        let event = rx
            .recv_timeout(std::time::Duration::from_millis(100))
            .expect("fast path must emit a VaultChanged event so subscribers refresh");
        assert!(
            event.paths.iter().any(|p| p == &canonical),
            "fast-path VaultChanged must include the touched canonical path; got {:?}",
            event.paths,
        );
    }

    #[test]
    fn folders_and_assets_are_sorted() {
        let dir = tempdir().unwrap();
        write(dir.path(), "zeta/n.md", "x");
        write(dir.path(), "alpha/n.md", "y");
        write(dir.path(), "mango/n.md", "z");
        write(dir.path(), "zzz.png", "p");
        write(dir.path(), "aaa.pdf", "p");
        let v = Vault::open_at(dir.path()).unwrap();
        let mut folders = v.folders().to_vec();
        let mut sorted_folders = folders.clone();
        sorted_folders.sort();
        assert_eq!(folders, sorted_folders, "folders must be sorted");
        let mut assets = v.assets().to_vec();
        let mut sorted_assets = assets.clone();
        sorted_assets.sort();
        assert_eq!(assets, sorted_assets, "assets must be sorted");
        // Silence unused warnings on the `mut` bindings above.
        folders.clear();
        assets.clear();
    }

    // ---------------------------------------------------------------------
    // Worklist 9.2.3 — wikilink parser + backlinks / outbound_links
    // ---------------------------------------------------------------------

    /// Helper for `wikilink_targets` callers in the tests — collects the
    /// iterator into a `Vec` so equality assertions stay one-liners.
    fn parse_targets(body: &str) -> Vec<&str> {
        wikilink_targets(body).collect()
    }

    #[test]
    fn wikilink_targets_extracts_plain_targets() {
        let body = "Linking to [[Alpha]] and [[Beta]] in the body.";
        assert_eq!(parse_targets(body), vec!["Alpha", "Beta"]);
    }

    #[test]
    fn wikilink_targets_extracts_pipe_aliases() {
        // `[[Target|Alias]]` must yield the target half, not the alias.
        let body = "See [[Alpha|first]] and [[Beta|second]].";
        assert_eq!(parse_targets(body), vec!["Alpha", "Beta"]);
    }

    #[test]
    fn wikilink_targets_trims_surrounding_whitespace() {
        let body = "[[  Alpha  ]] and [[\tBeta\t|alias]]";
        assert_eq!(parse_targets(body), vec!["Alpha", "Beta"]);
    }

    #[test]
    fn wikilink_targets_skips_empty_targets() {
        // `[[]]` and `[[|alias]]` carry no target — drop both.
        let body = "[[]] [[|alias]] [[Alpha]] [[ |only-pipe]]";
        assert_eq!(parse_targets(body), vec!["Alpha"]);
    }

    #[test]
    fn wikilink_targets_handles_no_links() {
        assert!(parse_targets("plain body without any wikilinks").is_empty());
    }

    #[test]
    fn wikilink_targets_handles_unclosed_bracket() {
        // An unterminated `[[Foo` must not panic and must not yield a
        // false-positive target.  Mirrors the React parser's tolerance.
        assert!(parse_targets("dangling [[Foo and nothing else").is_empty());
    }

    /// Three-note vault where A → B and C → B; `backlinks(B)` must
    /// return `[A.id, C.id]` sorted ascending so iteration-order
    /// jitter from the underlying `HashMap` doesn't leak into the
    /// public contract.
    #[test]
    fn backlinks_collects_inbound_links() {
        let dir = tempdir().unwrap();
        write(dir.path(), "A.md", "body links to [[B]].");
        write(dir.path(), "B.md", "I am the target.");
        write(dir.path(), "C.md", "C also points at [[B|nickname]].");
        let v = Vault::open_at(dir.path()).unwrap();
        let id_for_title = |t: &str| {
            v.iter_notes()
                .find(|n| n.title.as_ref() == t)
                .map(|n| n.id)
                .expect("id lookup")
        };
        let id_a = id_for_title("A");
        let id_b = id_for_title("B");
        let id_c = id_for_title("C");
        let mut expected = vec![id_a, id_c];
        expected.sort();
        assert_eq!(v.backlinks(id_b), expected);
    }

    #[test]
    fn backlinks_excludes_the_active_note_itself() {
        // A note linking to its own title (`[[A]]` inside `A.md`) must
        // not surface as its own backlink — the neighbourhood predicate
        // excludes the active note by id.
        let dir = tempdir().unwrap();
        write(dir.path(), "A.md", "self reference [[A]] inside body");
        write(dir.path(), "B.md", "[[A]] from B");
        let v = Vault::open_at(dir.path()).unwrap();
        let id_a = v.iter_notes().find(|n| n.title.as_ref() == "A").unwrap().id;
        let id_b = v.iter_notes().find(|n| n.title.as_ref() == "B").unwrap().id;
        assert_eq!(v.backlinks(id_a), vec![id_b]);
    }

    #[test]
    fn backlinks_returns_empty_for_unknown_id() {
        let dir = tempdir().unwrap();
        write(dir.path(), "A.md", "body");
        let v = Vault::open_at(dir.path()).unwrap();
        assert!(v.backlinks(NoteId(9_999)).is_empty());
    }

    #[test]
    fn outbound_links_resolves_targets_to_ids() {
        let dir = tempdir().unwrap();
        write(
            dir.path(),
            "A.md",
            "links to [[B]] and [[C|alias]] and [[Missing]]",
        );
        write(dir.path(), "B.md", "body");
        write(dir.path(), "C.md", "body");
        let v = Vault::open_at(dir.path()).unwrap();
        let id_a = v.iter_notes().find(|n| n.title.as_ref() == "A").unwrap().id;
        let id_b = v.iter_notes().find(|n| n.title.as_ref() == "B").unwrap().id;
        let id_c = v.iter_notes().find(|n| n.title.as_ref() == "C").unwrap().id;
        let mut expected = vec![id_b, id_c];
        expected.sort();
        let got = v.outbound_links(id_a);
        assert_eq!(got, expected, "missing-target wikilinks must drop silently");
    }

    #[test]
    fn outbound_links_deduplicates_repeated_targets() {
        // The same `[[B]]` mentioned three times must surface `B.id`
        // only once — neighbourhood-mode's id-set predicate would dupe
        // the entry otherwise.
        let dir = tempdir().unwrap();
        write(dir.path(), "A.md", "[[B]] [[B|alias]] and once more [[B]]");
        write(dir.path(), "B.md", "body");
        let v = Vault::open_at(dir.path()).unwrap();
        let id_a = v.iter_notes().find(|n| n.title.as_ref() == "A").unwrap().id;
        let id_b = v.iter_notes().find(|n| n.title.as_ref() == "B").unwrap().id;
        assert_eq!(v.outbound_links(id_a), vec![id_b]);
    }

    #[test]
    fn outbound_links_excludes_self_link() {
        let dir = tempdir().unwrap();
        write(dir.path(), "A.md", "self [[A]] and [[B]]");
        write(dir.path(), "B.md", "body");
        let v = Vault::open_at(dir.path()).unwrap();
        let id_a = v.iter_notes().find(|n| n.title.as_ref() == "A").unwrap().id;
        let id_b = v.iter_notes().find(|n| n.title.as_ref() == "B").unwrap().id;
        assert_eq!(v.outbound_links(id_a), vec![id_b]);
    }

    #[test]
    fn outbound_links_returns_empty_for_unknown_id() {
        let dir = tempdir().unwrap();
        write(dir.path(), "A.md", "body");
        let v = Vault::open_at(dir.path()).unwrap();
        assert!(v.outbound_links(NoteId(9_999)).is_empty());
    }
}
