//! Target meshes (P7-4): the printed surface of any target as triangles, ink
//! or paper — the one surface both renderers draw (Blender through the job,
//! the studio through `@etendue/wasm`).
//!
//! Built from the board layout ([`crate::board::layout`]):
//!
//! - every cell is a quad (two triangles); on a cylinder it is cut along the
//!   circumference so that no chord strays more than [`CHORD_ERROR_M`] from
//!   the true surface;
//! - every disk is a polar grid (rings × segments), lifted [`DISC_LIFT_M`]
//!   off the surface along its normal so it is drawn over the cell beneath
//!   without the two being coplanar. Its rim's chords stay within
//!   [`CHORD_ERROR_M`] of the circle; on a cylinder its rings are close
//!   enough that its interior sags less than the lift minus the chord error,
//!   so it stays above the cells (which lie at most the chord error below
//!   the true surface).
//!
//! The base cells tile the surface without overlap (no two faces coplanar
//! over the same area), and a cylinder's seam closes: the vertices at
//! `θ = 0` and `θ = 2π` coincide. A cylinder is closed by two paper end
//! caps, as the tube it is wrapped on is: its inside is never seen.

use etendue_scene::TargetGeometry;
use serde::{Deserialize, Serialize};

use crate::board::{BoardLayout, Disc, Surface, layout};
use crate::{Error, Result};

/// Largest distance of a chord from the curve it approximates (a disk's rim,
/// a cylinder's circumference), metres.
pub const CHORD_ERROR_M: f64 = 1e-6;
/// How far disks sit above the surface, metres: enough for a disk's
/// interior to stay above the cell under it, far below what a camera
/// resolves (disks are codes and orientation marks, not measured points).
pub const DISC_LIFT_M: f64 = 1e-5;

/// A target surface as triangles, in the target frame (metres).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TargetMesh {
    /// Vertex positions.
    pub positions: Vec<[f64; 3]>,
    /// Vertex normals (outward, unit).
    pub normals: Vec<[f64; 3]>,
    /// Triangles, counter-clockwise seen from outside.
    pub triangles: Vec<[u32; 3]>,
    /// Per triangle: ink (`true`) or paper.
    pub dark: Vec<bool>,
    /// How many of the first triangles are the base layer (the cells); the
    /// rest are disks, then a cylinder's end caps.
    pub base_triangles: usize,
}

impl TargetMesh {
    fn vertex(&mut self, surface: &Surface, uv: [f64; 2], lift: f64) -> u32 {
        let (p, n) = surface.at(uv);
        let i = u32::try_from(self.positions.len()).expect("a target mesh has < 2³² vertices");
        self.positions
            .push([p[0] + lift * n[0], p[1] + lift * n[1], p[2] + lift * n[2]]);
        self.normals.push(n);
        i
    }

    /// A triangle `a, b, c`, given counter-clockwise in surface coordinates.
    /// That is counter-clockwise from outside on a plane (`x × y = +Z`) but
    /// not on a cylinder (`ẑ × θ̂ = −r̂`), where it is turned round.
    fn triangle(&mut self, surface: &Surface, [a, b, c]: [u32; 3], dark: bool) {
        self.triangles.push(match surface {
            Surface::Plane => [a, b, c],
            Surface::Cylinder { .. } => [a, c, b],
        });
        self.dark.push(dark);
    }

    /// The area of triangle `t`, square metres.
    #[must_use]
    pub fn area(&self, t: usize) -> f64 {
        let [a, b, c] = self.triangles[t].map(|i| self.positions[i as usize]);
        let (u, v) = (sub(b, a), sub(c, a));
        let n = [
            u[1] * v[2] - u[2] * v[1],
            u[2] * v[0] - u[0] * v[2],
            u[0] * v[1] - u[1] * v[0],
        ];
        0.5 * (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt()
    }
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

/// Segments around a circle of `radius` whose chords stray at most
/// [`CHORD_ERROR_M`] from it.
fn segments(radius: f64) -> usize {
    if radius <= CHORD_ERROR_M {
        return 8;
    }
    let step = 2.0 * (1.0 - CHORD_ERROR_M / radius).acos();
    ((std::f64::consts::TAU / step).ceil() as usize).max(8)
}

/// The longest straight step along a cylinder's circumference (metres of
/// arc) whose chord strays at most [`CHORD_ERROR_M`]; unbounded on a plane.
fn arc_step(surface: &Surface) -> f64 {
    match *surface {
        Surface::Plane => f64::INFINITY,
        Surface::Cylinder { radius_m, .. } => {
            2.0 * (1.0 - CHORD_ERROR_M / radius_m).acos() * radius_m
        }
    }
}

/// The surface of a scene target as a mesh: the print
/// ([`crate::board::layout`]) or, for a target calib-targets does not print
/// (a plain rectangle, a ring grid), one paper quad over its extent.
///
/// # Errors
///
/// [`Error::InvalidInput`] if the target is invalid or its extent unknown (a
/// `board` puzzleboard named by layout: use a `printed` one).
pub fn geometry_mesh(geometry: &TargetGeometry) -> Result<TargetMesh> {
    if let Some(layout) = layout(geometry)? {
        return Ok(target_mesh(&layout));
    }
    let [w, h] = geometry
        .extent_m()
        .ok_or_else(|| Error::InvalidInput("extent unknown for this layout".into()))?;
    Ok(target_mesh(&BoardLayout {
        surface: Surface::Plane,
        size_m: [w, h],
        cells: vec![crate::board::Cell {
            rect: [-w / 2.0, -h / 2.0, w / 2.0, h / 2.0],
            dark: false,
        }],
        discs: Vec::new(),
        points: Vec::new(),
    }))
}

/// The mesh of a board layout.
#[must_use]
pub fn target_mesh(layout: &BoardLayout) -> TargetMesh {
    let surface = &layout.surface;
    let mut mesh = TargetMesh::default();
    // On a cylinder every cell is cut at the same arc lines, so neighbours
    // along the axis share their vertices (no T-junctions).
    let step = arc_step(surface);
    let v_lines: Vec<f64> = if step.is_finite() {
        let lo = layout
            .cells
            .iter()
            .map(|c| c.rect[1])
            .fold(f64::INFINITY, f64::min);
        let hi = layout
            .cells
            .iter()
            .map(|c| c.rect[3])
            .fold(f64::NEG_INFINITY, f64::max);
        let n = ((hi - lo) / step).ceil().max(1.0) as usize;
        (0..=n)
            .map(|k| lo + (hi - lo) * k as f64 / n as f64)
            .collect()
    } else {
        Vec::new()
    };
    for cell in &layout.cells {
        let [x0, y0, x1, y1] = cell.rect;
        let mut ys = vec![y0];
        ys.extend(
            v_lines
                .iter()
                .copied()
                .filter(|v| *v > y0 + 1e-12 && *v < y1 - 1e-12),
        );
        ys.push(y1);
        for w in ys.windows(2) {
            let a = mesh.vertex(surface, [x0, w[0]], 0.0);
            let b = mesh.vertex(surface, [x1, w[0]], 0.0);
            let c = mesh.vertex(surface, [x1, w[1]], 0.0);
            let d = mesh.vertex(surface, [x0, w[1]], 0.0);
            mesh.triangle(surface, [a, b, c], cell.dark);
            mesh.triangle(surface, [a, c, d], cell.dark);
        }
    }
    mesh.base_triangles = mesh.triangles.len();
    for disc in &layout.discs {
        disc_mesh(&mut mesh, surface, disc);
    }
    if let Surface::Cylinder { radius_m, .. } = *surface {
        let ends = layout
            .cells
            .iter()
            .fold([f64::INFINITY, f64::NEG_INFINITY], |e, c| {
                [e[0].min(c.rect[0]), e[1].max(c.rect[2])]
            });
        end_caps(&mut mesh, radius_m, ends);
    }
    mesh
}

/// Paper disks closing a cylinder of `radius` at axial `ends`, facing out.
fn end_caps(mesh: &mut TargetMesh, radius: f64, ends: [f64; 2]) {
    let n = segments(radius);
    for (z, nz) in [(ends[0], -1.0), (ends[1], 1.0)] {
        let base = u32::try_from(mesh.positions.len()).expect("a target mesh has < 2³² vertices");
        mesh.positions.push([0.0, 0.0, z]);
        mesh.normals.push([0.0, 0.0, nz]);
        for k in 0..n {
            let a = std::f64::consts::TAU * k as f64 / n as f64;
            mesh.positions.push([radius * a.cos(), radius * a.sin(), z]);
            mesh.normals.push([0.0, 0.0, nz]);
        }
        for k in 0..n as u32 {
            let (a, b) = (base + 1 + k, base + 1 + (k + 1) % n as u32);
            // Counter-clockwise seen from +Z faces +Z; the bottom cap is turned round.
            mesh.triangles
                .push(if nz > 0.0 { [base, a, b] } else { [base, b, a] });
            mesh.dark.push(false);
        }
    }
}

/// A disk as a polar grid, lifted off the surface.
fn disc_mesh(mesh: &mut TargetMesh, surface: &Surface, disc: &Disc) {
    let n = segments(disc.radius);
    // Rings close enough that the disk stays above the cells: a chord of
    // length d sags d² / 8r below a cylinder, which must stay under the lift
    // less the cells' own chord error.
    let rings = match *surface {
        Surface::Plane => 1,
        Surface::Cylinder { radius_m, .. } => {
            let d = (8.0 * (DISC_LIFT_M - CHORD_ERROR_M) * radius_m).sqrt();
            ((disc.radius / d).ceil() as usize).max(1)
        }
    };
    let [cx, cy] = disc.center;
    let centre = mesh.vertex(surface, [cx, cy], DISC_LIFT_M);
    let mut previous: Option<Vec<u32>> = None;
    for ring in 1..=rings {
        let r = disc.radius * ring as f64 / rings as f64;
        let current: Vec<u32> = (0..n)
            .map(|k| {
                let a = std::f64::consts::TAU * k as f64 / n as f64;
                mesh.vertex(surface, [cx + r * a.cos(), cy + r * a.sin()], DISC_LIFT_M)
            })
            .collect();
        for k in 0..n {
            let (k1, k2) = (k, (k + 1) % n);
            match &previous {
                None => mesh.triangle(surface, [centre, current[k1], current[k2]], disc.dark),
                Some(inner) => {
                    mesh.triangle(surface, [inner[k1], current[k1], current[k2]], disc.dark);
                    mesh.triangle(surface, [inner[k1], current[k2], inner[k2]], disc.dark);
                }
            }
        }
        previous = Some(current);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn segment_counts_keep_the_chord_error() {
        for r in [1e-4, 1e-3, 2.5e-3, 1e-2] {
            let n = segments(r) as f64;
            let sag = r * (1.0 - (std::f64::consts::PI / n).cos());
            assert!(sag <= CHORD_ERROR_M * (1.0 + 1e-9), "r {r}: sag {sag}");
        }
        let cyl = Surface::Cylinder {
            radius_m: 0.05,
            circumference_m: 0.05 * std::f64::consts::TAU,
        };
        let step = arc_step(&cyl);
        let sag = 0.05 * (1.0 - (step / 0.05 / 2.0).cos());
        assert!(sag <= CHORD_ERROR_M * (1.0 + 1e-9));
    }
}
