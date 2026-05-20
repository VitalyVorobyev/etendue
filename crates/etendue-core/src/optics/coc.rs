//! Thick-lens conjugates, the Scheimpflug plane of best focus, and the
//! geometric circle of confusion — derived from first principles.
//!
//! This module is the **physics core** of the M4 defocus model. Everything
//! here is a pure `f64` function with no UI, no rendering, and no `wgpu`
//! dependency. [`ThickLens`](crate::optics::ThickLens) wraps these functions
//! with a camera-aware API; the [`analysis`](crate::analysis) layer samples
//! them over a target.
//!
//! The full written derivation, with the hinge-rule discussion and the worked
//! test numbers, lives in `docs/derivations/scheimpflug_pobf.md`. This
//! doc-comment is the self-contained companion.
//!
//! # The model in one paragraph
//!
//! An **ideal** (aberration-free, paraxial) thick lens. It is characterised by
//! an effective focal length `f`, an f-number `N` (aperture diameter
//! `D = f / N`), an object-side focus distance `s_o`, and the inter-principal
//! gap `g = H - H'` between the front (`H`) and rear (`H'`) principal planes.
//! The sensor sits at the image-side conjugate of the focus plane and may be
//! **tilted** (Scheimpflug) by `(tau_x, tau_y)`. An object point off the plane
//! of best focus images to a blur disc — the circle of confusion (CoC) —
//! whose diameter this module computes.
//!
//! # Coordinates and the reference-plane convention
//!
//! Camera-local, right-handed, optical axis along `+z` pointing **toward the
//! object**; the object half-space is `z > 0`, the sensor lives at `z < 0`.
//!
//! The camera-local origin `z = 0` is the **rear principal plane `H'`**. The
//! front principal plane `H` is a distance `g` toward the object, at `z = +g`.
//! An object point at camera depth `z` is therefore at object distance
//! `d_H = z + g` from `H`. The Gaussian conjugate relation is written with the
//! object distance measured from `H` and the image distance from `H'`.
//!
//! This convention is the one modelling choice that makes the inter-principal
//! gap `g` *physically load-bearing*: camera-frame depth enters the model only
//! through `z + g`, so a non-zero `g` shifts focus, and `g -> 0` collapses the
//! thick lens onto the thin lens exactly. See [`image_distance`] and the
//! `docs/` derivation, Sections 1 and 6.
//!
//! # Key results (proven in the derivation note)
//!
//! - **Conjugate relation** ([`image_distance`]): with object distance `d_H`
//!   from `H` and image distance `d_i` from `H'`,
//!   `1/d_H + 1/d_i = 1/f`, hence `d_i = f * d_H / (d_H - f)`. Identical in
//!   form to the thin lens; the lens thickness is absorbed into `g`.
//! - **Tilted plane of best focus** ([`pobf_tilt_tan`], [`PlaneOfBestFocus`]):
//!   tilting the sensor by `tau` tilts the object-side in-focus plane by
//!   `theta`, with `tan(theta) = (s_o + g)/v0 * tan(tau)` — the object tilt is
//!   the sensor tilt scaled by the inverse magnification. The lens plane, the
//!   sensor plane, and the PoBF meet in one common line (the Scheimpflug
//!   line).
//! - **Geometric CoC** ([`coc_diameter_at_sensor`]): an object point at depth
//!   `Z` images at `s_i_P` from `H'`; the tilted sensor is at axial distance
//!   `v_sensor` at that image point; the converging cone, intercepted off
//!   focus, is a disc of diameter `c = D * |s_i_P - v_sensor| / s_i_P`.
//!
//! # Honest scope
//!
//! The lens is ideal. There is **no** Seidel/Zernike aberration term and **no**
//! field curvature. The direct consequence: with a fronto-parallel sensor the
//! CoC of a point depends on its axial defocus *only*, never on its lateral
//! field position. Field/lateral dependence of the CoC enters this model
//! *exclusively* through Scheimpflug tilt (a point's signed offset from the
//! *tilted* PoBF depends on its lateral position). No aberration-driven field
//! dependence is manufactured.
//!
//! The tilted-sensor CoC ([`coc_diameter_at_sensor`]) evaluates the sensor
//! surface at the paraxial image height; this is exact for the zero-CoC locus
//! (the PoBF) and first-order in `tan(tau)` for the *magnitude* of a nonzero
//! blur. See the derivation note, Section 5.

use nalgebra::{Point3, Vector3};

/// A negligible-defocus threshold for axial distances, in metres.
///
/// Distances closer than this are treated as "in focus" for the divide-by
/// guards below. It is far smaller than any pixel pitch, so it never affects
/// a CoC a heatmap would resolve.
const AXIAL_EPS_M: f64 = 1e-12;

/// The image-side conjugate distance of an object, for a thick lens.
///
/// Given an **object distance `d_h` measured from the front principal plane
/// `H`** and an effective focal length `f`, returns the **image distance
/// measured from the rear principal plane `H'`**:
///
/// ```text
/// 1/d_h + 1/d_i = 1/f      =>      d_i = f * d_h / (d_h - f)
/// ```
///
/// This is the Gaussian conjugate relation. Introducing the two principal
/// planes is defined so that the relation keeps its thin-lens algebraic form;
/// the entire effect of finite lens thickness is the separation of `H` and
/// `H'`, handled by the caller's `z + g` depth convention (see the module
/// doc-comment). With the principal planes coincident (`g = 0`) this is
/// exactly the thin-lens equation.
///
/// # Errors
///
/// Returns [`Error::InvalidInput`](crate::Error::InvalidInput) if `f` is not
/// finite and strictly positive, if `d_h` is not finite, or if
/// `d_h <= f` — an object at or inside the front focal point forms no real
/// image (the conjugate would be virtual or at infinity), which the geometric
/// CoC model does not cover.
pub fn image_distance(d_h: f64, f: f64) -> crate::Result<f64> {
    if !(f.is_finite() && f > 0.0) {
        return Err(crate::Error::InvalidInput {
            reason: format!("focal length must be finite and positive, got {f}"),
        });
    }
    if !d_h.is_finite() {
        return Err(crate::Error::InvalidInput {
            reason: format!("object distance from H must be finite, got {d_h}"),
        });
    }
    if d_h <= f {
        return Err(crate::Error::InvalidInput {
            reason: format!(
                "object distance from H ({d_h} m) must exceed the focal length \
                 ({f} m) to form a real image"
            ),
        });
    }
    Ok(f * d_h / (d_h - f))
}

/// The on-axis sensor distance: where the sensor must sit to focus `s_o`.
///
/// The design focus distance `s_o` is the object-side focus distance measured
/// from the rear principal plane `H'`; the in-focus object plane is thus at
/// distance `s_o + g` from `H`. This returns the conjugate image distance from
/// `H'` — the axial position of the (on-axis point of the) sensor:
///
/// ```text
/// v0 = image_distance(s_o + g, f)
/// ```
///
/// # Errors
///
/// Propagates [`image_distance`]'s errors, plus
/// [`Error::InvalidInput`](crate::Error::InvalidInput) if `s_o` or `g` is not
/// finite, or `g` is negative (the front principal plane cannot sit behind the
/// rear one for an ordinary lens).
pub fn sensor_distance(s_o: f64, g: f64, f: f64) -> crate::Result<f64> {
    if !s_o.is_finite() {
        return Err(crate::Error::InvalidInput {
            reason: format!("focus distance must be finite, got {s_o}"),
        });
    }
    if !(g.is_finite() && g >= 0.0) {
        return Err(crate::Error::InvalidInput {
            reason: format!("inter-principal gap must be finite and non-negative, got {g}"),
        });
    }
    image_distance(s_o + g, f)
}

/// The tangent of the object-side plane-of-best-focus tilt for one axis.
///
/// When the sensor is tilted by `tau` (radians) about a sensor-plane axis, the
/// object-side in-focus plane tilts by `theta` about the corresponding
/// camera-frame axis, with
///
/// ```text
/// tan(theta) = (s_o + g) / v0 * tan(tau)
/// ```
///
/// where `v0 = sensor_distance(s_o, g, f)`. The object-plane tilt is the
/// sensor tilt scaled by the inverse on-axis magnification `(s_o + g)/v0`; for
/// the usual `s_o + g > v0` regime the in-focus plane swings *more* than the
/// sensor. Derivation note, equations (11)–(14).
///
/// `tau` is taken as an arbitrary finite angle; the caller is responsible for
/// keeping it in a physically sensible range. With `tau = 0` the result is
/// `0` — a fronto-parallel sensor has a fronto-parallel PoBF.
///
/// # Errors
///
/// Propagates [`sensor_distance`]'s errors;
/// [`Error::InvalidInput`](crate::Error::InvalidInput) if `tau` is not finite.
pub fn pobf_tilt_tan(tau: f64, s_o: f64, g: f64, f: f64) -> crate::Result<f64> {
    if !tau.is_finite() {
        return Err(crate::Error::InvalidInput {
            reason: format!("sensor tilt must be finite, got {tau}"),
        });
    }
    let v0 = sensor_distance(s_o, g, f)?;
    let inv_mag = (s_o + g) / v0;
    Ok(inv_mag * tau.tan())
}

/// The object-side plane of best focus (PoBF) of a (possibly tilted) sensor.
///
/// The PoBF is the set of object points that image sharply onto the sensor.
/// For a fronto-parallel sensor it is the fronto-parallel plane at depth
/// `s_o`. For a Scheimpflug-tilted sensor it is a **tilted plane** through the
/// on-axis focus point `(0, 0, s_o)`, with the depth varying linearly with
/// lateral position:
///
/// ```text
/// z_pobf(X, Y) = s_o + X * tan(theta_y) + Y * tan(theta_x)
/// ```
///
/// A sensor tilt about `x` makes the in-focus depth vary with the lateral `Y`
/// coordinate; a tilt about `y` makes it vary with `X`. See
/// [`PlaneOfBestFocus::depth_at`] and [`PlaneOfBestFocus::signed_axial_offset`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlaneOfBestFocus {
    /// On-axis focus depth `s_o` (camera-frame `+z`, metres from `H'`).
    pub focus_depth: f64,
    /// `tan(theta_x)`: depth gradient of the PoBF along the lateral `Y` axis,
    /// induced by the sensor's `tau_x` tilt.
    pub tan_tilt_x: f64,
    /// `tan(theta_y)`: depth gradient of the PoBF along the lateral `X` axis,
    /// induced by the sensor's `tau_y` tilt.
    pub tan_tilt_y: f64,
}

impl PlaneOfBestFocus {
    /// Construct the PoBF from the lens parameters and sensor tilt.
    ///
    /// `tau_x`, `tau_y` are the sensor tilt angles (radians); `s_o` the focus
    /// distance from `H'`; `g` the inter-principal gap; `f` the focal length.
    ///
    /// # Errors
    ///
    /// Propagates [`pobf_tilt_tan`]'s errors.
    pub fn new(tau_x: f64, tau_y: f64, s_o: f64, g: f64, f: f64) -> crate::Result<Self> {
        Ok(Self {
            focus_depth: s_o,
            tan_tilt_x: pobf_tilt_tan(tau_x, s_o, g, f)?,
            tan_tilt_y: pobf_tilt_tan(tau_y, s_o, g, f)?,
        })
    }

    /// The in-focus depth of the PoBF at lateral position `(x, y)`:
    /// `s_o + x * tan(theta_y) + y * tan(theta_x)`.
    #[must_use]
    pub fn depth_at(&self, x: f64, y: f64) -> f64 {
        self.focus_depth + x * self.tan_tilt_y + y * self.tan_tilt_x
    }

    /// The signed axial (depth) offset of a camera-frame point from the PoBF.
    ///
    /// Positive when the point is *beyond* (further from the camera than) the
    /// PoBF at its lateral position; negative when nearer; zero on the PoBF.
    /// This is a depth difference along `+z`, not a perpendicular distance —
    /// the geometric CoC is governed by longitudinal defocus.
    #[must_use]
    pub fn signed_axial_offset(&self, point: &Point3<f64>) -> f64 {
        point.z - self.depth_at(point.x, point.y)
    }

    /// The unit normal of the PoBF in the camera frame.
    ///
    /// The plane `z = s_o + x * tan(theta_y) + y * tan(theta_x)` has normal
    /// proportional to `(-tan(theta_y), -tan(theta_x), 1)`. For a
    /// fronto-parallel sensor this is `+z`. Useful for rendering the in-focus
    /// plane in M4b.
    #[must_use]
    pub fn normal(&self) -> Vector3<f64> {
        Vector3::new(-self.tan_tilt_y, -self.tan_tilt_x, 1.0).normalize()
    }
}

/// The geometric circle-of-confusion diameter at the sensor, in metres.
///
/// This is the central physics routine. It computes the blur-disc diameter for
/// an object point `point` (camera-frame, metres, `+z` toward the object),
/// given the lens parameters and sensor tilt.
///
/// # Method (derivation note, Section 5)
///
/// 1. The point at camera depth `Z = point.z` is at object distance `Z + g`
///    from `H` and images at `s_i_P = image_distance(Z + g, f)` from `H'`,
///    with magnification `m_P = -s_i_P / (Z + g)`.
/// 2. Its paraxial image lands at image height `(m_P * X, m_P * Y)`.
/// 3. The tilted sensor's axial distance from `H'` at that image height is
///    `v_sensor = v0 + x_img * tan(tau_y) + y_img * tan(tau_x)`.
/// 4. The converging cone, of aperture `D = f / N`, intercepted a longitudinal
///    distance `|s_i_P - v_sensor|` off its focus, is a disc of diameter
///    `c = D * |s_i_P - v_sensor| / s_i_P`.
///
/// On the plane of best focus `s_i_P == v_sensor` and the result is `0`. With
/// `tau_x = tau_y = 0` the result depends on `point.z` alone — there is no
/// off-axis field term, as befits an ideal lens.
///
/// # Parameters
///
/// - `point`: object point in the camera frame (metres). Must have
///   `point.z > 0` (in front of the camera) — strictly, `point.z + g > f`.
/// - `f`: effective focal length (m).
/// - `n_fnum`: f-number (dimensionless); aperture diameter is `f / n_fnum`.
/// - `s_o`: object-side focus distance from `H'` (m).
/// - `g`: inter-principal gap `H - H'` (m), `>= 0`.
/// - `tau_x`, `tau_y`: sensor tilt about the camera `x` / `y` axes (radians).
///
/// # Errors
///
/// Returns [`Error::InvalidInput`](crate::Error::InvalidInput) if `n_fnum` is
/// not finite and strictly positive, if any tilt is not finite, or if the
/// object point or focus geometry fails [`image_distance`] /
/// [`sensor_distance`] (non-finite coordinate, point inside the front focal
/// point, etc.).
#[allow(clippy::too_many_arguments)]
pub fn coc_diameter_at_sensor(
    point: &Point3<f64>,
    f: f64,
    n_fnum: f64,
    s_o: f64,
    g: f64,
    tau_x: f64,
    tau_y: f64,
) -> crate::Result<f64> {
    if !(n_fnum.is_finite() && n_fnum > 0.0) {
        return Err(crate::Error::InvalidInput {
            reason: format!("f-number must be finite and positive, got {n_fnum}"),
        });
    }
    if !(tau_x.is_finite() && tau_y.is_finite()) {
        return Err(crate::Error::InvalidInput {
            reason: format!("sensor tilts must be finite, got ({tau_x}, {tau_y})"),
        });
    }
    if !(point.x.is_finite() && point.y.is_finite() && point.z.is_finite()) {
        return Err(crate::Error::InvalidInput {
            reason: format!(
                "object point must be finite, got ({}, {}, {})",
                point.x, point.y, point.z
            ),
        });
    }

    let aperture = f / n_fnum; // D = f / N

    // Object distance of the point from the front principal plane H.
    let d_h_point = point.z + g;
    // Image distance of the point from H'.  Rejects point at/inside the front
    // focal point (no real image) and bad f / g.
    let s_i_point = image_distance(d_h_point, f)?;
    // On-axis sensor distance from H'.
    let v0 = sensor_distance(s_o, g, f)?;

    // Paraxial magnification of the point's conjugate pair, and the image
    // height its paraxial image lands at.
    let m_point = -s_i_point / d_h_point;
    let x_img = m_point * point.x;
    let y_img = m_point * point.y;

    // Axial distance of the *tilted* sensor surface from H', at that image
    // height.  tau_x tilts the sensor with image-y; tau_y with image-x.
    let v_sensor = v0 + x_img * tau_y.tan() + y_img * tau_x.tan();

    // Similar-triangles cone: blur-disc diameter at the sensor.  s_i_point is
    // strictly positive for a real image, so the divide is guarded only
    // against an absurdly degenerate geometry.
    if s_i_point.abs() < AXIAL_EPS_M {
        return Err(crate::Error::Numerical(format!(
            "degenerate image distance {s_i_point} m for object point at depth {}",
            point.z
        )));
    }
    Ok(aperture * (s_i_point - v_sensor).abs() / s_i_point)
}

/// Convert a circle-of-confusion diameter from metres to pixels.
///
/// `coc_m` is a CoC diameter at the sensor (e.g. from
/// [`coc_diameter_at_sensor`]); `pixel_pitch_m` is the sensor's pixel pitch
/// (centre-to-centre pixel spacing, metres). The blur in pixels is simply
/// `coc_m / pixel_pitch_m`.
///
/// # Errors
///
/// Returns [`Error::InvalidInput`](crate::Error::InvalidInput) if
/// `pixel_pitch_m` is not finite and strictly positive, or if `coc_m` is not
/// finite.
pub fn coc_to_pixels(coc_m: f64, pixel_pitch_m: f64) -> crate::Result<f64> {
    if !(pixel_pitch_m.is_finite() && pixel_pitch_m > 0.0) {
        return Err(crate::Error::InvalidInput {
            reason: format!("pixel pitch must be finite and positive, got {pixel_pitch_m}"),
        });
    }
    if !coc_m.is_finite() {
        return Err(crate::Error::InvalidInput {
            reason: format!("circle-of-confusion diameter must be finite, got {coc_m}"),
        });
    }
    Ok(coc_m / pixel_pitch_m)
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;

    // -----------------------------------------------------------------------
    // A realistic machine-vision lens, reused across the tests.  All hand
    // calculations below use exactly these numbers; see
    // `docs/derivations/scheimpflug_pobf.md`, Section 7.
    //
    //   f   = 25 mm,  N = 4,  s_o = 0.30 m,  pixel pitch p = 3.45 um.
    //   D   = f / N  = 6.25 mm.
    // -----------------------------------------------------------------------
    const F: f64 = 0.025;
    const N: f64 = 4.0;
    const S_O: f64 = 0.30;
    const PIX: f64 = 3.45e-6;

    #[test]
    fn conjugate_relation_satisfies_the_lens_equation() {
        // 1/d_h + 1/d_i = 1/f must hold for the value image_distance returns.
        let d_h = 0.5;
        let d_i = image_distance(d_h, F).unwrap();
        assert_relative_eq!(1.0 / d_h + 1.0 / d_i, 1.0 / F, epsilon = 1e-12);
    }

    #[test]
    fn sensor_distance_matches_the_hand_computed_v0() {
        // Thin lens (g = 0): v0 = f s_o / (s_o - f)
        //                       = 0.025 * 0.30 / 0.275 = 0.0272727272... m.
        let v0 = sensor_distance(S_O, 0.0, F).unwrap();
        assert_relative_eq!(v0, 0.025 * 0.30 / 0.275, epsilon = 1e-15);
        assert_relative_eq!(v0, 0.027_272_727_272_727_27, epsilon = 1e-15);
    }

    #[test]
    fn image_distance_rejects_object_inside_the_focal_point() {
        // An object at exactly f, or inside it, forms no real image.
        assert!(image_distance(F, F).is_err());
        assert!(image_distance(0.5 * F, F).is_err());
        // Just outside the focal point is fine.
        assert!(image_distance(F * 1.001, F).is_ok());
    }

    // === KILL-GATE REGIME (a): tau = 0, point on the focus plane => CoC = 0 ==
    #[test]
    fn regime_a_on_focus_point_has_zero_coc() {
        // A point exactly on the (fronto-parallel) focus plane: depth = s_o.
        let p = Point3::new(0.0, 0.0, S_O);
        let c = coc_diameter_at_sensor(&p, F, N, S_O, 0.0, 0.0, 0.0).unwrap();
        assert_relative_eq!(c, 0.0, epsilon = 1e-15);

        // And it is sharp regardless of lateral field position — an ideal
        // lens has no off-axis blur term.
        let p_off = Point3::new(0.12, -0.07, S_O);
        let c_off = coc_diameter_at_sensor(&p_off, F, N, S_O, 0.0, 0.0, 0.0).unwrap();
        assert_relative_eq!(c_off, 0.0, epsilon = 1e-15);
    }

    // === KILL-GATE REGIME (b): tau = 0, known defocus => textbook CoC ========
    #[test]
    fn regime_b_defocused_point_matches_hand_computed_coc() {
        // Object point 30 mm beyond focus: depth z = 0.33 m, thin lens (g=0).
        //
        // Hand calculation (derivation note, Section 7b):
        //   v0       = f s_o / (s_o - f) = 0.0075 / 0.275  = 0.0272727272... m
        //   s_i(z)   = f z   / (z   - f) = 0.00825 / 0.305 = 0.0270491803... m
        //   D        = f / N = 0.00625 m
        //   c        = D * |s_i - v0| / s_i
        //            = 0.00625 * 0.00022354695 / 0.0270491803
        //            = 5.16528925619e-5 m   (51.6529 um)
        //   c_px     = c / p = 5.16528925619e-5 / 3.45e-6 = 14.97185291652 px
        let p = Point3::new(0.0, 0.0, 0.33);
        let c = coc_diameter_at_sensor(&p, F, N, S_O, 0.0, 0.0, 0.0).unwrap();
        assert_relative_eq!(c, 5.165_289_256_198_392e-5, epsilon = 1e-15);

        // Independent textbook closed form for the thin-lens CoC:
        //   c = D f |z - s_o| / ( z (s_o - f) ).
        let z = 0.33;
        let d = F / N;
        let c_textbook = d * F * (z - S_O).abs() / (z * (S_O - F));
        assert_relative_eq!(c, c_textbook, epsilon = 1e-14);

        // CoC in pixels.
        let c_px = coc_to_pixels(c, PIX).unwrap();
        assert_relative_eq!(c_px, 14.971_852_916_517_08, epsilon = 1e-11);

        // A point defocused the *other* way (nearer than focus) also blurs.
        let p_near = Point3::new(0.0, 0.0, 0.27);
        let c_near = coc_diameter_at_sensor(&p_near, F, N, S_O, 0.0, 0.0, 0.0).unwrap();
        assert!(c_near > 0.0);

        // And — the ideal-lens signature — the CoC at a fixed depth does NOT
        // change with lateral field position when the sensor is untilted.
        let c_far_axis = coc_diameter_at_sensor(&p, F, N, S_O, 0.0, 0.0, 0.0).unwrap();
        let c_far_off =
            coc_diameter_at_sensor(&Point3::new(0.10, 0.08, 0.33), F, N, S_O, 0.0, 0.0, 0.0)
                .unwrap();
        assert_relative_eq!(c_far_axis, c_far_off, epsilon = 1e-15);
    }

    // === KILL-GATE REGIME (c): tau != 0 => correctly tilted PoBF =============
    #[test]
    fn regime_c_pobf_tilt_matches_the_scheimpflug_relation() {
        // Sensor tilt tau_x = 5 deg, thin lens (g = 0).
        //   tan(theta_x) = (s_o / v0) * tan(tau_x)
        //                = 11.0 * tan(5 deg)
        //                = 11.0 * 0.0874886635 = 0.9623752988
        //   theta_x      = atan(0.9623752988) = 43.9016 deg.
        let tau_x = 5.0_f64.to_radians();
        let tan_theta = pobf_tilt_tan(tau_x, S_O, 0.0, F).unwrap();
        let v0 = sensor_distance(S_O, 0.0, F).unwrap();
        assert_relative_eq!(tan_theta, (S_O / v0) * tau_x.tan(), epsilon = 1e-14);
        assert_relative_eq!(tan_theta, 0.962_375_298_785_163_8, epsilon = 1e-12);
    }

    #[test]
    fn regime_c_point_on_the_tilted_pobf_has_zero_coc() {
        // Sensor tilt tau_x = 5 deg.  Build the tilted PoBF and verify that a
        // point lying on it has zero CoC, while a point off it does not.
        let tau_x = 5.0_f64.to_radians();
        let pobf = PlaneOfBestFocus::new(tau_x, 0.0, S_O, 0.0, F).unwrap();

        // On-axis: still on the PoBF (it pivots about the on-axis focus pt).
        let c_axis =
            coc_diameter_at_sensor(&Point3::new(0.0, 0.0, S_O), F, N, S_O, 0.0, tau_x, 0.0)
                .unwrap();
        assert_relative_eq!(c_axis, 0.0, epsilon = 1e-12);

        // A point ON the tilted PoBF at lateral height Y = 0.05 m:
        //   z_pobf = s_o + Y * tan(theta_x)
        //          = 0.30 + 0.05 * 0.9623752988 = 0.348118765 m.
        let y = 0.05;
        let z_on = pobf.depth_at(0.0, y);
        assert_relative_eq!(z_on, 0.348_118_764_939_258_2, epsilon = 1e-12);
        let c_on =
            coc_diameter_at_sensor(&Point3::new(0.0, y, z_on), F, N, S_O, 0.0, tau_x, 0.0).unwrap();
        // Exactly zero up to floating-point round-off (the PoBF derivation is
        // algebraically exact — see the derivation note, Section 5).
        assert_relative_eq!(c_on, 0.0, epsilon = 1e-12);

        // A point OFF the PoBF — on the *old* fronto-parallel focus plane,
        // i.e. depth s_o at the same Y — is now blurred.
        let c_off =
            coc_diameter_at_sensor(&Point3::new(0.0, y, S_O), F, N, S_O, 0.0, tau_x, 0.0).unwrap();
        assert!(
            c_off > 1e-6,
            "a point off the tilted PoBF must blur, got {c_off} m"
        );
    }

    #[test]
    fn regime_c_increasing_tilt_rotates_the_zero_coc_locus() {
        // The zero-CoC locus is the tilted PoBF.  As tau_x grows, tan(theta_x)
        // grows monotonically and with the correct sign: at a fixed positive
        // lateral height Y the in-focus depth moves further from the camera.
        let y = 0.05;
        let mut last_depth = f64::NEG_INFINITY;
        for tau_deg in [0.0_f64, 2.0, 5.0, 10.0, 15.0] {
            let tau = tau_deg.to_radians();
            let pobf = PlaneOfBestFocus::new(tau, 0.0, S_O, 0.0, F).unwrap();
            let depth = pobf.depth_at(0.0, y);
            assert!(
                depth > last_depth,
                "zero-CoC depth at Y={y} must increase with tau_x; \
                 tau={tau_deg} deg gave {depth} m, previous {last_depth} m"
            );
            last_depth = depth;

            // A point sitting on each tilted PoBF still has (near-)zero CoC —
            // the locus genuinely rotates, it does not merely shift.
            let c = coc_diameter_at_sensor(&Point3::new(0.0, y, depth), F, N, S_O, 0.0, tau, 0.0)
                .unwrap();
            assert_relative_eq!(c, 0.0, epsilon = 1e-12);
        }

        // Sign check: at tau_x = 0 the PoBF is fronto-parallel (depth == s_o).
        let flat = PlaneOfBestFocus::new(0.0, 0.0, S_O, 0.0, F).unwrap();
        assert_relative_eq!(flat.depth_at(0.0, y), S_O, epsilon = 1e-15);
    }

    // === KILL-GATE REGIME (d): thick lens reduces to thin when g = 0 =========
    #[test]
    fn regime_d_thick_lens_reduces_to_thin_when_gap_is_zero() {
        // With g = 0 the thick-lens code path must reproduce the exact
        // thin-lens regime (b) numbers — the principal planes coincide.
        let p = Point3::new(0.0, 0.0, 0.33);
        let c_thin = coc_diameter_at_sensor(&p, F, N, S_O, 0.0, 0.0, 0.0).unwrap();
        assert_relative_eq!(c_thin, 5.165_289_256_198_392e-5, epsilon = 1e-15);

        // sensor_distance and pobf_tilt_tan likewise collapse to thin-lens.
        let v0 = sensor_distance(S_O, 0.0, F).unwrap();
        assert_relative_eq!(v0, F * S_O / (S_O - F), epsilon = 1e-15);
        let tan_theta = pobf_tilt_tan(0.1, S_O, 0.0, F).unwrap();
        assert_relative_eq!(tan_theta, (S_O / v0) * 0.1_f64.tan(), epsilon = 1e-14);
    }

    #[test]
    fn regime_d_nonzero_gap_genuinely_shifts_the_coc() {
        // The inter-principal gap must be load-bearing: at the SAME camera
        // depth, a non-zero g changes the object distance from H (to z + g)
        // and therefore the CoC.  If g were ignored these would be equal.
        let p = Point3::new(0.0, 0.0, 0.33);
        let c_g0 = coc_diameter_at_sensor(&p, F, N, S_O, 0.0, 0.0, 0.0).unwrap();
        let c_g = coc_diameter_at_sensor(&p, F, N, S_O, 0.012, 0.0, 0.0).unwrap();
        assert!(
            (c_g - c_g0).abs() > 1e-7,
            "a non-zero inter-principal gap must change the CoC: \
             g=0 gave {c_g0} m, g=12mm gave {c_g} m"
        );

        // Hand check the g = 12 mm focus geometry against image_distance:
        //   in-focus object distance from H = s_o + g = 0.312 m
        //   v0 = f * 0.312 / (0.312 - 0.025) = 0.0078 / 0.287 = 0.027177...
        let v0_g = sensor_distance(S_O, 0.012, F).unwrap();
        assert_relative_eq!(v0_g, F * 0.312 / 0.287, epsilon = 1e-15);
    }

    #[test]
    fn coc_to_pixels_divides_by_pitch_and_validates() {
        assert_relative_eq!(coc_to_pixels(3.45e-5, PIX).unwrap(), 10.0, epsilon = 1e-12);
        assert!(coc_to_pixels(1.0, 0.0).is_err());
        assert!(coc_to_pixels(1.0, -1e-6).is_err());
        assert!(coc_to_pixels(f64::NAN, PIX).is_err());
    }

    #[test]
    fn coc_diameter_rejects_bad_inputs() {
        let p = Point3::new(0.0, 0.0, S_O);
        // Bad f-number.
        assert!(coc_diameter_at_sensor(&p, F, 0.0, S_O, 0.0, 0.0, 0.0).is_err());
        // Non-finite tilt.
        assert!(coc_diameter_at_sensor(&p, F, N, S_O, 0.0, f64::INFINITY, 0.0).is_err());
        // Object point at the focal point (no real image).
        let p_focal = Point3::new(0.0, 0.0, F);
        assert!(coc_diameter_at_sensor(&p_focal, F, N, S_O, 0.0, 0.0, 0.0).is_err());
    }

    #[test]
    fn pobf_normal_is_plus_z_when_untilted() {
        let flat = PlaneOfBestFocus::new(0.0, 0.0, S_O, 0.0, F).unwrap();
        let n = flat.normal();
        assert_relative_eq!(n, Vector3::new(0.0, 0.0, 1.0), epsilon = 1e-15);

        // The signed axial offset is the plain depth difference when flat.
        let off = flat.signed_axial_offset(&Point3::new(0.1, 0.2, 0.33));
        assert_relative_eq!(off, 0.33 - S_O, epsilon = 1e-15);
    }
}
