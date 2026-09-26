# Architecture Decision Records

Format follows calibration-rs: `# ADR NNNN: Title`, then `- Status:` and `- Date:`, then
Context / Decision / Consequences / Alternatives considered / References.

Status legend:
- **Proposed**: written, awaiting user review.
- **Accepted**: approved.
- **Superseded**: replaced by a later ADR, which is named in the record.

| ADR | Title | Status |
|---|---|---|
| [0001](0001-pivot.md) | Pivot to a family of independently published packages | Accepted |
| [0002](0002-frame-tree.md) | Frame tree | Accepted |
| [0003](0003-baking.md) | Kinematics runs once, in Rust — baked scenarios | Accepted |
| [0004](0004-canonical-render-camera.md) | Canonical render camera and remap LUT | Accepted |
| [0005](0005-blender-renderer.md) | Blender is a thin renderer | Accepted |
| [0006](0006-ground-truth.md) | Analytic ground truth and calibration-rs dataset emission | Accepted |

Gate results referenced by these ADRs are in [`docs/measurements/`](../measurements/).
