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

const ROOT = resolve(dirname(new URL(import.meta.url).pathname), "../..");
const OUT = join(ROOT, "crates/etendue-wasm/js/types");
const SCHEMAS = [
  "scene",
  "scenario",
  "baked_scenario",
  "robot_manifest",
  "motion_report",
  "captures",
  "library",
  "project",
];

const banner = `/**
 * DO NOT EDIT: generated from schemas/*.schema.json by \`bun run generate:types\` (in web/).
 * The source of truth is the \`etendue-scene\` Rust types.
 */`;

/** The index just past the `}` that closes the `{` at `open`, skipping comments and strings. */
function closing(ts: string, open: number): number {
  let depth = 0;
  for (let i = open; i < ts.length; i++) {
    const c = ts[i]!;
    if (c === "/" && ts[i + 1] === "*") {
      i = ts.indexOf("*/", i + 2) + 1;
    } else if (c === "/" && ts[i + 1] === "/") {
      i = ts.indexOf("\n", i);
    } else if (c === '"' || c === "'" || c === "`") {
      for (i++; ts[i] !== c; i++) if (ts[i] === "\\") i++;
    } else if (c === "{") {
      depth++;
    } else if (c === "}" && --depth === 0) {
      return i + 1;
    }
  }
  throw new Error("unbalanced braces in generated types");
}

/** Every top-level `export interface`: its name, its extent (with its doc comment), and its body. */
function interfaces(ts: string): { name: string; start: number; end: number; body: string }[] {
  const out = [];
  const head = /(?:\/\*\*(?:(?!\*\/)[\s\S])*\*\/\n)?export interface (\w+) \{/g;
  for (const m of ts.matchAll(head)) {
    const open = m.index + m[0].length - 1;
    const end = closing(ts, open);
    out.push({ name: m[1]!, start: m.index, end: ts[end] === "\n" ? end + 1 : end, body: ts.slice(open, end) });
  }
  return out;
}

/**
 * json-schema-to-typescript declares a `$ref` target again (`Iso3Schema1`, `Iso3Schema2`, …)
 * wherever the referencing property carries its own `description`. Fold every such copy
 * whose body equals the original's back into the original, keeping the property docs.
 */
function dedupe(ts: string): string {
  const found = interfaces(ts);
  const bodies = new Map(found.map((i) => [i.name, i.body]));
  const renames = new Map<string, string>();
  for (const { name, body } of found) {
    const base = /^(\w*?\D)\d+$/.exec(name)?.[1];
    if (base !== undefined && bodies.get(base) === body) renames.set(name, base);
  }
  let out = ts;
  for (const { name, start, end } of [...found].reverse()) {
    if (renames.has(name)) out = out.slice(0, start) + out.slice(end);
  }
  for (const [from, to] of renames) out = out.replaceAll(new RegExp(`\\b${from}\\b`, "g"), to);
  return out;
}

/** `ts` as oxfmt formats it under the repository's `.oxfmtrc.json` (what `bun run format` keeps). */
function format(ts: string, path: string): string {
  const oxfmt = Bun.which("oxfmt", { PATH: join(ROOT, "web/node_modules/.bin") });
  if (oxfmt === null) throw new Error("oxfmt not found: run `bun install` in web/");
  const run = Bun.spawnSync([oxfmt, `--stdin-filepath=${path}`], {
    cwd: ROOT,
    stdin: Buffer.from(ts),
  });
  if (run.exitCode !== 0) throw new Error(`oxfmt ${path}: ${run.stderr.toString()}`);
  return run.stdout.toString();
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
  const out = join(OUT, `${name.replaceAll("_", "-")}.d.ts`);
  const text = format(dedupe(ts), out);
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
