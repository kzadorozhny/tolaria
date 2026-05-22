// ---------------------------------------------------------------------------
// TolariaFormattingToolbar smoke (worklist 9.3.7)
// ---------------------------------------------------------------------------
//
// happy-dom can't drive BlockNote's FormattingToolbarController past
// its floating-UI portal — there's no layout, so the toolbar never
// becomes "open" from the toolbar-store's perspective.  Instead we
// exercise the *pure* parts of the port directly:
//
//   1. `filterTolariaFormattingToolbarItems` drops the five
//      markdown-incompatible keys (underline, textAlign{Left,Center,Right},
//      color).
//   2. `insertInlineCodeButton` inserts the inline-code button
//      *immediately after* the strike button (the rule the React-side
//      app pins) and leaves the order untouched when no strike is
//      present.
//   3. `TolariaFormattingToolbar` renders without crashing — the
//      BlockNote React internals are mocked so the FormattingToolbar
//      shell becomes a `<div>` we can introspect, and the
//      block-type select trigger lands as a `.tolaria-block-type-select`
//      button in the DOM (the load-bearing CSS hook for the parity
//      with the React app's BlockTypeSelect treatment).
//
// The full controller path (focus / hover / close-grace / hover-guard
// re-pinning) is covered transitively by the React-side suite at
// `src/components/tolariaBlockNoteFormattingToolbar*.test.tsx` —
// duplicating those tests here would just bind the same code paths to
// the same mocks.  The host smoke pins the *wiring* (filter, key
// order, DOM hooks), not the toolbar-store interactions.

import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
    createElement,
    type PropsWithChildren,
    type ReactElement,
    type ReactNode,
} from "react";

// ---------------------------------------------------------------------------
// BlockNote mocks
// ---------------------------------------------------------------------------
//
// `@blocknote/react`'s real exports try to use the BlockNote
// React-context, which happy-dom + a bare `render()` can't satisfy.
// Mock the modules to expose stub components / hooks that the
// formatting-toolbar code paths exercise.

type FormattingToolbarButtonProps = {
    className?: string;
    icon?: ReactNode;
    label: string;
    mainTooltip?: string;
    secondaryTooltip?: string;
    onClick?: () => void;
    isSelected?: boolean;
    children?: ReactNode;
};

type MenuItemProps = {
    className?: string;
    checked?: boolean;
    icon?: ReactNode;
    onClick?: () => void;
    children?: ReactNode;
};

let mockEditor: {
    domElement: HTMLElement;
    focus: ReturnType<typeof vi.fn>;
    getActiveStyles: ReturnType<typeof vi.fn>;
    getSelection: ReturnType<typeof vi.fn>;
    getTextCursorPosition: ReturnType<typeof vi.fn>;
    isEditable: boolean;
    schema: {
        styleSchema: Record<string, { type: string; propSchema: string }>;
    };
    toggleStyles: ReturnType<typeof vi.fn>;
    transact: ReturnType<typeof vi.fn>;
    updateBlock: ReturnType<typeof vi.fn>;
};

vi.mock("@blocknote/core/extensions", () => ({
    FormattingToolbarExtension: () => ({
        key: "formattingToolbar",
    }),
}));

vi.mock("@blocknote/core", async (importOriginal) => {
    const original = await importOriginal<typeof import("@blocknote/core")>();
    return {
        ...original,
        // editorHasBlockWithType is called against the mocked editor; the
        // real implementation introspects the editor's block schema,
        // which our minimal mock can't fulfil.  Return `true` so all
        // block-type select rows render.
        editorHasBlockWithType: () => true,
        blockHasType: () => true,
        defaultProps: { textAlignment: "left" },
    };
});

vi.mock("@blocknote/react", () => {
    return {
        // FormattingToolbar shell — render children inside a div so we
        // can assert on the rendered tree.
        FormattingToolbar: ({ children }: PropsWithChildren) =>
            createElement("div", { "data-testid": "formatting-toolbar" }, children),
        // PositionPopover — pass children through; happy-dom has no
        // layout so the floating-UI dance is meaningless here.
        PositionPopover: ({ children }: PropsWithChildren) =>
            createElement("div", { "data-testid": "position-popover" }, children),
        // The default toolbar item list — return a fixed list with stable
        // keys so the filter/insert tests can pin order.
        getFormattingToolbarItems: () => {
            const items: ReactElement[] = [
                createElement("span", {
                    key: "blockTypeSelect",
                    "data-test": "blockTypeSelect",
                }),
                createElement("span", {
                    key: "boldStyleButton",
                    "data-test": "boldStyleButton",
                }),
                createElement("span", {
                    key: "italicStyleButton",
                    "data-test": "italicStyleButton",
                }),
                createElement("span", {
                    key: "underlineStyleButton",
                    "data-test": "underlineStyleButton",
                }),
                createElement("span", {
                    key: "strikeStyleButton",
                    "data-test": "strikeStyleButton",
                }),
                createElement("span", {
                    key: "textAlignLeftButton",
                    "data-test": "textAlignLeftButton",
                }),
                createElement("span", {
                    key: "textAlignCenterButton",
                    "data-test": "textAlignCenterButton",
                }),
                createElement("span", {
                    key: "textAlignRightButton",
                    "data-test": "textAlignRightButton",
                }),
                createElement("span", {
                    key: "colorStyleButton",
                    "data-test": "colorStyleButton",
                }),
                createElement("span", {
                    key: "fileDownloadButton",
                    "data-test": "fileDownloadButton",
                }),
            ];
            return items;
        },
        useBlockNoteEditor: () => mockEditor,
        useComponentsContext: () => ({
            FormattingToolbar: {
                Root: ({ children, className }: PropsWithChildren<{ className?: string }>) =>
                    createElement("div", { className, role: "toolbar" }, children),
                Button: ({
                    children,
                    className,
                    icon,
                    label,
                    onClick,
                    isSelected,
                }: FormattingToolbarButtonProps) =>
                    createElement(
                        "button",
                        {
                            type: "button",
                            className,
                            "aria-label": label,
                            "aria-pressed": isSelected ? "true" : undefined,
                            onClick,
                        },
                        icon,
                        children,
                    ),
                Select: () => null,
            },
            Generic: {
                Menu: {
                    Root: ({ children }: PropsWithChildren) =>
                        createElement("div", { "data-testid": "menu-root" }, children),
                    Trigger: ({ children }: PropsWithChildren) =>
                        createElement("div", { "data-testid": "menu-trigger" }, children),
                    Dropdown: ({ children, className }: PropsWithChildren<{ className?: string }>) =>
                        createElement(
                            "div",
                            { className, "data-testid": "menu-dropdown" },
                            children,
                        ),
                    Item: ({ children, className, icon, onClick }: MenuItemProps) =>
                        createElement(
                            "div",
                            {
                                role: "menuitem",
                                className,
                                onClick,
                            },
                            icon,
                            children,
                        ),
                    Divider: () => null,
                    Label: ({ children }: PropsWithChildren) =>
                        createElement("div", null, children),
                    Button: ({ children }: PropsWithChildren) =>
                        createElement("button", { type: "button" }, children),
                },
            },
        }),
        useDictionary: () => ({
            formatting_toolbar: {
                file_download: {
                    tooltip: {
                        file: "Download file",
                        image: "Download image",
                    },
                },
            },
        }),
        useEditorState: (
            options: { selector?: (state: { editor: unknown }) => unknown } & {
                selector: (state: { editor: typeof mockEditor }) => unknown;
            },
        ) => options.selector({ editor: mockEditor }),
        useExtension: () => ({
            store: { setState: vi.fn() },
        }),
        useExtensionState: () => false,
    };
});

// useEditorComposing is local — stub it directly so happy-dom doesn't
// need to fire composition events.
vi.mock("./useEditorComposing.ts", () => ({
    useEditorComposing: () => false,
}));

// Hover-guard hook — stub so the test doesn't need a live editor
// container to bind listeners against.
vi.mock("./blockNoteFormattingToolbarHoverGuard.ts", () => ({
    useBlockNoteFormattingToolbarHoverGuard: () => undefined,
}));

import {
    TolariaFormattingToolbar,
    UNSUPPORTED_FORMATTING_TOOLBAR_KEYS,
    filterTolariaFormattingToolbarItems,
    insertInlineCodeButton,
} from "./tolariaBlockNoteFormattingToolbar.tsx";

describe("TolariaFormattingToolbar (editor-host) — pure helpers", () => {
    it("filters out the markdown-incompatible toolbar keys", () => {
        const items: ReactElement[] = [
            createElement("span", { key: "blockTypeSelect" }),
            createElement("span", { key: "boldStyleButton" }),
            createElement("span", { key: "underlineStyleButton" }),
            createElement("span", { key: "italicStyleButton" }),
            createElement("span", { key: "textAlignLeftButton" }),
            createElement("span", { key: "textAlignCenterButton" }),
            createElement("span", { key: "textAlignRightButton" }),
            createElement("span", { key: "colorStyleButton" }),
            createElement("span", { key: "strikeStyleButton" }),
        ];

        const filtered = filterTolariaFormattingToolbarItems(items);
        const remainingKeys = filtered.map((item) => String(item.key));

        // The five "unsupported" keys must all be removed.
        for (const key of UNSUPPORTED_FORMATTING_TOOLBAR_KEYS) {
            expect(remainingKeys).not.toContain(key);
        }

        // The supported keys must survive in their original order.
        expect(remainingKeys).toEqual([
            "blockTypeSelect",
            "boldStyleButton",
            "italicStyleButton",
            "strikeStyleButton",
        ]);
    });

    it("inserts the inline-code button immediately after the strike button", () => {
        const items: ReactElement[] = [
            createElement("span", { key: "blockTypeSelect" }),
            createElement("span", { key: "boldStyleButton" }),
            createElement("span", { key: "italicStyleButton" }),
            createElement("span", { key: "strikeStyleButton" }),
            createElement("span", { key: "fileDownloadButton" }),
        ];

        const augmented = insertInlineCodeButton(items);
        const keys = augmented.map((item) => String(item.key));

        // The inline-code button lives at the position right after strike.
        const strikeIndex = keys.indexOf("strikeStyleButton");
        expect(strikeIndex).toBeGreaterThanOrEqual(0);
        expect(keys[strikeIndex + 1]).toBe("codeStyleButton");

        // The pre-strike order is preserved and fileDownload still trails.
        expect(keys).toEqual([
            "blockTypeSelect",
            "boldStyleButton",
            "italicStyleButton",
            "strikeStyleButton",
            "codeStyleButton",
            "fileDownloadButton",
        ]);
    });

    it("leaves the items untouched when no strike button is present", () => {
        const items: ReactElement[] = [
            createElement("span", { key: "blockTypeSelect" }),
            createElement("span", { key: "boldStyleButton" }),
        ];

        const augmented = insertInlineCodeButton(items);
        expect(augmented.map((item) => String(item.key))).toEqual([
            "blockTypeSelect",
            "boldStyleButton",
        ]);
    });
});

describe("TolariaFormattingToolbar (editor-host) — render", () => {
    beforeEach(() => {
        const editorElement = document.createElement("div");
        editorElement.className = "bn-editor";
        document.body.appendChild(editorElement);

        const selectedBlock = {
            id: "block-1",
            type: "paragraph",
            props: { textAlignment: "left" },
            content: ["hello"],
        };

        mockEditor = {
            domElement: editorElement,
            focus: vi.fn(),
            getActiveStyles: vi.fn(() => ({ bold: false })),
            getSelection: vi.fn(() => ({ blocks: [selectedBlock] })),
            getTextCursorPosition: vi.fn(() => ({ block: selectedBlock })),
            isEditable: true,
            schema: {
                styleSchema: {
                    bold: { type: "bold", propSchema: "boolean" },
                    italic: { type: "italic", propSchema: "boolean" },
                    strike: { type: "strike", propSchema: "boolean" },
                    code: { type: "code", propSchema: "boolean" },
                },
            },
            toggleStyles: vi.fn(),
            transact: vi.fn((callback: () => void) => callback()),
            updateBlock: vi.fn(),
        };
    });

    afterEach(() => {
        cleanup();
        document.body.innerHTML = "";
    });

    it("renders without crashing and mounts the block-type select trigger", () => {
        render(<TolariaFormattingToolbar />);

        // The Tolaria-specific BlockTypeSelect lands as a button with
        // the `.tolaria-block-type-select` CSS hook.  This is the
        // load-bearing assertion that the BlockTypeSelect path
        // mounted at all (it returns null if the selected block has
        // no matching item in `TOLARIA_BLOCK_TYPE_SELECT_ITEMS`).
        const trigger = document.querySelector(".tolaria-block-type-select");
        expect(trigger).not.toBeNull();
        expect(trigger).toBeInstanceOf(HTMLElement);

        // The inline-code button is present and lands the
        // `.tolaria-format-code` CSS hook — the React-side rule the
        // styling parity row keys off.
        const codeButton = document.querySelector(".tolaria-format-code");
        expect(codeButton).not.toBeNull();

        // The five unsupported keys must not appear in the toolbar
        // DOM — `tolaria-format-underline` etc. are absent because
        // those items were never instantiated.
        expect(
            document.querySelector(".tolaria-format-underline"),
        ).toBeNull();

        // The toolbar shell itself rendered.
        expect(screen.getByTestId("formatting-toolbar")).toBeInTheDocument();
    });
});
