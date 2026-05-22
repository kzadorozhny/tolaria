import { useCallback, useRef } from "react";
import type { BlockNoteEditor } from "@blocknote/core";
import { filterSuggestionItems } from "@blocknote/core/extensions";
import {
    SideMenuController,
    SuggestionMenuController,
    getDefaultReactSlashMenuItems,
} from "@blocknote/react";
import { useBlockNoteSideMenuHoverGuard } from "./blockNoteSideMenuHoverGuard.ts";
import { TolariaSideMenu } from "./tolariaBlockNoteSideMenu.tsx";
import { TolariaFormattingToolbarController } from "./tolariaBlockNoteFormattingToolbar.tsx";

// ---------------------------------------------------------------------------
// EditorMenus — slash / side / formatting menus + hover guards (Phase 8.25)
// ---------------------------------------------------------------------------
//
// Mounts BlockNote's menu surfaces (slash-triggered suggestion menu,
// side drag-handle, selection formatting toolbar) inside the host
// `<BlockNoteViewRaw>` tree.
//
// 8.25 wired the side-menu hover guard.  9.3.1 swapped the default
// SideMenu for `TolariaSideMenu`.  9.3.7 swaps the default
// `FormattingToolbarController` for `TolariaFormattingToolbarController`
// (mantine-free port of `src/components/tolariaEditorFormatting.tsx`),
// which folds the formatting-toolbar hover-guard wiring + focus /
// hover / close-grace + composition gating into the controller itself
// — so this file no longer needs its own copy of those plumbing
// helpers.

export interface EditorMenusProps {
    editor: BlockNoteEditor;
}

/**
 * Resolve the side-menu's host-scope element.  The hover guard's
 * listeners are bound to whichever wrapper contains the editor (the
 * editor-host's `.editor-host-container`, or — for legacy compat —
 * the React shell's `.editor__blocknote-container`), falling back to
 * the editor element itself when neither is present.
 */
function resolveSideMenuContainer(editor: BlockNoteEditor): HTMLElement | null {
    const dom = editor.domElement;
    if (!(dom instanceof HTMLElement)) return null;
    return (
        dom.closest<HTMLElement>(".editor-host-container") ??
        dom.closest<HTMLElement>(".editor__blocknote-container") ??
        dom
    );
}

/**
 * Render the three menu controllers wrapped by their hover guards.
 * Must be mounted as a child of `<BlockNoteViewRaw>` so the BlockNote
 * React context is in scope.
 */
export function EditorMenus({ editor }: EditorMenusProps) {
    // Hold an HTMLElement ref pointing at the editor's host container
    // for the side-menu hover guard.  The ref is updated on every render
    // because BlockNote may reparent its DOM during lifecycle churn.
    const containerRef = useRef<HTMLElement | null>(null);
    containerRef.current = resolveSideMenuContainer(editor);

    // Side-menu hover guard: suppresses flicker on the editor / drag
    // handle bridge gutter.  The formatting-toolbar hover guard now
    // lives inside `TolariaFormattingToolbarController` (9.3.7) — it
    // sees the full `isOpen` signal (composition + focus + close grace),
    // which the bare "any file block selected" approximation from 8.25
    // couldn't.
    useBlockNoteSideMenuHoverGuard(containerRef);

    // Slash menu items closure — `getDefaultReactSlashMenuItems` returns
    // a fresh array each call so we memoize via the editor identity to
    // avoid rebuilding the array on every keystroke.
    const getSlashItems = useCallback(
        async (query: string) =>
            filterSuggestionItems(
                getDefaultReactSlashMenuItems(editor as never),
                query,
            ),
        [editor],
    );

    return (
        <>
            {/*
             * Worklist 2.24: SideMenuController re-mounted now that
             * `@blocknote/shadcn` installs a real ComponentsContext.Provider
             * upstream (see `EditorApp.tsx`'s `<BlockNoteView>` mount).
             * The default AddBlockButton dereferences `e.SideMenu.Button`
             * against that provider's components map instead of `undefined`,
             * so the first mousemove no longer throws and the WKWebView
             * keeps its drag-handle gutter.
             *
             * Worklist 9.3.1: mount our `TolariaSideMenu` instead of the
             * BlockNote default so the drag-handle picks up the
             * `.tolaria-block-drag-handle` CSS hook (declared in
             * `style.css` — see the `.editor-host-container
             * .tolaria-block-drag-handle` rule) and the pointer-based
             * reorder gesture takes over from the wonky HTML-5 drag
             * flow that BlockNote's stock control uses.
             *
             * Worklist 9.3.7: mount our `TolariaFormattingToolbarController`
             * instead of the BlockNote default so the selection menu
             * carries the React-side custom controls (Tolaria-flavoured
             * BlockTypeSelect, markdown-safe filter, inline-code button
             * inserted after strike).
             */}
            <SideMenuController sideMenu={TolariaSideMenu} />
            <TolariaFormattingToolbarController />
            <SuggestionMenuController
                triggerCharacter="/"
                getItems={getSlashItems}
            />
        </>
    );
}
