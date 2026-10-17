//! Gate G3.2 (P7-4): every calib-targets target, as etendue draws it.
//!
//! For a chessboard, a ChArUco board, a marker board, a PuzzleBoard and a
//! PuzzlePole:
//!
//! - the scene's mirror type and calib-targets' own type are the same JSON;
//! - the base layer of the target mesh covers the board's area (planar) or
//!   the cylinder's `2π r L`, and its cells tile the surface without overlap;
//! - every feature point is a corner of the drawn checker;
//! - every triangle faces outward;
//! - the disks' rims stay within 1 µm of their circles (the mesh builder's
//!   segment count, checked on the mesh);
//! - a PuzzlePole's points agree with calib-targets' pole object frame to
//!   1e-3 mm, and its surface closes at the seam.

use calib_targets_print::{
    CharucoTargetSpec, ChessboardTargetSpec, MarkerBoardTargetSpec, PuzzleBoardTargetSpec,
    PuzzlePoleTargetSpec, TargetSpec,
};
use etendue_scene::{PrintedTarget, TargetGeometry};
use etendue_synth::board::{BoardLayout, Surface, layout};
use etendue_synth::target_mesh::{CHORD_ERROR_M, DISC_LIFT_M, target_mesh};

fn targets() -> Vec<(&'static str, TargetSpec)> {
    let dict = calib_targets_charuco::builtins::builtin_dictionary("DICT_4X4_50").unwrap();
    vec![
        (
            "chessboard",
            TargetSpec::Chessboard(ChessboardTargetSpec::new(6, 9, 25.0)),
        ),
        (
            "charuco",
            TargetSpec::Charuco(CharucoTargetSpec::new(5, 7, 30.0, 0.75, dict)),
        ),
        (
            "marker_board",
            TargetSpec::MarkerBoard(MarkerBoardTargetSpec::new(
                6,
                9,
                25.0,
                MarkerBoardTargetSpec::default_circles(6, 9),
            )),
        ),
        (
            "puzzle_board",
            TargetSpec::PuzzleBoard(PuzzleBoardTargetSpec::new(10, 12, 15.0)),
        ),
        (
            "puzzlepole",
            TargetSpec::PuzzlePole(PuzzlePoleTargetSpec::new(12, 6, 30.0).unwrap()),
        ),
    ]
}

/// The calib-targets spec as a scene target, through JSON — and back.
fn scene_target(spec: &TargetSpec) -> TargetGeometry {
    let json = serde_json::to_value(spec).unwrap();
    let mirror: PrintedTarget = serde_json::from_value(json.clone()).unwrap();
    assert_eq!(
        serde_json::to_value(&mirror).unwrap(),
        json,
        "the mirror writes calib-targets' JSON"
    );
    TargetGeometry::Printed { target: mirror }
}

/// Ink or paper of the base layer at surface point `(u, v)`.
fn dark_at(l: &BoardLayout, u: f64, v: f64) -> bool {
    let hits: Vec<bool> = l
        .cells
        .iter()
        .filter(|c| c.rect[0] <= u && u < c.rect[2] && c.rect[1] <= v && v < c.rect[3])
        .map(|c| c.dark)
        .collect();
    assert_eq!(hits.len(), 1, "({u}, {v}) is in {} cells", hits.len());
    hits[0]
}

/// The cells' bounding box in surface coordinates.
fn bounds(l: &BoardLayout) -> [f64; 4] {
    l.cells.iter().fold(
        [
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
        ],
        |b, c| {
            [
                b[0].min(c.rect[0]),
                b[1].min(c.rect[1]),
                b[2].max(c.rect[2]),
                b[3].max(c.rect[3]),
            ]
        },
    )
}

#[test]
fn g3_2_every_target_is_drawn_as_printed() {
    for (name, spec) in targets() {
        let geometry = scene_target(&spec);
        let l = layout(&geometry).unwrap().unwrap();
        let mesh = target_mesh(&l);
        let [u0, v0, u1, v1] = bounds(&l);

        // Area of the base layer.
        let area: f64 = (0..mesh.base_triangles).map(|t| mesh.area(t)).sum();
        let expected = match l.surface {
            Surface::Plane => l.size_m[0] * l.size_m[1],
            Surface::Cylinder { radius_m, .. } => std::f64::consts::TAU * radius_m * (u1 - u0),
        };
        assert!(
            (area - expected).abs() <= 1e-4 * expected,
            "{name}: base area {area} vs {expected}"
        );

        // Non-overlap: a grid of probes lands in exactly one cell each.
        for i in 0..97 {
            for j in 0..83 {
                dark_at(
                    &l,
                    u0 + (u1 - u0) * (f64::from(i) + 0.5) / 97.0,
                    v0 + (v1 - v0) * (f64::from(j) + 0.5) / 83.0,
                );
            }
        }

        // Every point is a checker corner: around it the squares alternate.
        assert!(!l.points.is_empty(), "{name}: points");
        let d = 1e-4;
        for p in &l.points {
            let [u, v] = match l.surface {
                Surface::Plane => p.position_m,
                Surface::Cylinder {
                    circumference_m, ..
                } => {
                    // Back into the strip's window from the 3-D position.
                    let [x, y, z] = p.xyz_m.unwrap();
                    let mut arc = y.atan2(x) / std::f64::consts::TAU * circumference_m;
                    while arc < v0 {
                        arc += circumference_m;
                    }
                    [z, arc]
                }
            };
            let (a, b) = (dark_at(&l, u - d, v - d), dark_at(&l, u + d, v - d));
            let (c, e) = (dark_at(&l, u - d, v + d), dark_at(&l, u + d, v + d));
            assert!(
                a == e && b == c && a != b,
                "{name}: {p:?} is not a checker corner"
            );
        }

        // Every triangle faces outward: its winding agrees with its normals.
        for (t, tri) in mesh.triangles.iter().enumerate() {
            let [a, b, c] = tri.map(|i| mesh.positions[i as usize]);
            let (x, y) = (
                [b[0] - a[0], b[1] - a[1], b[2] - a[2]],
                [c[0] - a[0], c[1] - a[1], c[2] - a[2]],
            );
            let n = [
                x[1] * y[2] - x[2] * y[1],
                x[2] * y[0] - x[0] * y[2],
                x[0] * y[1] - x[1] * y[0],
            ];
            let normal = mesh.normals[tri[0] as usize];
            let dot = n[0] * normal[0] + n[1] * normal[1] + n[2] * normal[2];
            assert!(dot > 0.0, "{name}: triangle {t} faces inward");
        }

        // Disks: their rim vertices sit on the circle (lifted), and the rim's
        // chords stray at most 1 µm. Checked on planar targets, where the rim
        // is a plain circle around the centre vertex.
        if l.surface == Surface::Plane && !l.discs.is_empty() {
            let disc = l.discs[0];
            let lifted: Vec<[f64; 3]> = mesh
                .positions
                .iter()
                .copied()
                .filter(|p| (p[2] - DISC_LIFT_M).abs() < 1e-12)
                .filter(|p| {
                    let r = (p[0] - disc.center[0]).hypot(p[1] - disc.center[1]);
                    (r - disc.radius).abs() < 1e-12
                })
                .collect();
            let n = lifted.len() as f64;
            assert!(n >= 8.0, "{name}: rim vertices of the first disk");
            let sag = disc.radius * (1.0 - (std::f64::consts::PI / n).cos());
            assert!(sag <= CHORD_ERROR_M, "{name}: rim chord error {sag}");
        }
        println!(
            "G3.2 {name}: {} triangles ({} base), {} disks, {} points, base area {area:.6e} m² \
             (expected {expected:.6e})",
            mesh.triangles.len(),
            mesh.base_triangles,
            l.discs.len(),
            l.points.len(),
        );
    }
}

#[test]
fn g3_2_the_pole_matches_calib_targets_and_closes_at_its_seam() {
    let spec = PuzzlePoleTargetSpec::new(12, 6, 30.0).unwrap();
    let geometry = scene_target(&TargetSpec::PuzzlePole(spec.clone()));
    let l = layout(&geometry).unwrap().unwrap();
    let mut worst = 0.0_f64;
    for p in &l.points {
        let [axial, cyclic] = p.grid.unwrap();
        let theirs = calib_targets_puzzleboard::pole::geometry::object_position(
            12,
            axial as u32,
            cyclic as u32,
            30.0,
        );
        let [x, y, z] = p.xyz_m.unwrap().map(|c| c * 1000.0);
        let d = ((x - f64::from(theirs.x)).powi(2)
            + (y - f64::from(theirs.y)).powi(2)
            + (z - f64::from(theirs.z)).powi(2))
        .sqrt();
        worst = worst.max(d);
        // The id is calib-targets' corner id.
        assert_eq!(p.id, Some(cyclic as u32 * 7 + axial as u32));
    }
    println!("G3.2 pole points vs calib-targets object_position: max {worst:.3e} mm");
    assert!(worst <= 1e-3, "{worst} mm");
    assert_eq!(l.points.len(), 12 * 5);

    // The seam: the window's two edges are the same circle on the cylinder.
    let [u0, v0, u1, v1] = bounds(&l);
    for u in [u0, (u0 + u1) / 2.0, u1] {
        let (a, _) = l.surface.at([u, v0]);
        let (b, _) = l.surface.at([u, v1]);
        let gap = ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt();
        assert!(gap < 1e-12, "seam gap {gap} m");
    }
    // One whole turn, and the radius the paper quotes (114.59 mm across).
    let Surface::Cylinder {
        radius_m,
        circumference_m,
    } = l.surface
    else {
        panic!("a pole is a cylinder")
    };
    assert!(((v1 - v0) - circumference_m).abs() < 1e-12);
    assert!((2000.0 * radius_m - 114.59).abs() < 0.01);
}
