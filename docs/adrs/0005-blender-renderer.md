# ADR 0005: Blender Is a Thin Renderer

- Status: Accepted
- Date: 2026-09-26 (accepted 2026-09-26)

## Context

The photometric tier needs physically based light transport: materials,
lights, occlusion, and a laser sheet on real surfaces. [ADR 0001](0001-pivot.md)
names Blender/Cycles as the only realistic renderer.

Blender ships its own Python. The calibration-rs Python wheels
(`vision-calibration-py`, abi3-py310) could in principle be imported inside
Blender, but that couples etendue to Blender's bundled Python version. It
would also make Blender a second place where calibration math runs.

## Decision

### A job file is the whole interface

`etendue render --backend blender` (in `etendue-cli`) writes a versioned
`job.json` with the following contents. Its schema is emitted like every
other `etendue-scene` type.

- The capture samples of a `BakedScenario` ([ADR 0003](0003-baking.md)), with
  `world_se3_frame` for every mesh-, light- and camera-bearing frame.
- Meshes, as references to per-link `.glb` robot assets and to target and part
  meshes.
- Materials from a small PBR subset: base colour or texture, roughness,
  metallic, emission. P4-1 fixes the exact subset.
- Lights.
- The canonical camera from [ADR 0004](0004-canonical-render-camera.md):
  `lens` mm, `sensor_width` mm, resolution, clip range, and DOF settings for
  untilted cameras.
- Render settings: sample count, seed, device.

### Invocation

```text
$ETENDUE_BLENDER -b --factory-startup --python-exit-code 1 \
    --python <embedded script> -- job.json out/
```

- The script is embedded in the `etendue-cli` binary and written to a
  temporary file at run time.
- `--factory-startup` isolates the run from user preferences and add-ons.
- `--python-exit-code 1` turns script exceptions into a non-zero exit.

### The script is thin

- It imports only `bpy` and the Python standard library. It uses no PyO3
  wheels and no NumPy dependency beyond what Blender bundles.
- It applies transforms. It computes no FK, no camera model, no remap, no
  noise, and no ground truth.
- The CV↔Blender camera relation is a fixed `Rx(π)`. Blender's camera looks
  down −Z with +Y up, and the world is natively Z-up. That relation lives in
  exactly one function with a round-trip test.

### Render settings

- Cycles on the Metal GPU by default, with a fixed seed.
- **Standard** view transform (Filmic/AgX off).
- Denoising off.
- If P4-5 finds Metal non-deterministic, ground-truth-critical renders use
  whichever device is deterministic, or the variance bound is documented.

### Outputs and post-processing

- For each capture, the script writes **linear float EXR** radiance plus depth
  (Z) and object-index passes.
- Everything after the renderer happens in Rust (`etendue-synth`, reading EXR
  through the `exr` crate): the LUT remap, exposure, shot / read / PRNU noise,
  and quantisation.

### Version pin

- The Blender version is pinned in `etendue.toml` (decision D3: the version
  installed when P4-1 lands).
- It is checked at startup. A mismatch is a hard error unless explicitly
  overridden.

### Where it runs

Blender runs **locally only**, never in CI. Gates G4.x and G5.x run via
`cargo xtask measure <gate>` and write to `docs/measurements/`.

## Consequences

- Blender is needed only for the photometric tier. Every other package builds
  and tests without it.
- There is no Python ABI coupling. A Blender upgrade is a deliberate pin bump,
  re-validated by the G4.x gates.
- `job.json` is a contract between two languages, so it is versioned and
  schema-checked.
- Photometric Scheimpflug blur is wrong, as documented in ADR 0004.

## Alternatives considered

- **`bpy` as a pip module inside a regular Python process.** Rejected: it ties
  the Python version to Blender's and adds a heavyweight dependency to the
  toolchain.
- **Calling calibration-rs from inside Blender via the Python wheels.**
  Rejected because of ABI coupling (F7) and a second site for camera math.
- **Mitsuba 3.** Deferred. It is a possible later backend behind the same
  `job.json`.
- **An etendue-owned path tracer.** Rejected in ADR 0001.

## References

- `docs/pivot/PLAN.md` §4 (ADR-0005), P4-1 to P4-6, §6 (Blender jobs local-only)
- [ADR 0004](0004-canonical-render-camera.md), [ADR 0006](0006-ground-truth.md)
