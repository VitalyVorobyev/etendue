/**
 * Layering rules of the web packages (docs/pivot/PLAN.md §2; lab-ui PLAN §2), run in CI:
 *
 *     bun scripts/check-deps.ts
 *
 * Per package in packages/*:
 *   - runtime `dependencies` come from that package's allow-list;
 *   - `react` / `react-dom` / `three` / `@react-three/fiber` are peers, never dependencies;
 *   - every bare import in `src/` is a declared dependency or peer, and none is on the
 *     package's forbidden list (`@vitavision/three` never imports React; the 3D packages
 *     never import `@etendue/wasm` — kernel results are passed in);
 *   - the manifest is ESM-only, ships `exports` with `types`, and limits `sideEffects` to CSS.
 * The studio app may import only what it declares.
 */

import { readdirSync, readFileSync, statSync } from "node:fs";
import { dirname, join, resolve } from "node:path";

const ROOT = resolve(dirname(new URL(import.meta.url).pathname), "..");

interface Rule {
  /** Allowed runtime dependencies. */
  deps: (string | RegExp)[];
  /** Imports that must never appear in `src/`. */
  forbidden: (string | RegExp)[];
}

const REACT = [/^react($|\/)/, /^react-dom($|\/)/, /^@react-three\//];
const KERNEL = ["@etendue/wasm"];

const RULES: Record<string, Rule> = {
  "@vitavision/three": { deps: [], forbidden: [...REACT, ...KERNEL, /^@vitavision\//] },
  "@vitavision/three-react": { deps: ["@vitavision/three"], forbidden: [...KERNEL, /^@vitavision\/(?!three$)/] },
  "@vitavision/workbench": { deps: ["@vitavision/ui", "lucide-react"], forbidden: [/^three($|\/)/, /^@react-three\//, ...KERNEL] },
  "@vitavision/ui-next": { deps: ["@vitavision/ui"], forbidden: [/^three($|\/)/, /^@react-three\//, ...KERNEL] },
};
const PEERS_ONLY = ["react", "react-dom", "three", "@react-three/fiber"];

interface Manifest {
  name: string;
  type?: string;
  sideEffects?: boolean | string[];
  exports?: Record<string, unknown>;
  dependencies?: Record<string, string>;
  peerDependencies?: Record<string, string>;
  devDependencies?: Record<string, string>;
}

function* sources(dir: string, withTests = false): Generator<string> {
  for (const entry of readdirSync(dir)) {
    const path = join(dir, entry);
    if (statSync(path).isDirectory()) yield* sources(path, withTests);
    else if (/\.(ts|tsx)$/.test(entry) && (withTests || !/\.(test|stories)\.tsx?$/.test(entry))) yield path;
  }
}

/** Bare module specifiers imported by a source file, reduced to package names. */
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
const matches = (name: string, list: (string | RegExp)[]) => list.some((p) => (typeof p === "string" ? p === name : p.test(name)));

const errors: string[] = [];
const dirs = [
  ...readdirSync(join(ROOT, "packages")).map((d) => join(ROOT, "packages", d)),
  ...readdirSync(join(ROOT, "apps")).map((d) => join(ROOT, "apps", d)),
];
for (const dir of dirs) {
  const m = JSON.parse(readFileSync(join(dir, "package.json"), "utf8")) as Manifest;
  const declared = new Set([...Object.keys(m.dependencies ?? {}), ...Object.keys(m.peerDependencies ?? {})]);
  const rule = RULES[m.name];
  const isApp = dir.includes(`${join(ROOT, "apps")}/`);

  if (!isApp) {
    if (!rule) {
      errors.push(`${m.name}: no layering rule; add it to scripts/check-deps.ts`);
      continue;
    }
    if (m.type !== "module") errors.push(`${m.name}: must be "type": "module"`);
    const root = m.exports?.["."] as Record<string, string> | undefined;
    if (!root?.types) errors.push(`${m.name}: exports["."] needs "types"`);
    if (Array.isArray(m.sideEffects) ? m.sideEffects.some((s) => !s.endsWith(".css")) : m.sideEffects !== false) {
      errors.push(`${m.name}: sideEffects must be false or CSS only`);
    }
    for (const dep of Object.keys(m.dependencies ?? {})) {
      if (PEERS_ONLY.includes(dep)) errors.push(`${m.name}: ${dep} must be a peer dependency`);
      else if (!matches(dep, rule.deps)) errors.push(`${m.name}: dependency ${dep} is not allowed at this layer`);
    }
  }

  for (const file of sources(join(dir, "src"))) {
    for (const spec of imports(file)) {
      const pkg = packageOf(spec);
      const where = `${m.name}: ${file.slice(ROOT.length + 1)}`;
      if (rule && matches(spec, rule.forbidden)) errors.push(`${where} imports ${spec}, forbidden at this layer`);
      if (!declared.has(pkg)) errors.push(`${where} imports ${spec}, but ${pkg} is not a dependency or peer`);
    }
  }
}

if (errors.length > 0) {
  console.error(`layering violations:\n  ${errors.join("\n  ")}`);
  process.exit(1);
}
console.log(`layering ok (${dirs.length} packages)`);
