# Component Bank Schema

`etendue-core::bank` holds the *design-time* component specs a designer
picks from when setting up a triangulation sensor: image sensors, lenses,
line lasers. It is intentionally separate from the
[optics kernel's scene](scene_and_geometry.md): the scene holds posed,
parametric entities the kernel operates on; the bank holds the
manufacturer-spec sheets those entities are sized from. The module provides
the **schema, the catalogue loader and seed files**.

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
`CameraParams` chain:

- **`#[serde(tag = "type", rename_all = "snake_case")]`** on tagged enums.
  `LensMount` and `LineWidthModel` get this treatment, so the JSON literal
  carries a `"type": "..."` discriminator.
- **`#[serde(flatten)]`** for nested parameter structs (e.g.
  `GaussianBeamParams` inside `LineWidthModel::Gaussian`).
- **`#[serde(alias = "...")]`** for backward-compatible field names, e.g.
  `GaussianBeamParams::w0` accepts both `"w0"` and `"w_0"` in JSON;
  `LaserSpec::fan_half_angle` accepts `"fan_half_angle_rad"`. This lets
  field renames land without breaking existing files.
- **`#[serde(skip_serializing_if = "Option::is_none")]`** on the optional
  fields of `SensorSpec`, `LensSpec` and `LaserSpec`, so the on-disk JSON
  stays uncluttered.
- **`#[non_exhaustive]`** on tagged enums (`LensMount`, `LineWidthModel`), so
  adding a variant is not a breaking change for Rust callers.

## `SensorSpec`

```rust
pub struct SensorSpec {
    pub name: String,                       // e.g. "Sony IMX174"
    pub pixel_pitch_m: f64,                 // metres (e.g. 5.86e-6)
    pub resolution: (u32, u32),             // (width, height) px
    pub well_depth_e: Option<f64>,          // electrons
    pub read_noise_e: Option<f64>,          // electrons RMS
    pub peak_qe:      Option<f64>,          // [0, 1]
}
```

Pixel pitch and resolution are the primary sizing parameters. The optional
electro-optical fields are reserved for SNR and noise analysis.

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

`MtfPlaceholder` is a single-field struct on purpose: MTF-aware work can add
fields without a breaking schema change. A full MTF curve (contrast vs.
lp/mm) is not modelled.

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

`Gaussian` is the default line-width model; the [laser chapter](laser.md)
describes the `w(d) = w0·√(1 + (d/zR)²)` model that consumes it. `Measured`
is a placeholder for a lookup-table approach.

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
    pub fn load_from_str (s: &str)                -> crate::Result<Self>;
    pub fn load_from_path(path: impl AsRef<Path>) -> crate::Result<Self>;
    pub fn len(&self)      -> usize;
    pub fn is_empty(&self) -> bool;
}
```

Parse and read failures are reported as `Error::Io`. The JSON format is a
single top-level object with three arrays, each of which may be omitted:

```json
{
  "sensors": [ ... ],
  "lenses":  [ ... ],
  "lasers":  [ ... ]
}
```

Each array element is a serialised `SensorSpec` / `LensSpec` / `LaserSpec`.

## Seed files

Three seed bank files live at
`crates/etendue-core/assets/bank/{sensors,lenses,lasers}.json`. They cover
a handful of widely used machine-vision components: global-shutter sensors
(Sony IMX174 and IMX249, ON Semiconductor Python 1300), three C-mount lenses
(16, 25 and 50 mm), and red, green and blue line lasers. They are
`include_str!`-embedded in the catalogue unit tests, so a schema change that
the seed files do not follow fails the tests.

**Note on values.** Seed-file numbers are *illustrative*. They were
populated from public datasheet references to exercise the loader and serve
as a working example, and should be cross-checked against the current
datasheet before being used in a real design.

## From a spec to an entity

The scene entities accept the relevant spec fields directly. For example,
`LensSpec::focal_length_m` maps onto
`PhysicalOptics::effective_focal_length_m`, `SensorSpec::pixel_pitch_m` and
`SensorSpec::resolution` onto `PhysicalOptics::pixel_pitch_m` and
`CameraEntity::resolution`, and `LaserSpec::wavelength_nm` and
`LaserSpec::fan_half_angle` onto the `LaserEntity` fields of the same name.
After changing a camera's physical optics, call
`CameraEntity::sync_intrinsics_from_physical`.
