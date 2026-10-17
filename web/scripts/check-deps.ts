/**
 * Dependency hygiene of the web apps, run in CI:
 *
 *     bun scripts/check-deps.ts
 *
 * Every bare import in an app's `src/` is a declared dependency or peer of that app. The
 * `@vitavision/*` packages come from npm (published from lab-ui), which enforces their own
 * layering.
 */

import { readdirSync, readFileSync, statSync } from "node:fs";
import { dirname, join, resolve } from "node:path";

const ROOT = resolve(dirname(new URL(import.meta.url).pathname), "..");

interface Manifest {
  name: string;
  dependencies?: Record<string, string>;
  peerDependencies?: Record<string, string>;
}

function* sources(dir: string): Generator<string> {
  for (const entry of readdirSync(dir)) {
    const path = join(dir, entry);
    if (statSync(path).isDirectory()) yield* sources(path);
    else if (/\.(ts|tsx)$/.test(entry) && !/\.(test|stories)\.tsx?$/.test(entry)) yield path;
  }
}

/** Bare module specifiers imported by a source file. */
function imports(file: string): string[] {
  const text = readFileSync(file, "utf8");
  const out: string[] = [];
  for (const m of text.matchAll(/(?:^|\n)\s*(?:import|export)\s[^"'`]*?from\s+["']([^"']+)["']|import\(\s*["']([^"']+)["']\s*\)/g)) {
    const spec = m[1] ?? m[2]!;
    if (spec.startsWith(".") || spec.startsWith("node:") || spec.startsWith("virtual:")) continue;
    out.push(spec);
  }
  return out;
}

const packageOf = (spec: string) => (spec.startsWith("@") ? spec.split("/").slice(0, 2).join("/") : spec.split("/")[0]!);

const errors: string[] = [];
const dirs = readdirSync(join(ROOT, "apps"))
  .map((d) => join(ROOT, "apps", d))
  .filter((d) => statSync(d).isDirectory());
for (const dir of dirs) {
  const m = JSON.parse(readFileSync(join(dir, "package.json"), "utf8")) as Manifest;
  const declared = new Set([...Object.keys(m.dependencies ?? {}), ...Object.keys(m.peerDependencies ?? {})]);
  for (const file of sources(join(dir, "src"))) {
    for (const spec of imports(file)) {
      const pkg = packageOf(spec);
      if (!declared.has(pkg)) {
        errors.push(`${m.name}: ${file.slice(ROOT.length + 1)} imports ${spec}, but ${pkg} is not a dependency or peer`);
      }
    }
  }
}

if (errors.length > 0) {
  console.error(`dependency violations:\n  ${errors.join("\n  ")}`);
  process.exit(1);
}
console.log(`dependencies ok (${dirs.length} app${dirs.length === 1 ? "" : "s"})`);
