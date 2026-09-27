//! Analytic ground truth ([ADR 0006](https://github.com/VitalyVorobyev/etendue/blob/main/docs/adrs/0006-ground-truth.md)):
//! target feature points projected through the calibration-rs camera model,
//! with per-point visibility.
//!
//! Ground truth is never read back from rendered images: points come from the
//! target's layout, poses from the baked scenario, and pixels from
//! `CameraModel::project_point_c` — the same model the remap LUT inverts.

use nalgebra::{Isometry3, Point3, Vector3};
use serde::{Deserialize, Serialize};
use vision_calibration_core::CameraModel;

/// A target feature point, in the target frame: the `z = 0` plane, metres.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BoardPoint {
    /// Position `[x, y]` in the target frame, metres.
    pub position_m: [f64; 2],
    /// Grid coordinate on the board (column, row), if the layout has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grid: Option<[i32; 2]>,
    /// Point id within the board, if the layout numbers its points.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<u32>,
}

/// Why a point is not visible, if it is not.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Occlusion {
    /// On or behind the camera plane, or rejected by the projection chain.
    BehindCamera,
    /// Projected outside the image, or within the border margin of its edge.
    OutsideImage,
    /// The target faces away from the camera at this point.
    BackFacing,
}

/// One target point in one view.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GtPoint {
    /// Index into the target's [`BoardPoint`] list.
    pub point: usize,
    /// Projected pixel `[u, v]`; absent when the point is behind the camera.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pixel: Option<[f64; 2]>,
    /// `None` when visible; otherwise the first test the point failed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub occluded: Option<Occlusion>,
}

impl GtPoint {
    /// Whether the point passed every visibility test.
    #[must_use]
    pub fn visible(&self) -> bool {
        self.occluded.is_none()
    }
}

/// The geometric-tier visibility tests of ADR 0006 (the photometric tier adds
/// render occlusion).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VisibilitySpec {
    /// Border margin in pixels: a point closer than this to an image edge is
    /// not visible.
    pub margin_px: f64,
}

impl Default for VisibilitySpec {
    fn default() -> Self {
        Self { margin_px: 2.0 }
    }
}

/// Project `points` (target frame) into a camera.
///
/// `camera` is the calibration-rs model, `resolution` its image size, and the
/// poses are world poses of the camera (CV axes) and the target. The image
/// spans `[edge, edge + W] × [edge, edge + H]` with `edge` the
/// pixel-centre convention's edge (see [`crate::PixelCentre::edge`]); the
/// margin is measured from those edges.
#[must_use]
pub fn project_points(
    camera: &CameraModel,
    resolution: [u32; 2],
    world_se3_camera: &Isometry3<f64>,
    world_se3_target: &Isometry3<f64>,
    points: &[BoardPoint],
    visibility: &VisibilitySpec,
    edge: f64,
) -> Vec<GtPoint> {
    let camera_se3_target = world_se3_camera.inverse() * world_se3_target;
    // The target's outward normal (+Z) in the camera frame.
    let normal = camera_se3_target * Vector3::z();
    let (w, h) = (f64::from(resolution[0]), f64::from(resolution[1]));
    let m = visibility.margin_px;
    points
        .iter()
        .enumerate()
        .map(|(i, bp)| {
            let p = camera_se3_target * Point3::new(bp.position_m[0], bp.position_m[1], 0.0);
            let pixel = if p.z > 0.0 {
                camera.project_point_c(&p.coords)
            } else {
                None
            };
            let occluded = match pixel {
                None => Some(Occlusion::BehindCamera),
                Some(px)
                    if !(px.x >= edge + m
                        && px.x <= edge + w - m
                        && px.y >= edge + m
                        && px.y <= edge + h - m) =>
                {
                    Some(Occlusion::OutsideImage)
                }
                // Facing: the ray from the point to the camera centre is on
                // the normal's side of the target plane.
                Some(_) if normal.dot(&(-p.coords)) <= 0.0 => Some(Occlusion::BackFacing),
                Some(_) => None,
            };
            GtPoint {
                point: i,
                pixel: pixel.map(|px| [px.x, px.y]),
                occluded,
            }
        })
        .collect()
}

/// Inner corners of a chessboard or ChArUco target in the target frame, in
/// the layout etendue draws boards (render jobs and the web viewers): centred
/// on the target origin, columns along +X, rows along +Y, `grid = [column,
/// row]` and `id = row · columns + column` from the −X/−Y corner.
///
/// **Interim:** P3-3 makes calib-targets' printed layout the single source of
/// board geometry; until then this is the layout every etendue backend draws.
/// `None` for targets without a square grid.
#[must_use]
pub fn board_points(geometry: &etendue_scene::TargetGeometry) -> Option<Vec<BoardPoint>> {
    use vision_calibration_dataset::TargetSpec as Board;
    // Inner corners along X and Y, and the square size.
    let (nx, ny, s) = match geometry {
        etendue_scene::TargetGeometry::Board {
            board:
                Board::Chessboard {
                    rows,
                    cols,
                    square_size_m,
                },
        } => (*cols, *rows, *square_size_m),
        etendue_scene::TargetGeometry::Board {
            board:
                Board::Charuco {
                    rows,
                    cols,
                    square_size_m,
                    ..
                },
        } => (cols.checked_sub(1)?, rows.checked_sub(1)?, *square_size_m),
        _ => return None,
    };
    let [w, h] = geometry.extent_m()?;
    let mut out = Vec::with_capacity((nx * ny) as usize);
    for r in 0..ny {
        for c in 0..nx {
            out.push(BoardPoint {
                position_m: [
                    -w / 2.0 + f64::from(c + 1) * s,
                    -h / 2.0 + f64::from(r + 1) * s,
                ],
                grid: Some([c as i32, r as i32]),
                id: Some(r * nx + c),
            });
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use nalgebra::{Translation3, UnitQuaternion};
    use vision_calibration_core::{
        CameraParams, DistortionParams, FxFyCxCySkew, IntrinsicsParams, ProjectionParams,
        SensorParams,
    };

    fn camera() -> CameraModel {
        CameraParams {
            projection: ProjectionParams::Pinhole,
            distortion: DistortionParams::None,
            sensor: SensorParams::Identity,
            intrinsics: IntrinsicsParams::FxFyCxCySkew {
                params: FxFyCxCySkew {
                    fx: 100.0,
                    fy: 100.0,
                    cx: 49.5,
                    cy: 39.5,
                    skew: 0.0,
                },
            },
        }
        .build()
        .unwrap()
    }

    fn pt(x: f64, y: f64) -> BoardPoint {
        BoardPoint {
            position_m: [x, y],
            grid: None,
            id: None,
        }
    }

    #[test]
    fn classifies_each_visibility_case() {
        // Camera at the origin looking down +Z; the target 1 m ahead, facing
        // the camera (its +Z points back at it: rotated π about X).
        let target = Isometry3::from_parts(
            Translation3::new(0.0, 0.0, 1.0),
            UnitQuaternion::from_axis_angle(&Vector3::x_axis(), std::f64::consts::PI),
        );
        let points = [pt(0.0, 0.0), pt(0.495, 0.0), pt(0.1, 0.0)];
        let gt = project_points(
            &camera(),
            [100, 80],
            &Isometry3::identity(),
            &target,
            &points,
            &VisibilitySpec::default(),
            -0.5,
        );
        assert!(gt[0].visible());
        let [u, v] = gt[0].pixel.unwrap();
        assert!((u - 49.5).abs() < 1e-12 && (v - 39.5).abs() < 1e-12);
        // 0.495 m to the side at 1 m: u = 99 — within 2 px of the right edge (99.5).
        assert_eq!(gt[1].occluded, Some(Occlusion::OutsideImage));
        assert!(gt[2].visible());

        // The same target turned around faces away.
        let away = Isometry3::translation(0.0, 0.0, 1.0);
        let gt = project_points(
            &camera(),
            [100, 80],
            &Isometry3::identity(),
            &away,
            &points,
            &VisibilitySpec::default(),
            -0.5,
        );
        assert_eq!(gt[0].occluded, Some(Occlusion::BackFacing));

        // Behind the camera.
        let behind = Isometry3::translation(0.0, 0.0, -1.0);
        let gt = project_points(
            &camera(),
            [100, 80],
            &Isometry3::identity(),
            &behind,
            &points,
            &VisibilitySpec::default(),
            -0.5,
        );
        assert_eq!(gt[0].occluded, Some(Occlusion::BehindCamera));
        assert_eq!(gt[0].pixel, None);
        let json = serde_json::to_string(&gt[0]).unwrap();
        assert_eq!(json, r#"{"point":0,"occluded":"behind_camera"}"#);
    }

    #[test]
    fn board_points_are_the_inner_corners_of_the_drawn_squares() {
        use vision_calibration_dataset::TargetSpec as Board;
        let board = etendue_scene::TargetGeometry::Board {
            board: Board::Chessboard {
                rows: 6,
                cols: 9,
                square_size_m: 0.025,
            },
        };
        let points = super::board_points(&board).unwrap();
        assert_eq!(points.len(), 54);
        // 10 × 7 squares of 25 mm centred on the origin: first inner corner one
        // square in from the −X/−Y corner, last one square in from +X/+Y.
        let close =
            |a: [f64; 2], b: [f64; 2]| (a[0] - b[0]).abs() < 1e-12 && (a[1] - b[1]).abs() < 1e-12;
        assert!(close(points[0].position_m, [-0.1, -0.0625]));
        assert!(close(points[53].position_m, [0.1, 0.0625]));
        assert_eq!(points[10].grid, Some([1, 1]));
        assert_eq!(points[10].id, Some(10));
        assert!(
            super::board_points(&etendue_scene::TargetGeometry::Rectangle {
                width: 1.0,
                height: 1.0
            })
            .is_none()
        );
    }
}
