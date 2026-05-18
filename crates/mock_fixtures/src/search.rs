//! Deterministic in-memory full-text search over the fixture vault.
//!
//! [`MockSearch`] maps a small table of known query strings to hardcoded
//! [`SearchHit`] slices.  Unknown queries return an empty result.  All methods
//! return [`Task<T>`] for forward-compatibility with Phase 3 real services.

use gpui::{Global, Task};

use crate::vault::NoteId;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// A single full-text search result.
#[derive(Debug, Clone)]
pub struct SearchHit {
    /// ID of the matching note in the vault.
    pub note_id: NoteId,
    /// Short excerpt showing context around the match.
    pub excerpt: String,
    /// Relevance score in [0.0, 1.0]; higher is more relevant.
    pub score: f32,
}

// ---------------------------------------------------------------------------
// MockSearch
// ---------------------------------------------------------------------------

/// Stateless mock search service with hardcoded query → result mappings.
///
/// Install once at app startup:
/// ```rust,ignore
/// cx.set_global(MockSearch);
/// ```
pub struct MockSearch;

impl Global for MockSearch {}

impl MockSearch {
    /// Returns the stateless mock search service.  Provided for API symmetry
    /// with [`MockVault::seeded`], [`MockGit::seeded`], and [`MockAi::seeded`].
    pub fn seeded() -> Self {
        Self
    }

    /// Run a full-text search.
    ///
    /// # Recognised queries
    ///
    /// | Query | Note IDs returned |
    /// |---|---|
    /// | `"todo"` | 14, 15, 16 (project notes) |
    /// | `"sponsor"` / `"sponsorship"` | 20, 21, 24 |
    /// | `"quarterly"` | 20 |
    /// | `"laputa"` | 14, 15, 16 |
    /// | `"procedure"` | 20, 21, 22, 23 |
    /// | anything else | empty |
    pub fn query(&self, text: &str) -> Task<Vec<SearchHit>> {
        let lower = text.to_lowercase();
        let hits = match lower.trim() {
            "todo" => vec![
                SearchHit {
                    note_id: NoteId(14),
                    excerpt: "…The original spike that proved Tolaria could read a markdown vault…"
                        .to_string(),
                    score: 0.90,
                },
                SearchHit {
                    note_id: NoteId(15),
                    excerpt: "…The first usable release for daily browsing, quick open…"
                        .to_string(),
                    score: 0.85,
                },
                SearchHit {
                    note_id: NoteId(16),
                    excerpt: "…TODO: finalise dock layout and Phase 3 service wiring…".to_string(),
                    score: 0.80,
                },
            ],
            "sponsor" | "sponsorship" => vec![
                SearchHit {
                    note_id: NoteId(20),
                    excerpt: "…Review the pipeline, choose the next target companies…".to_string(),
                    score: 0.95,
                },
                SearchHit {
                    note_id: NoteId(21),
                    excerpt: "…Onboard a new sponsor after the contract is signed…".to_string(),
                    score: 0.90,
                },
                SearchHit {
                    note_id: NoteId(24),
                    excerpt: "…Owns the Laputa sponsorship pipeline end-to-end…".to_string(),
                    score: 0.85,
                },
            ],
            "quarterly" => vec![SearchHit {
                note_id: NoteId(20),
                excerpt: "…Send a fresh outreach batch each quarter…".to_string(),
                score: 0.98,
            }],
            "laputa" => vec![
                SearchHit {
                    note_id: NoteId(14),
                    excerpt: "…Start Laputa App Project…".to_string(),
                    score: 0.95,
                },
                SearchHit {
                    note_id: NoteId(15),
                    excerpt: "…Laputa App V1…".to_string(),
                    score: 0.90,
                },
                SearchHit {
                    note_id: NoteId(16),
                    excerpt: "…Laputa App V2…".to_string(),
                    score: 0.85,
                },
            ],
            "procedure" => vec![
                SearchHit {
                    note_id: NoteId(20),
                    excerpt: "…Quarterly Sponsor Outreach…".to_string(),
                    score: 0.88,
                },
                SearchHit {
                    note_id: NoteId(21),
                    excerpt: "…Sponsor Onboarding…".to_string(),
                    score: 0.85,
                },
                SearchHit {
                    note_id: NoteId(22),
                    excerpt: "…Release Checklist…".to_string(),
                    score: 0.82,
                },
                SearchHit {
                    note_id: NoteId(23),
                    excerpt: "…Incident Response…".to_string(),
                    score: 0.80,
                },
            ],
            _ => vec![],
        };
        Task::ready(hits)
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
    async fn sponsor_query_returns_three_hits(_cx: &mut TestAppContext) {
        let search = MockSearch;
        let hits = search.query("sponsor").await;
        assert_eq!(hits.len(), 3);
        assert_eq!(hits[0].note_id, NoteId(20));
    }

    #[gpui::test]
    async fn quarterly_query_returns_one_hit(_cx: &mut TestAppContext) {
        let search = MockSearch;
        let hits = search.query("quarterly").await;
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].note_id, NoteId(20));
    }

    #[gpui::test]
    async fn unknown_query_returns_empty(_cx: &mut TestAppContext) {
        let search = MockSearch;
        let hits = search.query("xyzzy").await;
        assert!(hits.is_empty(), "unknown query must return no hits");
    }

    #[gpui::test]
    async fn laputa_query_returns_three_projects(_cx: &mut TestAppContext) {
        let search = MockSearch;
        let hits = search.query("laputa").await;
        assert_eq!(hits.len(), 3);
        let ids: Vec<_> = hits.iter().map(|h| h.note_id).collect();
        assert!(ids.contains(&NoteId(14)));
        assert!(ids.contains(&NoteId(15)));
        assert!(ids.contains(&NoteId(16)));
    }

    #[gpui::test]
    async fn hits_are_ordered_by_descending_score(_cx: &mut TestAppContext) {
        let search = MockSearch;
        let hits = search.query("sponsor").await;
        let scores: Vec<f32> = hits.iter().map(|h| h.score).collect();
        let mut sorted = scores.clone();
        sorted.sort_by(|a, b| b.total_cmp(a));
        assert_eq!(scores, sorted, "hits must be ordered highest-score first");
    }
}
