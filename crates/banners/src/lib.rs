//! Persistent banner types and renderer for Tolaria (ADR-0115 Phase 2b).
//!
//! Banners are full-width alerts rendered above note content to surface
//! persistent states: archived, conflict, rename detected, update available,
//! trash warning, and delete-in-progress. Use [`render_banner`] to convert a
//! [`Banner`] value into a renderable GPUI element backed by
//! [`gpui_component::alert::Alert`].

use chrono::{DateTime, Utc};
use gpui::{AnyElement, IntoElement as _, SharedString};
use gpui_component::alert::Alert;

// ---------------------------------------------------------------------------
// BannerSeverity
// ---------------------------------------------------------------------------

/// Severity level of a [`Banner`], controls the visual style of the alert.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BannerSeverity {
    Info,
    Warning,
    Error,
}

// ---------------------------------------------------------------------------
// Banner
// ---------------------------------------------------------------------------

/// A persistent, full-width banner displayed above note content.
#[derive(Debug, Clone)]
pub enum Banner {
    /// The note has been archived.
    ArchivedNote { archived_at: DateTime<Utc> },
    /// The note has merge conflicts with another branch.
    ConflictNote { conflicting_branch: SharedString },
    /// The note file was renamed on disk.
    RenameDetected {
        old_path: SharedString,
        new_path: SharedString,
    },
    /// An application update is available.
    Update { available_version: SharedString },
    /// The note is in the trash and will be permanently deleted soon.
    TrashWarning { days_remaining: u32 },
    /// A batch delete operation is in progress.
    DeleteProgressNotice { current: u32, total: u32 },
}

impl Banner {
    /// Severity level that controls the visual style.
    pub fn severity(&self) -> BannerSeverity {
        match self {
            Self::ArchivedNote { .. } => BannerSeverity::Info,
            Self::ConflictNote { .. } => BannerSeverity::Error,
            Self::RenameDetected { .. } => BannerSeverity::Info,
            Self::Update { .. } => BannerSeverity::Info,
            Self::TrashWarning { .. } => BannerSeverity::Warning,
            Self::DeleteProgressNotice { .. } => BannerSeverity::Info,
        }
    }

    /// Human-readable message describing the banner state.
    pub fn message(&self) -> SharedString {
        match self {
            Self::ArchivedNote { archived_at } => {
                format!("Archived on {}.", archived_at.format("%B %-d, %Y")).into()
            }
            Self::ConflictNote { conflicting_branch } => {
                format!(
                    "This note has conflicts with branch \"{conflicting_branch}\"."
                )
                .into()
            }
            Self::RenameDetected { old_path, new_path } => {
                format!("File renamed from \"{old_path}\" to \"{new_path}\".").into()
            }
            Self::Update { available_version } => {
                format!("Update available: version {available_version}.").into()
            }
            Self::TrashWarning { days_remaining } => format!(
                "This note is in the Trash. {days_remaining} day{} remaining before permanent deletion.",
                if *days_remaining == 1 { "" } else { "s" },
            )
            .into(),
            Self::DeleteProgressNotice { current, total } => {
                format!("Deleting {current} of {total}\u{2026}").into()
            }
        }
    }

    /// Label for the primary action button, if any.
    pub fn action_label(&self) -> Option<SharedString> {
        match self {
            Self::ArchivedNote { .. } => Some("Unarchive".into()),
            Self::ConflictNote { .. } => Some("Resolve Conflicts".into()),
            Self::RenameDetected { .. } => Some("Accept Rename".into()),
            Self::Update { .. } => Some("Update Now".into()),
            Self::TrashWarning { .. } => Some("Restore".into()),
            Self::DeleteProgressNotice { .. } => None,
        }
    }

    /// Stable, machine-readable name for this variant.
    ///
    /// Suitable for use as a GPUI [`gpui::ElementId`].
    pub fn variant_name(&self) -> &'static str {
        match self {
            Self::ArchivedNote { .. } => "archived_note",
            Self::ConflictNote { .. } => "conflict_note",
            Self::RenameDetected { .. } => "rename_detected",
            Self::Update { .. } => "update",
            Self::TrashWarning { .. } => "trash_warning",
            Self::DeleteProgressNotice { .. } => "delete_progress_notice",
        }
    }
}

// ---------------------------------------------------------------------------
// render_banner
// ---------------------------------------------------------------------------

/// Render a [`Banner`] as a full-width GPUI element.
///
/// Uses [`gpui_component::alert::Alert`] in banner mode, styled by severity.
/// The returned [`AnyElement`] should be placed above note content in the
/// editor layout.
///
/// # Example
///
/// ```ignore
/// // Inside a GPUI Render impl:
/// let banner = Banner::TrashWarning { days_remaining: 7 };
/// let element = render_banner(&banner);
/// ```
pub fn render_banner(banner: &Banner) -> AnyElement {
    let message = banner.message();
    let id = banner.variant_name();
    match banner.severity() {
        BannerSeverity::Info => Alert::info(id, message).banner().into_any_element(),
        BannerSeverity::Warning => Alert::warning(id, message).banner().into_any_element(),
        BannerSeverity::Error => Alert::error(id, message).banner().into_any_element(),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone as _;
    use gpui::{Context, IntoElement, Render, TestAppContext, Window};

    fn install_theme(cx: &mut TestAppContext) {
        cx.update(gpui_component::init);
    }

    fn utc(year: i32, month: u32, day: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(year, month, day, 0, 0, 0)
            .single()
            .expect("valid date")
    }

    // -----------------------------------------------------------------------
    // Per-variant message tests
    // -----------------------------------------------------------------------

    #[test]
    fn archived_note_message_contains_date() {
        let banner = Banner::ArchivedNote {
            archived_at: utc(2024, 3, 15),
        };
        let msg = banner.message();
        assert!(
            msg.contains("March 15, 2024"),
            "expected date in message, got: {msg}"
        );
    }

    #[test]
    fn conflict_note_message_contains_branch() {
        let banner = Banner::ConflictNote {
            conflicting_branch: "feature/editor".into(),
        };
        let msg = banner.message();
        assert!(
            msg.contains("feature/editor"),
            "expected branch name in message, got: {msg}"
        );
    }

    #[test]
    fn rename_detected_message_contains_paths() {
        let banner = Banner::RenameDetected {
            old_path: "notes/old.md".into(),
            new_path: "notes/new.md".into(),
        };
        let msg = banner.message();
        assert!(
            msg.contains("notes/old.md") && msg.contains("notes/new.md"),
            "expected both paths in message, got: {msg}"
        );
    }

    #[test]
    fn update_message_contains_version() {
        let banner = Banner::Update {
            available_version: "2.3.1".into(),
        };
        let msg = banner.message();
        assert!(
            msg.contains("2.3.1"),
            "expected version in message, got: {msg}"
        );
    }

    #[test]
    fn trash_warning_message_contains_days() {
        let banner = Banner::TrashWarning { days_remaining: 7 };
        let msg = banner.message();
        assert!(
            msg.contains('7'),
            "expected day count in message, got: {msg}"
        );
    }

    #[test]
    fn delete_progress_notice_message_contains_counts() {
        let banner = Banner::DeleteProgressNotice {
            current: 3,
            total: 10,
        };
        let msg = banner.message();
        assert!(
            msg.contains('3') && msg.contains("10"),
            "expected current/total in message, got: {msg}"
        );
    }

    // -----------------------------------------------------------------------
    // Severity mapping
    // -----------------------------------------------------------------------

    #[test]
    fn severity_mapping_is_correct() {
        assert_eq!(
            Banner::ArchivedNote {
                archived_at: utc(2024, 1, 1)
            }
            .severity(),
            BannerSeverity::Info,
            "ArchivedNote should be Info"
        );
        assert_eq!(
            Banner::RenameDetected {
                old_path: "a.md".into(),
                new_path: "b.md".into(),
            }
            .severity(),
            BannerSeverity::Info,
            "RenameDetected should be Info"
        );
        assert_eq!(
            Banner::Update {
                available_version: "1.0".into()
            }
            .severity(),
            BannerSeverity::Info,
            "Update should be Info"
        );
        assert_eq!(
            Banner::ConflictNote {
                conflicting_branch: "main".into()
            }
            .severity(),
            BannerSeverity::Error,
            "ConflictNote should be Error"
        );
        assert_eq!(
            Banner::TrashWarning { days_remaining: 3 }.severity(),
            BannerSeverity::Warning,
            "TrashWarning should be Warning"
        );
        assert_eq!(
            Banner::DeleteProgressNotice {
                current: 1,
                total: 5
            }
            .severity(),
            BannerSeverity::Info,
            "DeleteProgressNotice should be Info"
        );
    }

    // -----------------------------------------------------------------------
    // Render-no-panic test
    // -----------------------------------------------------------------------

    struct BannerView {
        banner: Banner,
    }

    impl Render for BannerView {
        fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
            render_banner(&self.banner)
        }
    }

    #[gpui::test]
    fn render_each_variant_no_panic(cx: &mut TestAppContext) {
        install_theme(cx);

        let banners = [
            Banner::ArchivedNote {
                archived_at: utc(2024, 1, 1),
            },
            Banner::ConflictNote {
                conflicting_branch: "main".into(),
            },
            Banner::RenameDetected {
                old_path: "old.md".into(),
                new_path: "new.md".into(),
            },
            Banner::Update {
                available_version: "2.0.0".into(),
            },
            Banner::TrashWarning { days_remaining: 5 },
            Banner::DeleteProgressNotice {
                current: 2,
                total: 8,
            },
        ];

        for banner in banners {
            let _window = cx.add_window(|_window, _cx| BannerView { banner });
            cx.run_until_parked();
        }
    }
}
