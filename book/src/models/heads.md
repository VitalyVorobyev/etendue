# Triangulation heads

A laser-triangulation head is a camera and a line laser fixed to one body. The laser draws a
line on the part; the camera, looking at the laser plane from an angle, sees the line shift as
the surface moves along the laser. Designing a head means choosing the parts (sensor, lens,
laser), where the laser sits, and the camera's setting (focus, f-number, sensor tilt), so that a
required **window** of the laser plane is seen, sharp, and resolved finely enough. etendue
analyses a head where it measures: in its laser plane.

## The head and its window

Everything is in the camera's body frame (see [Physical cameras](../physical_cameras.md)). The
laser plane is parameterised from the laser's origin by $a$, along the laser line, and $b$, along
its central ray. The design window is

$$
\text{near} \le b \le \text{far}, \qquad |a| \le \tfrac{1}{2}\,\text{width},
$$

with a blur limit. In the Optics workspace you place the laser by its **baseline** (its offset
from the camera's axis, toward the camera's −y), its **height** along the axis, and its **aim**
(where its central ray crosses the camera's axis). The window is centred on the aim point:
**depth** along the laser, **width** along the line.

## What every point of the window gets

At each point $(a, b)$ of the laser plane etendue computes:

| Quantity | Meaning |
|---|---|
| seen | in front of the camera, on its sensor, and inside the laser's fan |
| blur | the circle of confusion, the Airy disc at the laser's wavelength, and their root sum of squares, in pixels ([Defocus and the blur budget](blur.md)) |
| lateral resolution | metres per pixel along the laser line |
| depth resolution (dZ/px) | how far along the central ray the surface moves for the imaged line to move one pixel across itself |
| triangulation angle | between the laser's ray and the camera's line of sight to the point |
| stripe width | the beam's imaged thickness and the blur, root sum of squares ([Laser line and stripe](../laser.md)) |

The resolutions come from the projection's Jacobian. With $J_a = \partial\mathbf{u}/\partial a$
and $J_b = \partial\mathbf{u}/\partial b$ the image motions along the line and along the ray, and
$\hat{\mathbf{n}}$ the unit vector across the imaged line (perpendicular to $J_a$):

$$
\text{lateral} = \frac{1}{\lVert J_a \rVert}, \qquad
\text{dZ/px} = \frac{1}{\lvert \hat{\mathbf{n}} \cdot J_b \rvert} .
$$

Depth resolution is coarser than lateral resolution by roughly $1/\sin$ of the triangulation
angle: a wider angle resolves depth better but shadows more and needs a wider baseline.

**The default head.** A Sony IMX174 (5.86 µm, 1936 × 1216) with a 25 mm lens at f/4 focused at
350 mm, and a 660 nm laser with a 50 µm waist focused at the aim point. The laser sits 100 mm off
the camera's axis (toward the camera's −y) and 50 mm in front of the image plane, aimed at the
axis 350 mm out. The window is 120 mm deep and 40 mm wide. The camera sees all of the window, and
over it:

| | As built (square sensor) | Scheimpflug |
|---|---|---|
| worst blur | 19.19 px (19.15 px of it defocus) | 1.19 px (diffraction only) |
| depth resolution | 150–306 µm/px | 140–304 µm/px |
| lateral resolution | 58–82 µm/px | 57–83 µm/px |
| triangulation angle | 15.6–22.5° | 15.6–22.5° |

## The Scheimpflug setting

The **Scheimpflug** button sets the focus where the laser plane crosses the camera's axis and
tilts the sensor so that the plane of best focus *is* the laser plane (see [Scheimpflug plane of
best focus](../scheimpflug.md)). For the default head that is a −14.13° tilt about x at 350 mm.
The whole laser plane is then in focus, and only diffraction is left.

## The optimiser

The window is not a single plane in practice: a finite stripe, a finite aperture and a limit on
the sensor's tilt make the exact Scheimpflug condition one good setting among others. The
**Optimise** button searches the camera's setting:

- **Variables:** the sensor tilt $(\tau_x, \tau_y)$ when **Tilt the sensor** is on, the focus
  distance, and the f-number within the lens's range (its widest aperture to f/22).
- **Objective:** the worst total blur over the window, sampled on a 9 × 9 grid, while the camera
  sees all of it. A setting that loses any sample of the window costs more than any blur, so the
  optimiser never trades coverage for sharpness.
- **Start:** the Scheimpflug setting at the current f-number (with the sensor square to the axis
  when it may not tilt).
- **Method:** Nelder–Mead, a derivative-free simplex search, because the worst blur is not smooth
  (it switches from sample to sample). It restarts from its best point with a fresh simplex until a
  restart stops helping (at most eight runs). It uses no random numbers, so the same head always
  gives the same answer, and it never returns a setting worse than its start. Tilts are kept below
  0.5 rad.

The result shows the worst blur now, at the start and at the optimum, and the setting; **Apply**
takes it.

On the default head:

| Search | Optimum | Worst blur |
|---|---|---|
| tilt free, f/1.4–f/22 | the Scheimpflug tilt, f/1.4 | 1.19 → **0.42 px** |
| square sensor, f/1.4–f/22 | focus 340 mm, f/14.8 | 19.19 → **6.25 px** |

With the tilt free the laser plane is in focus everywhere, so the widest aperture wins: it has
the smallest Airy disc. With the sensor square, focus and f-number balance defocus against
diffraction: at the optimum the two are equal (4.42 px each).

### Searching the lens library

**Search *n* lenses** (under Optimise) runs the same optimisation with every lens in the
project's library on the head, and ranks them: most of the window seen first (to a tenth of a
percent), then the smallest worst blur. Each row gives the share of the window seen, the worst blur, the coarsest depth
resolution and the f-number; **Apply** puts that lens and its optimum on the head. Lenses that
cannot work (one that cannot focus that close, say) are listed with the reason.

In the workspace, the map over the window shows any of these quantities (Blur, Stripe, dZ/px,
lateral resolution, Angle), and the Sensor view draws the laser line on the sensor at several
ranges across the window.

Try it: change the baseline, the aim and the window, and watch the blur, depth-resolution and
angle maps. Press "Optimise" with and without the tilt.

<div class="etendue-demo" data-demo="head"></div>

## Assumptions and limits

- **No light budget.** The optimiser minimises blur only. It knows nothing of exposure, laser
  power, or speckle, all of which favour or penalise a wide aperture in ways the blur does not
  show. With the tilt free it will always open the aperture fully.
- **The window is a plane.** Real parts are surfaces in the laser plane's neighbourhood; the map
  says nothing about occlusion by the part itself.
- **Ideal optics** (see [Physical cameras](../physical_cameras.md) and [Defocus and the blur
  budget](blur.md)).
- **The grid is a sample.** The optimiser judges the worst blur on 9 × 9 points of the window, the
  maps on finer grids; a narrow blur peak between samples can be missed.

API: `etendue-core` `head` (`Head::map`, `scheimpflug`, `optimise`, `section`); `@etendue/wasm`
`headMap`, `headScheimpflug`, `headOptimise`, `headSection`.
