// Build the npm package `@etendue/wasm` into crates/etendue-wasm/pkg/.
//
//   node crates/etendue-wasm/scripts/build-npm.mjs [--dev]
//
// 1. `wasm-pack build --target web` (release unless --dev): the raw wasm-bindgen module
//    (`etendue_wasm.js`), which takes documents as JSON text.
// 2. Add the typed layer from js/: `index.js` / `index.d.ts` (documents as objects) and the
//    schema types in `types/` (generated; see web/scripts/generate-wasm-types.ts).
// 3. Write the package manifest.
//
// The same package serves browsers (`await init()`) and Node (`initSync({ module: bytes })`,
// as tests/node/g01_parity.mjs does).

import { execFileSync } from "node:child_process";
import { cpSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const crate = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const pkg = join(crate, "pkg");
const dev = process.argv.includes("--dev");

rmSync(pkg, { recursive: true, force: true });
execFileSync("wasm-pack", ["build", crate, "--target", "web", dev ? "--dev" : "--release", "--out-dir", "pkg"], {
  stdio: "inherit",
});
// wasm-pack's own manifest and ignore file are replaced below.
rmSync(join(pkg, ".gitignore"), { force: true });

cpSync(join(crate, "js"), pkg, { recursive: true });

const cargo = readFileSync(join(crate, "../../Cargo.toml"), "utf8");
const version = /^\[workspace\.package\][^[]*?^version\s*=\s*"([^"]+)"/ms.exec(cargo)?.[1];
if (!version) throw new Error("workspace version not found in Cargo.toml");

const manifest = {
  name: "@etendue/wasm",
  version,
  description: "WebAssembly build of the etendue kernel: scene validation, scenario baking, camera projection.",
  license: "MIT",
  repository: {
    type: "git",
    url: "git+https://github.com/VitalyVorobyev/etendue.git",
    directory: "crates/etendue-wasm",
  },
  type: "module",
  sideEffects: ["./etendue_wasm.js"],
  files: [
    "index.js",
    "index.d.ts",
    "types",
    "etendue_wasm.js",
    "etendue_wasm.d.ts",
    "etendue_wasm_bg.wasm",
    "etendue_wasm_bg.wasm.d.ts",
  ],
  exports: {
    ".": { types: "./index.d.ts", default: "./index.js" },
    "./raw": { types: "./etendue_wasm.d.ts", default: "./etendue_wasm.js" },
    "./etendue_wasm_bg.wasm": "./etendue_wasm_bg.wasm",
    "./package.json": "./package.json",
  },
  publishConfig: { access: "public" },
};
writeFileSync(join(pkg, "package.json"), JSON.stringify(manifest, null, 2) + "\n");
console.log(`@etendue/wasm ${version} → ${pkg}`);
