import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { fileURLToPath } from "node:url";

import { defineConfig, searchForWorkspaceRoot } from "vite";

import { etendueRepo } from "./etendueRepo";

export default defineConfig({
  plugins: [react(), tailwindcss(), etendueRepo()],
  // The @vitavision packages incubating in web/packages resolve to their sources.
  resolve: { conditions: ["@vitavision/source"] },
  // wasm-bindgen's `new URL("…_bg.wasm", import.meta.url)` must not be pre-bundled away.
  optimizeDeps: { exclude: ["@etendue/wasm"] },
  server: {
    port: 5178,
    strictPort: true,
    // @etendue/wasm is a workspace member outside web/ (crates/etendue-wasm/pkg).
    fs: {
      allow: [
        searchForWorkspaceRoot(process.cwd()),
        fileURLToPath(new URL("../../../crates/etendue-wasm/pkg", import.meta.url)),
      ],
    },
  },
  preview: { port: 5179, strictPort: true },
});
