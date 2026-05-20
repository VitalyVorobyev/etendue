# Getting Started

## Prerequisites

- **Rust 1.95** — the toolchain is pinned via `rust-toolchain.toml` at the
  workspace root, so `rustup` will pick up the right version automatically the
  first time you build:

  ```toml
  [toolchain]
  channel = "1.95.0"
  components = ["rustfmt", "clippy"]
  ```

- **Platform.** macOS (Metal backend via `wgpu`), Linux (Vulkan), Windows
  (DX12). All three are covered by CI on every push.

- **A sibling [calibration-rs] checkout.** etendue depends on the
  `vision-calibration-core` crate by **path**, *not* by crates.io — the path
  dependency in the workspace `Cargo.toml` reads:

  ```toml
  vision-calibration-core = { path = "../calibration-rs/crates/vision-calibration-core" }
  ```

  So calibration-rs must live next to etendue on disk:

  ```text
  ~/vision/
    ├── calibration-rs/
    └── etendue/
  ```

  See [the architecture chapter](architecture.md) for the rationale (path
  dependency keeps the kernel reusable across both projects without crates.io
  publishing churn while the API moves).

## Clone and build

```bash
cd ~/vision
git clone https://github.com/VitalyVorobyev/calibration-rs
git clone https://github.com/VitalyVorobyev/etendue
cd etendue
cargo build --workspace
cargo test --workspace      # 172 tests
cargo run                   # launches the desktop app
```

`cargo run` resolves to the `etendue-ui` binary (the workspace has two
crates: a headless `etendue-core` kernel and the `etendue-ui` binary that
depends on it). The binary opens a single window — a 3D viewport with the
default MVP scene and a parameter side-panel.

## The MVP demo recipe

The default scene is a textbook single-camera / single-laser triangulation
rig: a 16 mm f/2.8 lens on a 1280×1024 / 3.45 µm sensor at a 0.617 m standoff
from a 0.15 m × 0.12 m target, with a 660 nm line laser sitting 0.28 m to
the side. The lens is focused on the on-axis target point and the Scheimpflug
tilt is zero. Both the heatmap and the working-volume overlays are on by
default.

To get the feel of the tool in five minutes:

1. **Pick a working distance.** Edit the camera section's pose translation to
   move it along the baseline; the working-volume patch on the laser fan and
   the focus heatmap on the target update in real time.
2. **Drag focus distance.** The on-axis focus shifts; the heatmap's green
   in-focus band (the iso-CoC contour where `coc_px ≤ 1`) moves with it.
3. **Tune focal length and f-number.** Wider lenses cover more of the target
   but tighten depth of field at the same f-number; closing the aperture
   (higher f-number) deepens it.
4. **Tune the Scheimpflug tilt (τₓ, τᵧ).** The in-focus band rotates across
   the target rather than translating: this is the hinge-rule swing of the
   Scheimpflug plane of best focus (see
   [the Scheimpflug derivation](scheimpflug.md)).
5. **Tune the fan angle and beam waist.** Wider fans paint more of the
   target; a smaller waist gives a thinner line near the waist but diverges
   faster past the Rayleigh range. The simulated-image panel (bottom dock)
   shows the projected line with per-vertex width encoded as a polygon band.
6. **Read the headline numbers.** The parameter panel surfaces the working-
   volume **area (m²)** and **depth range (m)** — these are what you size a
   real sensor against. The default scene reports ≈ 4990 mm² covered with a
   depth-z range of 603.5–629.8 mm.

Save the scene to JSON via the panel's "Save Scene" button to keep a working
point; re-load it with "Load Scene". The format is
`serde_json::to_string_pretty` on `Scene`; see the [component-bank
chapter](bank.md) for the serde conventions.

[calibration-rs]: https://github.com/VitalyVorobyev/calibration-rs
