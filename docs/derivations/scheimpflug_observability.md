# What a calibration can tell about a sensor tilt

A Scheimpflug camera's sensor tilt, calibrated from views of a planar target, is almost never
determined by the data. Small changes to the tilt are matched almost exactly by a principal-point
shift, a rotation of the camera frame and a change of the tangential distortion. This note derives
that equivalence in calibration-rs's camera model and says what remains to determine the tilt. It
also lists what a calibration does determine, and what that means for a triangulation head.

The measurements behind the numbers are in
[`docs/measurements/scheimpflug_observability.md`](../measurements/scheimpflug_observability.md).

## 1. The model

calibration-rs (like OpenCV) projects a point $P = (X, Y, Z)$ in the camera frame in four steps:

1. **Normalised coordinates:** $x = X/Z$, $y = Y/Z$.
2. **Brown–Conrady distortion about the optical axis:** with $r^2 = x^2 + y^2$,
   $$x_d = x\,(1 + k_1 r^2 + k_2 r^4 + k_3 r^6) + 2p_1 xy + p_2(r^2 + 2x^2),$$
   $$y_d = y\,(1 + k_1 r^2 + k_2 r^4 + k_3 r^6) + p_1(r^2 + 2y^2) + 2p_2 xy.$$
3. **The tilt homography** $T(\tau_x, \tau_y)$ (OpenCV's `computeTiltProjectionMatrix`), applied
   to $(x_d, y_d, 1)$.
4. **The intrinsics:** $u = f_x x_t + c_x$, $v = f_y y_t + c_y$.

To first order in the tilt, $T$ is the identity with a third row of $(\tau_y, -\tau_x, 1)$:
$$x_t = \frac{x_d}{1 + \tau_y x_d - \tau_x y_d}, \qquad y_t = \frac{y_d}{1 + \tau_y x_d - \tau_x y_d}.$$

## 2. Without distortion, a tilt is a rotation and a shift

Rotate a pinhole camera by a small angle $\omega = (\omega_x, \omega_y, \omega_z)$, so that
$P' = (I + [\omega]_\times)\,P$. Its normalised coordinates become
$$x' = \frac{x - \omega_z y + \omega_y}{1 - \omega_y x + \omega_x y}, \qquad
  y' = \frac{y + \omega_z x - \omega_x}{1 - \omega_y x + \omega_x y}.$$

With $\omega = (-\tau_x, -\tau_y, 0)$, the denominator is the tilt's. The numerators then differ
from the tilt's only by the constant shift $(-\tau_y, +\tau_x)$, and the principal point absorbs a
constant shift. So, to first order:

> **A camera with tilt $(\tau_x, \tau_y)$ and principal point $(c_x, c_y)$ projects like an
> untilted camera with principal point $(c_x + f_x\tau_y,\; c_y - f_y\tau_x)$, rotated by
> $(-\tau_x, -\tau_y, 0)$.**

The statement is exact up to a small intrinsic change. Any invertible $3\times3$ matrix factors
as an upper-triangular matrix times a rotation (the RQ decomposition). So $K\,T$ equals some
$K''R''$, where $K''$ differs from $K$ only by a skew and an aspect change of order $\tau^2$. The
data can tell the two cameras apart only through those $\tau^2$ terms, and they are tiny. Without
distortion, calibration-rs reports σ(tilt_y) = 3.9° for the head in the measurements, at 0.1 px of
corner noise.

The rotation $R''$ has nowhere to show: something in every calibration absorbs it.

| What is calibrated | What absorbs the camera-frame rotation |
|---|---|
| Intrinsics alone | each view's target pose |
| Hand-eye | the hand-eye |
| A rig | the camera's pose on the rig (`cam_se3_rig`) |
| A laser plane | the plane, which is estimated in the camera frame |

The tilt is not *distinguishable* from those changes. That is a different property from being
poorly conditioned on a given set of views.

## 3. Distortion is what reveals the tilt, unless p1 and p2 hide it

Distortion acts about the optical axis, *before* the tilt. In the equivalent untilted camera, that
axis no longer passes through the principal point: the distortion centre sits a normalised
distance $\delta \approx (\tau_y, -\tau_x)$ from it. A camera whose distortion is off-centre
projects differently from one whose distortion is centred, and that difference is what reveals
the tilt.

Expand radial distortion about a centre shifted by $(x_0, y_0)$. To first order in the shift, and
in $k_1$, the extra displacement is
$$\Delta x = -k_1\,[\,x_0(3x^2 + y^2) + 2y_0\,xy\,], \qquad
  \Delta y = -k_1\,[\,2x_0\,xy + y_0(x^2 + 3y^2)\,],$$
plus constant and linear terms that the principal point and the focal length absorb. That is
exactly Brown–Conrady's tangential pattern, with $p_2 = -k_1 x_0$ and $p_1 = -k_1 y_0$.
Decentring distortion *is* a shifted distortion centre, to this order.

So a change of tilt $\delta\tau = (\delta\tau_x, \delta\tau_y)$ is matched by the following changes:

| Parameter | Change |
|---|---|
| $c_x$ | $-f_x\,\delta\tau_y$ |
| $c_y$ | $+f_y\,\delta\tau_x$ |
| $p_2$ | $-k_1\,\delta\tau_y$ |
| $p_1$ | $+k_1\,\delta\tau_x$ |
| the camera frame | a rotation of $(-\delta\tau_x, -\delta\tau_y, 0)$ |

What remains are terms of order $\tau^2$, $k_2\,\delta\tau$ and $k_1^2\,\delta\tau$. On the head
in the measurements they are under 0.001 px.

## 4. Consequences

- **With p1 and p2 free, no set of views determines the tilt.** The tilt and the principal point
  are fully correlated. On the head at 0.1 px, calibration-rs reports σ(tilt_y) = 18° and
  σ(cx) = 730 px, and more views shrink these only as 1/√N. A solve leaves the tilt near wherever it
  was seeded and moves the principal point, p1, p2 and the camera frame to match.
- **With p1 and p2 held, radial distortion alone determines the tilt.** The precision scales
  roughly as $1/|k_1|$. On the head at 0.1 px, σ(tilt_y) is 0.12° with $k_1 = -0.08$ and 0.025°
  with $k_1 = -0.3$. Holding p1 and p2 assumes a centred lens: a real decentring is then read as
  tilt.
- **A robot, a laser or a second camera adds nothing.** Each carries the camera frame's rotation,
  as the table in §2 shows.
- **Mixing parameters from different solutions breaks the equivalence.** For example, combining
  datasheet tilts with calibrated principal points gives a camera whose rays are off by $f\,\delta\tau$
  pixels, 23 px on the head. Likewise, a laser plane from one solve and a hand-eye from another
  that re-estimated the camera sit in two different camera frames. etendue's head procedures did
  this, until they were fixed to hold the device's camera. Under 0.1 px of noise, a triangulated
  stripe point was 0.25–4.7 mm off on the example heads, against 0.013 mm with one camera
  ([`head_triangulation.md`](../measurements/head_triangulation.md)).

## 5. What a calibration does determine

Every quantity that does not depend on how the projection splits into tilt, principal point and
camera frame is determined. That covers each pixel's ray in the gripper or base frame, the laser
plane in the gripper frame, the target's pose in the base frame, and therefore every triangulated
point. On the head, on exact data, a solve whose tilt stayed 0.57° off still had:
- the true stripe points within 0.36 µm of the laser plane in the gripper frame;
- the target in the base frame within 3e-5° and 0.3 µm.

For measurement, the calibration is right, provided the plane and the hand-eye come from one camera
solution.

Only what reads the tilt itself, or a camera-frame quantity, is affected:
- **Focus.** The plane of best focus and the depth of field follow the physical tilt
  ([Scheimpflug plane of best focus](scheimpflug_pobf.md)).
- **Comparison with the mechanical design.** For example, checking an assembled head's tilt.
- **Camera-frame quantities.** A laser plane or hand-eye compared in the camera frame inherits the
  frame's rotation, as the checks of `etendue calibrate` do.

## 6. Ways to proceed

| Choice | Determined | Assumes |
|---|---|---|
| Hold the tilt at the design value | everything else; the principal point, p1, p2 and the frame absorb any tilt error | nothing for measurement; focus uses the design tilt |
| Hold p1, p2 at zero, estimate the tilt | the tilt, through radial distortion (weakly) | a centred lens |
| Estimate both | not the tilt | — |
| Measure the tilt optically (focus across the field) | the physical tilt | outside geometric calibration |
