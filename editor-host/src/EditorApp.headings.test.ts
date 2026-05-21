import { describe, expect, it } from "vitest";
import { extractHeadings, type BlockNoteHeadingBlock } from "./EditorApp.tsx";

// ---------------------------------------------------------------------------
// Phase 9 worklist 9.2.6 — heading extraction
// ---------------------------------------------------------------------------
//
// `extractHeadings` is the pure-logic helper that walks a BlockNote
// document and produces the `Heading[]` payload the native ToC panel
// consumes through `editor_bridge::FromHost::Headings`.  Testing it
// directly (instead of through a mounted React tree) keeps the
// coverage tight on the shape conversion without needing a live
// BlockNoteEditor.

function h(level: number, text: string, id?: string): BlockNoteHeadingBlock {
    return {
        type: "heading",
        id,
        props: { level },
        content: [{ type: "text", text }],
    };
}

describe("extractHeadings", () => {
    it("returns an empty array for a document with no headings", () => {
        const blocks: BlockNoteHeadingBlock[] = [
            { type: "paragraph", content: [{ type: "text", text: "body" }] },
        ];
        expect(extractHeadings(blocks)).toEqual([]);
    });

    it("extracts ordered headings with level + text + anchor", () => {
        const blocks: BlockNoteHeadingBlock[] = [
            h(1, "Top", "block-1"),
            { type: "paragraph", content: [{ type: "text", text: "intro" }] },
            h(2, "Sub", "block-2"),
            h(3, "Deep", "block-3"),
        ];
        expect(extractHeadings(blocks)).toEqual([
            { level: 1, text: "Top", anchor: "block-1" },
            { level: 2, text: "Sub", anchor: "block-2" },
            { level: 3, text: "Deep", anchor: "block-3" },
        ]);
    });

    it("falls back to a slug anchor when the block has no id", () => {
        // When BlockNote hasn't assigned a block id yet (e.g. during
        // the initial markdown parse), the anchor falls back to a
        // text-derived slug.  The native side treats the anchor as
        // opaque so the exact format only needs to stay stable.
        const blocks: BlockNoteHeadingBlock[] = [h(1, "Hello World")];
        const result = extractHeadings(blocks);
        expect(result).toHaveLength(1);
        expect(result[0]?.text).toBe("Hello World");
        expect(result[0]?.anchor).toBe("hello-world");
    });

    it("skips heading blocks with empty text", () => {
        // An empty heading block in BlockNote is the user typing `#`
        // with no content yet.  Pushing that into the panel would
        // make the row blank — skip it until the user types something.
        const blocks: BlockNoteHeadingBlock[] = [
            h(1, "", "block-1"),
            h(2, "  ", "block-2"),
            h(3, "Real", "block-3"),
        ];
        expect(extractHeadings(blocks)).toEqual([
            { level: 3, text: "Real", anchor: "block-3" },
        ]);
    });

    it("clamps to BlockNote's 1..=3 level range", () => {
        // BlockNote only ships H1/H2/H3 today; a level outside that
        // range is either a wire-bogus value or a future BlockNote
        // version.  Drop it rather than render an unsanitised level
        // that the native panel would clamp anyway.
        const blocks: BlockNoteHeadingBlock[] = [
            { type: "heading", id: "x", props: { level: 0 }, content: [{ type: "text", text: "Zero" }] },
            { type: "heading", id: "y", props: { level: 4 }, content: [{ type: "text", text: "Four" }] },
            h(2, "Two", "z"),
        ];
        expect(extractHeadings(blocks)).toEqual([
            { level: 2, text: "Two", anchor: "z" },
        ]);
    });

    it("ignores heading blocks with no level prop", () => {
        const blocks: BlockNoteHeadingBlock[] = [
            { type: "heading", id: "x", content: [{ type: "text", text: "Missing" }] },
            h(1, "Has level", "y"),
        ];
        expect(extractHeadings(blocks)).toEqual([
            { level: 1, text: "Has level", anchor: "y" },
        ]);
    });

    it("joins multi-fragment heading content into a single trimmed text", () => {
        // Inline marks (`**bold**`, `*italic*`) parse into multiple
        // text fragments under one heading block.  The ToC must show
        // the visible string, not the fragments.
        const block: BlockNoteHeadingBlock = {
            type: "heading",
            id: "block-1",
            props: { level: 1 },
            content: [
                { type: "text", text: "Hello " },
                { type: "text", text: "Bold" },
                { type: "text", text: " World" },
            ],
        };
        expect(extractHeadings([block])).toEqual([
            { level: 1, text: "Hello Bold World", anchor: "block-1" },
        ]);
    });
});
