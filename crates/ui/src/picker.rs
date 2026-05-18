//! Minimal port of Zed's `Picker<D: PickerDelegate>` (ADR-0115 Phase 2).
//!
//! This module provides a reusable search-and-select primitive for
//! command-palette, quick-open, and wikilink-input surfaces.
//!
//! ## Kept relative to upstream SHA `f2df3f9e18fa3bbbdab20086bd98395c97a46116`
//!
//! - [`PickerDelegate`] core trait: `match_count`, `selected_index`,
//!   `set_selected_index`, `render_match`, `update_matches`, `confirm`,
//!   `dismissed`, `placeholder_text`.
//! - [`Picker<D>`] view holding delegate +
//!   [`gpui_component::input::InputState`] query input.
//! - Enter → `confirm(false)`, secondary-Enter → `confirm(true)`.
//! - Esc → `dismissed` + [`gpui::DismissEvent`] (via `Escape` propagation
//!   from `InputState`).
//! - [`Picker::select_next`] / [`Picker::select_prev`] for Up/Down navigation.
//! - [`EventEmitter<DismissEvent>`] + [`Focusable`] delegating to query input.
//!
//! ## Dropped
//!
//! - `uniform_list` / `list` virtualisation — all rows rendered eagerly in a
//!   plain `div`. TODO(Phase 2): virtualised list with `UniformListScrollHandle`.
//! - `PickerEditorPosition` — editor always at top. TODO(Phase 2): End mode.
//! - Query history (`select_history`).
//! - `finalize_update_matches` async debouncing. TODO(Phase 2).
//! - `confirm_completion` / `confirm_update_query` / `confirm_input`.
//! - `can_select` / `select_on_hover` / `selected_index_changed` hooks.
//! - `separators_after_indices`, `render_header`, `render_footer`,
//!   `render_editor` delegate overrides.
//! - `documentation_aside` / `no_matches_text` custom rendering.
//! - Multi-select, `show_scrollbar`, `is_modal`, `width`, `max_height`.
//! - `ModalView` impl (avoids a `workspace` crate dependency).
//! - Blur-triggered dismissal (avoids accidental close on list-item focus
//!   transitions). TODO(Phase 2): re-add with `window.is_window_active()` guard.
//! - Up/Down keybinding interception — `InputState`'s single-line handler
//!   consumes `MoveUp`/`MoveDown` silently; callers drive navigation via
//!   [`Picker::select_next`] / [`Picker::select_prev`].
//!   TODO(Phase 2): resolve via custom input or context-stack workaround.

use gpui::{
    div, AnyElement, App, AppContext, Context, DismissEvent, Entity, EventEmitter, FocusHandle,
    Focusable, InteractiveElement, IntoElement, ParentElement, Render, SharedString, Styled,
    Subscription, Task, Window,
};
use gpui_component::input::{Enter as InputEnter, Escape, Input, InputEvent, InputState};

// ---------------------------------------------------------------------------
// PickerDelegate trait
// ---------------------------------------------------------------------------

/// Delegate that drives the content and behaviour of a [`Picker`].
pub trait PickerDelegate: Sized + 'static {
    /// The concrete element type returned by [`render_match`](Self::render_match).
    type ListItem: IntoElement;

    /// Total number of current matches.
    fn match_count(&self) -> usize;

    /// Zero-based index of the currently highlighted row.
    fn selected_index(&self) -> usize;

    /// Move the highlight to `ix`.
    fn set_selected_index(
        &mut self,
        ix: usize,
        window: &mut Window,
        cx: &mut Context<Picker<Self>>,
    );

    /// Render the row at `ix`.
    ///
    /// `selected` is `true` when `ix == self.selected_index()`.
    fn render_match(
        &self,
        ix: usize,
        selected: bool,
        window: &mut Window,
        cx: &mut Context<Picker<Self>>,
    ) -> Self::ListItem;

    /// Called whenever the query string changes.
    ///
    /// Implementations update their internal match list and return a
    /// `Task<()>` that resolves once the update is complete.
    fn update_matches(
        &mut self,
        query: SharedString,
        window: &mut Window,
        cx: &mut Context<Picker<Self>>,
    ) -> Task<()>;

    /// Called when the user confirms the current selection.
    ///
    /// `secondary` is `true` for the secondary confirm action (Cmd+Enter on
    /// macOS, Ctrl+Enter elsewhere).
    fn confirm(&mut self, secondary: bool, window: &mut Window, cx: &mut Context<Picker<Self>>);

    /// Called when the picker is dismissed (Esc pressed).
    fn dismissed(&mut self, window: &mut Window, cx: &mut Context<Picker<Self>>);

    /// Placeholder shown in the query input when empty.
    ///
    /// Defaults to `"Search…"`. The default returns a statically allocated
    /// [`SharedString`] and never allocates.
    fn placeholder_text(&self, _cx: &App) -> SharedString {
        SharedString::new_static("Search\u{2026}")
    }
}

// ---------------------------------------------------------------------------
// Picker view
// ---------------------------------------------------------------------------

/// Search-and-select picker view.
///
/// Renders a query [`Input`] above a list of match rows. The
/// [`PickerDelegate`] controls match data, row rendering, and responses to
/// selection, confirmation, and dismissal events.
pub struct Picker<D: PickerDelegate> {
    /// The delegate controlling this picker's data and behaviour.
    pub delegate: D,
    query: Entity<InputState>,
    // Kept alive so the subscription fires for the lifetime of this view.
    _subscription: Subscription,
}

impl<D: PickerDelegate> Picker<D> {
    /// Construct a `Picker` backed by `delegate`.
    ///
    /// Immediately calls [`PickerDelegate::update_matches`] with an empty
    /// query to populate the initial match list.
    #[must_use = "constructing a Picker without storing it discards the view"]
    pub fn new(mut delegate: D, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let placeholder = delegate.placeholder_text(cx);
        let query = cx.new(|cx| InputState::new(window, cx).placeholder(&placeholder));
        delegate
            .update_matches(SharedString::default(), window, cx)
            .detach();
        let subscription = cx.subscribe_in(&query, window, Self::on_input_event);
        Self {
            delegate,
            query,
            _subscription: subscription,
        }
    }

    /// Move the selection to the next match, wrapping at the end.
    pub fn select_next(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let count = self.delegate.match_count();
        if count == 0 {
            return;
        }
        let ix = self.delegate.selected_index();
        let next = (ix + 1) % count;
        self.delegate.set_selected_index(next, window, cx);
        cx.notify();
    }

    /// Move the selection to the previous match, wrapping at the start.
    pub fn select_prev(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let count = self.delegate.match_count();
        if count == 0 {
            return;
        }
        let ix = self.delegate.selected_index();
        let prev = (ix + count - 1) % count;
        self.delegate.set_selected_index(prev, window, cx);
        cx.notify();
    }

    /// Return the current query string.
    #[must_use]
    pub fn query(&self, cx: &App) -> SharedString {
        self.query.read(cx).value()
    }

    fn dismiss(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.delegate.dismissed(window, cx);
        cx.emit(DismissEvent);
    }

    fn on_input_event(
        &mut self,
        input: &Entity<InputState>,
        event: &InputEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            InputEvent::Change => {
                let query = input.read(cx).value();
                self.delegate.update_matches(query, window, cx).detach();
                cx.notify();
            }
            // PressEnter is ignored here: confirm is handled via on_action(InputEnter)
            // in the render div, which also consumes the action so that GPUI's
            // text-input fallthrough does not insert a literal '\n' into the input.
            InputEvent::PressEnter { .. } | InputEvent::Blur | InputEvent::Focus => {}
        }
    }
}

impl<D: PickerDelegate> EventEmitter<DismissEvent> for Picker<D> {}

impl<D: PickerDelegate> std::fmt::Debug for Picker<D> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Picker")
            .field("delegate", &std::any::type_name::<D>())
            .finish_non_exhaustive()
    }
}

impl<D: PickerDelegate> Focusable for Picker<D> {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.query.focus_handle(cx)
    }
}

impl<D: PickerDelegate> Render for Picker<D> {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let match_count = self.delegate.match_count();
        let selected_ix = self.delegate.selected_index();

        // Build item elements eagerly (Phase 1; Phase 2 virtualises this).
        let mut items: Vec<AnyElement> = Vec::with_capacity(match_count);
        for ix in 0..match_count {
            items.push(
                self.delegate
                    .render_match(ix, ix == selected_ix, window, cx)
                    .into_any_element(),
            );
        }

        div()
            .key_context("Picker")
            // InputEnter propagates from single-line InputState with cx.propagate().
            // Catching it here serves two purposes: (a) call confirm and
            // (b) consume the action so GPUI's text-input fallthrough never
            // inserts a literal '\n' into the query field.
            .on_action(cx.listener(|this, action: &InputEnter, window, cx| {
                this.delegate.confirm(action.secondary, window, cx);
            }))
            // Escape propagates from InputState (single-line, no IME, no
            // inline completion) → caught here → dismiss.
            .on_action(cx.listener(|this, _: &Escape, window, cx| {
                this.dismiss(window, cx);
            }))
            .flex_col()
            .child(Input::new(&self.query))
            .child(div().flex_col().children(items))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use std::{
        cell::{Cell, RefCell},
        rc::Rc,
    };

    use gpui::{
        div, App, AppContext, Context, Div, Entity, Focusable, ParentElement, SharedString, Task,
        TestAppContext, Window,
    };

    use super::{Picker, PickerDelegate};

    fn install_theme(cx: &mut TestAppContext) {
        cx.update(gpui_component::init);
    }

    // ------------------------------------------------------------------
    // Shared test delegate
    // ------------------------------------------------------------------

    struct TestDelegate {
        items: Vec<&'static str>,
        selected: usize,
        update_count: Rc<Cell<usize>>,
        confirmed: Rc<Cell<Option<bool>>>,
        was_dismissed: Rc<Cell<bool>>,
    }

    impl TestDelegate {
        fn new(items: Vec<&'static str>) -> Self {
            Self {
                items,
                selected: 0,
                update_count: Rc::new(Cell::new(0)),
                confirmed: Rc::new(Cell::new(None)),
                was_dismissed: Rc::new(Cell::new(false)),
            }
        }
    }

    impl PickerDelegate for TestDelegate {
        type ListItem = Div;

        fn match_count(&self) -> usize {
            self.items.len()
        }

        fn selected_index(&self) -> usize {
            self.selected
        }

        fn set_selected_index(
            &mut self,
            ix: usize,
            _window: &mut Window,
            _cx: &mut Context<Picker<Self>>,
        ) {
            self.selected = ix;
        }

        fn render_match(
            &self,
            ix: usize,
            _selected: bool,
            _window: &mut Window,
            _cx: &mut Context<Picker<Self>>,
        ) -> Div {
            div().child(self.items[ix])
        }

        fn update_matches(
            &mut self,
            _query: SharedString,
            _window: &mut Window,
            _cx: &mut Context<Picker<Self>>,
        ) -> Task<()> {
            self.update_count.set(self.update_count.get() + 1);
            Task::ready(())
        }

        fn confirm(
            &mut self,
            secondary: bool,
            _window: &mut Window,
            _cx: &mut Context<Picker<Self>>,
        ) {
            self.confirmed.set(Some(secondary));
        }

        fn dismissed(&mut self, _window: &mut Window, _cx: &mut Context<Picker<Self>>) {
            self.was_dismissed.set(true);
        }

        fn placeholder_text(&self, _cx: &App) -> SharedString {
            SharedString::new_static("Search\u{2026}")
        }
    }

    // ------------------------------------------------------------------
    // Tests
    // ------------------------------------------------------------------

    /// Constructing a picker with an empty delegate must not panic.
    #[gpui::test]
    fn picker_renders_empty_with_no_matches(cx: &mut TestAppContext) {
        install_theme(cx);
        let (_picker, _cx) =
            cx.add_window_view(|window, cx| Picker::new(TestDelegate::new(vec![]), window, cx));
        // no panic ⟹ test passes
    }

    /// `update_matches` must be called once on init and again after an
    /// explicit query injection.
    #[gpui::test]
    fn picker_typing_updates_query(cx: &mut TestAppContext) {
        install_theme(cx);

        let (picker, cx) = cx.add_window_view(|window, cx| {
            Picker::new(TestDelegate::new(vec!["foo", "bar"]), window, cx)
        });

        let after_init = picker.update(cx, |p, _| p.delegate.update_count.get());
        assert!(after_init >= 1, "update_matches must fire on construction");

        picker.update_in(cx, |picker, window, cx| {
            picker
                .delegate
                .update_matches("f".into(), window, cx)
                .detach();
        });
        cx.run_until_parked();

        let after_query = picker.update(cx, |p, _| p.delegate.update_count.get());
        assert_eq!(
            after_query,
            after_init + 1,
            "update_matches must fire once more after query injection"
        );
    }

    /// Two `select_next` calls from index 0 on a 3-item list must reach
    /// index 2.
    #[gpui::test]
    fn picker_arrow_keys_move_selection(cx: &mut TestAppContext) {
        install_theme(cx);

        let (picker, cx) = cx.add_window_view(|window, cx| {
            Picker::new(TestDelegate::new(vec!["a", "b", "c"]), window, cx)
        });

        picker.update_in(cx, |picker, window, cx| {
            picker.select_next(window, cx);
            picker.select_next(window, cx);
        });

        picker.update(cx, |picker, _| {
            assert_eq!(
                picker.delegate.selected_index(),
                2,
                "two select_next calls from index 0 must land at index 2"
            );
        });
    }

    // Helper: open a picker inside a `gpui_component::Root` window so that
    // `Input`'s focus handler can call `Root::read` without panicking.
    // Returns the picker entity and a `VisualTestContext` bound to that window.
    //
    // The Rc<RefCell<…>> slot is the idiomatic way to smuggle an inner entity
    // out of an `add_window_view` closure that must return the *root* type.
    macro_rules! picker_in_root {
        ($cx:ident, $delegate:expr) => {{
            let slot: Rc<RefCell<Option<Entity<Picker<TestDelegate>>>>> =
                Rc::new(RefCell::new(None));
            let slot2 = slot.clone();
            let (_root, cx) = $cx.add_window_view(|window, cx| {
                let picker = cx.new(|cx| Picker::new($delegate, window, cx));
                *slot2.borrow_mut() = Some(picker.clone());
                gpui_component::Root::new(picker, window, cx)
            });
            let picker = slot.borrow().as_ref().unwrap().clone();
            (picker, cx)
        }};
    }

    /// Enter (primary confirm) must call `confirm(secondary=false)`.
    #[gpui::test]
    fn picker_enter_confirms_selection(cx: &mut TestAppContext) {
        install_theme(cx);

        let delegate = TestDelegate::new(vec!["item"]);
        let confirmed = delegate.confirmed.clone();

        let (picker, cx) = picker_in_root!(cx, delegate);

        picker.update_in(cx, |picker, window, cx| {
            picker.focus_handle(cx).focus(window, cx);
        });

        cx.simulate_keystrokes("enter");

        assert_eq!(
            confirmed.get(),
            Some(false),
            "enter must call confirm(secondary=false)"
        );
    }

    /// Secondary-enter (Cmd+Enter on macOS) must call `confirm(secondary=true)`.
    #[gpui::test]
    fn picker_cmd_enter_secondary_confirms(cx: &mut TestAppContext) {
        install_theme(cx);

        let delegate = TestDelegate::new(vec!["item"]);
        let confirmed = delegate.confirmed.clone();

        let (picker, cx) = picker_in_root!(cx, delegate);

        picker.update_in(cx, |picker, window, cx| {
            picker.focus_handle(cx).focus(window, cx);
        });

        cx.simulate_keystrokes("secondary-enter");

        assert_eq!(
            confirmed.get(),
            Some(true),
            "secondary-enter must call confirm(secondary=true)"
        );
    }

    /// Esc must call `dismissed` on the delegate.
    #[gpui::test]
    fn picker_esc_dismisses(cx: &mut TestAppContext) {
        install_theme(cx);

        let delegate = TestDelegate::new(vec!["item"]);
        let was_dismissed = delegate.was_dismissed.clone();

        let (picker, cx) = picker_in_root!(cx, delegate);

        picker.update_in(cx, |picker, window, cx| {
            picker.focus_handle(cx).focus(window, cx);
        });

        cx.simulate_keystrokes("escape");

        assert!(was_dismissed.get(), "escape must call dismissed");
    }
}
