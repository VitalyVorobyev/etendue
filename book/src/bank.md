# Component Bank Schema

`etendue-core::bank` holds the *design-time* component specs a designer
picks from when setting up a triangulation sensor — image sensors, lenses,
line lasers. It is intentionally separate from the runtime
[`Scene`](scene_and_geometry.md): the scene holds posed, parametric entities
the kernel operates on; the bank holds the manufacturer-spec sheets those
entities are sized from. The component-picker UI itself is post-MVP — the
M3 deliverable is the **schema + catalogue loader + seed files**, ready for
a future picker.

## Module layout

```text
bank/
├── schema.rs       // SensorSpec / LensSpec / LaserSpec, tagged enums
└── catalog.rs      // Catalog + load_from_str / load_from_path
```

Re-exports through `bank::mod`:

```rust
pub use catalog::Catalog;
pub use schema::{
    GaussianBeamParams, LaserSpec, LensMount, LensSpec,
    LineWidthModel, MtfPlaceholder, SensorSpec,
};
```

## Serde conventions (mirrored from calibration-rs)

The schema follows the same serde patterns as `vision-calibration-core`'s
`CameraParams` chain — one of the architectural rules to preserve when
reusing the calibration kernel:

- **`#[serde(tag = "type", rename_all = "snake_case")]`** on tagged enums.
  `LensMount`, `LineWidthModel`, and any future enum variant get this
  treatment so the JSON literal carries a `"type": "..."` discriminator.
- **`#[serde(flatten)]`** for nested parameter structs (e.g.
  `GaussianBeamParams` inside `LineWidthModel::Gaussian`).
- **`#[serde(alias = "...")]`** for backward-compatible field names — e.g.
  `GaussianBeamParams::w0` accepts both `"w0"` and `"w_0"` in JSON;
  `LaserSpec::fan_half_angle` accepts `"fan_half_angle_rad"`. This lets
  field renames land non-breakingly.
- **`#[serde(skip_serializing_if = "Option::is_none")]`** on every optional
  field so the on-disk JSON stays uncluttered.
- **`#[non_exhaustive]`** on tagged enums (`LensMount`, `LineWidthModel`) so
  newer-format files with unseen variants do not break deserialization in
  older binaries.

## `SensorSpec`

```rust
pub struct SensorSpec {
    pub name: String,                       // e.g. "Sony IMX174"
    pub pixel_pitch_m: f64,                 // metres (e.g. 5.86e-6)
    pub resolution: (u32, u32),             // (width, height) px
    pub well_depth_e: Option<f64>,          // electrons (noise model, M6+)
    pub read_noise_e: Option<f64>,          // electrons RMS
    pub peak_qe:      Option<f64>,          // [0, 1]
}
```

Pixel pitch and resolution are the primary sizing parameters. The optional
electro-optical fields are reserved for future SNR/noise analysis.

## `LensSpec`

```rust
pub struct LensSpec {
    pub name: String,                       // e.g. "Kowa LM25HC"
    pub focal_length_m: f64,                // metres (e.g. 0.025)
    pub max_aperture_f: f64,                // dimensionless (e.g. 1.4)
    pub mount: LensMount,                   // tagged enum
    pub fov_diagonal_deg: Option<f64>,      // optional; derivable from f + sensor
    pub mtf: Option<MtfPlaceholder>,        // single-point MTF placeholder
}

#[non_exhaustive]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LensMount {
    CMount,
    CsMount,
    FMount,
    M12,
    Other { description: String },
}

pub struct MtfPlaceholder {
    #[serde(alias = "mtf50_lp_per_mm")]
    pub mtf50_lp_mm: Option<f64>,           // lp/mm at MTF = 50%
}
```

`MtfPlaceholder` is a single-field struct on purpose: future MTF-aware work
adds fields without a breaking schema change. A real MTF curve (contrast vs.
lp/mm) is M6+.

## `LaserSpec`

```rust
pub struct LaserSpec {
    pub name: String,                       // e.g. "Coherent StingRay 660 nm"
    pub wavelength_nm: f64,                 // nm — laser-industry convention
    #[serde(alias = "fan_half_angle_rad")]
    pub fan_half_angle: f64,                // radians (e.g. 0.262 ≈ 15°)
    pub power_mw:   Option<f64>,            // milliwatts
    pub line_width: Option<LineWidthModel>, // tagged enum
}

#[non_exhaustive]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LineWidthModel {
    Gaussian {
        #[serde(flatten)]
        params: GaussianBeamParams,
    },
    Measured {
        note: String,                       // datasheet-figure reference
    },
}

pub struct GaussianBeamParams {
    #[serde(alias = "w_0")]
    pub w0: f64,                            // beam waist (1/e² radius), metres
}
```

`Gaussian` is the M5 default — the [laser chapter](laser.md) describes the
`w(d) = w0·√(1 + (d/zR)²)` model that consumes it. `Measured` is a
placeholder for a future lookup-table approach.

## `Catalog`

```rust
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Catalog {
    #[serde(default)] pub sensors: Vec<SensorSpec>,
    #[serde(default)] pub lenses:  Vec<LensSpec>,
    #[serde(default)] pub lasers:  Vec<LaserSpec>,
}

impl Catalog {
    pub fn empty() -> Self;
    pub fn load_from_str (s: &str)                -> Result<Self, serde_json::Error>;
    pub fn load_from_path(path: impl AsRef<Path>) -> crate::Result<Self>;
    pub fn len(&self)      -> usize;
    pub fn is_empty(&self) -> bool;
}
```

The JSON format is the obvious one — a single top-level object with three
arrays:

```json
{
  "sensors": [ ... ],
  "lenses":  [ ... ],
  "lasers":  [ ... ]
}
```

Each array element is a serialized `SensorSpec` / `LensSpec` / `LaserSpec`.

## Seed files

Three seed bank files live at
`crates/etendue-core/assets/bank/{sensors,lenses,lasers}.json`. They cover
a handful of widely-used machine-vision components (Sony Pregius sensors, a
small set of common C-mount lenses, a couple of red and green line lasers).
They are `include_str!`-embedded in the catalogue unit tests so a schema
drift breaks the test the moment a seed file is edited inconsistently.

**Note on values.** Seed-file numbers are *illustrative*. They were
populated from public datasheet references during M3 to exercise the loader
and serve as a working example, but they should be cross-checked against
the current datasheet before being used in a real design.

## Picker UI

Post-MVP. The schema, the catalogue loader, the seed files, and the
round-trip tests are the M3 deliverable. A future picker will index the
catalogue, filter by `LensMount` / wavelength / pixel-pitch range, and let
the user drag a spec onto the scene to populate a `CameraEntity` /
`LaserEntity` / `TargetEntity` — all of which already accept the relevant
spec fields directly (e.g. `LensSpec::focal_length_m` maps straight onto
`CameraEntity::effective_focal_length_m`).
