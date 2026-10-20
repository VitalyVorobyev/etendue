# Scheimpflug plane of best focus

A laser-triangulation camera looks at its laser plane from the side. With the sensor square to
the lens, the camera is sharp on a plane square to its axis, which crosses the laser plane in a
single line: everything nearer or farther along the laser blurs. Opening the aperture for light
makes that band thinner, stopping down for depth of field costs light and adds diffraction.
The Scheimpflug principle offers a way out: **tilt the sensor and the plane of best focus tilts
with it**, and at the right tilt it lies on the laser plane. Then the whole measuring range is
in focus at any aperture.

## The rule

For an ideal lens, the object points that image sharply onto a tilted sensor form an exact
plane, the **plane of best focus**. If the sensor is tilted by $\tau$ about an axis, the plane of
best focus is tilted about the parallel axis by $\theta$, with

$$
\tan\theta = \frac{\tan\tau}{m}, \qquad m = \frac{v_0}{d},
$$

where $m$ is the magnification at the focus setting ($v_0$ the image distance from H′, $d$ the
object distance from H). The plane passes through the on-axis focus point. Because $m$ is small
in machine vision (0.01–0.1), **a few degrees of sensor tilt swing the plane of best focus by
tens of degrees**.

Geometrically, the sensor plane, the lens and the plane of best focus meet in one line, the
*Scheimpflug line*. For a thick lens the statement splits across the principal planes: the
sensor plane crosses the rear principal plane (through H′) at the same height at which the plane
of best focus crosses the front principal plane (through H). This is the **hinge rule** the
Optics workspace draws: when the condition holds, the two crossing points line up.

<svg viewBox="0 0 640 230" role="img" aria-label="Scheimpflug condition: sensor, lens and plane of best focus meet in one line" style="max-width:640px;width:100%;color:currentColor">
  <g stroke="currentColor" fill="none" stroke-width="1.5">
    <line x1="20" y1="120" x2="620" y2="120" stroke-dasharray="4 3"/>
    <line x1="160" y1="5" x2="160" y2="225" stroke-dasharray="2 3"/>
    <line x1="160" y1="75" x2="160" y2="165" stroke-width="4"/>
    <line x1="33" y1="150" x2="87" y2="90" stroke-width="4"/>
    <line x1="250" y1="37.5" x2="600" y2="144.4"/>
    <path d="M87 90 L160 10 L250 37.5" stroke-dasharray="5 3"/>
  </g>
  <circle cx="160" cy="10" r="4" fill="currentColor"/>
  <g fill="currentColor" font-family="sans-serif" font-size="13">
    <text x="20" y="180">tilted sensor</text>
    <text x="170" y="180">lens</text>
    <text x="400" y="70">plane of best focus</text>
    <text x="172" y="14">Scheimpflug line</text>
    <text x="590" y="112">axis</text>
  </g>
</svg>

**Worked example.** A 25 mm lens focused at 0.5 m has $m = 0.0557$. Tilting its sensor by 3°
tilts the plane of best focus by $\arctan(\tan 3° / 0.0557) = 43.2°$.

**The inverse problem** is what a head designer solves. Given a laser plane at angle $\theta$ to
the camera's image plane, set the focus where the laser plane crosses the camera's axis and tilt
the sensor by

$$
\tan\tau = m\,\tan\theta .
$$

In the Optics workspace's default head, the laser sits 100 mm off the camera's axis (toward the camera's
−y, the top of its image) and 50 mm in front of its image plane, aimed at the axis 350 mm out. Its plane rises 3 mm along the axis
for every 1 mm across it, an angle of 71.6° to the image plane. Focused at 350 mm, a 25 mm lens
has $m = 0.0839$, so the sensor tilts by $\arctan(0.0839 \times 3) = 14.1°$. The **Scheimpflug**
button computes this setting; the kernel reports the tilt as −14.13° about x, because in
calibration-rs's convention a laser on the camera's −y side needs a negative `tilt_x`.

## Conventions

- The tilts in a scene, `tilt_rad: [τx, τy]` on a physical camera or `tilt_x, tilt_y` on a
  Scheimpflug sensor model, are **calibration-rs's** (OpenCV's). Their `tilt_x` is the negative of
  the textbook tilt that the derivation below uses; `tilt_y` is the same. See [Camera
  models](models/camera_models.md#the-scheimpflug-sensor).
- The sensor tilts about the image-plane datum, the origin of the camera body (see [Physical
  cameras](physical_cameras.md)). Refocusing moves the lens, not the sensor.
- The plane of best focus depends only on the focal length and the principal planes. The pupils
  change where an image lands (the projection), not where focus is.

## How etendue uses it

- **Optics workspace.** The section view draws the head in the plane through the camera's axis,
  square to the laser plane: the sensor, the lens's principal planes and pupils, chief and
  marginal rays, the plane of best focus, the laser plane and the hinge. The **tilt x** and
  **tilt y** sliders tilt the sensor (±25°); **Scheimpflug** solves the condition at the current
  f-number; **Square sensor** undoes it. With a blur limit, the depth-of-field wedge around the
  plane of best focus is drawn too.
- **The optimiser** starts from the Scheimpflug setting and may move away from it, because the
  best setting for a window of finite width is not always the exact condition (see
  [Triangulation heads](models/heads.md)).
- **Scenes and datasets.** A physical camera's tilt becomes calibration-rs's Scheimpflug sensor
  model in its derived `params`, so ground truth and renders follow the tilted geometry exactly.

Try it: tilt the sensor and watch the plane of best focus swing far more than the sensor does,
always pivoting on the hinge. Press "Scheimpflug" to put it on the laser plane.

<div class="etendue-demo" data-demo="scheimpflug"></div>

## Assumptions and limits

- **Ideal, paraxial lens.** The plane of best focus is exact for that model. Real lenses have
  field curvature, so the sharp surface is not quite a plane.
- **Blur off the plane is first order** in the tilt: the model reads the sensor's position under
  the paraxial image, while an oblique chief ray meets the tilted sensor beside it. The error is
  within $0.6\tan\tau$ of a ray trace, about 5 % at 5°.
- **Mounts limit the tilt.** A Scheimpflug mount or tilt adapter allows only so much.
  etendue's optimiser keeps tilts below 0.5 rad (28.6°); whether your mount reaches the tilt it
  proposes is for you to check.
- **Calibrating a tilted camera is harder.** calibration-rs models the tilt, but the tilt, the
  focal length and the principal point trade off against one another; see [Ground truth and
  closed-loop calibration](models/closed_loop.md#a-scheimpflug-rig).

## The derivation

The derivation below works in the textbook sign convention and measures depth from the rear
principal plane; the results depend only on the object distance from H and the image distance
from H′. Its $s_o$ is the focus distance measured from H′, its $g$ the gap with H on the image side
of H′ (the opposite sign to the physical camera's `principal_gap_m`); with $g = 0$, as in the
worked numbers, both conventions agree.

{{#include ../../docs/derivations/scheimpflug_pobf.md:derivation}}
