//! Mock git service for Phase 2 chrome rendering.
//!
//! [`MockGit`] is a [`Global`] holding a hardcoded working-tree status (3
//! modified files + 1 untracked) and a five-commit linear history.  All
//! methods return [`Task<T>`] for forward-compatibility with Phase 3 real
//! services.

use chrono::{DateTime, TimeZone as _, Utc};
use gpui::{Global, Task};

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Status of a single tracked or untracked path.
///
/// `Added` and `Deleted` are present for Phase-3 forward-compatibility even
/// though the seeded fixture only uses `Modified` and `Untracked`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileStatus {
    Modified,
    Untracked,
    Added,
    Deleted,
}

/// Status entry for one file in the working tree.
#[derive(Debug, Clone)]
pub struct MockFileStatus {
    pub path: String,
    pub status: FileStatus,
}

/// Aggregated working-tree status.
#[derive(Debug, Clone)]
pub struct MockGitStatus {
    pub files: Vec<MockFileStatus>,
}

impl MockGitStatus {
    /// Count of files with the given status.
    pub fn count(&self, status: FileStatus) -> usize {
        self.files.iter().filter(|f| f.status == status).count()
    }
}

/// A single commit in the mock linear history.
#[derive(Debug, Clone)]
pub struct MockCommit {
    pub sha: String,
    pub message: String,
    pub author: String,
    pub timestamp: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// Error types
// ---------------------------------------------------------------------------

/// Errors returned by [`MockGit`] mutation methods.
#[derive(Debug, thiserror::Error)]
pub enum GitError {
    /// A commit was attempted with an empty or whitespace-only message.
    #[error("commit message must not be empty")]
    EmptyMessage,
}

// ---------------------------------------------------------------------------
// MockGit
// ---------------------------------------------------------------------------

/// Mock git service: 3 modified files, 1 untracked, 5-commit linear history.
///
/// Install once at app startup:
/// ```rust,ignore
/// cx.set_global(MockGit::seeded());
/// ```
pub struct MockGit {
    status: MockGitStatus,
    history: Vec<MockCommit>,
    /// Monotonically-increasing counter used to generate unique commit SHAs.
    next_sha_nonce: u64,
}

impl Global for MockGit {}

impl MockGit {
    /// Construct the fixture git state.
    pub fn seeded() -> Self {
        let ts = |y: i32, m: u32, d: u32, h: u32| {
            Utc.with_ymd_and_hms(y, m, d, h, 0, 0)
                .single()
                .expect("fixture timestamp components are valid (compile-time constants)")
        };
        Self {
            status: MockGitStatus {
                files: vec![
                    MockFileStatus {
                        path: "25q2-laputa-v2.md".to_string(),
                        status: FileStatus::Modified,
                    },
                    MockFileStatus {
                        path: "measure-close-rate.md".to_string(),
                        status: FileStatus::Modified,
                    },
                    MockFileStatus {
                        path: "procedure-quarterly-sponsor-outreach.md".to_string(),
                        status: FileStatus::Modified,
                    },
                    MockFileStatus {
                        path: "draft-feature-ideas.md".to_string(),
                        status: FileStatus::Untracked,
                    },
                ],
            },
            history: vec![
                MockCommit {
                    sha: "a1b2c3d".to_string(),
                    message: "docs: update sponsorship close-rate tracking".to_string(),
                    author: "Luca Rossi".to_string(),
                    timestamp: ts(2026, 5, 17, 10),
                },
                MockCommit {
                    sha: "e4f5a6b".to_string(),
                    message: "feat: add Q2 project milestones".to_string(),
                    author: "Sofia Chen".to_string(),
                    timestamp: ts(2026, 5, 16, 15),
                },
                MockCommit {
                    sha: "c7d8e9f".to_string(),
                    message: "refactor: reorganise sponsorship procedures".to_string(),
                    author: "Matteo Cellini".to_string(),
                    timestamp: ts(2026, 5, 14, 11),
                },
                MockCommit {
                    sha: "0a1b2c3".to_string(),
                    message: "feat: add RTL mixed-direction QA note".to_string(),
                    author: "James Okafor".to_string(),
                    timestamp: ts(2026, 5, 10, 9),
                },
                MockCommit {
                    sha: "d4e5f6a".to_string(),
                    message: "init: seed demo vault v2 with initial notes".to_string(),
                    author: "Luca Rossi".to_string(),
                    timestamp: ts(2026, 4, 30, 8),
                },
            ],
            next_sha_nonce: 5,
        }
    }

    /// Current working-tree status.
    pub fn status(&self) -> Task<MockGitStatus> {
        Task::ready(self.status.clone())
    }

    /// Five-commit linear history, newest first.
    pub fn history(&self) -> Task<Vec<MockCommit>> {
        Task::ready(self.history.clone())
    }

    /// Record a new commit, clear modified files from the status, and prepend
    /// the commit to the history.  Returns `Err` if `message` is empty.
    ///
    /// Call via `cx.global_mut::<MockGit>().commit(…)` so GPUI notifies
    /// observers after the mutation.
    pub fn commit(&mut self, message: impl Into<String>) -> Task<Result<(), GitError>> {
        let message = message.into();
        if message.trim().is_empty() {
            return Task::ready(Err(GitError::EmptyMessage));
        }
        log::debug!("MockGit: committing \"{}\"", message);
        self.next_sha_nonce += 1;
        let new = MockCommit {
            sha: format!("{:07x}", self.next_sha_nonce),
            message,
            author: "Fixture Author".to_string(),
            timestamp: Utc
                .with_ymd_and_hms(2026, 5, 17, 12, 0, 0)
                .single()
                .expect("fixture timestamp components are valid (compile-time constants)"),
        };
        self.history.insert(0, new);
        self.status
            .files
            .retain(|f| f.status == FileStatus::Untracked);
        Task::ready(Ok(()))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::TestAppContext;

    #[gpui::test]
    async fn git_status_returns_expected_counts(_cx: &mut TestAppContext) {
        let git = MockGit::seeded();
        let status = git.status().await;
        assert_eq!(status.count(FileStatus::Modified), 3);
        assert_eq!(status.count(FileStatus::Untracked), 1);
    }

    #[gpui::test]
    async fn git_history_has_five_commits(_cx: &mut TestAppContext) {
        let git = MockGit::seeded();
        let commits = git.history().await;
        assert_eq!(
            commits.len(),
            5,
            "seeded history must have exactly 5 commits"
        );
        assert_eq!(commits[0].sha, "a1b2c3d", "newest commit must be first");
    }

    #[gpui::test]
    async fn commit_prepends_to_history(_cx: &mut TestAppContext) {
        let mut git = MockGit::seeded();
        git.commit("test: add fixture commit".to_string())
            .await
            .expect("commit must succeed with valid message");
        let commits = git.history().await;
        assert_eq!(commits.len(), 6, "history must grow by one after commit");
        assert_eq!(commits[0].message, "test: add fixture commit");
    }

    #[gpui::test]
    async fn commit_clears_modified_files(_cx: &mut TestAppContext) {
        let mut git = MockGit::seeded();
        git.commit("chore: clear working tree".to_string())
            .await
            .unwrap();
        let status = git.status().await;
        assert_eq!(
            status.count(FileStatus::Modified),
            0,
            "modified files must be cleared after commit"
        );
    }

    #[gpui::test]
    async fn commit_fails_with_empty_message(_cx: &mut TestAppContext) {
        let mut git = MockGit::seeded();
        let result = git.commit(String::new()).await;
        assert!(result.is_err(), "empty commit message must return an error");
    }

    #[gpui::test]
    fn mock_git_installs_as_global(cx: &mut TestAppContext) {
        cx.update(|cx| {
            cx.set_global(MockGit::seeded());
            let commits = cx.global::<MockGit>().history.len();
            assert_eq!(commits, 5);
        });
    }
}
