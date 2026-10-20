# Camera models

A calibration estimates the numbers of a camera model: the map from a point in front of the
camera to the pixel it lands on. etendue does not have a camera model of its own. Every scene
camera carries a calibration-rs model, and every pixel etendue computes (ground truth, the remap
that bends renders into the camera, the studio's camera views) goes through calibration-rs's
code. A dataset made by etendue and a calibration by calibration-rs therefore agree on what a
camera is, exactly, and the ground truth can be compared with an estimate number for number.

## The projection chain

A point $\mathbf{p} = (X, Y, Z)$ in the camera frame (+Z forward, +X right, +Y down) becomes a
pixel in four stages:

$$
\text{pixel} = K\Big(\,\text{sensor}\big(\,\text{distort}(\,\text{project}(\mathbf{p})\,)\big)\Big).
$$

1. **Projection** (pinhole): $\mathbf{n}_u = (X/Z,\; Y/Z)$, the normalised image coordinates. A
   point with $Z \le 0$ does not project.
2. **Distortion** bends $\mathbf{n}_u$ into $\mathbf{n}_d$ (below).
3. **Sensor**: identity, a general homography, or the Scheimpflug tilt (below), applied to
   $\mathbf{n}_d$ as a homography $\mathbf{s} \sim H\,[\mathbf{n}_d; 1]$.
4. **Intrinsics** $K$ turn sensor coordinates into pixels:

$$
u = f_x\,s_x + \text{skew}\cdot s_y + c_x, \qquad v = f_y\,s_y + c_y .
$$

Back-projection runs the chain backwards: $K^{-1}$, the inverse homography, undistortion, and a
ray through the point on the $Z = 1$ plane.

In a scene the model is the camera's `params`, one object per stage, each tagged by `type`:

```json
"params": {
  "projection": { "type": "pinhole" },
  "distortion": { "type": "brown_conrady5", "k1": -0.08, "k2": 0.02, "k3": 0.0, "p1": 0.0, "p2": 0.0, "iters": 8 },
  "sensor": { "type": "scheimpflug", "tilt_x": 0.05, "tilt_y": -0.01 },
  "intrinsics": { "type": "fx_fy_cx_cy_skew", "fx": 2318.8, "fy": 2318.8, "cx": 640.0, "cy": 512.0, "skew": 0.0 }
}
```

(This is a camera of the example `laser_scheimpflug_ur5e`.) `fx` and `fy` are in pixels: the
focal length divided by the pixel pitch, for the image distance the lens is focused at. `cx, cy`
is the principal point, where the optical axis meets the image.

## Distortion models

All five calibration-rs distortion models are available. With $r^2 = x^2 + y^2$ for
$\mathbf{n}_u = (x, y)$:

| `type` | Parameters | Forward map |
|---|---|---|
| `none` | | $\mathbf{n}_d = \mathbf{n}_u$ |
| `brown_conrady5` | `k1 k2 k3 p1 p2` | radial $1 + k_1r^2 + k_2r^4 + k_3r^6$ times $\mathbf{n}_u$, plus tangential terms |
| `rational` | `k1 … k6 p1 p2` | radial $\dfrac{1 + k_1r^2 + k_2r^4 + k_3r^6}{1 + k_4r^2 + k_5r^4 + k_6r^6}$, plus tangential terms |
| `thin_prism` | `k1 k2 k3 p1 p2 s1 s2 s3 s4` | Brown–Conrady plus $x_d \mathrel{+}= s_1r^2 + s_2r^4$, $y_d \mathrel{+}= s_3r^2 + s_4r^4$ |
| `division` | `lambda` | undistortion $\mathbf{n}_u = \mathbf{n}_d / (1 + \lambda r_d^2)$, closed form both ways |

The Brown–Conrady tangential terms are

$$
x_d = x\,(\text{radial}) + 2p_1xy + p_2(r^2 + 2x^2), \qquad
y_d = y\,(\text{radial}) + p_1(r^2 + 2y^2) + 2p_2xy .
$$

Undistortion has no closed form for the first three models; calibration-rs solves it by Newton's
method, at most `iters` steps (20 when `iters` is 0). It reaches machine precision in three or
four steps even in the corners of a strongly distorted image: over 10 000 random pixels of each
supported model, including Scheimpflug tilts up to 6°, projecting a back-projected pixel lands
within $10^{-12}$ px of where it started.

## The Scheimpflug sensor

A Scheimpflug camera has its sensor tilted against the lens, so that a plane oblique to the
camera is in focus (see [Scheimpflug plane of best focus](../scheimpflug.md)). Geometrically the
tilt changes where rays land, and calibration-rs models that with OpenCV's tilted-sensor
homography. For tilts $\tau_x$ (about the camera's x axis) and $\tau_y$ (about y):

$$
R = R_y(\tau_y)\,R_x(\tau_x), \qquad
H(\tau_x, \tau_y) =
\begin{pmatrix} R_{33} & 0 & -R_{13} \\ 0 & R_{33} & -R_{23} \\ 0 & 0 & 1 \end{pmatrix} R,
$$

with OpenCV's sign convention for $R_x$ and $R_y$. In a scene the angles are `tilt_x` and
`tilt_y`, in radians.

Two points matter when you work with tilts:

- **The homography says where rays land, not where focus is.** The plane of best focus is a
  separate physical computation, which etendue does from the lens.
- **The sign of `tilt_x`.** OpenCV's tilted plane is a virtual image plane in front of the
  centre of projection. The real sensor is that plane reflected through the lens, so for a
  positive `tilt_x` the real sensor's +y edge comes *toward* the lens. A textbook drawing of the
  sensor tilt (and etendue's optics derivation) uses the opposite sign about x; about y the two
  agree. Scene files always store calibration-rs's angles, and etendue converts where it needs
  to.

## Where the numbers come from

A camera's model is either typed in (the camera as calibrated, or as a datasheet promises) or
**derived from optics**. A camera with an `optics` block is a [physical
camera](../physical_cameras.md): its `fx`, `fy`, skew and sensor tilt are computed from the
sensor, the lens and the focus setting, and `etendue validate` refuses a scene whose stored
values differ from the derived ones by more than one part in $10^{9}$. The principal point and the
distortion stay as typed: they are properties of the built camera that the paraxial optics do
not determine.

## What is not modelled

- Projections other than the pinhole: fisheye and other wide-angle projections are represented
  only through the `division` or `rational` distortion of a pinhole.
- Rolling shutter, and any change of the model across the image other than distortion.
- Defocus, diffraction and noise: a camera model maps points to pixels, nothing more. etendue's
  blur models are on [Defocus and the blur budget](blur.md), its image noise on [Synthetic
  images](synthetic_images.md).

API: calibration-rs `vision-calibration-core` (`CameraParams`, `CameraModel`).
