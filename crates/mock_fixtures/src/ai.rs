//! Mock AI service for Phase 2 chrome rendering.
//!
//! [`MockAi`] is a [`Global`] holding a single in-memory conversation thread
//! with four turns, including one tool-use round-trip.  All methods return
//! [`Task<T>`] for forward-compatibility with Phase 3 real streaming services.

use chrono::{DateTime, TimeZone as _, Utc};
use gpui::{Global, Task};

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Unique identifier for a conversation thread.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ThreadId(pub u64);

/// Role of a message participant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MessageRole {
    User,
    Assistant,
    /// A tool-use / function-call result round-trip.
    Tool,
}

/// A single message in a conversation thread.
#[derive(Debug, Clone)]
pub struct MockMessage {
    pub role: MessageRole,
    pub content: String,
    pub timestamp: DateTime<Utc>,
}

/// A conversation thread between the user and the AI.
#[derive(Debug, Clone)]
pub struct MockThread {
    pub id: ThreadId,
    pub title: String,
    pub messages: Vec<MockMessage>,
}

// ---------------------------------------------------------------------------
// Error types
// ---------------------------------------------------------------------------

/// Errors returned by [`MockAi`] mutation methods.
#[derive(Debug, thiserror::Error)]
pub enum AiError {
    /// The requested conversation thread does not exist.
    #[error("thread {0:?} not found")]
    ThreadNotFound(ThreadId),
}

// ---------------------------------------------------------------------------
// MockAi
// ---------------------------------------------------------------------------

/// Mock AI service with one seeded four-turn conversation thread.
///
/// Install once at app startup:
/// ```rust,ignore
/// cx.set_global(MockAi::seeded());
/// ```
pub struct MockAi {
    pub(crate) threads: Vec<MockThread>,
}

impl Global for MockAi {}

impl MockAi {
    /// Construct the fixture AI state.
    pub fn seeded() -> Self {
        let ts = |h: u32, min: u32| {
            Utc.with_ymd_and_hms(2026, 5, 17, h, min, 0)
                .single()
                .expect("fixture timestamp components are valid (compile-time constants)")
        };
        Self {
            threads: vec![MockThread {
                id: ThreadId(1),
                title: "Summarise Q2 project status".to_string(),
                messages: vec![
                    MockMessage {
                        role: MessageRole::User,
                        content: "Can you summarise the status of the Q2 Laputa project?"
                            .to_string(),
                        timestamp: ts(10, 0),
                    },
                    MockMessage {
                        role: MessageRole::Assistant,
                        content: "I'll look up the Q2 project note for you.".to_string(),
                        timestamp: ts(10, 0),
                    },
                    MockMessage {
                        role: MessageRole::Tool,
                        content: concat!(
                            r#"{"tool":"get_note","input":{"path":"25q2-laputa-v2.md"},"#,
                            r#""output":"Ships wikilink autocomplete, live inspector, and the full command-palette action set."}"#
                        )
                        .to_string(),
                        timestamp: ts(10, 1),
                    },
                    MockMessage {
                        role: MessageRole::Assistant,
                        content: concat!(
                            "The Q2 Laputa project (Laputa App V2) is currently in progress. ",
                            "Key deliverables include wikilink autocomplete, a live inspector, ",
                            "and the full command-palette action set. ",
                            "The project note shows 3 modified files pending commit."
                        )
                        .to_string(),
                        timestamp: ts(10, 1),
                    },
                ],
            }],
        }
    }

    /// IDs of all conversation threads.
    pub fn threads(&self) -> Task<Vec<ThreadId>> {
        Task::ready(self.threads.iter().map(|t| t.id).collect())
    }

    /// Look up a thread by ID; returns `None` if not found.
    pub fn thread(&self, id: ThreadId) -> Task<Option<MockThread>> {
        Task::ready(self.threads.iter().find(|t| t.id == id).cloned())
    }

    /// Append a user message and an immediate canned assistant reply.
    ///
    /// Returns [`AiError::ThreadNotFound`] when `thread_id` does not exist, so
    /// callers detect typos rather than silently losing messages.
    ///
    /// The instant resolution preserves `Task<Result<(), AiError>>` forward-
    /// compatibility with Phase 3 real services that will return streaming tasks.
    ///
    /// Call via `cx.global_mut::<MockAi>().send_message(…)` so GPUI notifies
    /// observers after the mutation.
    pub fn send_message(
        &mut self,
        thread_id: ThreadId,
        text: impl Into<String>,
    ) -> Task<Result<(), AiError>> {
        let text = text.into();
        log::debug!("MockAi: send_message on thread {}", thread_id.0);
        let ts = Utc
            .with_ymd_and_hms(2026, 5, 17, 12, 0, 0)
            .single()
            .expect("fixture timestamp components are valid (compile-time constants)");
        match self.threads.iter_mut().find(|t| t.id == thread_id) {
            Some(thread) => {
                thread.messages.push(MockMessage {
                    role: MessageRole::User,
                    content: text,
                    timestamp: ts,
                });
                thread.messages.push(MockMessage {
                    role: MessageRole::Assistant,
                    content: "This is a canned response from the fixture AI service.".to_string(),
                    timestamp: ts,
                });
                Task::ready(Ok(()))
            }
            None => Task::ready(Err(AiError::ThreadNotFound(thread_id))),
        }
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
    async fn seeded_ai_has_one_thread(_cx: &mut TestAppContext) {
        let ai = MockAi::seeded();
        let ids = ai.threads().await;
        assert_eq!(ids.len(), 1);
        assert_eq!(ids[0], ThreadId(1));
    }

    #[gpui::test]
    async fn thread_has_four_messages(_cx: &mut TestAppContext) {
        let ai = MockAi::seeded();
        let thread = ai.thread(ThreadId(1)).await.expect("thread 1 must exist");
        assert_eq!(
            thread.messages.len(),
            4,
            "seeded thread must have exactly 4 messages"
        );
    }

    #[gpui::test]
    async fn thread_contains_tool_use_turn(_cx: &mut TestAppContext) {
        let ai = MockAi::seeded();
        let thread = ai.thread(ThreadId(1)).await.unwrap();
        let has_tool = thread.messages.iter().any(|m| m.role == MessageRole::Tool);
        assert!(has_tool, "seeded thread must include a tool-use message");
    }

    #[gpui::test]
    async fn send_message_appends_user_and_assistant_messages(_cx: &mut TestAppContext) {
        let mut ai = MockAi::seeded();
        let before = ai.threads[0].messages.len();
        ai.send_message(ThreadId(1), "follow-up question")
            .await
            .expect("send_message must succeed for known thread");
        let thread = ai.thread(ThreadId(1)).await.unwrap();
        assert_eq!(
            thread.messages.len(),
            before + 2,
            "send_message must append a user turn and an assistant reply"
        );
        assert_eq!(thread.messages[before].role, MessageRole::User);
        assert_eq!(thread.messages[before + 1].role, MessageRole::Assistant);
    }

    #[gpui::test]
    async fn send_message_fails_for_unknown_thread(_cx: &mut TestAppContext) {
        let mut ai = MockAi::seeded();
        let result = ai.send_message(ThreadId(999), "hello").await;
        assert!(
            result.is_err(),
            "send_message must fail for unknown thread id"
        );
    }

    #[gpui::test]
    async fn thread_lookup_returns_none_for_unknown_id(_cx: &mut TestAppContext) {
        let ai = MockAi::seeded();
        let result = ai.thread(ThreadId(999)).await;
        assert!(result.is_none());
    }

    #[gpui::test]
    fn mock_ai_installs_as_global(cx: &mut TestAppContext) {
        cx.update(|cx| {
            cx.set_global(MockAi::seeded());
            assert_eq!(cx.global::<MockAi>().threads.len(), 1);
        });
    }
}
