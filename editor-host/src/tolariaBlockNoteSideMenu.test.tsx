// ---------------------------------------------------------------------------
// TolariaSideMenu smoke (worklist 9.3.1)
// ---------------------------------------------------------------------------
//
// happy-dom can't drive the BlockNote SideMenuController past its
// internal `mousemove` listener (no real layout), so we can't assert
// the integrated mount here.  Instead we render the
// `TolariaSideMenu` component directly with mocked BlockNote
// internals (mirrors the React-side
// `src/components/tolariaBlockNoteSideMenu.test.tsx` setup) and
// confirm:
//
//   1. The `.tolaria-block-drag-handle` class — the load-bearing
//      CSS hook for the parity with the React app's drag-handle
//      treatment — lands on a DOM element.
//   2. The component renders both controls (Add block + Drag block)
//      and a Delete menu item (the rename from BlockNote's "Colors"
//      to the markdown-safe item set the React app ships).
//
// The pure-logic helpers (pointer reorder geometry, alignment math)
// are covered transitively by the React-side suite at
// `src/components/tolariaBlockNoteSideMenu.test.tsx` — re-running
// them here would just duplicate ~600 lines of fixtures.  The host
// smoke pins the *wiring* (CSS hook + DOM shape), not the math.

import { render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { PropsWithChildren, ReactNode } from "react";
import { TolariaSideMenu } from "./tolariaBlockNoteSideMenu.tsx";

type SideMenuButtonProps = {
    className?: string;
    draggable?: boolean;
    icon?: ReactNode;
    label: string;
    onClick?: () => void;
};

type MenuItemProps = PropsWithChildren<{
    checked?: boolean;
    className?: string;
    onClick?: () => void;
}>;

type MockBlock = {
    children?: MockBlock[];
    id: string;
    type: string;
    content?: unknown;
};

let sideMenuBlock: MockBlock | undefined;
let mockEditor: {
    domElement: HTMLElement;
    focus: ReturnType<typeof vi.fn>;
    getBlock: ReturnType<typeof vi.fn>;
    insertBlocks: ReturnType<typeof vi.fn>;
    removeBlocks: ReturnType<typeof vi.fn>;
    setTextCursorPosition: ReturnType<typeof vi.fn>;
    settings: { tables: { headers: boolean } };
    transact: ReturnType<typeof vi.fn>;
    updateBlock: ReturnType<typeof vi.fn>;
};
let mockSideMenu: {
    blockDragEnd: ReturnType<typeof vi.fn>;
    freezeMenu: ReturnType<typeof vi.fn>;
    unfreezeMenu: ReturnType<typeof vi.fn>;
};
let mockSuggestionMenu: { openSuggestionMenu: ReturnType<typeof vi.fn> };

vi.mock("@blocknote/core/extensions", () => ({
    SideMenuExtension: { key: "side-menu" },
    SuggestionMenu: { key: "suggestion-menu" },
}));

vi.mock("@blocknote/react", () => ({
    DragHandleMenu: ({ children }: PropsWithChildren) => (
        <div data-testid="drag-handle-menu">{children}</div>
    ),
    SideMenu: ({ children }: PropsWithChildren) => (
        <div data-testid="side-menu">{children}</div>
    ),
    useBlockNoteEditor: () => mockEditor,
    useComponentsContext: () => ({
        Generic: {
            Menu: {
                Item: ({ children, onClick, className }: MenuItemProps) => (
                    <div role="menuitem" className={className} onClick={onClick}>
                        {children}
                    </div>
                ),
                Root: ({ children }: PropsWithChildren) => <div data-testid="menu-root">{children}</div>,
                Trigger: ({ children }: PropsWithChildren) => <div>{children}</div>,
            },
        },
        SideMenu: {
            Button: ({ className, label, onClick, icon }: SideMenuButtonProps) => (
                <button type="button" className={className} onClick={onClick} aria-label={label}>
                    {icon}
                    {label}
                </button>
            ),
        },
    }),
    useDictionary: () => ({
        drag_handle: {
            delete_menuitem: "Delete",
            header_row_menuitem: "Header row",
            header_column_menuitem: "Header column",
        },
        side_menu: {
            add_block_label: "Add block",
            drag_handle_label: "Drag block",
        },
    }),
    useExtension: (extension: { key: string }) =>
        extension.key === "suggestion-menu" ? mockSuggestionMenu : mockSideMenu,
    useExtensionState: (
        _extension: unknown,
        options?: { selector?: (state: { block?: MockBlock }) => unknown },
    ) =>
        options?.selector
            ? options.selector({ block: sideMenuBlock })
            : { block: sideMenuBlock },
}));

describe("TolariaSideMenu (editor-host)", () => {
    beforeEach(() => {
        const editorElement = document.createElement("div");
        editorElement.className = "bn-editor";
        document.body.appendChild(editorElement);

        sideMenuBlock = {
            id: "block-1",
            type: "paragraph",
            content: ["hello"],
            children: [],
        };
        mockEditor = {
            domElement: editorElement,
            focus: vi.fn(),
            getBlock: vi.fn((id: string) => (id === sideMenuBlock?.id ? sideMenuBlock : undefined)),
            insertBlocks: vi.fn(() => [{ id: "inserted", type: "paragraph", content: [] }]),
            removeBlocks: vi.fn(),
            setTextCursorPosition: vi.fn(),
            settings: { tables: { headers: true } },
            transact: vi.fn((callback: () => void) => callback()),
            updateBlock: vi.fn(),
        };
        mockSideMenu = {
            blockDragEnd: vi.fn(),
            freezeMenu: vi.fn(),
            unfreezeMenu: vi.fn(),
        };
        mockSuggestionMenu = { openSuggestionMenu: vi.fn() };
    });

    afterEach(() => {
        document.body.innerHTML = "";
    });

    it("renders the `.tolaria-block-drag-handle` CSS hook on a DOM node", () => {
        render(<TolariaSideMenu />);

        const dragHandle = document.querySelector(".tolaria-block-drag-handle");
        expect(dragHandle).not.toBeNull();
        // The CSS hook must be on a real element (not a text node), so
        // the editor-host stylesheet's `.editor-host-container
        // .tolaria-block-drag-handle` selector lands.
        expect(dragHandle).toBeInstanceOf(HTMLElement);
    });

    it("renders the Tolaria-flavoured add-block + drag-handle controls", () => {
        render(<TolariaSideMenu />);

        expect(screen.getByTestId("side-menu")).toBeInTheDocument();
        expect(screen.getByRole("button", { name: "Add block" })).toBeInTheDocument();
        expect(screen.getByRole("button", { name: "Drag block" })).toBeInTheDocument();
        // The React-era SideMenu replaces BlockNote's stock "Colors"
        // pane (markdown can't round-trip block colours) with a
        // markdown-safe `Delete` menu item — assert the rename here
        // too so a future BlockNote upgrade can't sneak the upstream
        // default back in.
        expect(screen.getByText("Delete")).toBeInTheDocument();
    });
});
