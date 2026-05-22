// ---------------------------------------------------------------------------
// TolariaFormattingToolbar — custom BlockNote formatting toolbar (9.3.7)
// ---------------------------------------------------------------------------
//
// Mirrors the 9.3.1 SideMenu port: a verbatim TypeScript port of
// `src/components/tolariaEditorFormatting.tsx` adapted to the
// editor-host's mantine-free stack.  The React app wraps BlockNote's
// default `<FormattingToolbar>` (the floating selection menu) with a
// Tolaria-flavoured shell that:
//
//   - drops controls that don't round-trip through markdown
//     (underline, text-alignment, colour),
//   - injects an inline-code button right after the strike button,
//   - swaps Mantine's `<Menu>` for BlockNote's vanilla
//     `Components.Generic.Menu.*` primitives for the block-type select,
//   - tracks toolbar focus / hover so the menu stays open when the
//     pointer crosses from the toolbar back onto a file-block,
//   - re-uses the formatting-toolbar hover guard already ported to
//     `blockNoteFormattingToolbarHoverGuard.ts` so file-block bridges
//     don't flicker on hover.
//
// Mantine-free port: the React app uses `MantineButton`,
// `MantineCheckIcon`, `MantineMenu` for the BlockTypeSelect dropdown.
// Editor-host has no Mantine — we rebuild the dropdown trigger via
// `Components.FormattingToolbar.Button` (BlockNote's vanilla button
// primitive) inside `Components.Generic.Menu.Root/Trigger`, and the
// drop-down rows via `Components.Generic.Menu.Dropdown/Item` (the
// same primitives the 9.3.1 side-menu port uses for its drag-handle
// menu).
//
// Icons: phosphor-icons is not in the editor-host bundle (the
// single-file dist would balloon by ~100 kB).  All glyphs are inline
// SVGs sized at 16 px to match the BlockNote toolbar baseline (see
// `style.css`'s `.bn-toolbar button svg` rule).
//
// File download: the React app routes through
// `openEditorAttachmentOrUrl`, which falls back to Tauri shell IPC
// for vault-relative attachments.  Editor-host has no such bridge yet
// — we fall back to `window.open(url, '_blank')` so external URLs
// still work; vault-relative attachments are a TODO until the
// host gains an attachment bridge.

import {
    FormattingToolbar,
    PositionPopover,
    getFormattingToolbarItems,
    useBlockNoteEditor,
    useComponentsContext,
    useDictionary,
    useEditorState,
    useExtension,
    useExtensionState,
} from "@blocknote/react";
import type {
    FloatingUIOptions,
    FormattingToolbarProps,
} from "@blocknote/react";
import {
    blockHasType,
    defaultProps,
    editorHasBlockWithType,
} from "@blocknote/core";
import type {
    BlockNoteEditor,
    BlockSchema,
    DefaultProps,
    InlineContentSchema,
    StyleSchema,
} from "@blocknote/core";
import { FormattingToolbarExtension } from "@blocknote/core/extensions";
import {
    useCallback,
    useEffect,
    useMemo,
    useRef,
    useState,
    type Dispatch,
    type FC,
    type MutableRefObject,
    type ReactElement,
    type SetStateAction,
} from "react";
import { useEditorComposing } from "./useEditorComposing.ts";
import { useBlockNoteFormattingToolbarHoverGuard } from "./blockNoteFormattingToolbarHoverGuard.ts";

// ---------------------------------------------------------------------------
// Types and constants
// ---------------------------------------------------------------------------

type TolariaBlockNoteEditor = BlockNoteEditor<
    BlockSchema,
    InlineContentSchema,
    StyleSchema
>;

type TolariaSelectedBlock = ReturnType<
    TolariaBlockNoteEditor["getTextCursorPosition"]
>["block"];

type TolariaBasicTextStyle = "bold" | "italic" | "strike" | "code";

type TolariaSelectedFileBlock = {
    type: string;
    url: string;
};

type TolariaBlockTypeSelectItem = {
    name: string;
    type: string;
    props?: Record<string, boolean | number | string>;
    iconElement: ReactElement;
};

type TolariaBlockTypeSelectOption = TolariaBlockTypeSelectItem & {
    isSelected: boolean;
};

const FORMATTER_CLOSE_GRACE_MS = 160;

/**
 * Toolbar items that don't round-trip through markdown — we filter
 * them out before rendering.  Mirrors
 * `UNSUPPORTED_FORMATTING_TOOLBAR_KEYS` in
 * `src/components/tolariaEditorFormattingConfig.ts`.
 */
export const UNSUPPORTED_FORMATTING_TOOLBAR_KEYS = new Set([
    "underlineStyleButton",
    "textAlignLeftButton",
    "textAlignCenterButton",
    "textAlignRightButton",
    "colorStyleButton",
]);

const FORMATTING_TOOLBAR_FILE_BLOCK_TYPES = new Set([
    "audio",
    "file",
    "image",
    "video",
]);

const TOLARIA_BASIC_TEXT_STYLE_TOOLTIPS = {
    bold: {
        label: "Bold",
        mainTooltip: "Bold (persists in markdown)",
        secondaryTooltip: "**strong**",
    },
    italic: {
        label: "Italic",
        mainTooltip: "Italic (persists in markdown)",
        secondaryTooltip: "*emphasis*",
    },
    strike: {
        label: "Strikethrough",
        mainTooltip: "Strikethrough (persists in markdown)",
        secondaryTooltip: "~~strike~~",
    },
    code: {
        label: "Inline code",
        mainTooltip: "Inline code (persists in markdown)",
        secondaryTooltip: "`code`",
    },
} satisfies Record<
    TolariaBasicTextStyle,
    { label: string; mainTooltip: string; secondaryTooltip: string }
>;

// ---------------------------------------------------------------------------
// Inline SVG glyphs
// ---------------------------------------------------------------------------
//
// Replacements for phosphor-icons: each glyph is sized at 16 px to
// match BlockNote's toolbar baseline.  Stroke / fill match the
// phosphor "Regular" weight closely enough that QA can't tell them
// apart side-by-side.

type IconProps = { size?: number };

function BoldIcon({ size = 16 }: IconProps = {}) {
    return (
        <svg
            data-test="boldIcon"
            width={size}
            height={size}
            viewBox="0 0 24 24"
            fill="currentColor"
            aria-hidden="true"
        >
            <path d="M14.5 11.2A4 4 0 0 0 12 5H7a1 1 0 0 0-1 1v12a1 1 0 0 0 1 1h6a4.2 4.2 0 0 0 1.5-7.8ZM8 7h4a2 2 0 1 1 0 4H8V7Zm5 10H8v-4h5a2 2 0 1 1 0 4Z" />
        </svg>
    );
}

function ItalicIcon({ size = 16 }: IconProps = {}) {
    return (
        <svg
            data-test="italicIcon"
            width={size}
            height={size}
            viewBox="0 0 24 24"
            fill="currentColor"
            aria-hidden="true"
        >
            <path d="M19 6a1 1 0 0 1-1 1h-3.06l-4 10H14a1 1 0 1 1 0 2H6a1 1 0 1 1 0-2h2.94l4-10H10a1 1 0 1 1 0-2h8a1 1 0 0 1 1 1Z" />
        </svg>
    );
}

function StrikethroughIcon({ size = 16 }: IconProps = {}) {
    return (
        <svg
            data-test="strikeIcon"
            width={size}
            height={size}
            viewBox="0 0 24 24"
            fill="currentColor"
            aria-hidden="true"
        >
            <path d="M21 13H3a1 1 0 1 1 0-2h18a1 1 0 1 1 0 2ZM7 8a4 4 0 0 1 4-4h2a4 4 0 0 1 3.86 3 1 1 0 1 1-1.94.5 2 2 0 0 0-1.92-1.5H11A2 2 0 0 0 9 8a1 1 0 1 1-2 0Zm10 8a4 4 0 0 1-4 4h-2a4 4 0 0 1-3.86-3 1 1 0 1 1 1.94-.5A2 2 0 0 0 11 18h2a2 2 0 0 0 2-2 1 1 0 1 1 2 0Z" />
        </svg>
    );
}

function CodeIcon({ size = 16 }: IconProps = {}) {
    return (
        <svg
            data-test="codeIcon"
            width={size}
            height={size}
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            strokeWidth="2"
            strokeLinecap="round"
            strokeLinejoin="round"
            aria-hidden="true"
        >
            <polyline points="16 18 22 12 16 6" />
            <polyline points="8 6 2 12 8 18" />
        </svg>
    );
}

function ExternalLinkIcon({ size = 16 }: IconProps = {}) {
    return (
        <svg
            data-test="externalLinkIcon"
            width={size}
            height={size}
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            strokeWidth="2"
            strokeLinecap="round"
            strokeLinejoin="round"
            aria-hidden="true"
        >
            <path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6" />
            <polyline points="15 3 21 3 21 9" />
            <line x1="10" y1="14" x2="21" y2="3" />
        </svg>
    );
}

function CaretDownIcon({ size = 14 }: IconProps = {}) {
    return (
        <svg
            data-test="caretDownIcon"
            width={size}
            height={size}
            viewBox="0 0 24 24"
            fill="currentColor"
            aria-hidden="true"
        >
            <path d="m12 16-6-6h12z" />
        </svg>
    );
}

function CheckIcon({ size = 12 }: IconProps = {}) {
    return (
        <svg
            data-test="checkIcon"
            width={size}
            height={size}
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            strokeWidth="3"
            strokeLinecap="round"
            strokeLinejoin="round"
            aria-hidden="true"
        >
            <polyline points="20 6 9 17 4 12" />
        </svg>
    );
}

// Block-type select icons.  Inline SVGs again — phosphor's
// `Paragraph`, `TextHOne…TextHSix`, `Quotes`, `ListBullets`,
// `ListNumbers`, `ListChecks`, `CodeBlock` are all visually distinct
// at 16 px but cheap to inline.

function ParagraphGlyph() {
    return (
        <svg
            width="16"
            height="16"
            viewBox="0 0 24 24"
            fill="currentColor"
            aria-hidden="true"
        >
            <path d="M19 5h-9a5 5 0 0 0 0 10h2v4a1 1 0 1 0 2 0V7h2v12a1 1 0 1 0 2 0V7h1a1 1 0 1 0 0-2Z" />
        </svg>
    );
}

function HeadingGlyph({ level }: { level: 1 | 2 | 3 | 4 | 5 | 6 }) {
    return (
        <svg
            width="16"
            height="16"
            viewBox="0 0 24 24"
            aria-hidden="true"
        >
            <text
                x="12"
                y="18"
                textAnchor="middle"
                fontFamily="system-ui, sans-serif"
                fontSize="14"
                fontWeight="700"
                fill="currentColor"
            >
                H{level}
            </text>
        </svg>
    );
}

function QuoteGlyph() {
    return (
        <svg
            width="16"
            height="16"
            viewBox="0 0 24 24"
            fill="currentColor"
            aria-hidden="true"
        >
            <path d="M7 7h4v4H8c0 2 1 3 3 4l-1 2c-3-1-5-3-5-7V8a1 1 0 0 1 1-1Zm10 0h4v4h-3c0 2 1 3 3 4l-1 2c-3-1-5-3-5-7V8a1 1 0 0 1 1-1Z" />
        </svg>
    );
}

function BulletListGlyph() {
    return (
        <svg
            width="16"
            height="16"
            viewBox="0 0 24 24"
            fill="currentColor"
            aria-hidden="true"
        >
            <circle cx="5" cy="6" r="1.5" />
            <circle cx="5" cy="12" r="1.5" />
            <circle cx="5" cy="18" r="1.5" />
            <rect x="9" y="5" width="12" height="2" rx="1" />
            <rect x="9" y="11" width="12" height="2" rx="1" />
            <rect x="9" y="17" width="12" height="2" rx="1" />
        </svg>
    );
}

function NumberedListGlyph() {
    return (
        <svg
            width="16"
            height="16"
            viewBox="0 0 24 24"
            aria-hidden="true"
        >
            <text x="2" y="9" fontSize="6" fontFamily="system-ui, sans-serif" fontWeight="700" fill="currentColor">1.</text>
            <text x="2" y="15" fontSize="6" fontFamily="system-ui, sans-serif" fontWeight="700" fill="currentColor">2.</text>
            <text x="2" y="21" fontSize="6" fontFamily="system-ui, sans-serif" fontWeight="700" fill="currentColor">3.</text>
            <rect x="9" y="5" width="12" height="2" rx="1" fill="currentColor" />
            <rect x="9" y="11" width="12" height="2" rx="1" fill="currentColor" />
            <rect x="9" y="17" width="12" height="2" rx="1" fill="currentColor" />
        </svg>
    );
}

function CheckListGlyph() {
    return (
        <svg
            width="16"
            height="16"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            strokeWidth="2"
            strokeLinecap="round"
            strokeLinejoin="round"
            aria-hidden="true"
        >
            <rect x="3" y="4" width="6" height="6" rx="1" />
            <polyline points="4 7 5.5 8.5 8 5.5" />
            <line x1="12" y1="6" x2="21" y2="6" />
            <rect x="3" y="14" width="6" height="6" rx="1" />
            <line x1="12" y1="17" x2="21" y2="17" />
        </svg>
    );
}

function CodeBlockGlyph() {
    return (
        <svg
            width="16"
            height="16"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            strokeWidth="2"
            strokeLinecap="round"
            strokeLinejoin="round"
            aria-hidden="true"
        >
            <rect x="3" y="4" width="18" height="16" rx="2" />
            <polyline points="8 10 6 12 8 14" />
            <polyline points="16 10 18 12 16 14" />
            <line x1="13" y1="9" x2="11" y2="15" />
        </svg>
    );
}

// ---------------------------------------------------------------------------
// Block-type select configuration
// ---------------------------------------------------------------------------

/**
 * The full list of block-type select items the toolbar offers.  Each
 * row carries its inline-SVG icon as a `ReactElement` so the lookup
 * doesn't have to instantiate icon components inside the render path.
 */
const TOLARIA_BLOCK_TYPE_SELECT_ITEMS: TolariaBlockTypeSelectItem[] = [
    { name: "Paragraph", type: "paragraph", iconElement: <ParagraphGlyph /> },
    {
        name: "Heading 1",
        type: "heading",
        props: { level: 1 },
        iconElement: <HeadingGlyph level={1} />,
    },
    {
        name: "Heading 2",
        type: "heading",
        props: { level: 2 },
        iconElement: <HeadingGlyph level={2} />,
    },
    {
        name: "Heading 3",
        type: "heading",
        props: { level: 3 },
        iconElement: <HeadingGlyph level={3} />,
    },
    {
        name: "Heading 4",
        type: "heading",
        props: { level: 4 },
        iconElement: <HeadingGlyph level={4} />,
    },
    {
        name: "Heading 5",
        type: "heading",
        props: { level: 5 },
        iconElement: <HeadingGlyph level={5} />,
    },
    {
        name: "Heading 6",
        type: "heading",
        props: { level: 6 },
        iconElement: <HeadingGlyph level={6} />,
    },
    { name: "Quote", type: "quote", iconElement: <QuoteGlyph /> },
    { name: "Bullet List", type: "bulletListItem", iconElement: <BulletListGlyph /> },
    { name: "Numbered List", type: "numberedListItem", iconElement: <NumberedListGlyph /> },
    { name: "Checklist", type: "checkListItem", iconElement: <CheckListGlyph /> },
    { name: "Code Block", type: "codeBlock", iconElement: <CodeBlockGlyph /> },
];

/**
 * Drop the controls we don't support (underline / alignment /
 * colour).  Exposed for the unit test to assert the filter wires up.
 */
export function filterTolariaFormattingToolbarItems<T extends ReactElement>(
    items: T[],
): T[] {
    return items.filter(
        (item) => !UNSUPPORTED_FORMATTING_TOOLBAR_KEYS.has(String(item.key)),
    );
}

// ---------------------------------------------------------------------------
// Close-grace + toolbar-store deduplication
// ---------------------------------------------------------------------------
//
// The React side keeps the toolbar pinned for a short grace window
// after the selection collapses — otherwise tapping outside while the
// cursor lingers in the middle of a word flickers the toolbar
// closed-then-open.  Ported verbatim.

function isFocusStillWithinToolbar(
    currentTarget: EventTarget & Element,
    nextTarget: EventTarget | null,
) {
    return nextTarget instanceof Node && currentTarget.contains(nextTarget);
}

function clearToolbarCloseGrace(
    timeoutRef: MutableRefObject<number | null>,
    setCloseGraceActive: Dispatch<SetStateAction<boolean>>,
) {
    if (timeoutRef.current !== null) {
        window.clearTimeout(timeoutRef.current);
        timeoutRef.current = null;
    }
    setCloseGraceActive(false);
}

function startToolbarCloseGrace(
    timeoutRef: MutableRefObject<number | null>,
    setCloseGraceActive: Dispatch<SetStateAction<boolean>>,
) {
    setCloseGraceActive(true);
    if (timeoutRef.current !== null) {
        window.clearTimeout(timeoutRef.current);
    }
    timeoutRef.current = window.setTimeout(() => {
        timeoutRef.current = null;
        setCloseGraceActive(false);
    }, FORMATTER_CLOSE_GRACE_MS);
}

function useFormattingToolbarCloseGrace({
    show,
    toolbarHasFocus,
    toolbarHovered,
}: {
    show: boolean;
    toolbarHasFocus: boolean;
    toolbarHovered: boolean;
}) {
    const [closeGraceActive, setCloseGraceActive] = useState(false);
    const closeGraceTimeoutRef = useRef<number | null>(null);
    const previousShowRef = useRef(show);

    const clearCloseGrace = useCallback(() => {
        clearToolbarCloseGrace(closeGraceTimeoutRef, setCloseGraceActive);
    }, []);

    useEffect(() => {
        const toolbarInteractionActive = show || toolbarHasFocus || toolbarHovered;

        if (toolbarInteractionActive) {
            clearCloseGrace();
        } else if (previousShowRef.current) {
            startToolbarCloseGrace(closeGraceTimeoutRef, setCloseGraceActive);
        }

        previousShowRef.current = show;
    }, [clearCloseGrace, show, toolbarHasFocus, toolbarHovered]);

    useEffect(
        () => () => {
            if (closeGraceTimeoutRef.current !== null) {
                window.clearTimeout(closeGraceTimeoutRef.current);
            }
        },
        [],
    );

    return { closeGraceActive, clearCloseGrace };
}

type FormattingToolbarStore = {
    setState(open: boolean): void;
};

function useDeduplicatedFormattingToolbarStore(
    store: FormattingToolbarStore,
    show: boolean,
) {
    const openRef = useRef(show);

    useEffect(() => {
        openRef.current = show;
    }, [show]);

    return useCallback(
        (open: boolean) => {
            if (openRef.current === open) return;
            openRef.current = open;
            store.setState(open);
        },
        [store],
    );
}

// ---------------------------------------------------------------------------
// Helpers — schema / selection introspection
// ---------------------------------------------------------------------------

function textAlignmentToPlacement(textAlignment: DefaultProps["textAlignment"]) {
    switch (textAlignment) {
        case "left":
            return "top-start";
        case "center":
            return "top";
        case "right":
            return "top-end";
        default:
            return "top-start";
    }
}

function editorSupportsTextStyle(
    style: TolariaBasicTextStyle,
    editor: TolariaBlockNoteEditor,
) {
    const styleSchema = Reflect.get(editor.schema.styleSchema, style) as
        | { type?: string; propSchema?: unknown }
        | undefined;
    return (
        style in editor.schema.styleSchema &&
        styleSchema?.type === style &&
        styleSchema.propSchema === "boolean"
    );
}

function getSelectedBlocksSafely(
    editor: TolariaBlockNoteEditor,
): TolariaSelectedBlock[] {
    try {
        const selectionBlocks = editor.getSelection()?.blocks;
        if (selectionBlocks?.length) {
            return selectionBlocks as TolariaSelectedBlock[];
        }
    } catch {
        // BlockNote can briefly expose an invalid selection while inline actions remount blocks.
    }

    try {
        return [editor.getTextCursorPosition().block as TolariaSelectedBlock];
    } catch {
        return [];
    }
}

function getCursorBlockSafely(
    editor: TolariaBlockNoteEditor,
): TolariaSelectedBlock | null {
    try {
        return editor.getTextCursorPosition().block as TolariaSelectedBlock;
    } catch {
        return null;
    }
}

function selectionSupportsInlineFormatting(editor: TolariaBlockNoteEditor) {
    return getSelectedBlocksSafely(editor).some(
        (block) => block.content !== undefined,
    );
}

function getBasicTextStyleButtonState(
    basicTextStyle: TolariaBasicTextStyle,
    editor: TolariaBlockNoteEditor,
) {
    if (!editor.isEditable) return undefined;
    if (!editorSupportsTextStyle(basicTextStyle, editor)) return undefined;
    if (!selectionSupportsInlineFormatting(editor)) return undefined;

    return {
        active: basicTextStyle in editor.getActiveStyles(),
    };
}

function isSelectedBlockTypeItem(
    item: TolariaBlockTypeSelectItem,
    firstSelectedBlock: TolariaSelectedBlock,
) {
    if (item.type !== firstSelectedBlock.type) return false;

    return Object.entries(item.props || {}).every(
        ([propName, propValue]) =>
            propValue === Reflect.get(firstSelectedBlock.props, propName),
    );
}

function getTolariaBlockTypeSelectOptions(
    editor: TolariaBlockNoteEditor,
    firstSelectedBlock: TolariaSelectedBlock,
): TolariaBlockTypeSelectOption[] {
    return TOLARIA_BLOCK_TYPE_SELECT_ITEMS.filter((item) =>
        editorHasBlockWithType(
            editor,
            item.type,
            Object.fromEntries(
                Object.entries(item.props || {}).map(([propName, propValue]) => [
                    propName,
                    typeof propValue,
                ]),
            ) as Record<string, "string" | "number" | "boolean">,
        ),
    ).map((item) => ({
        ...item,
        isSelected: isSelectedBlockTypeItem(item, firstSelectedBlock),
    }));
}

function getFormattingToolbarBridgeBlockId(editor: TolariaBlockNoteEditor) {
    const selectedBlock = getSelectedBlocksSafely(editor).at(0);
    if (!selectedBlock) return null;

    return FORMATTING_TOOLBAR_FILE_BLOCK_TYPES.has(selectedBlock.type)
        ? selectedBlock.id
        : null;
}

function getSelectedFileBlockState(
    editor: TolariaBlockNoteEditor,
): TolariaSelectedFileBlock | null {
    const selectedBlocks = getSelectedBlocksSafely(editor);
    if (selectedBlocks.length !== 1) return null;

    const block = selectedBlocks.at(0);
    if (!block) return null;
    if (!FORMATTING_TOOLBAR_FILE_BLOCK_TYPES.has(block.type)) return null;

    const url = (block.props as Record<string, unknown>).url;
    return typeof url === "string" && url.trim().length > 0
        ? { type: block.type, url }
        : null;
}

function fileDownloadTooltip(dict: unknown, blockType: string): string {
    const tooltip = (
        dict as {
            formatting_toolbar?: {
                file_download?: {
                    tooltip?: Record<string, string>;
                };
            };
        }
    ).formatting_toolbar?.file_download?.tooltip;

    return (
        (tooltip
            ? (Reflect.get(tooltip, blockType) as string | undefined)
            : undefined) ??
        tooltip?.file ??
        "Download file"
    );
}

function getFormattingToolbarAnchorElement(editor: TolariaBlockNoteEditor) {
    const anchor = editor.domElement?.firstElementChild;
    return anchor instanceof Element && anchor.isConnected ? anchor : null;
}

/**
 * Walk up from the BlockNote editor element to find the host
 * container that scopes the hover-guard listeners.  The editor-host
 * uses `.editor-host-container`; the React app uses
 * `.editor__blocknote-container`.  Fall back to the editor element
 * itself if neither wrapper is present (defensive).
 */
function resolveHoverGuardContainer(editor: TolariaBlockNoteEditor) {
    const dom = editor.domElement;
    if (!(dom instanceof HTMLElement)) return null;
    const host = dom.closest<HTMLElement>(".editor-host-container");
    return host ?? dom;
}

function updateSelectedBlocksToType(
    editor: TolariaBlockNoteEditor,
    selectedBlocks: TolariaSelectedBlock[],
    item: TolariaBlockTypeSelectItem,
) {
    editor.focus();
    editor.transact(() => {
        for (const block of selectedBlocks) {
            editor.updateBlock(block, {
                type: item.type as never,
                props: item.props as never,
            });
        }
    });
}

// ---------------------------------------------------------------------------
// Components
// ---------------------------------------------------------------------------

function TolariaBasicTextStyleButton({
    basicTextStyle,
}: {
    basicTextStyle: TolariaBasicTextStyle;
}) {
    const Components = useComponentsContext()!;
    const editor = useBlockNoteEditor<
        BlockSchema,
        InlineContentSchema,
        StyleSchema
    >();
    const buttonState = useEditorState({
        editor,
        selector: ({ editor }) => getBasicTextStyleButtonState(basicTextStyle, editor),
    });

    const toggleStyle = useCallback(() => {
        editor.focus();
        editor.toggleStyles({ [basicTextStyle]: true } as never);
    }, [basicTextStyle, editor]);

    if (buttonState === undefined) return null;

    const copy = TOLARIA_BASIC_TEXT_STYLE_TOOLTIPS[basicTextStyle];
    const iconBySytle: Record<TolariaBasicTextStyle, ReactElement> = {
        bold: <BoldIcon />,
        italic: <ItalicIcon />,
        strike: <StrikethroughIcon />,
        code: <CodeIcon />,
    };

    return (
        <Components.FormattingToolbar.Button
            className={`bn-button tolaria-format-${basicTextStyle}`}
            onClick={toggleStyle}
            isSelected={buttonState.active}
            label={copy.label}
            mainTooltip={copy.mainTooltip}
            secondaryTooltip={copy.secondaryTooltip}
            icon={iconBySytle[basicTextStyle]}
        />
    );
}

/**
 * Mantine-free port of the React app's `<TolariaBlockTypeSelect>`.
 * Rebuilds the dropdown trigger with `Components.FormattingToolbar.Button`
 * and the menu rows with `Components.Generic.Menu.*` — the same
 * primitives the 9.3.1 SideMenu port uses for its drag-handle menu
 * (`tolariaBlockNoteSideMenu.tsx::TolariaDragHandleMenu`).
 */
function TolariaBlockTypeSelect() {
    const Components = useComponentsContext()!;
    const editor = useBlockNoteEditor<
        BlockSchema,
        InlineContentSchema,
        StyleSchema
    >();
    const selectedBlocks = useEditorState({
        editor,
        selector: ({ editor }): TolariaSelectedBlock[] =>
            getSelectedBlocksSafely(editor),
    });
    const firstSelectedBlock = selectedBlocks[0] ?? null;
    const selectItems = useMemo(
        () =>
            firstSelectedBlock
                ? getTolariaBlockTypeSelectOptions(editor, firstSelectedBlock)
                : [],
        [editor, firstSelectedBlock],
    );
    const selectedItem = selectItems.find(
        (item): item is TolariaBlockTypeSelectOption => item.isSelected,
    );

    if (!selectedItem || !editor.isEditable) return null;

    return (
        <Components.Generic.Menu.Root position="bottom-start">
            <Components.Generic.Menu.Trigger>
                <Components.FormattingToolbar.Button
                    className="bn-select tolaria-block-type-select"
                    label={selectedItem.name}
                    mainTooltip={selectedItem.name}
                    icon={selectedItem.iconElement}
                >
                    <span className="tolaria-block-type-select-label">
                        {selectedItem.name}
                    </span>
                    <span className="tolaria-block-type-select-caret">
                        <CaretDownIcon />
                    </span>
                </Components.FormattingToolbar.Button>
            </Components.Generic.Menu.Trigger>
            <Components.Generic.Menu.Dropdown className="bn-select">
                {selectItems.map((item) => (
                    <Components.Generic.Menu.Item
                        key={item.name}
                        className="bn-menu-item tolaria-block-type-select-item"
                        icon={item.iconElement}
                        checked={item.isSelected}
                        onClick={() => {
                            updateSelectedBlocksToType(editor, selectedBlocks, item);
                        }}
                    >
                        <span className="tolaria-block-type-select-name">
                            {item.name}
                        </span>
                        {item.isSelected ? (
                            <span className="bn-tick-icon">
                                <CheckIcon />
                            </span>
                        ) : (
                            <span className="bn-tick-space" />
                        )}
                    </Components.Generic.Menu.Item>
                ))}
            </Components.Generic.Menu.Dropdown>
        </Components.Generic.Menu.Root>
    );
}

/**
 * File-block download button.  The React app routes through
 * `openEditorAttachmentOrUrl`, which falls back to Tauri shell IPC
 * for vault-relative attachments.  Editor-host has no attachment
 * bridge yet — this falls back to `window.open(url, '_blank')` so
 * external URLs still open in the system browser.  TODO: wire a
 * `FromHost::OpenAttachment` bridge message and route vault-relative
 * URLs through the host instead.
 */
function TolariaFileDownloadButton() {
    const Components = useComponentsContext()!;
    const dict = useDictionary();
    const editor = useBlockNoteEditor<
        BlockSchema,
        InlineContentSchema,
        StyleSchema
    >();
    const selectedFileBlock = useEditorState({
        editor,
        selector: ({ editor }) => getSelectedFileBlockState(editor),
    });
    const handleOpen = useCallback(() => {
        if (!selectedFileBlock) return;

        editor.focus();
        try {
            window.open(selectedFileBlock.url, "_blank", "noopener,noreferrer");
        } catch (error) {
            console.warn(
                "[editor-host] Failed to open file-block URL:",
                selectedFileBlock.url,
                error,
            );
        }
    }, [editor, selectedFileBlock]);

    if (!selectedFileBlock || !editor.isEditable) return null;

    const label = fileDownloadTooltip(dict, selectedFileBlock.type);
    return (
        <Components.FormattingToolbar.Button
            className="bn-button tolaria-format-file-download"
            onClick={handleOpen}
            isSelected={false}
            label={label}
            mainTooltip={label}
            icon={<ExternalLinkIcon />}
        />
    );
}

// ---------------------------------------------------------------------------
// Toolbar item assembly
// ---------------------------------------------------------------------------

function replaceToolbarControls(items: ReactElement[]): ReactElement[] {
    return items.flatMap<ReactElement>((item) => {
        switch (String(item.key)) {
            case "blockTypeSelect":
                return [<TolariaBlockTypeSelect key={item.key} />];
            case "boldStyleButton":
                return [
                    <TolariaBasicTextStyleButton basicTextStyle="bold" key={item.key} />,
                ];
            case "italicStyleButton":
                return [
                    <TolariaBasicTextStyleButton basicTextStyle="italic" key={item.key} />,
                ];
            case "strikeStyleButton":
                return [
                    <TolariaBasicTextStyleButton basicTextStyle="strike" key={item.key} />,
                ];
            case "fileDownloadButton":
                return [<TolariaFileDownloadButton key={item.key} />];
            default:
                return [item];
        }
    });
}

/**
 * Insert the inline-code button immediately after the strike button.
 * Exposed so the test suite can pin the insertion order.
 */
export function insertInlineCodeButton(items: ReactElement[]): ReactElement[] {
    const strikeButtonIndex = items.findIndex(
        (item) => String(item.key) === "strikeStyleButton",
    );
    if (strikeButtonIndex === -1) return items;

    return [
        ...items.slice(0, strikeButtonIndex + 1),
        <TolariaBasicTextStyleButton basicTextStyle="code" key="codeStyleButton" />,
        ...items.slice(strikeButtonIndex + 1),
    ];
}

function getTolariaFormattingToolbarItems(): ReactElement[] {
    return insertInlineCodeButton(
        replaceToolbarControls(
            filterTolariaFormattingToolbarItems(getFormattingToolbarItems()),
        ),
    );
}

// ---------------------------------------------------------------------------
// Public components
// ---------------------------------------------------------------------------

export function TolariaFormattingToolbar() {
    return <FormattingToolbar>{getTolariaFormattingToolbarItems()}</FormattingToolbar>;
}

export function TolariaFormattingToolbarController(props: {
    formattingToolbar?: FC<FormattingToolbarProps>;
    floatingUIOptions?: FloatingUIOptions;
}) {
    const editor = useBlockNoteEditor<
        BlockSchema,
        InlineContentSchema,
        StyleSchema
    >();
    const formattingToolbar = useExtension(FormattingToolbarExtension, {
        editor,
    });
    const show = useExtensionState(FormattingToolbarExtension, {
        editor,
    });
    const isComposing = useEditorComposing(editor);
    const [toolbarHasFocus, setToolbarHasFocus] = useState(false);
    const [toolbarHovered, setToolbarHovered] = useState(false);
    const { closeGraceActive, clearCloseGrace } = useFormattingToolbarCloseGrace({
        show,
        toolbarHasFocus,
        toolbarHovered,
    });
    const setFormattingToolbarOpen = useDeduplicatedFormattingToolbarStore(
        formattingToolbar.store,
        show,
    );

    const isOpen =
        !isComposing &&
        (show || toolbarHasFocus || toolbarHovered || closeGraceActive);
    const hasFloatingToolbarAnchor =
        getFormattingToolbarAnchorElement(editor) !== null;
    const shouldRenderFloatingToolbar = isOpen && hasFloatingToolbarAnchor;
    const currentBridgeBlockId = useEditorState({
        editor,
        selector: ({ editor }) => getFormattingToolbarBridgeBlockId(editor),
    });

    useBlockNoteFormattingToolbarHoverGuard({
        editor: editor as never,
        container: resolveHoverGuardContainer(editor),
        selectedFileBlockId: currentBridgeBlockId,
        isOpen,
    });

    const position = useEditorState({
        editor,
        selector: ({ editor }) =>
            shouldRenderFloatingToolbar
                ? {
                      from: editor.prosemirrorState.selection.from,
                      to: editor.prosemirrorState.selection.to,
                  }
                : undefined,
    });

    const placement = useEditorState({
        editor,
        selector: ({ editor }) => {
            const block = getCursorBlockSafely(editor);
            if (!block) return "top-start";

            if (
                !blockHasType(block, editor, block.type, {
                    textAlignment: defaultProps.textAlignment,
                })
            ) {
                return "top-start";
            }

            return textAlignmentToPlacement(block.props.textAlignment);
        },
    });

    const floatingUIOptions = useMemo<FloatingUIOptions>(
        () => ({
            ...props.floatingUIOptions,
            useFloatingOptions: {
                open: shouldRenderFloatingToolbar,
                onOpenChange: (open, _event, reason) => {
                    setFormattingToolbarOpen(open);
                    if (!open) {
                        setToolbarHasFocus(false);
                        setToolbarHovered(false);
                        clearCloseGrace();
                    }
                    if (reason === "escape-key") {
                        editor.focus();
                    }
                },
                placement,
                ...props.floatingUIOptions?.useFloatingOptions,
            },
            elementProps: {
                style: {
                    zIndex: 40,
                },
                ...props.floatingUIOptions?.elementProps,
            },
        }),
        [
            clearCloseGrace,
            editor,
            placement,
            props.floatingUIOptions,
            setFormattingToolbarOpen,
            shouldRenderFloatingToolbar,
        ],
    );

    const Component = props.formattingToolbar || TolariaFormattingToolbar;

    return (
        <PositionPopover position={position} {...floatingUIOptions}>
            {shouldRenderFloatingToolbar && (
                <div
                    onPointerEnter={() => {
                        setToolbarHovered(true);
                    }}
                    onPointerLeave={(event) => {
                        if (
                            isFocusStillWithinToolbar(
                                event.currentTarget,
                                event.relatedTarget,
                            )
                        ) {
                            return;
                        }

                        setToolbarHovered(false);
                    }}
                    onFocusCapture={() => {
                        setToolbarHasFocus(true);
                    }}
                    onBlurCapture={(event) => {
                        if (
                            isFocusStillWithinToolbar(
                                event.currentTarget,
                                event.relatedTarget,
                            )
                        ) {
                            return;
                        }

                        setToolbarHasFocus(false);
                        setFormattingToolbarOpen(false);
                    }}
                >
                    <Component />
                </div>
            )}
        </PositionPopover>
    );
}
