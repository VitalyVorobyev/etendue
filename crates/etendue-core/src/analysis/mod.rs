//! Analysis layer: derived quantities computed over a posed scene.
//!
//! Where [`crate::optics`] holds the bare physics and [`crate::scene`] holds
//! the posed entities, the analysis layer combines them into the maps and
//! volumes a caller can visualise. Everything here is headless and pure `f64`.
//!
//! # Contents
//!
//! - [`defocus_map`](mod@defocus_map) — the [`defocus_map`](defocus_map())
//!   function and its [`DefocusMap`] output: the geometric circle of confusion
//!   sampled on a grid over a planar target, ready to draw as a focus
//!   heatmap.
//! - [`working_volume`](mod@working_volume) — the [`working_volume`](working_volume())
//!   function and its [`WorkingVolume`] output: the set of points on the
//!   laser fan plane that are simultaneously illuminated by the laser,
//!   visible to the camera, and in focus, together with the covered
//!   measurement area and depth range.
//! - [`voxel_overlap`](mod@voxel_overlap) — the [`voxelized_overlap`](voxelized_overlap())
//!   function and its [`VoxelOverlap`] output: a 3D scalar field counting,
//!   per voxel, how many `(camera, laser)` pairs of a multi-pair rig
//!   simultaneously see, illuminate, and focus on that voxel.

pub mod defocus_map;
pub mod voxel_overlap;
pub mod working_volume;

pub use defocus_map::{DefocusMap, defocus_map};
pub use voxel_overlap::{VoxelBox, VoxelOverlap, VoxelResolution, voxelized_overlap};
pub use working_volume::{
    DEFAULT_COC_THRESHOLD_PX, WorkingVolume, WorkingVolumeCell, working_volume,
};
