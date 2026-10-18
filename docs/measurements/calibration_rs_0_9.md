# calibration-rs 0.9.0: gate re-runs

- Change: `vision-calibration-{core,dataset}` 0.8.3 → **0.9.0** (crates.io), the
  `vision-calibration` wheel 0.8.3 → 0.9.0 (PyPI, `tools/closed-loop`), and CI's
  `CALIBRATION_RS_REF` → `v0.9.0`. 0.9.0 changes nothing etendue calls: the dataset crate adds
  `ChessCornersDetectorSpec::strategy` (etendue does not build detector specs), and core's
  changes are documentation. The schemas are unchanged (`cargo xtask emit-schemas --check`).
- Measured: 2026-10-17, on etendue commit `e989eed` (branch `deps/calibration-rs-0.9`), against
  calibration-rs `v0.9.0` (`5a862a24`) exactly: a `git archive` of the tag, not the sibling
  checkout's `main`, which is ahead of the tag.
- Result: **every re-run gate gives the same result as on 0.8.3.**

| Gate | Record | 0.9.0 |
|---|---|---|
| Workspace tests | — | 299 passed; per crate with all features: core 152, scene 21, kinematics 20 (G1.1, G1.2), synth 38 |
| G0.1 wasm parity | `g0_1_wasm_parity.md` | **PASS**, 2920 values, 0 bit mismatches |
| G2.1 wasm size | `g2_1_wasm_size.md` | **PASS**, 319 111 B gzip -9 (≤ 1 572 864) |
| G3.1 remap | `g3_1_remap.md` | **PASS** for every camera; max error ≤ 6.4e-13 px |
| G5.1 closed loop | `closed_loop.md` | **PASS** on `closed_loop_ur5e` (rig and `--camera cam_left`) and `eye_in_hand_ur5e` |
| G5.3 laser closed loop | `g5_3_laser_closed_loop.md` | **PASS** on `laser_ur5e` (normal 1.32e-7° / 1.21e-7°); Scheimpflug characterisation unchanged (0.0080° / 0.0102°) |
| Studio shell | — | clippy clean, 7 tests passed |

Not re-run, because they need the pinned Blender 5.1.1, which was not installed (5.2.2 was):
- G4.1, G4.2, G4.3 and P4-5. 0.9.0 does not touch their code paths: projection is covered by
  G0.1 and G3.1, and detection does not use calibration-rs.
- **P5-1b, the image-level loop: not verified on 0.9.0.** Its render and detect steps do not use
  calibration-rs, but its calibrate step (`closed_loop.py --features`) runs the wheel, which 0.9.0
  changes (the release adds Radon corners and the whole-board chessboard runner). It is re-run
  with the next Blender session.
- **Re-run 2026-10-18 on Blender 5.2.2** (the pin moved to the 5.2 series): G4.1, G4.3 pass, G4.2
  and P4-5 unchanged, and P5-1b runs on 0.9.0 (still noise-limited; its mean reprojection error
  0.0005 px above the white-noise max). See each file's 2026-10-18 entry and
  `closed_loop_images.md`.

## Procedure

```bash
git -C ../calibration-rs archive v0.9.0 | tar -x -C <tmp>/cal090   # patch [patch.crates-io] to it
cargo test --workspace --locked
cargo test -p <crate> --all-features --locked                    # core, scene, kinematics, synth
node crates/etendue-wasm/scripts/build-npm.mjs
node crates/etendue-wasm/tests/node/g01_parity.mjs > target/g01.json
cargo run -p etendue-wasm --example g01_verify -- target/g01.json
gzip -9c crates/etendue-wasm/pkg/etendue_wasm_bg.wasm | wc -c
cargo run --release -p etendue-synth --example g3_1_remap
for ex in closed_loop_ur5e eye_in_hand_ur5e laser_ur5e laser_scheimpflug_ur5e; do
  cargo run -p etendue-cli -- gt examples/$ex/scene.json examples/$ex/scenario.json -o target/cl_$ex
  uv run --locked --project tools/closed-loop tools/closed-loop/closed_loop.py target/cl_$ex
done
```
