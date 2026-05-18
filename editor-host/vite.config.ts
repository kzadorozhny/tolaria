import { defineConfig } from "vite";
import { viteSingleFile } from "vite-plugin-singlefile";

// MVP build: a single self-contained `dist/index.html` that the
// `note_item` crate loads via `wry::WebViewBuilder::with_html`.  All
// JS + CSS is inlined into one file; no separate fetch traffic, no
// asset-server complexity.
//
// `vite-plugin-singlefile` folds JS modules into a `<script>` block
// and CSS into a `<style>` block, producing a true single-file
// payload that `include_str!()` can embed at the Rust call site.
export default defineConfig({
    base: "./",
    plugins: [viteSingleFile()],
    build: {
        outDir: "dist",
        emptyOutDir: true,
        assetsInlineLimit: 100_000_000,
        cssCodeSplit: false,
        rollupOptions: {
            output: {
                inlineDynamicImports: true,
            },
        },
    },
});
