//! `etendue detect` on every target kind, without Blender: each target's
//! print (`etendue_synth::board::layout`, the cells and disks Blender paints)
//! is ray-cast through a pinhole and area-sampled, and the detector's labels
//! are checked against the points' exact projections.

use std::collections::HashMap;

use etendue_cli::detect::{Detector, ViewStatus, check};
use etendue_scene::TargetGeometry;
use etendue_synth::board::{BoardLayout, Disc, Surface, layout};
use nalgebra::{Isometry3, Point3, Rotation3, Translation3, UnitQuaternion, Vector3};
use serde_json::json;

/// Radiances on the 8-bit scale: background, paper, ink (G4.2's, as the
/// chessboard tests have always used).
const BACKGROUND: f64 = 240.0;
const PAPER: f64 = 204.0;
const INK: f64 = 7.0;
/// Subsamples per pixel side.
const SUBSAMPLES: u32 = 4;

/// A target's print, indexed for lookup: the cells' edges cut the surface
/// into elementary rectangles, each inside exactly one cell; disks are binned.
struct Print {
    surface: Surface,
    xs: Vec<f64>,
    ys: Vec<f64>,
    dark: Vec<bool>,
    discs: Vec<Disc>,
    bin: f64,
    bins: HashMap<(i64, i64), Vec<usize>>,
}

/// Sorted edges, merged within a nanometre.
fn edges(values: impl Iterator<Item = f64>) -> Vec<f64> {
    let mut v: Vec<f64> = values.collect();
    v.sort_by(f64::total_cmp);
    v.dedup_by(|a, b| (*a - *b).abs() < 1e-9);
    v
}

/// The elementary interval of `edges` holding `x`.
fn slot(edges: &[f64], x: f64) -> Option<usize> {
    let k = edges.partition_point(|e| *e <= x);
    (k > 0 && k < edges.len()).then(|| k - 1)
}

impl Print {
    fn new(layout: &BoardLayout) -> Self {
        let xs = edges(layout.cells.iter().flat_map(|c| [c.rect[0], c.rect[2]]));
        let ys = edges(layout.cells.iter().flat_map(|c| [c.rect[1], c.rect[3]]));
        let nx = xs.len() - 1;
        let mut dark = vec![false; nx * (ys.len() - 1)];
        for c in &layout.cells {
            let (i0, i1) = (slot(&xs, c.rect[0] + 1e-9), slot(&xs, c.rect[2] - 1e-9));
            let (j0, j1) = (slot(&ys, c.rect[1] + 1e-9), slot(&ys, c.rect[3] - 1e-9));
            for j in j0.unwrap()..=j1.unwrap() {
                for i in i0.unwrap()..=i1.unwrap() {
                    dark[j * nx + i] = c.dark;
                }
            }
        }
        let bin = layout
            .discs
            .iter()
            .map(|d| 2.0 * d.radius)
            .fold(1e-3, f64::max);
        let mut bins: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
        for (k, d) in layout.discs.iter().enumerate() {
            let lo = |c: f64| ((c - d.radius) / bin).floor() as i64;
            let hi = |c: f64| ((c + d.radius) / bin).floor() as i64;
            for bx in lo(d.center[0])..=hi(d.center[0]) {
                for by in lo(d.center[1])..=hi(d.center[1]) {
                    bins.entry((bx, by)).or_default().push(k);
                }
            }
        }
        Self {
            surface: layout.surface,
            xs,
            ys,
            dark,
            discs: layout.discs.clone(),
            bin,
            bins,
        }
    }

    /// Ink at surface coordinates `[u, v]`; `None` off the print.
    fn ink(&self, [u, v]: [f64; 2]) -> Option<bool> {
        let (i, j) = (slot(&self.xs, u)?, slot(&self.ys, v)?);
        let mut dark = self.dark[j * (self.xs.len() - 1) + i];
        let key = ((u / self.bin).floor() as i64, (v / self.bin).floor() as i64);
        // Disks in paint order: the last one covering the point wins.
        for &k in self.bins.get(&key).map_or(&[][..], Vec::as_slice) {
            let d = &self.discs[k];
            if (u - d.center[0]).hypot(v - d.center[1]) < d.radius {
                dark = d.dark;
            }
        }
        Some(dark)
    }

    /// Surface coordinates where a ray (target frame) first meets the print
    /// from outside.
    fn hit(&self, o: Vector3<f64>, d: Vector3<f64>) -> Option<[f64; 2]> {
        match self.surface {
            Surface::Plane => {
                let t = -o.z / d.z;
                (o.z > 0.0 && t > 0.0).then(|| [o.x + t * d.x, o.y + t * d.y])
            }
            Surface::Cylinder {
                radius_m,
                circumference_m,
            } => {
                let a = d.x * d.x + d.y * d.y;
                let b = 2.0 * (o.x * d.x + o.y * d.y);
                let c = o.x * o.x + o.y * o.y - radius_m * radius_m;
                let disc = b * b - 4.0 * a * c;
                if disc < 0.0 {
                    return None;
                }
                let t = (-b - disc.sqrt()) / (2.0 * a);
                if t <= 0.0 {
                    return None;
                }
                let p = o + t * d;
                let theta = p.y.atan2(p.x).rem_euclid(std::f64::consts::TAU);
                let mut v = theta / std::f64::consts::TAU * circumference_m;
                // The cells tile the window `[v0, v0 + C)`.
                if v < self.ys[0] {
                    v += circumference_m;
                }
                Some([p.z, v])
            }
        }
    }
}

/// A pinhole camera: focal length and image size in pixels, pose in the
/// target frame (CV axes).
struct Camera {
    f: f64,
    size: [usize; 2],
    target_se3_camera: Isometry3<f64>,
}

impl Camera {
    fn centre(&self) -> [f64; 2] {
        [self.size[0] as f64 / 2.0, self.size[1] as f64 / 2.0]
    }

    /// Area-sampled 8-bit image of the print.
    fn image(&self, print: &Print) -> Vec<u8> {
        let [w, h] = self.size;
        let [cx, cy] = self.centre();
        let o = self.target_se3_camera.translation.vector;
        let r = self.target_se3_camera.rotation;
        let n = SUBSAMPLES;
        let mut gray = vec![0_u8; w * h];
        for j in 0..h {
            for i in 0..w {
                let mut acc = 0.0;
                for b in 0..n {
                    for a in 0..n {
                        // chess-corners' convention: pixel `i` centred at `i`.
                        let u = i as f64 - 0.5 + (f64::from(a) + 0.5) / f64::from(n);
                        let v = j as f64 - 0.5 + (f64::from(b) + 0.5) / f64::from(n);
                        let d = r * Vector3::new((u - cx) / self.f, (v - cy) / self.f, 1.0);
                        acc += match print.hit(o, d).and_then(|uv| print.ink(uv)) {
                            Some(true) => INK,
                            Some(false) => PAPER,
                            None => BACKGROUND,
                        };
                    }
                }
                gray[j * w + i] = (acc / f64::from(n * n)).round() as u8;
            }
        }
        gray
    }

    /// Every point's exact pixel, `None` when it faces away or leaves the
    /// image.
    fn truth(&self, layout: &BoardLayout) -> Vec<Option<[f64; 2]>> {
        let [cx, cy] = self.centre();
        let eye = Point3::from(self.target_se3_camera.translation.vector);
        let inv = self.target_se3_camera.inverse();
        layout
            .points
            .iter()
            .map(|p| {
                let x = p.xyz();
                if p.surface_normal().dot(&(eye - x)) <= 0.0 {
                    return None;
                }
                let c = inv * x;
                let q = [self.f * c.x / c.z + cx, self.f * c.y / c.z + cy];
                let inside = (0.0..self.size[0] as f64).contains(&q[0])
                    && (0.0..self.size[1] as f64).contains(&q[1]);
                (c.z > 0.0 && inside).then_some(q)
            })
            .collect()
    }
}

/// A camera at `distance` in front of a planar target (on its +Z side),
/// looking at its centre, the print upright in the image, then rolled by
/// `roll` and tilted about the image's x axis by `tilt` (radians).
fn facing(f: f64, size: [usize; 2], distance: f64, roll: f64, tilt: f64) -> Camera {
    // Looking down −Z: image x along +X, image y (down) along −Y.
    let base = Rotation3::from_matrix_unchecked(nalgebra::Matrix3::new(
        1.0, 0.0, 0.0, //
        0.0, -1.0, 0.0, //
        0.0, 0.0, -1.0,
    ));
    let orbit = Rotation3::from_axis_angle(&Vector3::x_axis(), tilt);
    let rotation = orbit * base * Rotation3::from_axis_angle(&Vector3::z_axis(), roll);
    let eye = orbit * Vector3::new(0.0, 0.0, distance);
    Camera {
        f,
        size,
        target_se3_camera: Isometry3::from_parts(
            Translation3::from(eye),
            UnitQuaternion::from_rotation_matrix(&rotation),
        ),
    }
}

/// A camera at `distance` from a pole's axis at azimuth `phi`, level with
/// height `z`, looking at the axis; the axis is the image's vertical, +Z up.
fn beside(f: f64, size: [usize; 2], distance: f64, phi: f64, z: f64) -> Camera {
    let (s, c) = phi.sin_cos();
    let forward = Vector3::new(-c, -s, 0.0);
    let down = Vector3::new(0.0, 0.0, -1.0);
    let right = down.cross(&forward);
    let rotation =
        Rotation3::from_matrix_unchecked(nalgebra::Matrix3::from_columns(&[right, down, forward]));
    Camera {
        f,
        size,
        target_se3_camera: Isometry3::from_parts(
            Translation3::new(distance * c, distance * s, z),
            UnitQuaternion::from_rotation_matrix(&rotation),
        ),
    }
}

fn printed(target: serde_json::Value) -> TargetGeometry {
    serde_json::from_value(json!({ "type": "printed", "target": target })).unwrap()
}

/// Detect in `gray` and check every label: returns the status, the number
/// of features and the largest error, after asserting no label is wrong.
fn detect(
    detector: &Detector,
    gray: &[u8],
    camera: &Camera,
    layout: &BoardLayout,
    what: &str,
) -> (ViewStatus, usize, f64) {
    let found = detector.detect(gray, camera.size[0], camera.size[1]);
    let c = check(&found.points, &camera.truth(layout));
    assert_eq!(
        (c.mislabelled, c.unmatched, found.off_board),
        (0, 0, 0),
        "{what}: {c:?}, {:?}",
        found.reason
    );
    (found.status, found.points.len(), c.max_px)
}

fn chessboard(cols: u32, rows: u32) -> TargetGeometry {
    serde_json::from_value(json!({
        "type": "board",
        "board": { "kind": "chessboard", "rows": rows, "cols": cols, "square_size_m": 0.02 }
    }))
    .unwrap()
}

#[test]
fn labels_a_chessboard_in_every_rotation() {
    // 30 px squares, the board centred.
    let geometry = chessboard(9, 6);
    let (layout, detector) = (
        layout(&geometry).unwrap().unwrap(),
        Detector::new(&geometry).unwrap(),
    );
    let print = Print::new(&layout);
    for degrees in [0.0_f64, 12.0, 90.0, 105.0, 180.0, 200.0, 270.0, 290.0] {
        let camera = facing(1500.0, [480, 480], 1.0, degrees.to_radians(), 0.0);
        let what = format!("{degrees}°");
        let (status, n, max) = detect(&detector, &camera.image(&print), &camera, &layout, &what);
        assert_eq!((status, n), (ViewStatus::Ok, 54), "{what}");
        assert!(max < 0.2, "{what}: {max}");
    }
}

#[test]
fn chessboard_colours_survive_an_illumination_gradient() {
    // Light falls off to a quarter across the image: the far paper squares
    // (204 × 0.25 ≈ 51) end up darker than a global midpoint of the near
    // ink and paper, but the two square classes still separate.
    let geometry = chessboard(9, 6);
    let (layout, detector) = (
        layout(&geometry).unwrap().unwrap(),
        Detector::new(&geometry).unwrap(),
    );
    let print = Print::new(&layout);
    for degrees in [0.0_f64, 180.0] {
        let camera = facing(1500.0, [480, 480], 1.0, degrees.to_radians(), 0.0);
        let mut gray = camera.image(&print);
        graded(&mut gray, 480);
        let what = format!("{degrees}°");
        assert_eq!(
            detect(&detector, &gray, &camera, &layout, &what).0,
            ViewStatus::Ok
        );
    }
}

#[test]
fn drops_a_partial_chessboard() {
    // The board spans x = 90 … 390 with its last inner corners at 360:
    // blank everything right of 345, as if the image ended there.
    let geometry = chessboard(9, 6);
    let (layout, detector) = (
        layout(&geometry).unwrap().unwrap(),
        Detector::new(&geometry).unwrap(),
    );
    let camera = facing(1500.0, [480, 480], 1.0, 0.0, 0.0);
    let mut gray = camera.image(&Print::new(&layout));
    for row in gray.chunks_exact_mut(480) {
        row[345..].fill(240);
    }
    let found = detector.detect(&gray, 480, 480);
    assert_eq!(found.status, ViewStatus::Partial);
    assert!(found.points.is_empty());
}

#[test]
fn a_half_turn_symmetric_chessboard_is_ambiguous() {
    // 8 × 6 inner corners: 9 × 7 squares, the same after a half turn.
    let geometry = chessboard(8, 6);
    let (layout, detector) = (
        layout(&geometry).unwrap().unwrap(),
        Detector::new(&geometry).unwrap(),
    );
    let camera = facing(1500.0, [480, 480], 1.0, 0.2, 0.0);
    let gray = camera.image(&Print::new(&layout));
    assert_eq!(
        detector.detect(&gray, 480, 480).status,
        ViewStatus::Ambiguous
    );
}

/// Rolls and tilts (degrees) of the views every planar kind is checked in.
const VIEWS: [(f64, f64); 4] = [(0.0, 0.0), (-35.0, 0.0), (30.0, 25.0), (-20.0, -30.0)];
/// Rolls past 45°: calib-targets' chessboard labels then run turned against
/// the print, and the board alignments must turn them back (calib-targets
/// before 0.15.5 left every id one square off).
const TURNED: [(f64, f64); 3] = [(100.0, 0.0), (215.0, 0.0), (290.0, 20.0)];

/// A planar kind seen square on, rolled and tilted: all labels right, and
/// the whole board found when it is seen square on.
fn planar(geometry: &TargetGeometry, f: f64, size: [usize; 2], views: &[(f64, f64)]) {
    let (layout, detector) = (
        layout(geometry).unwrap().unwrap(),
        Detector::new(geometry).unwrap(),
    );
    let print = Print::new(&layout);
    let all = layout.points.len();
    for &(roll, tilt) in views {
        let camera = facing(f, size, 1.0, roll.to_radians(), tilt.to_radians());
        let what = format!("{}: roll {roll}°, tilt {tilt}°", detector.kind());
        let (status, n, max) = detect(&detector, &camera.image(&print), &camera, &layout, &what);
        assert_eq!(status, ViewStatus::Ok, "{what}");
        if tilt == 0.0 {
            assert_eq!(n, all, "{what}");
        } else {
            assert!(n * 10 >= all * 9, "{what}: {n} of {all}");
        }
        assert!(max < 0.25, "{what}: {max}");
    }
}

/// A 10 × 7 ChArUco board, 36 px squares: markers of 27 px, 4.5 px a bit.
fn charuco() -> (TargetGeometry, f64, [usize; 2]) {
    let geometry = printed(json!({
        "kind": "charuco", "rows": 7, "cols": 10, "square_size_mm": 20.0,
        "marker_size_rel": 0.75, "dictionary": "DICT_4X4_50"
    }));
    (geometry, 1800.0, [520, 440])
}

/// A 10 × 7-square marker board, 30 px squares.
fn marker_board() -> (TargetGeometry, f64, [usize; 2]) {
    let geometry = printed(json!({
        "kind": "marker_board", "inner_rows": 6, "inner_cols": 9, "square_size_mm": 20.0,
        "circles": [
            { "i": 4, "j": 2, "polarity": "white" },
            { "i": 5, "j": 2, "polarity": "black" },
            { "i": 5, "j": 3, "polarity": "white" }
        ]
    }));
    (geometry, 1500.0, [480, 440])
}

#[test]
fn labels_a_charuco_board_by_id() {
    let (geometry, f, size) = charuco();
    planar(&geometry, f, size, &VIEWS);
}

#[test]
fn labels_a_marker_board_by_its_circles() {
    let (geometry, f, size) = marker_board();
    planar(&geometry, f, size, &VIEWS);
}

/// ChArUco markers and marker board circles are aligned by cell; once the
/// chessboard labels run turned against the print (rolls past 45°), the
/// corners need the cell alignment turned into a corner alignment.
/// calib-targets before 0.15.5 skipped that, leaving every id one square off
/// (calib-targets-rs#109).
#[test]
fn rotated_charuco_and_marker_boards() {
    for (geometry, f, size) in [charuco(), marker_board()] {
        planar(&geometry, f, size, &TURNED);
    }
}

/// Light falling off to a quarter across the image, as in
/// `chessboard_colours_survive_an_illumination_gradient`.
fn graded(gray: &mut [u8], width: usize) {
    for row in gray.chunks_exact_mut(width) {
        for (i, v) in row.iter_mut().enumerate() {
            let gain = 1.0 - 0.75 * i as f64 / (width - 1) as f64;
            *v = (f64::from(*v) * gain).round() as u8;
        }
    }
}

#[test]
fn self_identifying_boards_survive_an_illumination_gradient() {
    for (geometry, f, size) in [charuco(), marker_board(), puzzleboard()] {
        let (layout, detector) = (
            layout(&geometry).unwrap().unwrap(),
            Detector::new(&geometry).unwrap(),
        );
        let camera = facing(f, size, 1.0, 0.2, 0.0);
        let mut gray = camera.image(&Print::new(&layout));
        graded(&mut gray, size[0]);
        let what = format!("{} under a gradient", detector.kind());
        let (status, n, _) = detect(&detector, &gray, &camera, &layout, &what);
        assert_eq!(status, ViewStatus::Ok, "{what}");
        assert!(n * 10 >= layout.points.len() * 9, "{what}: {n}");
    }
}

/// A 12 × 10 PuzzleBoard, 26 px squares, cut from the master away from its
/// origin, so a local label would not pass for a master one.
fn puzzleboard() -> (TargetGeometry, f64, [usize; 2]) {
    let geometry = printed(json!({
        "kind": "puzzle_board", "rows": 10, "cols": 12, "square_size_mm": 20.0,
        "origin_row": 31, "origin_col": 57
    }));
    (geometry, 1300.0, [440, 400])
}

#[test]
fn labels_a_puzzleboard_by_master_id() {
    let (geometry, f, size) = puzzleboard();
    planar(&geometry, f, size, &VIEWS);
    planar(&geometry, f, size, &TURNED);
}

#[test]
fn labels_a_puzzlepole_by_pole_id() {
    let (squares, start_row) = etendue_synth::board::pole_periods()
        .into_iter()
        .find(|p| p.0 == 24)
        .unwrap();
    let geometry = printed(json!({
        "kind": "puzzlepole", "circumference_squares": squares, "start_row": start_row,
        "axial_squares": 14, "square_size_mm": 12.0
    }));
    let (layout, detector) = (
        layout(&geometry).unwrap().unwrap(),
        Detector::new(&geometry).unwrap(),
    );
    let print = Print::new(&layout);
    // 14 × 12 mm = 168 mm along the axis, 92 mm across, 0.5 m away.
    for phi in [0.0_f64, 2.0, 4.4] {
        let camera = beside(1200.0, [360, 480], 0.5, phi, 0.084);
        let what = format!("pole at azimuth {phi}");
        let (status, n, max) = detect(&detector, &camera.image(&print), &camera, &layout, &what);
        assert_eq!(status, ViewStatus::Ok, "{what}");
        // 13 inner axial corners × at least 5 rows round the front.
        assert!(n >= 13 * 5, "{what}: {n}");
        assert!(max < 0.3, "{what}: {max}");
    }
}
