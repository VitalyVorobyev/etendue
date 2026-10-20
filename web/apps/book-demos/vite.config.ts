import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

import { type Plugin, defineConfig } from "vite";

const repo = (path: string) => fileURLToPath(new URL(`../../../${path}`, import.meta.url));

/**
 * The documents the widgets load at run time, copied from the repository next to `demos.js`:
 * the UR5e's description (no meshes: the widgets draw link frames) and two example cells.
 */
const DATA: Record<string, string> = {
  "data/ur5e/robot.json": "assets/robots/ur5e/robot.json",
  "data/ur5e/robot.urdf": "assets/robots/ur5e/robot.urdf",
  "data/closed_loop_ur5e/scene.json": "examples/closed_loop_ur5e/scene.json",
  "data/closed_loop_ur5e/scenario.json": "examples/closed_loop_ur5e/scenario.json",
};

function bookData(): Plugin {
  return {
    name: "etendue-book-data",
    generateBundle() {
      for (const [fileName, from] of Object.entries(DATA)) {
        this.emitFile({ type: "asset", fileName, source: readFileSync(repo(from)) });
      }
    },
  };
}

// One ES module, `demos.js`, beside the wasm kernel and the data, in book/src/demos: mdBook copies
// it into the book, and `book/js/demos-loader.js` imports it on pages that have a widget. A
// relative base resolves the wasm and data against `import.meta.url`, at any page depth.
export default defineConfig({
  base: "./",
  plugins: [bookData()],
  build: {
    outDir: repo("book/src/demos"),
    emptyOutDir: true,
    target: "es2022",
    assetsInlineLimit: 0,
    rollupOptions: {
      input: "src/main.ts",
      preserveEntrySignatures: "exports-only",
      output: {
        format: "es",
        entryFileNames: "demos.js",
        chunkFileNames: "[name]-[hash].js",
        assetFileNames: "[name]-[hash][extname]",
      },
    },
  },
});
