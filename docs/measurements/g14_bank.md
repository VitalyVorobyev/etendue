# G14.1, G14.2: the component bank

Gates of P14 (`docs/pivot/PLAN.md`, ADR 0010). Measured 2026-11-02, Apple M4 Pro, etendue commit
`669700b` (branch `p14d/bank-gates`), calibration-rs `v0.12.0` (a clean clone of the tag).

Each gate has two sides, both run in CI:
- **The CLI's side:** `crates/etendue-cli/tests/project_check.rs`, `g14_*`.
- **The studio's side:** `web/apps/studio/src/io/g14.test.ts`. It runs the studio's own save and
  copy functions on the real kernel (the wasm checks and hashes every item; nothing is mocked),
  against files on disk.

## G14.1: every built-in item round-trips byte-identically: PASS

- **`etendue library check examples`** passes on all 24 built-in items, of 8 kinds.
- **The studio's save path** reads every built-in through the kernel and writes it with
  `saveLibraryItem`. Every file comes out byte for byte the same.

**Finding.** Before this change, 4 of the 24 did not round-trip: the lenses `lm16hc`, `lm25hc`,
`m5018_mp2` and the camera `imx174_lm25hc`.
- **Why.** The kernel writes a lens's defaulted fields: `principal_gap_m: 0`,
  `entrance_pupil_m: 0` and `focusing: {"type": "unit"}`. The hand-written files left them out.
- **Fix.** These four files are now written in that canonical form.
- **Effect on hashes.** Their content hashes do not change, because the hash is over the parsed
  item, so no project copy goes stale.

## G14.2: a project that uses bank items passes `project check` without the bank: PASS

**CLI**
1. A bank (`library init`) gets three built-ins (`library import`).
2. A project holds copies of them, written as the studio's `copyOf` writes them: the item, version
   2, and `origin` with the source's content hash. Its defaults name them.
3. `project check --bank` gives no notes.
4. Once the bank folder is removed:
   - `project check` passes;
   - `project check --bank` passes too, with one note per copy ("the bank no longer has …").
     These are notes, not problems.

**Studio**
1. A bank on disk gets two items through `saveToBank`.
2. A project made by `createProject` takes them through `useInProject` (layered with
   `withLayers`), and its defaults name them.
3. Once the bank folder is removed, `loadProject` reads the project with no problems. Each copy is
   its bank item plus `origin` (the same fields, values and id), and it hashes to its
   `origin.sha256`. Every default resolves to the project's own item.

## Procedure

```bash
cargo test -p etendue-cli --test project_check g14 -- --nocapture
cd web/apps/studio && bun x vitest run --project unit src/io/g14.test.ts   # after build-npm.mjs
```
