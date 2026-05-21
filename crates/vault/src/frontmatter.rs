//! Markdown frontmatter parser.
//!
//! Extracts the YAML block delimited by `---\n…\n---\n` at the very start
//! of a note body, returning a structured map plus the remaining body.
//! Used by [`Vault::note_content`] (Phase 8.11) so chrome surfaces such as
//! `frontmatter_panel` can render typed properties without each call site
//! re-parsing the YAML.
//!
//! # Supported value shapes
//!
//! - Strings (quoted or bare scalar)
//! - Integers and floats
//! - Booleans
//! - ISO 8601 date strings (kept as text — callers parse with `chrono`
//!   only when they actually need a `DateTime`)
//! - Sequences of any of the above
//! - `null` / missing → entry omitted
//!
//! Nested mappings and YAML anchors are **not** supported; they survive
//! as `FrontmatterValue::Text` (the raw YAML scalar repr). Documented
//! limitation — vault frontmatter in Tolaria today is a flat key/value
//! sheet so the extra complexity isn't worth carrying.

use std::collections::BTreeMap;

use gpui::SharedString;
use serde::{Deserialize, Serialize};

/// A single value drawn from a note's YAML frontmatter.
///
/// `Date` is distinct from `Text` so callers can render dates with a
/// dedicated widget without having to sniff the format themselves.  The
/// underlying representation is the raw ISO 8601 string — callers parse
/// to `chrono::DateTime` only when they actually need to do arithmetic
/// on the value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum FrontmatterValue {
    /// A string scalar.  Quoting in the source YAML is preserved as
    /// content but not exposed (the surrounding quotes are stripped).
    Text(SharedString),
    /// An integer or floating-point scalar.  Stored as `f64` so the
    /// public API has a single number variant; integer notes use a
    /// trivially-round value.
    Number(f64),
    /// A boolean (`true` / `false`, plus YAML's `yes`/`no`/`on`/`off`
    /// aliases).
    Bool(bool),
    /// An ISO 8601 date or date-time string.  Detected by shape
    /// (`\d{4}-\d{2}-\d{2}` prefix) — not by `serde_yaml`'s `chrono`
    /// integration so the parser stays cheap and dependency-light.
    Date(SharedString),
    /// A flat sequence.  Nested lists are flattened to `Text`
    /// representations to keep the public model first-order.
    List(Vec<FrontmatterValue>),
}

/// Structured view of a note's YAML frontmatter.
///
/// Stable lexicographic key order makes diffs of "what frontmatter
/// keys does this vault expose?" trivial; callers that want a specific
/// presentation order should re-sort at the render site.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Frontmatter {
    values: BTreeMap<SharedString, FrontmatterValue>,
}

impl Frontmatter {
    /// Look up a single key.  Returns `None` for absent keys; an empty
    /// frontmatter block always returns `None`.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&FrontmatterValue> {
        self.values.get(key)
    }

    /// Iterate keys in sorted (lexicographic) order.
    pub fn keys(&self) -> impl Iterator<Item = &SharedString> {
        self.values.keys()
    }

    /// True iff there are no parsed entries.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// Number of parsed entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.values.len()
    }

    /// Iterate `(key, value)` pairs in sorted (lexicographic) order.
    pub fn iter(&self) -> impl Iterator<Item = (&SharedString, &FrontmatterValue)> {
        self.values.iter()
    }

    /// True iff the note's `_favorite` flag is set to a literal `true`
    /// in its YAML frontmatter.  Absent or non-boolean values read as
    /// `false`, matching the React handler's `entry.favorite` shape.
    ///
    /// Wired to the note-toolbar star cell (worklist 9.2.1) and to the
    /// sidebar `Favorites` section that lists every note where this
    /// returns `true`.
    #[must_use]
    pub fn favorite(&self) -> bool {
        matches!(self.get("_favorite"), Some(FrontmatterValue::Bool(true)))
    }

    /// True iff the note's `_organized` flag is set to a literal `true`
    /// in its YAML frontmatter.  Absent or non-boolean values read as
    /// `false`.
    ///
    /// Wired to the note-toolbar "organized" cell (worklist 9.2.2).
    /// The inbox-advance behaviour driven by the React
    /// `useInboxOrganizeAdvance` hook is intentionally NOT implemented
    /// at this layer; the cell is a pure frontmatter toggle until
    /// `explicit_organization_enabled` lands on `settings_store`.
    #[must_use]
    pub fn organized(&self) -> bool {
        matches!(self.get("_organized"), Some(FrontmatterValue::Bool(true)))
    }

    /// Insert or replace a boolean key in the in-memory map.
    ///
    /// `crate`-visible because it is paired exclusively with
    /// [`crate::Vault::set_frontmatter_bool`]: callers that go through
    /// any other path would skip the on-disk rewrite, leaving disk and
    /// memory out of sync.
    pub(crate) fn insert_bool(&mut self, key: &str, value: bool) {
        self.values.insert(
            SharedString::from(key.to_owned()),
            FrontmatterValue::Bool(value),
        );
    }

    /// Remove a key from the in-memory map.  No-op if the key is
    /// absent.  Paired with [`crate::Vault::set_frontmatter_bool`] —
    /// see [`insert_bool`][Self::insert_bool] for the rationale on the
    /// limited visibility.
    pub(crate) fn remove(&mut self, key: &str) {
        self.values.remove(key);
    }
}

/// Extract the YAML frontmatter block from the start of `raw`.
///
/// Returns the parsed [`Frontmatter`] plus the remaining body (everything
/// after the closing `---` delimiter).  When `raw` does not start with a
/// frontmatter block — or the block is malformed (no closing delimiter,
/// unparseable YAML, etc.) — returns `(Frontmatter::default(), raw)`
/// without panic so callers can render the body unchanged.
///
/// # Recognised opener
///
/// `---\n` at byte 0.  A BOM or leading whitespace disqualifies the
/// document from having frontmatter — Tolaria's editor never produces
/// either.
#[must_use]
pub fn parse(raw: &str) -> (Frontmatter, &str) {
    let Some(rest) = strip_opener(raw) else {
        return (Frontmatter::default(), raw);
    };
    let Some((yaml, body)) = split_at_closing_delimiter(rest) else {
        return (Frontmatter::default(), raw);
    };
    let parsed = parse_yaml_block(yaml);
    (parsed, body)
}

/// `---\n` or `---\r\n` at byte 0; returns the slice past the opener.
fn strip_opener(raw: &str) -> Option<&str> {
    raw.strip_prefix("---\n")
        .or_else(|| raw.strip_prefix("---\r\n"))
}

/// Find the `\n---\n` (or `\r\n---\r\n`) closing delimiter and return
/// `(yaml_block, body_after_close)`.
fn split_at_closing_delimiter(after_open: &str) -> Option<(&str, &str)> {
    // Empty frontmatter block: opener immediately followed by `---\n`
    // (the body is empty, so there is no `\n---\n` sequence with a
    // leading newline).  Handle as a fast path before the general
    // search below.
    if let Some(body) = after_open.strip_prefix("---\n") {
        return Some(("", body));
    }
    if let Some(body) = after_open.strip_prefix("---\r\n") {
        return Some(("", body));
    }
    // Two acceptable closers; check the lf form first since Tolaria
    // notes use lf line endings.
    for delim in ["\n---\n", "\r\n---\r\n"] {
        if let Some(end) = after_open.find(delim) {
            let yaml = &after_open[..end];
            let body = &after_open[end + delim.len()..];
            return Some((yaml, body));
        }
    }
    // Trailing `---` at EOF (no newline after the close) is tolerated
    // so a note with frontmatter but no body still parses.
    if let Some(stripped) = after_open.strip_suffix("\n---") {
        return Some((stripped, ""));
    }
    if let Some(stripped) = after_open.strip_suffix("\r\n---") {
        return Some((stripped, ""));
    }
    None
}

/// Run the YAML through `serde_yaml` and project the resulting
/// `serde_yaml::Value` into our flat [`FrontmatterValue`] model.
fn parse_yaml_block(yaml: &str) -> Frontmatter {
    let parsed: Result<serde_yaml::Value, _> = serde_yaml::from_str(yaml);
    let Ok(value) = parsed else {
        return Frontmatter::default();
    };
    let serde_yaml::Value::Mapping(map) = value else {
        return Frontmatter::default();
    };
    let mut values = BTreeMap::new();
    for (k, v) in map {
        let Some(key) = yaml_key_to_shared_string(&k) else {
            continue;
        };
        if let Some(value) = yaml_value_to_frontmatter(&v) {
            values.insert(key, value);
        }
    }
    Frontmatter { values }
}

fn yaml_key_to_shared_string(value: &serde_yaml::Value) -> Option<SharedString> {
    match value {
        serde_yaml::Value::String(s) => Some(SharedString::from(s.clone())),
        serde_yaml::Value::Number(n) => Some(SharedString::from(n.to_string())),
        serde_yaml::Value::Bool(b) => Some(SharedString::from(b.to_string())),
        _ => None,
    }
}

fn yaml_value_to_frontmatter(value: &serde_yaml::Value) -> Option<FrontmatterValue> {
    match value {
        serde_yaml::Value::Null => None,
        serde_yaml::Value::Bool(b) => Some(FrontmatterValue::Bool(*b)),
        serde_yaml::Value::Number(n) => n.as_f64().map(FrontmatterValue::Number),
        serde_yaml::Value::String(s) => Some(if looks_like_iso_date(s) {
            FrontmatterValue::Date(SharedString::from(s.clone()))
        } else {
            FrontmatterValue::Text(SharedString::from(s.clone()))
        }),
        serde_yaml::Value::Sequence(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                if let Some(v) = yaml_value_to_frontmatter(item) {
                    out.push(v);
                }
            }
            Some(FrontmatterValue::List(out))
        }
        // Nested mappings + tagged values fall back to a serialised
        // text representation so the caller still has a printable
        // value to render.
        serde_yaml::Value::Mapping(_) | serde_yaml::Value::Tagged(_) => {
            let rendered = serde_yaml::to_string(value).ok()?;
            Some(FrontmatterValue::Text(SharedString::from(
                rendered.trim_end().to_string(),
            )))
        }
    }
}

// ---------------------------------------------------------------------------
// Byte-identical rewriter (worklist 9.2.1 / 9.2.2 — `set_frontmatter_bool`)
// ---------------------------------------------------------------------------

/// Rewrite the YAML frontmatter block of `raw` so the boolean `key` lands
/// at `value`, preserving every byte outside the affected line.
///
/// Behaviour matrix:
///
/// | Note has frontmatter? | New value | Existing line for `key`? | Result                                            |
/// |-----------------------|-----------|--------------------------|---------------------------------------------------|
/// | yes                   | `true`    | yes                      | line rewritten in place (`{key}: true`)           |
/// | yes                   | `true`    | no                       | new line appended just before the closing `---`   |
/// | yes                   | `false`   | yes                      | line **removed** (absent ⇔ `false`)               |
/// | yes                   | `false`   | no                       | unchanged                                         |
/// | no                    | `true`    | —                        | fresh `---\n{key}: true\n---\n` prepended         |
/// | no                    | `false`   | —                        | unchanged                                         |
///
/// `false` is encoded as **absence** rather than `{key}: false` to
/// mirror the React handler `useEntryActions.handleToggleFavorite`,
/// where omitting the key is the canonical "off" representation.
///
/// Outside of the rewritten / inserted / removed line the body is
/// preserved byte-for-byte: this matches the editor-host pin from
/// Phase 8.26 (byte-identical round-trip for editor-driven saves) at
/// the chrome-side write path so frontmatter toggles don't reflow the
/// rest of the YAML keys, the body, or line endings.
///
/// `crate`-visible because it is paired exclusively with
/// [`crate::Vault::set_frontmatter_bool`].
#[must_use]
pub(crate) fn set_bool_in_raw(raw: &str, key: &str, value: bool) -> String {
    let Some(parts) = FrontmatterParts::from_raw(raw) else {
        // No frontmatter block — only need to act when toggling to
        // `true`.  Otherwise the file already matches the desired
        // (absent ⇔ false) state.
        if !value {
            return raw.to_owned();
        }
        // Default to lf line endings (Tolaria's editor never writes
        // CRLF).  A user-authored CRLF body without a frontmatter
        // block survives intact because we only insert the lf-flavoured
        // prefix at the very top.
        let mut out = String::with_capacity(raw.len() + 4 + key.len() + 8);
        out.push_str("---\n");
        out.push_str(key);
        out.push_str(": true\n");
        out.push_str("---\n");
        out.push_str(raw);
        return out;
    };

    let (new_yaml, new_closer) = parts.rewrite_yaml(key, value);
    let mut out = String::with_capacity(
        parts.opener.len() + new_yaml.len() + new_closer.len() + parts.body.len(),
    );
    out.push_str(parts.opener);
    out.push_str(&new_yaml);
    out.push_str(&new_closer);
    out.push_str(parts.body);
    out
}

/// The four byte slices of a note that has a frontmatter block:
/// `opener` (e.g. `"---\n"`), `yaml`, `closer` (e.g. `"\n---\n"`),
/// `body`.  Concatenating them reconstructs the original input — the
/// invariant the rewriter relies on for byte-identical preservation.
struct FrontmatterParts<'a> {
    opener: &'a str,
    yaml: &'a str,
    closer: &'a str,
    body: &'a str,
    /// Line ending used inside the YAML block — `"\n"` for lf,
    /// `"\r\n"` for crlf.  Detected from the opener delimiter so
    /// newly-inserted lines match the existing flavour.
    newline: &'static str,
}

impl<'a> FrontmatterParts<'a> {
    /// Split `raw` into its frontmatter parts, or `None` if the input
    /// has no recognised frontmatter block.
    fn from_raw(raw: &'a str) -> Option<Self> {
        // Detect the opener flavour first; the slice after it drives
        // the closer search.
        let (opener, rest, newline): (&str, &str, &'static str) =
            if let Some(rest) = raw.strip_prefix("---\n") {
                ("---\n", rest, "\n")
            } else if let Some(rest) = raw.strip_prefix("---\r\n") {
                ("---\r\n", rest, "\r\n")
            } else {
                return None;
            };
        // Empty frontmatter — opener immediately followed by the
        // closing `---\n` / `---\r\n`.  Treat as a yaml-less block so
        // appends still produce a well-formed file.
        if let Some(body) = rest.strip_prefix("---\n") {
            return Some(Self {
                opener,
                yaml: "",
                closer: "---\n",
                body,
                newline,
            });
        }
        if let Some(body) = rest.strip_prefix("---\r\n") {
            return Some(Self {
                opener,
                yaml: "",
                closer: "---\r\n",
                body,
                newline,
            });
        }
        // Search for the closing delimiter in the same line-ending
        // flavour as the opener; tolerate the lf variant inside a
        // crlf-flavoured opener (mixed-eol files exist in the wild).
        for closer in ["\n---\n", "\r\n---\r\n"] {
            if let Some(end) = rest.find(closer) {
                let yaml = &rest[..end];
                let body = &rest[end + closer.len()..];
                return Some(Self {
                    opener,
                    yaml,
                    closer,
                    body,
                    newline,
                });
            }
        }
        // Trailing `---` at EOF (no newline after the close).
        if let Some(yaml) = rest.strip_suffix("\n---") {
            return Some(Self {
                opener,
                yaml,
                closer: "\n---",
                body: "",
                newline,
            });
        }
        if let Some(yaml) = rest.strip_suffix("\r\n---") {
            return Some(Self {
                opener,
                yaml,
                closer: "\r\n---",
                body: "",
                newline,
            });
        }
        None
    }

    /// Produce a new YAML block with `key` set / appended / removed,
    /// preserving every other line byte-for-byte.  Returns both the
    /// rewritten YAML body **and** a (possibly adjusted) closer
    /// delimiter so the rewriter can re-emit a well-formed block when
    /// transitioning between empty / non-empty yaml.
    fn rewrite_yaml(&self, key: &str, value: bool) -> (String, String) {
        let span = LineSpan::find(self.yaml, key);
        match (span, value) {
            (Some(span), true) => (self.rewrite_line(span, key), self.closer.to_owned()),
            (Some(span), false) => self.remove_line(span),
            (None, true) => self.append_line(key),
            (None, false) => (self.yaml.to_owned(), self.closer.to_owned()),
        }
    }

    fn rewrite_line(&self, span: LineSpan, key: &str) -> String {
        let mut out = String::with_capacity(self.yaml.len() + key.len() + 8);
        out.push_str(&self.yaml[..span.start]);
        out.push_str(key);
        out.push_str(": true");
        out.push_str(&self.yaml[span.value_end..]);
        out
    }

    /// Remove the matched line and re-balance the closer.
    ///
    /// Three cases:
    ///
    /// 1. **Removed line has a terminator** (mid-block): drop the line
    ///    and its terminator; the surrounding lines concatenate
    ///    cleanly.  Closer stays as-is.
    /// 2. **Removed line is the last in the block** (no terminator):
    ///    the predecessor's terminator becomes a phantom trailing
    ///    newline — strip it so the new yaml ends without a terminator
    ///    (matches the original invariant that the closer owns the
    ///    leading newline).
    /// 3. **Removed line was the *only* line** (yaml becomes empty):
    ///    convert the closer to the empty-block flavour (`"---\n"` /
    ///    `"---\r\n"`) so the resulting file matches the canonical
    ///    empty-frontmatter shape.
    fn remove_line(&self, span: LineSpan) -> (String, String) {
        let mut out = String::with_capacity(self.yaml.len());
        out.push_str(&self.yaml[..span.start]);
        out.push_str(&self.yaml[span.line_end..]);
        if !out.is_empty() && span.value_end == self.yaml.len() {
            // Case 2: trim the orphaned terminator we left in front of
            // the removed last line.
            if let Some(stripped) = out.strip_suffix(self.newline) {
                out.truncate(stripped.len());
            }
        }
        let closer = if out.is_empty() {
            // Case 3: collapse to the empty-block closer flavour.
            empty_block_closer(self.newline).to_owned()
        } else {
            self.closer.to_owned()
        };
        (out, closer)
    }

    /// Append a new `{key}: true` line at the end of the block.
    ///
    /// When the block is currently empty the closer is the
    /// empty-block flavour (`"---\n"`) and does NOT carry a leading
    /// newline — we have to upgrade both the yaml and the closer to
    /// the non-empty flavour so the resulting file is well-formed.
    fn append_line(&self, key: &str) -> (String, String) {
        let mut out = String::with_capacity(self.yaml.len() + key.len() + self.newline.len() + 8);
        out.push_str(self.yaml);
        // Non-empty existing yaml ends without a newline (the closer
        // owns the separator).  When it doesn't (a malformed block),
        // re-add one so the new line lands cleanly.
        if !self.yaml.is_empty() && !self.yaml.ends_with(self.newline) {
            out.push_str(self.newline);
        }
        out.push_str(key);
        out.push_str(": true");
        let closer = if self.yaml.is_empty() {
            // Upgrade the empty-block closer to its non-empty
            // flavour so the appended line is followed by a
            // separating newline.
            non_empty_closer(self.newline).to_owned()
        } else {
            self.closer.to_owned()
        };
        (out, closer)
    }
}

/// Empty-block closer for the given line-ending flavour (`"---\n"`
/// for lf, `"---\r\n"` for crlf).
fn empty_block_closer(newline: &str) -> &'static str {
    if newline == "\r\n" {
        "---\r\n"
    } else {
        "---\n"
    }
}

/// Non-empty closer for the given line-ending flavour (`"\n---\n"`
/// for lf, `"\r\n---\r\n"` for crlf).
fn non_empty_closer(newline: &str) -> &'static str {
    if newline == "\r\n" {
        "\r\n---\r\n"
    } else {
        "\n---\n"
    }
}

/// Byte range of a `{key}: …` line inside a YAML block, plus the
/// position immediately after its value (used by the rewriter to
/// preserve any trailing whitespace / comment that follows).
#[derive(Debug, Clone, Copy)]
struct LineSpan {
    /// Inclusive start of the matched line within the YAML block.
    start: usize,
    /// One past the end of the *value* on the matched line — i.e. the
    /// position of the line's terminator (or end-of-block).  Used by
    /// `rewrite_line` so any trailing comment / whitespace on the same
    /// line stays intact.
    value_end: usize,
    /// One past the end of the matched line, **including** its
    /// terminator (`\n` or `\r\n`).  Used by `remove_line` so the
    /// surrounding lines concatenate without leaving an empty gap.
    line_end: usize,
}

impl LineSpan {
    /// Scan `yaml` for a line that starts with `{key}:` (optionally
    /// preceded by ASCII spaces — top-level keys are not indented in
    /// frontmatter, but we tolerate a stray space).  Returns the line
    /// span when found.
    ///
    /// Only matches keys at the top level: a nested-map key with the
    /// same name (`other:\n  _favorite: true`) is intentionally
    /// skipped because frontmatter in Tolaria is a flat sheet (see the
    /// crate docs).  Lines belonging to a nested block are
    /// distinguished by leading whitespace ≥ 2 chars.
    fn find(yaml: &str, key: &str) -> Option<Self> {
        let bytes = yaml.as_bytes();
        let mut line_start = 0;
        while line_start <= bytes.len() {
            // Locate this line's terminator (\n or \r\n) and the
            // byte offset of the next line.
            let (value_end, line_end) = next_line_terminator(bytes, line_start);
            let line = &yaml[line_start..value_end];
            if line_starts_with_key(line, key) {
                return Some(Self {
                    start: line_start,
                    value_end,
                    line_end,
                });
            }
            if line_end == line_start {
                // Empty input or terminal newline-less line we've
                // already inspected — done.
                break;
            }
            line_start = line_end;
        }
        None
    }
}

/// Find the end-of-line terminator starting at `from`.  Returns
/// `(value_end, line_end)`:
///
/// - `value_end` is the byte offset of the terminator character (or
///   `bytes.len()` when the line runs to EOF without one).
/// - `line_end` is the byte offset *after* the terminator (or
///   `bytes.len()` when there is no terminator).
///
/// Handles both `\n` and `\r\n`.
fn next_line_terminator(bytes: &[u8], from: usize) -> (usize, usize) {
    let mut i = from;
    while i < bytes.len() {
        if bytes[i] == b'\n' {
            return (i, i + 1);
        }
        if bytes[i] == b'\r' && bytes.get(i + 1) == Some(&b'\n') {
            return (i, i + 2);
        }
        i += 1;
    }
    (bytes.len(), bytes.len())
}

/// Returns `true` iff `line` starts with `{key}:` at the top level —
/// i.e. no leading whitespace ≥ 2 chars, no `#` comment marker, no
/// `-` list marker.
fn line_starts_with_key(line: &str, key: &str) -> bool {
    // Drop up to one leading ASCII space; reject deeper indentation as
    // a nested-map child.
    let stripped = match line.strip_prefix(' ') {
        Some(rest) if rest.starts_with(' ') => return false,
        Some(rest) => rest,
        None => line,
    };
    let Some(rest) = stripped.strip_prefix(key) else {
        return false;
    };
    rest.starts_with(':')
}

/// Cheap shape-only detection: `YYYY-MM-DD[Tt ...]?` matches.  Avoids
/// pulling `chrono`'s strict parser into the hot path; callers that
/// need a real `DateTime` parse the string themselves with the format
/// they expect.
fn looks_like_iso_date(s: &str) -> bool {
    let bytes = s.as_bytes();
    if bytes.len() < 10 {
        return false;
    }
    bytes[0..4].iter().all(u8::is_ascii_digit)
        && bytes[4] == b'-'
        && bytes[5..7].iter().all(u8::is_ascii_digit)
        && bytes[7] == b'-'
        && bytes[8..10].iter().all(u8::is_ascii_digit)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_input_has_no_frontmatter() {
        let (fm, body) = parse("");
        assert!(fm.is_empty());
        assert_eq!(body, "");
    }

    #[test]
    fn body_without_frontmatter_passes_through() {
        let raw = "# Just a heading\n\nNo frontmatter here.\n";
        let (fm, body) = parse(raw);
        assert!(fm.is_empty());
        assert_eq!(body, raw, "raw body must round-trip unchanged");
    }

    #[test]
    fn parses_simple_key_value_pairs() {
        let raw = "---\ntype: Note\ntitle: hello\n---\n\n# body\n";
        let (fm, body) = parse(raw);
        assert_eq!(fm.len(), 2);
        assert_eq!(
            fm.get("type"),
            Some(&FrontmatterValue::Text(SharedString::from("Note")))
        );
        assert_eq!(
            fm.get("title"),
            Some(&FrontmatterValue::Text(SharedString::from("hello")))
        );
        assert_eq!(body, "\n# body\n");
    }

    #[test]
    fn parses_quoted_strings_lists_and_scalars() {
        let raw = "---\n\
                   title: \"Quoted Title\"\n\
                   count: 7\n\
                   active: true\n\
                   tags:\n  - alpha\n  - beta\n\
                   ---\n\
                   body\n";
        let (fm, _body) = parse(raw);
        assert_eq!(
            fm.get("title"),
            Some(&FrontmatterValue::Text(SharedString::from("Quoted Title")))
        );
        assert_eq!(fm.get("count"), Some(&FrontmatterValue::Number(7.0)));
        assert_eq!(fm.get("active"), Some(&FrontmatterValue::Bool(true)));
        let Some(FrontmatterValue::List(items)) = fm.get("tags") else {
            panic!("expected tags to be a list");
        };
        assert_eq!(items.len(), 2);
        assert!(matches!(&items[0], FrontmatterValue::Text(s) if s.as_ref() == "alpha"));
        assert!(matches!(&items[1], FrontmatterValue::Text(s) if s.as_ref() == "beta"));
    }

    #[test]
    fn detects_iso_dates_as_date_variant() {
        // serde_yaml renders bare ISO dates as native dates; quoting
        // them keeps them as strings that our shape detector recognises.
        let raw = "---\ndate: \"2025-04-20\"\nstart: \"2025-04-20T10:00:00\"\n---\n";
        let (fm, _) = parse(raw);
        assert!(matches!(fm.get("date"), Some(FrontmatterValue::Date(_))));
        assert!(matches!(fm.get("start"), Some(FrontmatterValue::Date(_))));
    }

    #[test]
    fn null_entries_are_dropped() {
        // `serde_yaml` follows YAML 1.2: `true`/`false` are bools but
        // `yes`/`no` stay as strings.  The relevant invariant here is
        // that a bare `null` entry (`dropped:`) is dropped.
        let raw = "---\nkept: true\ndropped:\n---\n";
        let (fm, _) = parse(raw);
        assert_eq!(fm.len(), 1);
        assert_eq!(fm.get("kept"), Some(&FrontmatterValue::Bool(true)));
        assert!(fm.get("dropped").is_none());
    }

    #[test]
    fn malformed_close_returns_empty_with_raw_body() {
        // Open delimiter but no close → not real frontmatter; the
        // whole document is the body.
        let raw = "---\ntype: Note\n\n# body without close\n";
        let (fm, body) = parse(raw);
        assert!(
            fm.is_empty(),
            "malformed frontmatter must yield empty map, got {fm:?}"
        );
        assert_eq!(body, raw, "body slice must equal the original raw input");
    }

    #[test]
    fn malformed_yaml_does_not_panic() {
        // Closing delimiter is present but the YAML body is gibberish
        // that serde_yaml will reject.  Must surface as empty map +
        // body after the closer (not a panic).
        let raw = "---\n: : :\n---\nrest\n";
        let (fm, body) = parse(raw);
        assert!(fm.is_empty());
        assert_eq!(body, "rest\n");
    }

    #[test]
    fn keys_iter_is_sorted() {
        let raw = "---\nzebra: 1\nalpha: 2\nmango: 3\n---\n";
        let (fm, _) = parse(raw);
        let keys: Vec<&str> = fm.keys().map(SharedString::as_ref).collect();
        assert_eq!(keys, ["alpha", "mango", "zebra"]);
    }

    #[test]
    fn crlf_line_endings_are_accepted() {
        let raw = "---\r\ntype: Note\r\n---\r\nbody\r\n";
        let (fm, body) = parse(raw);
        assert_eq!(fm.len(), 1);
        assert_eq!(
            fm.get("type"),
            Some(&FrontmatterValue::Text(SharedString::from("Note")))
        );
        assert_eq!(body, "body\r\n");
    }

    #[test]
    fn empty_frontmatter_block_yields_default() {
        let raw = "---\n---\nbody\n";
        let (fm, body) = parse(raw);
        assert!(fm.is_empty());
        assert_eq!(body, "body\n");
    }

    // -----------------------------------------------------------------
    // set_bool_in_raw — byte-identical rewrite tests
    // -----------------------------------------------------------------

    #[test]
    fn set_bool_inserts_block_when_absent_and_value_true() {
        let raw = "# Heading\n\nbody body body\n";
        let out = set_bool_in_raw(raw, "_favorite", true);
        assert_eq!(
            out,
            "---\n_favorite: true\n---\n# Heading\n\nbody body body\n"
        );
    }

    #[test]
    fn set_bool_leaves_input_unchanged_when_value_false_and_no_block() {
        let raw = "# Heading\n\nno frontmatter\n";
        let out = set_bool_in_raw(raw, "_favorite", false);
        assert_eq!(out, raw, "false on a frontmatter-less note must be a no-op");
    }

    #[test]
    fn set_bool_appends_line_when_key_missing_and_value_true() {
        let raw = "---\ntype: Note\nstatus: Done\n---\n\n# body\n";
        let out = set_bool_in_raw(raw, "_favorite", true);
        assert_eq!(
            out, "---\ntype: Note\nstatus: Done\n_favorite: true\n---\n\n# body\n",
            "append must preserve existing keys and body verbatim",
        );
    }

    #[test]
    fn set_bool_rewrites_existing_line_value_true() {
        let raw = "---\ntype: Note\n_favorite: false\nstatus: Done\n---\nbody\n";
        let out = set_bool_in_raw(raw, "_favorite", true);
        assert_eq!(
            out, "---\ntype: Note\n_favorite: true\nstatus: Done\n---\nbody\n",
            "rewrite must touch only the target line",
        );
    }

    #[test]
    fn set_bool_removes_line_when_value_false() {
        let raw = "---\ntype: Note\n_favorite: true\nstatus: Done\n---\nbody\n";
        let out = set_bool_in_raw(raw, "_favorite", false);
        assert_eq!(
            out, "---\ntype: Note\nstatus: Done\n---\nbody\n",
            "false must remove the matching line",
        );
    }

    #[test]
    fn set_bool_false_on_absent_key_is_a_noop() {
        let raw = "---\ntype: Note\n---\nbody\n";
        let out = set_bool_in_raw(raw, "_favorite", false);
        assert_eq!(out, raw, "false on an absent key must round-trip verbatim");
    }

    #[test]
    fn set_bool_preserves_body_bytes_exactly() {
        // Body deliberately mixes hard tabs, double newlines, trailing
        // whitespace, and a wikilink — the rewriter must not normalise
        // any of it.
        let body = "\n# Heading\n\nA paragraph with\ta tab and trailing space.   \n\n- bullet [[wiki link]]\n";
        let raw = format!("---\ntype: Note\n---\n{body}");
        let out = set_bool_in_raw(&raw, "_favorite", true);
        assert_eq!(
            out,
            format!("---\ntype: Note\n_favorite: true\n---\n{body}"),
            "body bytes must round-trip identically",
        );
    }

    #[test]
    fn set_bool_round_trip_to_false_restores_original() {
        let raw = "---\ntype: Note\nstatus: Done\n---\n\nbody\n";
        let after_true = set_bool_in_raw(raw, "_favorite", true);
        let after_false = set_bool_in_raw(&after_true, "_favorite", false);
        assert_eq!(
            after_false, raw,
            "toggle-on then toggle-off must restore the original bytes exactly",
        );
    }

    #[test]
    fn set_bool_handles_crlf_line_endings() {
        let raw = "---\r\ntype: Note\r\nstatus: Done\r\n---\r\nbody\r\n";
        let out = set_bool_in_raw(raw, "_favorite", true);
        assert_eq!(
            out, "---\r\ntype: Note\r\nstatus: Done\r\n_favorite: true\r\n---\r\nbody\r\n",
            "appended line must use the same line-ending flavour as the existing block",
        );
    }

    #[test]
    fn set_bool_handles_empty_frontmatter_block() {
        let raw = "---\n---\nbody\n";
        let out = set_bool_in_raw(raw, "_favorite", true);
        assert_eq!(out, "---\n_favorite: true\n---\nbody\n");
    }

    #[test]
    fn favorite_and_organized_accessors_read_bool_keys() {
        let raw = "---\n_favorite: true\n_organized: true\nother: foo\n---\nbody\n";
        let (fm, _) = parse(raw);
        assert!(fm.favorite());
        assert!(fm.organized());

        let (fm_none, _) = parse("---\nother: foo\n---\nbody\n");
        assert!(!fm_none.favorite());
        assert!(!fm_none.organized());

        let (fm_false, _) = parse("---\n_favorite: false\n---\nbody\n");
        assert!(
            !fm_false.favorite(),
            "literal `_favorite: false` must read as not-favorite",
        );
    }

    #[test]
    fn insert_bool_and_remove_mutate_in_memory_map() {
        let mut fm = Frontmatter::default();
        fm.insert_bool("_favorite", true);
        assert!(fm.favorite());
        fm.remove("_favorite");
        assert!(!fm.favorite());
    }
}
