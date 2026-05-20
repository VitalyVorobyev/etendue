# Thick lens, Scheimpflug plane of best focus, and geometric circle of confusion

This note derives, from first principles, the optical model implemented in
`etendue-core::optics` (`optics/coc.rs` and `optics/thick_lens.rs`). It is the
written companion to the `coc.rs` module doc-comment. Milestone **M4a** rests
on this derivation being correct — it has an explicit kill gate.

The lens is treated as **ideal**: Gaussian (paraxial) imaging only. Seidel /
Zernike aberrations, field curvature, and chromatic effects are explicitly out
of scope. The single consequence we lean on repeatedly: **for an ideal lens
with a fronto-parallel sensor the circle of confusion (CoC) of an object point
depends only on that point's axial defocus, not on its lateral field
position.** Any field/lateral dependence of the CoC in this model enters
*exclusively* through Scheimpflug sensor tilt.

The derivation is not a transcription of OpenCV's
`computeTiltProjectionMatrix`. That homography is a *geometric sensor-plane
remap* (it answers "where on the tilted sensor does a ray land"); it says
nothing about *where focus lies*. The plane of best focus is a separate
physical computation, carried out below.

## 1. Coordinates and sign conventions

Camera-local, right-handed. The optical axis is camera-local `+z`, pointing
**from the lens toward the object**. The object half-space is `z > 0`; the
image half-space (where the sensor lives) is `z < 0`.

A thick lens has two principal planes:

- **H**, the *front* (object-side) principal plane;
- **H'**, the *rear* (image-side) principal plane.

They are separated by the **inter-principal gap** `g = H - H' >= 0`. For an
ideal *thin* lens the two planes coincide and `g = 0`.

**Reference-point convention (this is the load-bearing modelling choice).**
The camera-local origin `z = 0` is the **rear principal plane H'**. This is the
natural choice: `vision-calibration-core`'s pinhole `Camera` projects through a
single center, and the center that reproduces image-formation geometry is the
rear nodal point, which (in air) coincides with H'. Consequently:

- The **front** principal plane H sits a distance `g` *toward the object*,
  i.e. at `z = +g`.
- An object point given at camera depth `z` (its `+z` coordinate) lies at
  object distance **`d_H = z + g`** from H.
- The Gaussian conjugate equation is written with object distance from H and
  image distance from H'.

This convention is what makes the inter-principal gap `g` a *physically
load-bearing* parameter rather than an unused field, and it makes the
thick → thin reduction (`g -> 0`) an exact, testable statement (Section 6).

The four scalar lens parameters:

| symbol | meaning                                   | unit |
|--------|-------------------------------------------|------|
| `f`    | effective focal length                    | m    |
| `N`    | f-number                                  | —    |
| `s_o`  | object-side focus distance, **from H'**    | m    |
| `g`    | inter-principal gap `H - H'`               | m    |

The entrance-pupil / aperture diameter is `D = f / N`.

## 2. Thick-lens conjugate relation

For an ideal lens the Gaussian imaging equation, **with object distance
measured from H and image distance measured from H'**, has the same algebraic
form as the thin-lens equation:

```
1 / d_H  +  1 / d_i  =  1 / f                                            (1)
```

where `d_H` is the object distance from H and `d_i` is the conjugate image
distance from H'. This is standard thick-lens Gaussian optics: introducing the
principal planes is *defined* precisely so that (1) keeps the thin-lens form;
the entire effect of finite lens thickness is absorbed into the separation `g`
of the two reference planes.

Solving (1) for the image distance:

```
d_i(d_H)  =  f * d_H / (d_H - f)                                         (2)
```

valid for a real object beyond the front focal point, `d_H > f`.

In the camera frame an object point at depth `z` has `d_H = z + g`, so its
**image distance from H'** is

```
s_i(z)  =  f * (z + g) / (z + g - f).                                    (3)
```

The transverse magnification of that conjugate pair is

```
m(z)  =  - s_i(z) / (z + g)                                              (4)
```

(negative: real images are inverted).

**Focus distance and the sensor.** The design focus distance is `s_o`
(measured from H', per Section 1). The in-focus object plane is therefore at
object distance `s_o + g` from H, and the sensor is placed at the conjugate
image distance

```
v0  =  s_i(s_o)  =  f * (s_o + g) / (s_o + g - f)                        (5)
```

from H'. `v0` is the on-axis sensor distance; it is fixed once `f`, `s_o`, `g`
are chosen.

## 3. Geometric circle of confusion — fronto-parallel sensor

Take the sensor fronto-parallel first (`tau_x = tau_y = 0`): a plane at axial
distance `v0` from H'.

An object point at camera depth `z` images, by (3), at axial distance `s_i(z)`
from H'. If `z != s_o` then `s_i(z) != v0`: the point focuses either in front
of or behind the sensor, and the converging cone of rays is intercepted by the
sensor as a **blur disc** — the circle of confusion.

The cone is bounded by the aperture. The aperture has diameter `D = f / N`,
and (in the thin-aperture idealisation used here) lies at the lens, i.e. at
axial distance `s_i(z)` from the point where the cone collapses to a focus.
The cone therefore has full opening "diameter-per-unit-length" `D / s_i(z)`.
At the sensor, displaced longitudinally by `|s_i(z) - v0|` from that focus, the
cone has spread to a disc of diameter

```
c(z)  =  D * |s_i(z) - v0| / s_i(z).                                     (6)
```

This is the exact geometric CoC diameter **at the sensor**, in length units. It
is the textbook lens-defocus CoC; (6) is derived purely by similar triangles on
the converging image-side cone.

Two properties worth stating explicitly, because the kill-gate tests check
them:

- `z = s_o  =>  s_i(z) = v0  =>  c = 0`. A point on the focus plane is sharp.
- `c(z)` depends on `z` **only**. For a fronto-parallel sensor the CoC has **no
  dependence on the object point's lateral position** `(X, Y)`. This is the
  honest consequence of an ideal (aberration-free) lens — there is no
  field-curvature or off-axis blur term to add, and none is added.

**CoC in pixels.** The sensor samples with a finite pixel pitch `p` (metres
per pixel). The blur in pixels is

```
c_px(z)  =  c(z) / p.                                                    (7)
```

This is the quantity a focus heatmap and a depth-of-field criterion use.

## 4. Scheimpflug tilt — the plane of best focus is a tilted plane

Now tilt the sensor. `tau_x` is a rotation of the sensor plane about the
camera `x`-axis, `tau_y` about the `y`-axis (both in radians). The on-axis
sensor point stays at axial distance `v0` from H'; off-axis, the sensor
surface departs from the fronto-parallel plane.

Consider tilt about a single axis first — `tau` about `x` — so all the action
is in the `y`–`z` plane. Parameterise the sensor by image height `y_s`
(distance from the optical axis, measured in the image plane). The tilted
sensor's axial distance from H' is, to first order in the tilt,

```
v(y_s)  =  v0  +  y_s * tan(tau).                                        (8)
```

**Sign convention.** Positive `tau_x` tips the sensor so that its `+y` edge
moves *away from the lens* (toward more negative `z`, i.e. larger axial
distance from H'). This fixes the `+` sign in (8) and, through the derivation
below, the direction in which the plane of best focus swings. The hinge-rule
sanity check at the end of this section confirms the choice is physical.

**Conjugate of a tilted plane.** Each sensor point at image height `y_s` is the
sharp image of exactly one object point — its optical conjugate. We find the
locus of those conjugates; that locus is, by definition, the **plane of best
focus (PoBF)**: the set of object points that image sharply onto the (tilted)
sensor.

A sensor point at height `y_s` has axial distance `v = v(y_s)` from H'. Its
conjugate object point has, by inverting (2),

```
u(v)  =  f * v / (v - f)                    (object distance from H)
```

and, by the magnification (4), object height

```
Y  =  - y_s * u / v.
```

We now show `{ (Y, u) }` is an exact plane and find its tilt. Write
`a = v0 - f` (so the on-axis conjugate is `s_o + g = u(v0) = f*v0/a`). With
`v = v0 + y_s*tan(tau)`:

```
u - (s_o + g)  =  f * (v0 + y_s tanτ)/(a + y_s tanτ)  -  f*v0/a
              =  f * [ a(v0 + y_s tanτ) - v0(a + y_s tanτ) ] / [ a (a + y_s tanτ) ]
              =  f * y_s tanτ * (a - v0) / [ a (a + y_s tanτ) ].
```

Since `a - v0 = (v0 - f) - v0 = -f`:

```
u - (s_o + g)  =  - f^2 * y_s tanτ / [ a (a + y_s tanτ) ].               (9)
```

And from `Y = -y_s f / (a + y_s tanτ)` we get `y_s = -Y(a + y_s tanτ)/f`.
Substituting into (9):

```
u - (s_o + g)  =  ( f / a ) * Y * tan(tau).                             (10)
```

Equation (10) is **linear in `Y`** — so the locus of conjugates is **exactly a
plane** (a *line* in the `y`–`z` section, extended trivially in `x`). The PoBF
passes through the on-axis focus point and is tilted, in object space, by an
angle `theta` about the `x`-axis with

```
tan(theta)  =  f / (v0 - f) * tan(tau).                                 (11)
```

The factor simplifies. From the conjugate relation, `v0 - f = f^2 / (s_o+g-f)`
[substitute (5)], hence `f/(v0-f) = (s_o + g - f)/f`. And from (5),
`(s_o+g-f)/f = (s_o + g)/v0`. Therefore

```
tan(theta)  =  (s_o + g) / v0  *  tan(tau)  =  (1/|m|) * tan(tau),       (12)
```

where `m = -v0/(s_o+g)` is the on-axis magnification (4). **The object-plane
tilt is the image-plane tilt scaled by the inverse magnification.** This is the
classical Scheimpflug plane-of-best-focus relation.

**Three planes, one line (the Scheimpflug condition).** The extended sensor
plane meets the lens plane (the plane through H', `z = 0` on the image side)
where `v(y_s) = 0`, i.e. at image height `y_s = -v0 / tan(tau)`. The PoBF, by
construction the conjugate locus, meets the lens plane through H at the
conjugate of that same line. Both the object plane (PoBF), the lens plane, and
the image plane (sensor) therefore intersect in **one common line** — the
Scheimpflug line. Equation (12) is the quantitative content of that geometric
statement.

**Hinge-rule sanity check.** For a distant object, `s_o + g >> f`, the sensor
distance `v0 -> f` and (12) gives `tan(theta) ~ (s_o+g)/f * tan(tau)` — a
*large* PoBF swing for a *small* sensor tilt. Physically: tilting the sensor by
a few degrees swings the in-focus plane by tens of degrees, and the swing is
**larger** than the sensor tilt whenever `s_o + g > v0` (the usual photographic
regime). The PoBF, the lens plane, and the sensor pivot about the common
Scheimpflug line — the "hinge". A downward sensor tilt (`+tau_x` here) lowers
the near edge of the in-focus plane and raises the far edge: the in-focus band
sweeps across depth as a function of field height. This is the correct,
expected behaviour of a Scheimpflug system, and it is what the M4 focus
heatmap will visualise.

**Two-axis tilt.** `tau_x` and `tau_y` act on orthogonal axes and, to first
order, independently. The PoBF is the plane through the on-axis focus point
`(0, 0, s_o)` whose depth varies with lateral position as

```
z_PoBF(X, Y)  =  s_o  +  X * tan(theta_y)  +  Y * tan(theta_x)           (13)
```

with each object-side tilt obtained from the corresponding sensor tilt by (12):

```
tan(theta_x) = (s_o + g)/v0 * tan(tau_x),
tan(theta_y) = (s_o + g)/v0 * tan(tau_y).                                (14)
```

(A tilt about `x` makes depth vary with `Y`; a tilt about `y` makes depth vary
with `X`.)

## 5. Geometric CoC with a tilted sensor

With the sensor tilted, the blur of an object point `P = (X, Y, Z)` is governed
by the longitudinal mismatch between **where `P` images** and **where the
tilted sensor surface is at that image location**.

1. `P` is at camera depth `Z`, hence object distance `Z + g` from H. By (3) it
   images at axial distance

   ```
   s_i_P  =  f (Z + g) / (Z + g - f)
   ```

   from H', with magnification `m_P = - s_i_P / (Z + g)`.

2. The paraxial image of `P` lands at image height

   ```
   (x_img, y_img)  =  m_P * (X, Y).
   ```

3. The tilted sensor's axial distance from H' at that image height is, by the
   two-axis form of (8),

   ```
   v_sensor  =  v0  +  x_img * tan(tau_y)  +  y_img * tan(tau_x).         (15)
   ```

4. The CoC follows from the same similar-triangles cone argument as (6), with
   the fronto-parallel sensor distance `v0` replaced by the *local* tilted
   sensor distance `v_sensor`:

   ```
   c(P)  =  D * | s_i_P - v_sensor | / s_i_P,        D = f / N.           (16)
   ```

   In pixels, `c_px(P) = c(P) / p`.

**Consistency checks (these are exactly the kill-gate regimes).**

- *On the PoBF, `c = 0`.* The PoBF was *defined* in Section 4 as the conjugate
  locus of the tilted sensor; equivalently, equation (10) is precisely the
  statement that a PoBF object point images onto the sensor surface, i.e.
  `s_i_P = v_sensor`. Substituting into (16) gives `c = 0`. So every point of
  the tilted plane (13) is sharp — the in-focus locus is the correctly tilted
  plane.

- *`tau = 0` removes all field dependence.* With `tau_x = tau_y = 0`, (15)
  collapses to `v_sensor = v0` regardless of `(X, Y)`, and (16) reduces exactly
  to the fronto-parallel CoC (6), a function of `Z` alone. No off-axis blur is
  manufactured.

- *Increasing `tau` rotates the zero-CoC locus.* By (14), the PoBF tilt angle
  is strictly monotincreasing in the sensor tilt, with the sign fixed by the
  convention in Section 4 — the in-focus band rotates in the geometrically
  correct (hinge-rule) direction as `tau` increases.

**Modelling note (honest scope).** `v_sensor` in (15) is evaluated at the
*paraxial* image height `m_P (X, Y)`. The exact intercept of the ray bundle
with the tilted sensor surface differs from this at second order in
`tan(tau)`. Crucially, the **zero-CoC locus is exact**: the PoBF derivation in
Section 4 uses exactly this first-order parameterisation and nonetheless yields
an *exact* plane (10). The `O(tan^2 tau)` term only perturbs the *magnitude* of
an already-nonzero blur. For the design-guidance role of this CoC (a focus
falloff estimate, sensor tilts up to ~30 deg) that is acceptable, and it is the
standard first-order Scheimpflug treatment. It is recorded here so the
approximation is explicit rather than hidden.

## 6. Thick → thin reduction (`g -> 0`)

Setting `g = H - H' = 0` makes the two principal planes coincide: the lens is
thin. Every equation above degrades correctly and *exactly*:

- conjugate relation (3): `s_i(z) = f z / (z - f)` — the thin-lens equation
  with object distance `z` measured from the single lens plane;
- sensor distance (5): `v0 = f s_o / (s_o - f)`;
- CoC (6) / (16): the textbook thin-lens defocus CoC;
- PoBF tilt (12): `tan(theta) = (s_o / v0) tan(tau)`, the thin-lens Scheimpflug
  relation.

Because the camera-frame object depth `z` enters the model only through the
combination `z + g` (Section 1), the gap `g` is a genuine, load-bearing
parameter: with `g > 0` an object at the same camera depth `z` is further from
H and focuses differently; with `g = 0` the model collapses, term by term, onto
the thin lens. The kill-gate test (d) verifies this reduction numerically.

## 7. Worked numbers (used verbatim as test expectations)

A realistic machine-vision lens: `f = 25 mm`, `N = 4`, focus distance
`s_o = 0.30 m`, pixel pitch `p = 3.45 um`. Aperture `D = f/N = 6.25 mm`.

**(b) Fronto-parallel, defocused point.** Thin lens (`g = 0`). In-focus sensor
distance, from (5):

```
v0 = f s_o / (s_o - f) = 0.025 * 0.30 / (0.30 - 0.025)
   = 0.0075 / 0.275 = 0.02727272727... m.
```

Object point at depth `z = 0.33 m` (30 mm beyond focus). Its image distance,
from (3):

```
s_i(0.33) = 0.025 * 0.33 / (0.33 - 0.025) = 0.00825 / 0.305
          = 0.02704918032... m.
```

CoC at the sensor, from (6):

```
c = D * |s_i - v0| / s_i
  = 0.00625 * |0.02704918032 - 0.02727272727| / 0.02704918032
  = 0.00625 * 0.00022354695 / 0.02704918032
  = 0.00625 * 0.008264462...
  = 5.16528925e-5 m   (51.65 um).
```

In pixels: `c_px = c / p = 5.16528925e-5 / 3.45e-6 = 14.9719... px`.

(A compact exact route to the same number: for a thin lens
`c = D f * |z - s_o| / ( z (s_o - f) )`
`= 0.00625 * 0.025 * 0.03 / (0.33 * 0.275)`
`= 4.6875e-6 / 0.09075 = 5.16528925e-5 m`. The two agree.)

**(d) Thick reduces to thin.** With `g = 0` the thick-lens code path computes
exactly the (b) numbers. With `g != 0` the same camera-depth point uses
`d_H = z + g`, giving a different — and correctly shifted — CoC; the test
checks both: identical at `g = 0`, divergent for `g > 0`.

**(c) Scheimpflug, point on the tilted PoBF.** Same lens, `s_o = 0.30 m`,
`g = 0`, sensor tilt `tau_x = 5 deg`. From (5), `v0 = 0.0272727... m`. PoBF
tilt, from (12):

```
tan(theta_x) = (s_o / v0) * tan(tau_x)
             = (0.30 / 0.0272727...) * tan(5 deg)
             = 11.0 * 0.0874886...
             = 0.9623747...
theta_x = atan(0.9623747) = 43.9026... deg.
```

The PoBF depth at lateral height `Y` is `z_PoBF(Y) = s_o + Y tan(theta_x)`
[from (13)]. Pick `Y = 0.05 m`: `z_PoBF = 0.30 + 0.05 * 0.9623747 =
0.3481187... m`. The point `(0, 0.05, 0.3481187)` lies on the tilted PoBF, so
its CoC must be `0` (to within solver tolerance). The on-axis point
`(0, 0, 0.30)` is also on the PoBF, CoC `0`. A point off the PoBF —
e.g. `(0, 0.05, 0.30)`, which is on the *old* fronto-parallel focus plane but
*below* the tilted PoBF — has a strictly positive CoC. Increasing `tau_x`
increases `tan(theta_x)` (12), rotating the zero-CoC plane further: the model
swings the in-focus band in the hinge-correct direction.
