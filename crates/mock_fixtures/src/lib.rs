#![forbid(unsafe_code)]
//! Hardcoded fixture services for Tolaria Phase 2 chrome development.
//!
//! Every public method returns [`gpui::Task<T>`] — even when resolved
//! instantly via `Task::ready(…)` — so chrome crates compile against the same
//! call shape that Phase 3 real services will expose.  Swapping out
//! `mock_fixtures` for live crates in Phase 3 requires only a `Cargo.toml`
//! dependency change, not API changes in the chrome crates.
//!
//! # Registering as GPUI globals (`TOLARIA_MOCK=1` mode)
//!
//! ```rust,ignore
//! cx.set_global(MockVault::seeded());
//! cx.set_global(MockGit::seeded());
//! cx.set_global(MockAi::seeded());
//! cx.set_global(MockSearch);
//! ```

pub mod ai;
pub mod git;
pub mod search;
pub mod settings;
pub mod vault;

pub use ai::{AiError, MessageRole, MockAi, MockMessage, MockThread, ThreadId};
pub use git::{FileStatus, GitError, MockCommit, MockFileStatus, MockGit, MockGitStatus};
pub use search::{MockSearch, SearchHit};
pub use settings::{MockSettings, ThemeChoice, WindowSettings};
pub use vault::{MockNote, MockVault, NoteId, NoteKind, VaultError};

// Compile-time assertion: all `Global` mock types must be `Send + Sync` so they
// can be safely accessed from background tasks when Phase 3 services are wired.
const _: fn() = || {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<MockVault>();
    assert_send_sync::<MockGit>();
    assert_send_sync::<MockAi>();
    assert_send_sync::<MockSearch>();
};
