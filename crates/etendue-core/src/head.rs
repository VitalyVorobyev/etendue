//! A laser-triangulation head (ADR 0008 §5, PLAN P10): a physical camera and a line laser fixed to
//! its body, analysed where they meet, in the laser plane.
//!
//! # Coordinates
//!
//! Everything is in the camera's mechanical frame M. The laser plane is parameterised from the
//! laser's origin: `a` along the laser line (laser `+y`), `b` along its central ray (laser `+z`).
//! A head's design *window* is `b ∈ [near, far]`, `|a| ≤ width/2`.
//!
//! The *section* is the plane through the camera's axis perpendicular to the laser plane: the
//! plane the Scheimpflug construction is drawn in. Its 2-D coordinates are `(s, z)`: `z` along the
//! camera axis, `s` across it (camera `+y` for a laser plane that tilts about camera `x`).
//!
//! # What a sample gives
//!
//! At a point of the laser plane:
//! - whether the camera sees it (in front, on the sensor, within the fan);
//! - the geometric blur ([`PhysicalCamera::coc_m`]), the diffraction blur (the Airy diameter at the
//!   working f-number and the laser's wavelength), and their root sum of squares, in pixels;
//! - the size of a pixel along the laser line (lateral resolution), and of a pixel across the
//!   imaged line in depth (`dZ/px`, the depth resolution of triangulation), from the projection's
//!   Jacobian;
//! - the triangulation angle: between the laser's ray and the camera's view of the point.

use nalgebra::{Isometry3, Point2, Point3, Vector2, Vector3};
use serde::Serialize;
use vision_calibration_core::{CameraParams, IntrinsicsParams};

use etendue_scene::{HeadDesign, HeadWindow};

use crate::from_scene::{derive, physical_camera};
use crate::optics::camera::{CardinalPoints, Lens, PhysicalCamera};
use crate::optics::paraxial::{ray_through, sensor_pixel};
use crate::solver::nelder_mead::{self, Options};
use crate::{Error, Result};

/// The line laser of a head.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Beam {
    /// Fan half-angle (radians).
    pub half_angle: f64,
    /// Fan reach along its edge rays (m).
    pub length: f64,
    /// Wavelength (m).
    pub wavelength: f64,
}

/// Where a head must work, in its laser plane (m from the laser's origin).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Window {
    /// Range along the central ray.
    pub near: f64,
    pub far: f64,
    /// Width along the line, centred.
    pub width: f64,
}

/// A head: a physical camera and a line laser in the camera's body frame.
#[derive(Clone, Debug)]
pub struct Head {
    camera: PhysicalCamera,
    resolution: [u32; 2],
    beam: Beam,
    origin: Point3<f64>,
    /// Along the line (laser `+y`).
    line: Vector3<f64>,
    /// Along the central ray (laser `+z`).
    central: Vector3<f64>,
    /// The laser plane's normal (laser `+x`).
    normal: Vector3<f64>,
}

/// What a head does at one point of its laser plane.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Sample {
    /// The camera sees it: in front, on the sensor, within the fan.
    pub visible: bool,
    /// Where it images (px), when in front of the camera.
    pub pixel: Option<[f64; 2]>,
    /// Geometric blur (px).
    pub coc_px: f64,
    /// Diffraction blur: the Airy diameter (px).
    pub airy_px: f64,
    /// Both, root sum of squares (px).
    pub blur_px: f64,
    /// Along the laser line, metres per pixel.
    pub lateral_m_per_px: f64,
    /// Depth (along the central ray) per pixel of stripe displacement across the line.
    pub depth_m_per_px: f64,
    /// Between the laser's ray and the camera's view of the point (rad).
    pub triangulation_rad: f64,
}

/// A 2-D point or direction in the section, `(s, z)`.
pub type P2 = [f64; 2];

/// One traced ray, in the section: object → entrance pupil → exit pupil → sensor (and the image
/// point every ray from the object passes through).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SectionRay {
    /// `chief` or `marginal`.
    pub kind: &'static str,
    pub object: P2,
    pub entrance: P2,
    pub exit: P2,
    pub image: P2,
    pub on_sensor: P2,
}

/// A line in the section: a point on it and a unit direction.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Line2 {
    pub point: P2,
    pub direction: P2,
}

/// Everything the Scheimpflug drawing needs, in the section's `(s, z)` (m).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Section {
    /// The section's `s` axis in the camera's body frame (`z` is the camera axis).
    pub s_axis: [f64; 3],
    /// The lens's points on the axis (`z`).
    pub cardinal_points: CardinalPoints,
    /// Entrance and exit pupil radii.
    pub entrance_radius: f64,
    pub exit_radius: f64,
    /// The sensor where the section cuts it.
    pub sensor: [P2; 2],
    /// The plane of best focus's trace.
    pub focus: Line2,
    /// The laser: its origin, its central ray, and its plane's trace.
    pub laser_origin: P2,
    pub laser_central: P2,
    pub laser_trace: Line2,
    /// The window's range on the laser trace, if a window was given.
    pub window: Option<[P2; 2]>,
    /// Where the sensor's trace crosses the rear principal plane, and the plane of best focus's
    /// the front one: at one height when the Scheimpflug condition holds (the hinge rule).
    pub sensor_at_rear_principal: Option<P2>,
    pub focus_at_front_principal: Option<P2>,
    /// The field: rays from the entrance pupil through the sensor's ends.
    pub field: [Line2; 2],
    /// Chief and marginal rays from each probe point.
    pub rays: Vec<SectionRay>,
    /// Where the blur reaches the limit, near and far of the plane of best focus, along the
    /// section's field (polylines; a far side that never reaches it is left out).
    pub depth_of_field: Option<[Vec<P2>; 2]>,
}

/// A camera setting a head can be built with.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Setting {
    /// Physical sensor tilt `[τx, τy]` (rad, calibration-rs's convention).
    pub tilt: [f64; 2],
    /// Focus distance from the image-plane datum (m).
    pub focus_distance_m: f64,
    /// f-number.
    pub f_number: f64,
}

/// What [`Head::optimise`] found.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Optimum {
    /// The setting it started from (the Scheimpflug condition at the given f-number).
    pub start: Setting,
    pub start_worst_blur_px: f64,
    /// The best setting found.
    pub best: Setting,
    pub worst_blur_px: f64,
    pub iterations: u32,
    pub converged: bool,
}

/// How [`Head::optimise`] searches.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Search {
    /// Samples across and along the window.
    pub grid: [usize; 2],
    /// The f-numbers it may use; `None` keeps the head's.
    pub f_number_range: Option<[f64; 2]>,
    /// Whether the sensor may tilt; without, it stays square to the axis and only focus (and the
    /// f-number) move.
    pub tilt_free: bool,
}

fn p2(v: Vector2<f64>) -> P2 {
    [v.x, v.y]
}

impl From<&HeadWindow> for Window {
    fn from(w: &HeadWindow) -> Self {
        Self {
            near: w.near_m,
            far: w.far_m,
            width: w.width_m,
        }
    }
}

/// `design` with its camera at `setting`: the optics' tilt, focus and f-number changed, and the
/// camera's params derived again.
///
/// # Errors
///
/// [`Error::InvalidInput`] for a camera without optics; as [`PhysicalCamera::new`].
pub fn apply(design: &HeadDesign, setting: &Setting) -> Result<HeadDesign> {
    let mut out = design.clone();
    let optics = out
        .camera
        .optics
        .as_mut()
        .ok_or_else(|| Error::InvalidInput {
            reason: "a head's camera must be physical (have optics)".into(),
        })?;
    optics.tilt_rad = setting.tilt;
    optics.focus_distance_m = setting.focus_distance_m;
    optics.lens.f_number = setting.f_number;
    out.camera.params = derive(optics, &out.camera.params)?.0;
    Ok(out)
}

impl Head {
    /// The head of a library design.
    ///
    /// # Errors
    ///
    /// [`Error::InvalidInput`] for a camera without optics; as [`PhysicalCamera::new`].
    pub fn from_design(design: &HeadDesign) -> Result<Self> {
        let optics = design
            .camera
            .optics
            .as_ref()
            .ok_or_else(|| Error::InvalidInput {
                reason: "a head's camera must be physical (have optics)".into(),
            })?;
        let camera = physical_camera(optics, &design.camera.params)?;
        let beam = Beam {
            half_angle: design.laser.fan_half_angle,
            length: design.fan_length_m,
            wavelength: design.laser.wavelength_nm * 1e-9,
        };
        Ok(Self::new(
            camera,
            design.camera.resolution,
            beam,
            &design.camera_se3_laser,
        ))
    }

    /// A head with `camera` (built at `resolution`), `beam`, and its laser at `mech_se3_laser`.
    #[must_use]
    pub fn new(
        camera: PhysicalCamera,
        resolution: [u32; 2],
        beam: Beam,
        mech_se3_laser: &Isometry3<f64>,
    ) -> Self {
        let r = mech_se3_laser.rotation;
        Self {
            camera,
            resolution,
            beam,
            origin: mech_se3_laser.translation.vector.into(),
            line: r * Vector3::y(),
            central: r * Vector3::z(),
            normal: r * Vector3::x(),
        }
    }

    /// The camera.
    #[must_use]
    pub fn camera(&self) -> &PhysicalCamera {
        &self.camera
    }

    /// The same head with its camera rebuilt at `setting`.
    ///
    /// # Errors
    ///
    /// As [`PhysicalCamera::new`].
    pub fn with_setting(&self, setting: &Setting) -> Result<Self> {
        let c = &self.camera;
        let lens = Lens {
            f_number: setting.f_number,
            ..*c.lens()
        };
        let camera = PhysicalCamera::new(
            lens,
            c.pixel_pitch_m(),
            setting.focus_distance_m,
            setting.tilt,
            &self.as_built(),
        )?;
        Ok(Self {
            camera,
            ..self.clone()
        })
    }

    fn as_built(&self) -> CameraParams {
        let mut p = self.camera.pinhole().clone();
        let IntrinsicsParams::FxFyCxCySkew { params } = &mut p.intrinsics;
        let [cx, cy] = self.camera.principal_point();
        params.cx = cx;
        params.cy = cy;
        p
    }

    /// The point `(a, b)` of the laser plane.
    #[must_use]
    pub fn point(&self, a: f64, b: f64) -> Point3<f64> {
        self.origin + a * self.line + b * self.central
    }

    fn on_sensor(&self, pixel: &Point2<f64>) -> bool {
        let [w, h] = self.resolution.map(f64::from);
        (0.0..=w).contains(&pixel.x) && (0.0..=h).contains(&pixel.y)
    }

    fn in_fan(&self, a: f64, b: f64) -> bool {
        b > 0.0
            && libm::atan2(a, b).abs() <= self.beam.half_angle
            && libm::hypot(a, b) <= self.beam.length
    }

    /// What the head does at `(a, b)` of its laser plane.
    #[must_use]
    pub fn sample(&self, a: f64, b: f64) -> Sample {
        let cam = &self.camera;
        let pitch = cam.pixel_pitch_m();
        let p = self.point(a, b);
        let pixel = cam.project(&p);
        let visible = pixel.is_some_and(|px| self.on_sensor(&px)) && self.in_fan(a, b);
        let coc_px = cam.coc_px(&p).unwrap_or(f64::INFINITY);
        let airy_px = cam.airy_diameter_m(self.beam.wavelength) / pitch;
        let blur_px = libm::hypot(coc_px, airy_px);
        // The projection's Jacobian along the line and along the central ray.
        let h = 1e-5;
        let d = |dir: Vector3<f64>| -> Option<Vector2<f64>> {
            let plus = cam.project(&(p + h * dir))?;
            let minus = cam.project(&(p - h * dir))?;
            Some((plus - minus) / (2.0 * h))
        };
        let (lateral_m_per_px, depth_m_per_px) = match (d(self.line), d(self.central)) {
            (Some(ja), Some(jb)) if ja.norm() > 0.0 => {
                let along = ja / ja.norm();
                let across = Vector2::new(-along.y, along.x);
                (1.0 / ja.norm(), 1.0 / across.dot(&jb).abs())
            }
            _ => (f64::INFINITY, f64::INFINITY),
        };
        let pupil = Point3::new(0.0, 0.0, cam.cardinal_points().entrance_pupil);
        let laser_ray = (p - self.origin).normalize();
        let view = (pupil - p).normalize();
        Sample {
            visible,
            pixel: pixel.map(|px| [px.x, px.y]),
            coc_px,
            airy_px,
            blur_px,
            lateral_m_per_px,
            depth_m_per_px,
            triangulation_rad: libm::acos((-laser_ray).dot(&view).clamp(-1.0, 1.0)),
        }
    }

    /// Samples over `window`: `grid[0]` across (along `a`) by `grid[1]` along the range (`b`), row
    /// by row from `near`, each row from `−width/2`.
    #[must_use]
    pub fn map(&self, window: &Window, grid: [usize; 2]) -> Vec<Sample> {
        let [na, nb] = grid;
        let at = |i: usize, n: usize, lo: f64, hi: f64| {
            if n <= 1 {
                0.5 * (lo + hi)
            } else {
                lo + (hi - lo) * i as f64 / (n - 1) as f64
            }
        };
        let mut out = Vec::with_capacity(na * nb);
        for j in 0..nb {
            let b = at(j, nb, window.near, window.far);
            for i in 0..na {
                out.push(self.sample(at(i, na, -0.5 * window.width, 0.5 * window.width), b));
            }
        }
        out
    }

    /// The camera setting that puts the plane of best focus on the laser plane (the Scheimpflug
    /// condition), at the head's f-number: the focus where the plane meets the camera's axis, and
    /// the sensor parallel to the plane's conjugate.
    ///
    /// # Errors
    ///
    /// [`Error::InvalidInput`] when the laser plane contains the camera's axis or meets it where
    /// the lens cannot focus; propagates the camera model's errors.
    pub fn scheimpflug(&self) -> Result<Setting> {
        let n = self.normal;
        if n.z.abs() < 1e-9 {
            return Err(Error::InvalidInput {
                reason: "the laser plane contains the camera's axis".into(),
            });
        }
        let focus = n.dot(&self.origin.coords) / n.z;
        let untilted = Setting {
            tilt: [0.0, 0.0],
            focus_distance_m: focus,
            f_number: self.camera.lens().f_number,
        };
        let head = self.with_setting(&untilted)?;
        let cam = &head.camera;
        let axis = Point3::new(0.0, 0.0, focus);
        // Two directions in the laser plane, and their images.
        let step = 1e-2 * focus.abs().max(1e-3);
        let u = self.line;
        let v = n.cross(&u);
        let i0 = cam.image_of(&axis)?;
        let iu = cam.image_of(&(axis + step * u))?;
        let iv = cam.image_of(&(axis + step * v))?;
        let mut normal = (iu - i0).cross(&(iv - i0)).normalize();
        if normal.z < 0.0 {
            normal = -normal;
        }
        // The sensor normal in calibration-rs's tilt form: (sin τy, −cos τy sin τx, cos τy cos τx).
        let tilt_y = libm::asin(normal.x.clamp(-1.0, 1.0));
        let tilt_x = libm::atan2(-normal.y, normal.z);
        Ok(Setting {
            tilt: [tilt_x, tilt_y],
            ..untilted
        })
    }

    /// The worst total blur over `window` sampled on `grid`, and how many samples the camera
    /// does not see.
    fn worst_blur(&self, window: &Window, grid: [usize; 2]) -> (f64, usize) {
        let samples = self.map(window, grid);
        let hidden = samples.iter().filter(|s| !s.visible).count();
        let worst = samples
            .iter()
            .filter(|s| s.visible)
            .map(|s| s.blur_px)
            .fold(0.0, f64::max);
        (worst, hidden)
    }

    /// The camera setting (tilt when free, focus, and f-number when free) that minimises the worst
    /// total blur over `window` while the camera sees all of it. It starts from the Scheimpflug
    /// condition (with the sensor square to the axis when it may not tilt) and never ends worse
    /// than that start.
    ///
    /// # Errors
    ///
    /// As [`Head::scheimpflug`].
    pub fn optimise(&self, window: &Window, search: &Search) -> Result<Optimum> {
        let hinge = self.scheimpflug()?;
        let start = if search.tilt_free {
            hinge
        } else {
            Setting {
                tilt: [0.0, 0.0],
                ..hinge
            }
        };
        // The variables: the tilt (when free), the focus, the f-number (when free).
        let setting = |x: &[f64]| -> Setting {
            let (tilt, rest) = if search.tilt_free {
                ([x[0], x[1]], &x[2..])
            } else {
                ([0.0, 0.0], x)
            };
            Setting {
                tilt,
                focus_distance_m: rest[0],
                f_number: rest.get(1).copied().unwrap_or(start.f_number),
            }
        };
        let cost = |x: &[f64]| -> f64 {
            let setting = setting(x);
            if let Some([lo, hi]) = search.f_number_range
                && !(lo..=hi).contains(&setting.f_number)
            {
                return f64::INFINITY;
            }
            if setting.tilt[0].abs() >= 0.5 || setting.tilt[1].abs() >= 0.5 {
                return f64::INFINITY;
            }
            let Ok(head) = self.with_setting(&setting) else {
                return f64::INFINITY;
            };
            let (worst, hidden) = head.worst_blur(window, search.grid);
            // Anything out of view outweighs any blur.
            if hidden > 0 {
                1e6 + hidden as f64
            } else {
                worst
            }
        };
        let mut x = Vec::new();
        let mut steps = Vec::new();
        if search.tilt_free {
            x.extend(start.tilt);
            steps.extend([0.01, 0.01]);
        }
        x.push(start.focus_distance_m);
        steps.push(0.02 * start.focus_distance_m);
        if search.f_number_range.is_some() {
            x.push(start.f_number);
            steps.push(0.2 * start.f_number);
        }
        let start_cost = cost(&x);
        let (mut best, mut best_cost) = (x.clone(), start_cost);
        let mut iterations = 0;
        let mut converged = false;
        for _ in 0..8 {
            let mut simplex = vec![best.clone()];
            for (k, step) in steps.iter().enumerate() {
                let mut v = best.clone();
                v[k] += step;
                simplex.push(v);
            }
            let found = nelder_mead::minimise(
                cost,
                simplex,
                Options {
                    cost_tolerance: 1e-9,
                    max_iterations: 400,
                },
            );
            iterations += found.iterations;
            converged = found.converged;
            let improved = found.cost < best_cost - 1e-9 * best_cost.max(1.0);
            if found.cost <= best_cost {
                best = found.point;
                best_cost = found.cost;
            }
            if !improved {
                break;
            }
        }
        Ok(Optimum {
            start,
            start_worst_blur_px: start_cost,
            best: setting(&best),
            worst_blur_px: best_cost,
            iterations,
            converged,
        })
    }

    /// The section's `s` axis: across the camera axis, in the plane perpendicular to the laser
    /// plane (camera `+y` when the laser plane is perpendicular to the camera's `y–z` plane).
    fn s_axis(&self) -> Vector3<f64> {
        let normal_of_section = Vector3::z().cross(&self.normal);
        let t = if normal_of_section.norm() < 1e-9 {
            Vector3::x()
        } else {
            normal_of_section.normalize()
        };
        Vector3::z().cross(&t)
    }

    /// The Scheimpflug drawing: see [`Section`]. Rays are traced from the laser's central ray at
    /// ranges `probes` (m). With `blur_limit_px`, the depth of field for that blur is drawn.
    ///
    /// # Errors
    ///
    /// Propagates the camera model's errors (a probe inside the front focal plane, say).
    pub fn section(
        &self,
        window: Option<&Window>,
        probes: &[f64],
        blur_limit_px: Option<f64>,
    ) -> Result<Section> {
        let cam = &self.camera;
        let s = self.s_axis();
        let t = s.cross(&Vector3::z()); // the section's normal
        let to2 = |p: &Point3<f64>| -> P2 { [p.coords.dot(&s), p.z] };
        let dir2 = |v: &Vector3<f64>| -> P2 { p2(Vector2::new(v.dot(&s), v.z).normalize()) };
        // A plane (point, normal) where the section cuts it.
        let trace = |point: &Point3<f64>, normal: &Vector3<f64>| -> Line2 {
            let n2 = Vector2::new(normal.dot(&s), normal.z);
            let c = normal.dot(&point.coords);
            let on = n2 * (c / n2.norm_squared());
            Line2 {
                point: p2(on),
                direction: p2(Vector2::new(-n2.y, n2.x).normalize()),
            }
        };
        let cp = *cam.cardinal_points();
        let plane = cam.sensor_plane();
        // The sensor segment: where the section's line on the sensor stays within the image.
        let along = plane.normal.cross(&t).normalize();
        let pitch = cam.pixel_pitch_m();
        let [cx, cy] = cam.principal_point();
        let [w, h] = self.resolution.map(f64::from);
        let mut range = (f64::NEG_INFINITY, f64::INFINITY);
        for (axis, c, extent) in [(plane.x_axis, cx, w), (plane.y_axis, cy, h)] {
            // pixel = c − (t·along·axis)/pitch ∈ [0, extent]
            let k = along.dot(&axis) / pitch;
            if k.abs() > 1e-15 {
                let (t0, t1) = ((c - extent) / k, c / k);
                range = (range.0.max(t0.min(t1)), range.1.min(t0.max(t1)));
            }
        }
        let sensor = [
            to2(&Point3::from(range.0 * along)),
            to2(&Point3::from(range.1 * along)),
        ];
        let (focus_point, focus_normal) = cam.plane_of_best_focus()?;
        let focus = trace(&focus_point, &focus_normal);
        let laser_trace = trace(&self.origin, &self.normal);
        let line_at = |line: &Line2, z: f64| -> Option<P2> {
            (line.direction[1].abs() > 1e-12).then(|| {
                let k = (z - line.point[1]) / line.direction[1];
                [line.point[0] + k * line.direction[0], z]
            })
        };
        let sensor_line = Line2 {
            point: sensor[0],
            direction: p2((Vector2::from(sensor[1]) - Vector2::from(sensor[0])).normalize()),
        };
        let field_ray = |end: &P2| -> Line2 {
            let [u, v] = [end[0], end[1]];
            let ends = Point3::from(u * s + v * Vector3::z());
            let pixel = sensor_pixel(cam, &ends);
            let (origin, d) = cam.backproject(&pixel);
            Line2 {
                point: to2(&origin),
                direction: dir2(&d),
            }
        };
        let field = [field_ray(&sensor[0]), field_ray(&sensor[1])];
        let r = cam.entrance_pupil_diameter_m() / 2.0;
        let mut rays = Vec::new();
        for &b in probes {
            let object = self.point(0.0, b);
            for (kind, k) in [("chief", 0.0), ("marginal", 1.0), ("marginal", -1.0)] {
                let offset = k * r * s;
                let ray = ray_through(cam, &object, [offset.x, offset.y])?;
                rays.push(SectionRay {
                    kind,
                    object: to2(&ray.object),
                    entrance: to2(&ray.entrance),
                    exit: to2(&ray.exit),
                    image: to2(&ray.image),
                    on_sensor: to2(&ray.on_sensor),
                });
            }
        }
        let depth_of_field =
            blur_limit_px.map(|limit| self.depth_of_field(&field, limit, &to2, &s));
        Ok(Section {
            s_axis: [s.x, s.y, s.z],
            cardinal_points: cp,
            entrance_radius: r,
            exit_radius: cam.exit_pupil_diameter_m() / 2.0,
            sensor,
            focus,
            laser_origin: to2(&self.origin),
            laser_central: dir2(&self.central),
            laser_trace,
            window: window.map(|w| [to2(&self.point(0.0, w.near)), to2(&self.point(0.0, w.far))]),
            sensor_at_rear_principal: line_at(&sensor_line, cp.rear_principal),
            focus_at_front_principal: line_at(&focus, cp.front_principal),
            field,
            rays,
            depth_of_field,
        })
    }

    /// Near and far polylines where the blur reaches `limit_px`, along chief rays spread over the
    /// section's field.
    fn depth_of_field(
        &self,
        field: &[Line2; 2],
        limit_px: f64,
        to2: &dyn Fn(&Point3<f64>) -> P2,
        s: &Vector3<f64>,
    ) -> [Vec<P2>; 2] {
        let cam = &self.camera;
        let pupil = Point3::new(0.0, 0.0, cam.cardinal_points().entrance_pupil);
        let (focus_point, focus_normal) = match cam.plane_of_best_focus() {
            Ok(f) => f,
            Err(_) => return [Vec::new(), Vec::new()],
        };
        let airy_px = cam.airy_diameter_m(self.beam.wavelength) / cam.pixel_pitch_m();
        let blur = |p: &Point3<f64>| {
            cam.coc_px(p)
                .map(|c| libm::hypot(c, airy_px))
                .unwrap_or(f64::INFINITY)
        };
        let (mut near, mut far) = (Vec::new(), Vec::new());
        let dirs = field.map(|l| Vector2::from(l.direction));
        for i in 0..=40 {
            let d2 = dirs[0] + (dirs[1] - dirs[0]) * (f64::from(i) / 40.0);
            let d = (d2.x * s + d2.y * Vector3::z()).normalize();
            // Where this chief ray meets the plane of best focus.
            let denom = focus_normal.dot(&d);
            if denom.abs() < 1e-12 {
                continue;
            }
            let t_focus = focus_normal.dot(&(focus_point - pupil)) / denom;
            if t_focus <= 0.0 {
                continue;
            }
            let at = |t: f64| pupil + t * d;
            let bisect = |mut lo: f64, mut hi: f64| -> f64 {
                // blur(lo) and blur(hi) straddle the limit.
                for _ in 0..80 {
                    let mid = 0.5 * (lo + hi);
                    if (blur(&at(mid)) > limit_px) == (blur(&at(lo)) > limit_px) {
                        lo = mid
                    } else {
                        hi = mid
                    }
                }
                0.5 * (lo + hi)
            };
            if blur(&at(t_focus)) >= limit_px {
                continue;
            }
            // Near: between the front focal plane and the focus.
            let front = cam.cardinal_points().front_focal;
            let t_front = ((front - pupil.z) / d.z).max(0.0) * (1.0 + 1e-9) + 1e-9;
            if t_front < t_focus && blur(&at(t_front)) > limit_px {
                near.push(to2(&at(bisect(t_front, t_focus))));
            }
            // Far: beyond the focus, out to 100 times its distance.
            let t_far = 100.0 * t_focus;
            if blur(&at(t_far)) > limit_px {
                far.push(to2(&at(bisect(t_focus, t_far))));
            }
        }
        [near, far]
    }
}
