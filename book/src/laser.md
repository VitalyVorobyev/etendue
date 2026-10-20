# Laser line and stripe

A line laser spreads its beam into a flat fan. Where the fan meets a surface it paints a
**stripe**, and a camera beside the laser sees the stripe shift across its image as the surface
moves nearer or farther: that is laser triangulation. To measure with it, the system must know
the fan's plane in the camera's frame, which is what a laser-plane calibration recovers. etendue
models the fan's geometry (for ground truth and coverage), the beam's thickness (for how wide the
stripe images) and the stripe's light (for synthetic laser images).

## The fan

A laser's frame has the fan in its local $x = 0$ plane, opening symmetrically about local +Z (the
central ray) toward ±Y. A point of the fan is

$$
\mathbf{p}(r, \varphi) = \mathbf{o} + r\,(\cos\varphi\;\hat{\mathbf{z}} + \sin\varphi\;\hat{\mathbf{y}}),
\qquad |\varphi| \le \alpha,\quad 0 \le r \le L,
$$

with $\mathbf{o}$ the laser's origin (the fan's apex), $\alpha$ the fan half-angle
(`fan_half_angle`) and $L$ the reach (`fan_length`), the same along every ray. The fan's plane is

$$
\hat{\mathbf{n}} \cdot \mathbf{p} + d = 0, \qquad \hat{\mathbf{n}} = \text{the laser's } {+X}, \quad
d = -\hat{\mathbf{n}} \cdot \mathbf{o} .
$$

This unit-normal-and-offset form, in metres, is the one calibration-rs's laser-plane estimate
uses. The ground truth gives each laser's true plane in the frame of the camera that images it
(and in the rig's frame); the sign of $(\hat{\mathbf{n}}, d)$ is arbitrary, and comparisons
remove it.

## The beam's thickness

Across the fan the beam is a focused Gaussian beam. Its $1/e^2$ radius at a distance $z$ from its
waist is

$$
w(z) = w_0\sqrt{1 + \left(\frac{z}{z_R}\right)^2}, \qquad z_R = \frac{\pi w_0^2}{\lambda},
$$

with $w_0$ the waist radius (`beam_waist_m`) and $z_R$ the Rayleigh range. The waist lies
`waist_distance_m` from the laser along each ray, where the laser is focused (0, the default,
puts it at the laser). A point at distance $r$ from the origin is at $z = r - r_0$ from the
waist. etendue reports the stripe's thickness as the full diameter $2w$.

**Focus the laser where you measure.** A 50 µm waist at 660 nm has $z_R = 11.9$ mm. Focused at
the laser, it is a 2.7 mm thick sheet 0.32 m out; focused at 0.32 m, it is 0.1 mm thick there.
For a measuring range of depth $\Delta$ centred on the waist, the stripe is thinnest at the range's
ends when $z_R = \Delta/2$, that is $w_0 = \sqrt{\lambda\Delta / 2\pi}$, and it is then $\sqrt{2}$
times its waist there. For $\Delta = 120$ mm at 660 nm that is a 112 µm waist radius.

The examples' calibration lasers use a 0.5 mm waist, whose Rayleigh range is 1.19 m: over their
0.4–0.6 m working distance the beam is nearly parallel, 1.06–1.12 mm thick.

## The stripe

Two planes meet in a line, so on a planar target the stripe is a straight segment: the fan's
plane intersected with the target's plane, clipped to the fan's angle and reach and to the
target's printed extent. On a PuzzlePole (a cylinder) it is an arc of an ellipse. The ground
truth samples each stripe every 1 mm in the target's frame and projects the samples with the
camera model, with the same visibility tests as the board's corners.

## The stripe in the image

How wide the stripe images decides how precisely its centre can be found. In a head design
etendue computes, at every point of the laser plane:

- the **beam's imaged thickness**: the beam diameter $2w$ projected across the imaged line,
  through the projection's Jacobian along the laser plane's normal (the stripe's own width, on a
  target whose surface contains that normal);
- the camera's **blur** there (defocus and diffraction, see [Defocus and the blur
  budget](models/blur.md));
- the **stripe width**, the two combined by root sum of squares.

On the Optics workspace's default head (a 25 mm lens at f/4 on 5.86 µm pixels, a 50 µm waist at
660 nm, a 120 mm deep window):

| Laser focused | Camera | Stripe width over the window |
|---|---|---|
| at the laser | as built | 36.0–39.8 px |
| at the laser | Scheimpflug | 37.1–37.4 px |
| at the aim point | as built | 1.8–20.9 px (the widest is the camera's defocus at the window's ends) |
| at the aim point | Scheimpflug | 1.8–8.9 px |

A laser part from the library that does not say where it is focused is focused at the aim point
when you build a head with it; the Laser panel has a button to refocus it there.

## Laser images

Synthetic laser images are not path-traced: a sub-millimetre beam is a light a path tracer samples
noisily. etendue renders the scene once, then adds the laser's light in a separate step from the
render's depth, surface normal and surface colour. Each surface point at distance $r$ from the
apex within the fan's plane and $\delta$ from the plane receives the irradiance

$$
E = \frac{P}{2\alpha}\cdot\frac{1}{r}\cdot
\frac{\exp\!\left(-2\delta^2 / w(r)^2\right)}{w(r)\sqrt{\pi/2}}\cdot\max(0,\;\hat{\mathbf{n}}_s\cdot\hat{\mathbf{l}}),
$$

the power $P$ (`power_w`, 0.5 mW by default) spread evenly over the fan angle, a Gaussian profile
across the plane, and the cosine between the surface normal and the direction to the laser. The
surface reflects it as a Lambertian surface of its colour. A point is lit only if the laser sees
it, so the stripe has shadows. The power counts as the camera responds to it: fold the camera's
spectral response at the laser's wavelength into `power_w`.

On the three-camera example's renders (132 laser images, 0.41–0.60 m from the laser), the
stripe's fitted width is 2–3.5 % wider than the beam's. Pixel integration explains all of it:
corrected for the pixel's area, the ratio is 0.999–1.002.

## Assumptions and limits

- **Only the across-stripe width is modelled.** Along the line the power is spread evenly over
  the fan angle; a real line's intensity profile along the line (bright centre, fading ends) is
  not.
- **An ideal Gaussian beam**: no beam-quality factor ($M^2 = 1$), no truncation by the laser's
  aperture, no astigmatism.
- **An exactly flat fan from one apex**, cut off sharply at its angle and reach.
- **Laser images** have no camera defocus (the renders are pinhole), no speckle, no specular
  reflection of the beam, and no interreflection.

API: `etendue-core` `laser` (fan, `GaussianBeamWidth`, `stripe_on_target`, `stripe_on_cylinder`),
`head` (`beam_px`); `etendue-synth` `laser`, `laser_render`.
