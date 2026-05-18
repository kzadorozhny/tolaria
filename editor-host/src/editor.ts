import { send, type ToHost, type Mods } from "./bridge.ts";

/**
 * MVP editor: a single `<textarea>` driven by `ToHost` messages and
 * emitting `FromHost::Save` / `Dirty` over the bridge.
 *
 * Phase 4-MVP+ swaps the textarea for the BlockNote + CodeMirror
 * carry-over from `src/`; the bridge contract stays unchanged.
 */
export class Editor {
    private readonly textarea: HTMLTextAreaElement;
    private currentId: number | null = null;
    /** Body as of the last note_open or save — used to compute dirty. */
    private cleanBody: string = "";
    /** Whether we have already announced dirty for `currentId`. */
    private dirtyAnnounced: boolean = false;

    constructor(root: HTMLElement) {
        this.textarea = document.createElement("textarea");
        this.textarea.className = "editor-textarea";
        this.textarea.spellcheck = false;
        this.textarea.autocapitalize = "off";
        this.textarea.autocomplete = "off";
        this.textarea.placeholder = "Select a note from the sidebar to begin editing.";
        this.textarea.disabled = true;
        root.appendChild(this.textarea);

        this.textarea.addEventListener("input", () => this.onInput());
        this.textarea.addEventListener("keydown", (e) => this.onKeydown(e));
    }

    handle(msg: ToHost): void {
        switch (msg.k) {
            case "note_open": {
                this.currentId = msg.v.id;
                this.cleanBody = msg.v.body;
                this.dirtyAnnounced = false;
                this.textarea.value = msg.v.body;
                this.textarea.disabled = false;
                this.textarea.focus();
                break;
            }
            case "focus_editor": {
                this.textarea.focus();
                break;
            }
            case "save_request": {
                this.flushSave();
                break;
            }
            case "theme_set": {
                document.documentElement.dataset.theme = msg.v.mode;
                break;
            }
        }
    }

    private onInput(): void {
        if (this.currentId === null) return;
        if (this.textarea.value === this.cleanBody) {
            // Edit reverted to clean; we don't reset dirtyAnnounced —
            // the native shell debounces its own dirty state, and a
            // stuck dirty dot is less bad than message spam.
            return;
        }
        if (!this.dirtyAnnounced) {
            this.dirtyAnnounced = true;
            send({ k: "dirty", v: { id: this.currentId } });
        }
    }

    private onKeydown(e: KeyboardEvent): void {
        // Pass-through to native action registry so Cmd+S inside the
        // WKWebView reaches the `Save` action (ADR-0115 §6 trigger #4).
        const mods: Mods = {};
        if (e.altKey) mods.alt = true;
        if (e.ctrlKey) mods.ctrl = true;
        if (e.metaKey) mods.meta = true;
        if (e.shiftKey) mods.shift = true;

        // Only relay shortcut-shaped events; raw typing flooding IPC
        // would be wasteful and the native side has no use for it.
        if (e.altKey || e.ctrlKey || e.metaKey) {
            send({ k: "keydown", v: { key: e.key, mods } });
        }

        // Local handling of Cmd+S — also send save now so user gets
        // immediate persistence even if the action-registry round-trip
        // somehow drops the message.
        if (e.metaKey && (e.key === "s" || e.key === "S")) {
            e.preventDefault();
            this.flushSave();
        }
    }

    private flushSave(): void {
        if (this.currentId === null) return;
        const id = this.currentId;
        const body = this.textarea.value;
        if (body === this.cleanBody) {
            send({ k: "saved", v: { id } });
            return;
        }
        this.cleanBody = body;
        this.dirtyAnnounced = false;
        send({ k: "save", v: { id, body } });
    }
}
