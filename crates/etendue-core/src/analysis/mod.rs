//! Analysis layer: derived quantities computed over a posed scene.
//!
//! Where [`crate::optics`] holds the bare physics and [`crate::scene`] holds
//! the posed entities, the analysis layer combines them into the maps and
//! volumes the UI visualises. Everything here is headless and pure `f64`.
//!
//! # Contents
//!
//! - [`defocus_map`](mod@defocus_map) — the [`defocus_map`](defocus_map())
//!   function and its [`DefocusMap`] output: the geometric circle of confusion
//!   sampled on a grid over a planar target. M4a's headless deliverable; M4b
//!   renders it as a focus heatmap.
//! - [`working_volume`](mod@working_volume) — the [`working_volume`](working_volume())
//!   function and its [`WorkingVolume`] output: the set of points on the
//!   laser fan plane that are simultaneously illuminated by the laser,
//!   visible to the camera, and in focus. M6's headless deliverable; the UI
//!   renders it as a translucent patch on the laser fan and surfaces the
//!   covered measurement area + depth range to the designer.
//! - [`voxel_overlap`](mod@voxel_overlap) — the [`voxelized_overlap`](voxelized_overlap())
//!   function and its [`VoxelOverlap`] output: a 3D scalar field counting,
//!   per voxel, how many `(camera, laser)` pairs of an M10 multi-pair rig
//!   simultaneously see, illuminate, and focus on that voxel. Headless
//!   companion to the future viewport overlay.

pub mod defocus_map;
pub mod voxel_overlap;
pub mod working_volume;

pub use defocus_map::{DefocusMap, defocus_map};
pub use voxel_overlap::{VoxelBox, VoxelOverlap, VoxelResolution, voxelized_overlap};
pub use working_volume::{
    DEFAULT_COC_THRESHOLD_PX, WorkingVolume, WorkingVolumeCell, working_volume,
};
