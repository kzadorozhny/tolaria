#![forbid(unsafe_code)]
#![warn(missing_docs)]
//! Typed JSON envelope for native ⇄ embedded editor IPC (ADR-0115 Phase
//! 4-MVP).
//!
//! Two enums describe the wire protocol:
//!
//! - [`ToHost`] — messages the native shell sends down to the editor
//!   running inside the `WKWebView` (`window.tolariaBridge.receive(json)`).
//! - [`FromHost`] — messages the editor sends back over `wry`'s IPC
//!   channel (`window.ipc.postMessage(json)`).
//!
//! Both enums share the `{ "k": <kind>, "v": <payload> }` envelope shape
//! pioneered by `embed_poc` so the JS side can dispatch with a single
//! `switch (msg.k)`.
//!
//! # Versioning
//!
//! The wire protocol is *not* versioned for MVP — the editor host and
//! native shell ship as one binary.  When Phase 8 grows out-of-process
//! editor sandboxing we will add a `v: u32` header and treat unknown
//! variants as protocol-version mismatch.

use serde::{Deserialize, Serialize};
use thiserror::Error;
use vault::NoteId;

// ---------------------------------------------------------------------------
// Native → Editor
// ---------------------------------------------------------------------------

/// Messages the native shell sends to the embedded editor.
///
/// The serialised form is `{ "k": "<kind>", "v": <payload> }`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "k", content = "v", rename_all = "snake_case")]
pub enum ToHost {
    /// Load a note into the editor.  Replaces any current buffer.
    NoteOpen(NoteOpen),

    /// Move focus into the editor body (used after Cmd+L → quick-open
    /// selects a note, etc.).
    FocusEditor,

    /// Editor should respond by sending [`FromHost::Save`] with the
    /// current buffer body, or — if not dirty — [`FromHost::Saved`].
    SaveRequest,

    /// Theme tracking; the editor restyles its chrome to match.
    ThemeSet(ThemeSet),

    /// Flip the embedded editor between BlockNote (rich) and the
    /// CodeMirror raw-text view (Phase 9 worklist 9.2.4).
    ///
    /// Chrome side owns the toggle state on a per-`NoteItem` basis; the
    /// editor reacts to this envelope by forcing the corresponding
    /// surface to mount.  Markdown notes are the only ones that can
    /// flip — non-markdown paths already route through the raw editor
    /// unconditionally via `shouldUseRawEditor(path)` and treat this
    /// envelope as a no-op.
    SetRawMode(SetRawMode),
}

/// Payload for [`ToHost::NoteOpen`].  `path` is included for
/// window-title / debug surface; `body` is the on-disk markdown.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoteOpen {
    /// Vault-scoped note identifier.
    pub id: NoteId,
    /// Absolute path on disk (informational; the editor never writes
    /// directly — it sends [`FromHost::Save`] and the native shell
    /// persists through `vault::Vault`).
    pub path: String,
    /// Initial buffer body in markdown.
    pub body: String,
}

/// Light / dark theme selector for [`ToHost::ThemeSet`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThemeMode {
    /// Light theme.
    Light,
    /// Dark theme.
    Dark,
}

/// Payload for [`ToHost::ThemeSet`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThemeSet {
    /// Active theme mode.
    pub mode: ThemeMode,
}

/// Payload for [`ToHost::SetRawMode`].
///
/// `enabled = true` forces the CodeMirror raw editor to mount over the
/// currently-active markdown note; `enabled = false` reverts to the
/// BlockNote rich editor.  Non-markdown notes (the host's
/// `shouldUseRawEditor(path)` returns `true`) ignore the toggle —
/// they're already raw by construction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetRawMode {
    /// `true` mounts the raw editor; `false` mounts BlockNote.
    pub enabled: bool,
}

// ---------------------------------------------------------------------------
// Editor → Native
// ---------------------------------------------------------------------------

/// Messages the embedded editor sends to the native shell.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "k", content = "v", rename_all = "snake_case")]
pub enum FromHost {
    /// Editor mounted; safe to send [`ToHost::NoteOpen`].  Carries no
    /// payload.
    Ready,

    /// Buffer diverged from the last `NoteOpen` / save.  Native shell
    /// surfaces this in the status bar (dirty dot) and in
    /// confirm-discard prompts.
    Dirty(NoteRef),

    /// Editor wishes to persist `body` for note `id`.  Sent in reply to
    /// [`ToHost::SaveRequest`] when the buffer is dirty, or
    /// autonomously on Cmd+S.
    Save(NoteSave),

    /// Editor wishes to persist nothing — buffer was clean when the
    /// save was requested.
    Saved(NoteRef),

    /// User clicked an internal wikilink or external link.  Native
    /// shell routes the target via `vault::search_titles` (for
    /// wikilinks) or `cx.open_url` (for `http(s)://`).
    LinkClick(LinkClick),

    /// Pass-through of a keydown event the editor caught.  Native side
    /// runs it through the action registry — this is how Cmd+S inside
    /// the `WKWebView` dispatches the `Save` action, satisfying
    /// ADR-0115 §6 re-evaluation trigger #4.
    Keydown(Keydown),
}

/// Reference to a note by ID, used by stateless [`FromHost`] events.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoteRef {
    /// Vault-scoped note identifier.
    pub id: NoteId,
}

/// Payload for [`FromHost::Save`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoteSave {
    /// Vault-scoped note identifier.
    pub id: NoteId,
    /// Full buffer body to persist.
    pub body: String,
}

/// Payload for [`FromHost::LinkClick`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinkClick {
    /// Either a wikilink target (`MyNote`) or a fully-qualified URL.
    pub target: String,
}

/// Modifier set carried by [`Keydown`].
///
/// Booleans serialise on the wire only when set, so a `Cmd+S` arrives
/// as `{"meta":true}` rather than the full four-field record.  Lets
/// the native dispatch table pattern-match without parsing a
/// dash-joined string.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mods {
    /// `Option` / `Alt` key.
    #[serde(default, skip_serializing_if = "is_false")]
    pub alt: bool,
    /// `Control` key.
    #[serde(default, skip_serializing_if = "is_false")]
    pub ctrl: bool,
    /// `Command` (macOS) / `Meta` (other platforms) key.
    #[serde(default, skip_serializing_if = "is_false")]
    pub meta: bool,
    /// `Shift` key.
    #[serde(default, skip_serializing_if = "is_false")]
    pub shift: bool,
}

fn is_false(b: &bool) -> bool {
    !*b
}

/// Payload for [`FromHost::Keydown`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Keydown {
    /// `event.key` — e.g. `"s"`, `"Escape"`.
    pub key: String,
    /// Active modifier set at keydown time.
    #[serde(default)]
    pub mods: Mods,
}

// ---------------------------------------------------------------------------
// Encode / decode helpers
// ---------------------------------------------------------------------------

/// Errors returned by the encode / decode helpers.  Each variant wraps
/// the underlying [`serde_json::Error`] via `#[source]` so callers
/// walking `std::error::Error::source()` retain the original
/// line / column info.
#[derive(Debug, Error)]
pub enum BridgeError {
    /// Failed to serialise a [`ToHost`] / [`FromHost`] message.
    #[error("encode failed")]
    Encode(#[source] serde_json::Error),
    /// Failed to deserialise an incoming envelope.
    #[error("decode failed")]
    Decode(#[source] serde_json::Error),
}

/// Serialise a [`ToHost`] message for delivery into the `WKWebView`.
///
/// # Errors
///
/// Returns [`BridgeError::Encode`] if `serde_json` rejects the payload
/// — in practice never, because every field is a primitive or `String`.
pub fn encode_to_host(msg: &ToHost) -> Result<String, BridgeError> {
    serde_json::to_string(msg).map_err(BridgeError::Encode)
}

/// Serialise a [`FromHost`] message — symmetric to [`encode_to_host`],
/// useful for tests and for any future tool that wants to replay an
/// IPC trace.
///
/// # Errors
///
/// Returns [`BridgeError::Encode`] under the same conditions as
/// [`encode_to_host`].
pub fn encode_from_host(msg: &FromHost) -> Result<String, BridgeError> {
    serde_json::to_string(msg).map_err(BridgeError::Encode)
}

/// Parse a [`FromHost`] message arriving over wry's IPC channel.
///
/// # Errors
///
/// Returns [`BridgeError::Decode`] if `body` is not valid JSON or does
/// not match the envelope schema.
pub fn decode_from_host(body: &str) -> Result<FromHost, BridgeError> {
    serde_json::from_str(body).map_err(BridgeError::Decode)
}

/// Parse a [`ToHost`] message — symmetric to [`decode_from_host`], so
/// tests and tooling that round-trip in the opposite direction stay
/// on the public surface.
///
/// # Errors
///
/// Returns [`BridgeError::Decode`] under the same conditions as
/// [`decode_from_host`].
pub fn decode_to_host(body: &str) -> Result<ToHost, BridgeError> {
    serde_json::from_str(body).map_err(BridgeError::Decode)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn nid(n: u64) -> NoteId {
        NoteId::from_raw(n)
    }

    #[test]
    fn to_host_note_open_roundtrip() {
        let msg = ToHost::NoteOpen(NoteOpen {
            id: nid(7),
            path: "/v/a.md".into(),
            body: "# hi\n".into(),
        });
        let json = encode_to_host(&msg).unwrap();
        assert!(json.contains(r#""k":"note_open""#), "envelope kind tag");
        assert!(
            json.contains(r#""id":7"#),
            "NoteId(7) must serialise as bare integer 7, got {json}"
        );
        assert_eq!(decode_to_host(&json).unwrap(), msg);
    }

    #[test]
    fn to_host_focus_editor_has_no_payload() {
        let msg = ToHost::FocusEditor;
        let json = encode_to_host(&msg).unwrap();
        assert_eq!(json, r#"{"k":"focus_editor"}"#);
    }

    #[test]
    fn to_host_save_request_has_no_payload() {
        assert_eq!(
            encode_to_host(&ToHost::SaveRequest).unwrap(),
            r#"{"k":"save_request"}"#
        );
    }

    #[test]
    fn to_host_theme_set_serialises_mode_lowercase() {
        let json = encode_to_host(&ToHost::ThemeSet(ThemeSet {
            mode: ThemeMode::Dark,
        }))
        .unwrap();
        assert!(json.contains(r#""mode":"dark""#));
    }

    #[test]
    fn to_host_set_raw_mode_roundtrip() {
        // Worklist 9.2.4 — the chrome owns raw-mode state and pushes
        // the toggle down through this envelope.  Lock the wire shape
        // (`{"k":"set_raw_mode","v":{"enabled":true}}`) so a future
        // rename can't silently break the TypeScript dispatcher.
        let msg = ToHost::SetRawMode(SetRawMode { enabled: true });
        let json = encode_to_host(&msg).unwrap();
        assert_eq!(json, r#"{"k":"set_raw_mode","v":{"enabled":true}}"#);
        assert_eq!(decode_to_host(&json).unwrap(), msg);
    }

    #[test]
    fn to_host_set_raw_mode_disabled_roundtrip() {
        // Same shape, opposite boolean — the field must not be elided
        // when `false`, so the editor side can always read `v.enabled`
        // without an `?? false` fallback.
        let msg = ToHost::SetRawMode(SetRawMode { enabled: false });
        let json = encode_to_host(&msg).unwrap();
        assert_eq!(json, r#"{"k":"set_raw_mode","v":{"enabled":false}}"#);
        assert_eq!(decode_to_host(&json).unwrap(), msg);
    }

    #[test]
    fn from_host_ready_decodes() {
        assert_eq!(
            decode_from_host(r#"{"k":"ready"}"#).unwrap(),
            FromHost::Ready
        );
    }

    #[test]
    fn from_host_dirty_decodes() {
        let parsed = decode_from_host(r#"{"k":"dirty","v":{"id":42}}"#).unwrap();
        assert_eq!(parsed, FromHost::Dirty(NoteRef { id: nid(42) }));
    }

    #[test]
    fn from_host_save_decodes_with_body() {
        let parsed = decode_from_host(r#"{"k":"save","v":{"id":1,"body":"text"}}"#).unwrap();
        assert_eq!(
            parsed,
            FromHost::Save(NoteSave {
                id: nid(1),
                body: "text".into()
            })
        );
    }

    #[test]
    fn from_host_keydown_decodes_with_meta_mod() {
        let parsed =
            decode_from_host(r#"{"k":"keydown","v":{"key":"s","mods":{"meta":true}}}"#).unwrap();
        assert_eq!(
            parsed,
            FromHost::Keydown(Keydown {
                key: "s".into(),
                mods: Mods {
                    meta: true,
                    ..Default::default()
                },
            })
        );
    }

    #[test]
    fn from_host_keydown_omitted_mods_defaults_to_no_mods() {
        // `Keydown.mods` is `#[serde(default)]` so an empty modifier
        // set can omit the field entirely on the wire.
        let parsed = decode_from_host(r#"{"k":"keydown","v":{"key":"a"}}"#).unwrap();
        assert_eq!(
            parsed,
            FromHost::Keydown(Keydown {
                key: "a".into(),
                mods: Mods::default(),
            })
        );
    }

    #[test]
    fn keydown_mods_omits_false_fields_on_serialise() {
        let json = encode_from_host(&FromHost::Keydown(Keydown {
            key: "s".into(),
            mods: Mods {
                meta: true,
                ..Default::default()
            },
        }))
        .unwrap();
        // Cmd+S sends just `{"meta":true}`, not the full four-field
        // record — keeps the wire compact.
        assert!(
            json.contains(r#""mods":{"meta":true}"#),
            "mods must skip false fields, got {json}"
        );
    }

    #[test]
    fn from_host_link_click_decodes() {
        let parsed = decode_from_host(r#"{"k":"link_click","v":{"target":"OtherNote"}}"#).unwrap();
        assert_eq!(
            parsed,
            FromHost::LinkClick(LinkClick {
                target: "OtherNote".into()
            })
        );
    }

    #[test]
    fn from_host_malformed_json_errors() {
        let err = decode_from_host("not json").unwrap_err();
        assert!(matches!(err, BridgeError::Decode(_)));
    }

    #[test]
    fn from_host_unknown_kind_errors() {
        assert!(matches!(
            decode_from_host(r#"{"k":"future_message","v":{}}"#).unwrap_err(),
            BridgeError::Decode(_)
        ));
    }

    #[test]
    fn from_host_saved_roundtrip() {
        let msg = FromHost::Saved(NoteRef { id: nid(9) });
        let json = encode_from_host(&msg).unwrap();
        assert_eq!(json, r#"{"k":"saved","v":{"id":9}}"#);
        assert_eq!(decode_from_host(&json).unwrap(), msg);
    }

    #[test]
    fn bridge_error_preserves_source_chain() {
        // The `#[source]` attribute on BridgeError::Decode keeps the
        // inner serde_json::Error available via Error::source(), which
        // is what surfaces line / column info to higher layers.
        let err = decode_from_host("not json").unwrap_err();
        let source = std::error::Error::source(&err)
            .expect("BridgeError::Decode must expose the inner serde_json error via source()");
        assert!(
            source.to_string().contains("expected"),
            "serde_json error message should mention what it expected, got {source}"
        );
    }

    #[test]
    fn envelope_kinds_use_snake_case_for_every_to_host_variant() {
        // Lock the wire spelling — TypeScript dispatch lives or dies
        // by these strings, and a thoughtless rename would break the
        // editor host without producing a Rust compiler error.
        let cases = [
            (
                ToHost::NoteOpen(NoteOpen {
                    id: nid(0),
                    path: String::new(),
                    body: String::new(),
                }),
                "note_open",
            ),
            (ToHost::FocusEditor, "focus_editor"),
            (ToHost::SaveRequest, "save_request"),
            (
                ToHost::ThemeSet(ThemeSet {
                    mode: ThemeMode::Light,
                }),
                "theme_set",
            ),
            (
                ToHost::SetRawMode(SetRawMode { enabled: false }),
                "set_raw_mode",
            ),
        ];
        for (msg, want) in cases {
            let json = encode_to_host(&msg).unwrap();
            assert!(
                json.contains(&format!(r#""k":"{want}""#)),
                "expected kind {want} in {json}"
            );
        }
    }

    #[test]
    fn envelope_kinds_use_snake_case_for_every_from_host_variant() {
        let cases = [
            (FromHost::Ready, "ready"),
            (FromHost::Dirty(NoteRef { id: nid(0) }), "dirty"),
            (
                FromHost::Save(NoteSave {
                    id: nid(0),
                    body: String::new(),
                }),
                "save",
            ),
            (FromHost::Saved(NoteRef { id: nid(0) }), "saved"),
            (
                FromHost::LinkClick(LinkClick {
                    target: String::new(),
                }),
                "link_click",
            ),
            (
                FromHost::Keydown(Keydown {
                    key: String::new(),
                    mods: Mods::default(),
                }),
                "keydown",
            ),
        ];
        for (msg, want) in cases {
            let json = encode_from_host(&msg).unwrap();
            assert!(
                json.contains(&format!(r#""k":"{want}""#)),
                "expected kind {want} in {json}"
            );
        }
    }
}
