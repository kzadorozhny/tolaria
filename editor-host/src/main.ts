import { Editor } from "./editor.ts";
import { onReceive, send } from "./bridge.ts";
import "./style.css";

const root = document.getElementById("editor-root");
if (!root) throw new Error("missing #editor-root in index.html");

const editor = new Editor(root);
onReceive((msg) => editor.handle(msg));

// Announce ready *after* the bridge is installed so the native side
// can immediately send a NoteOpen without racing the handler.
send({ k: "ready" });
