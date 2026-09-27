/**
 * TypeScript types for `@etendue/wasm`, generated from the committed JSON Schemas
 * (`schemas/*.schema.json`, emitted by `cargo xtask emit-schemas` from the `etendue-scene`
 * types). The Rust types are the source of truth; edit those, re-emit, and rerun this.
 *
 *     bun scripts/generate-wasm-types.ts           # write crates/etendue-wasm/js/types/
 *     bun scripts/generate-wasm-types.ts --check   # fail if the committed output drifted
 *
 * One file per schema: each is self-contained, so a shared definition (`Iso3Schema`) is
 * declared in more than one. `crates/etendue-wasm/js/index.d.ts` re-exports the public names.
 */

import { readFileSync, writeFileSync, mkdirSync } from "node:fs";
import { dirname, join, resolve } from "node:path";

import { compileFromFile } from "json-schema-to-typescript";
import { format } from "prettier";

const ROOT = resolve(dirname(new URL(import.meta.url).pathname), "../..");
const OUT = join(ROOT, "crates/etendue-wasm/js/types");
const SCHEMAS = ["scene", "scenario", "baked_scenario", "robot_manifest"];

const banner = `/**
 * DO NOT EDIT: generated from schemas/*.schema.json by \`bun run generate:types\` (in web/).
 * The source of truth is the \`etendue-scene\` Rust types.
 */`;

/**
 * json-schema-to-typescript declares a `$ref` target again (`Iso3Schema1`, `Iso3Schema2`, …)
 * wherever the referencing property carries its own `description`. Fold every such copy
 * whose body equals the original's back into the original, keeping the property docs.
 */
function dedupe(ts: string): string {
  const block = /(?:\/\*\*(?:(?!\*\/)[\s\S])*\*\/\n)?export interface (\w+) (\{\n[\s\S]*?\n\})\n/g;
  const bodies = new Map<string, string>();
  for (const [, name, body] of ts.matchAll(block)) bodies.set(name!, body!);
  const renames = new Map<string, string>();
  for (const [name, body] of bodies) {
    const base = /^(\w*?\D)\d+$/.exec(name)?.[1];
    if (base !== undefined && bodies.get(base) === body) renames.set(name, base);
  }
  let out = ts.replace(block, (whole, name: string) => (renames.has(name) ? "" : whole));
  for (const [from, to] of renames) out = out.replaceAll(new RegExp(`\\b${from}\\b`, "g"), to);
  return out;
}

const check = process.argv.includes("--check");
let drifted = 0;
mkdirSync(OUT, { recursive: true });
for (const name of SCHEMAS) {
  const schema = join(ROOT, "schemas", `${name}.schema.json`);
  const ts = await compileFromFile(schema, {
    bannerComment: banner,
    additionalProperties: false,
    declareExternallyReferenced: true,
    enableConstEnums: false,
    format: false,
    cwd: dirname(schema),
  });
  const text = await format(dedupe(ts), { parser: "typescript", printWidth: 100 });
  const out = join(OUT, `${name.replaceAll("_", "-")}.d.ts`);
  if (check) {
    let current = "";
    try {
      current = readFileSync(out, "utf8");
    } catch {
      // missing counts as drift
    }
    if (current !== text) {
      console.error(`drift: ${out} is out of date; run \`bun run generate:types\``);
      drifted++;
    }
  } else {
    writeFileSync(out, text);
    console.log(`wrote ${out}`);
  }
}
if (drifted > 0) process.exit(1);
