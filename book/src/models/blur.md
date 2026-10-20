# Defocus and the blur budget

A corner detector needs a sharp corner, and a laser-line extractor a narrow, well-shaped stripe.
Both degrade as the image blurs. In a calibration cell the blur is set by geometry: a camera is
sharp only around its focus distance, and the board or the part moves through a range of depths.
etendue keeps a **blur budget**: the total blur, in pixels, that a point may have and still count
as seen. It combines two physical sources, defocus and diffraction, and it decides where a head
is usable, what a rig covers and what the optimiser aims for.

## Geometric defocus: the circle of confusion

A point off the plane of best focus forms its sharp image in front of or behind the sensor. The
cone of light from the exit pupil to that image point is cut by the sensor in a disc, the
**circle of confusion** (CoC).

<svg viewBox="0 0 600 180" role="img" aria-label="Circle of confusion from the exit pupil cone" style="max-width:600px;width:100%;color:currentColor">
  <g stroke="currentColor" fill="none" stroke-width="1.5">
    <line x1="20" y1="90" x2="585" y2="90" stroke-dasharray="4 3"/>
    <line x1="470" y1="40" x2="470" y2="140" stroke-width="2"/>
    <line x1="150" y1="30" x2="150" y2="150" stroke-width="3"/>
    <path d="M150 30 L420 90 L150 150"/>
    <path d="M420 90 L470 101 M420 90 L470 79" stroke-dasharray="3 2"/>
    <line x1="490" y1="79" x2="490" y2="101"/>
    <path d="M150 165 L420 165 M150 160 L150 170 M420 160 L420 170"/>
    <path d="M420 120 L470 120 M420 115 L420 125 M470 115 L470 125"/>
  </g>
  <circle cx="420" cy="90" r="3" fill="currentColor"/>
  <g fill="currentColor" font-family="sans-serif" font-size="13">
    <text x="105" y="25">exit pupil, ⌀ Dₓ</text>
    <text x="395" y="80">image</text>
    <text x="478" y="35">sensor</text>
    <text x="497" y="95">c</text>
    <text x="275" y="160">ℓ</text>
    <text x="440" y="137">δ</text>
  </g>
</svg>

By similar triangles,

$$
c = D_x\,\frac{\delta}{\ell}, \qquad D_x = m_p\,\frac{f}{N},
$$

where $D_x$ is the exit pupil's diameter, $\ell$ the distance from the exit pupil to the image
point, and $\delta$ the distance between the image point and the sensor. The image point comes
from the Gaussian lens equation: an object at distance $d_P$ from H images at
$s = f d_P / (d_P - f)$ behind H′. For a thin lens with its pupil on it ($m_p = 1$, the exit
pupil at H′) this is the textbook

$$
c = \frac{f}{N}\,\frac{|s - v_0|}{s} .
$$

The CoC in pixels is $c / p$, with $p$ the pixel pitch. With an untilted sensor, $c$ depends only
on the point's depth, not on where it is in the field; an ideal lens has no other off-axis blur.
With a [tilted sensor](../scheimpflug.md), $\delta$ is taken at the sensor's position under the
image point, which makes the blur depend on the field position.

**Worked example.** 25 mm at f/4 on 5.86 µm pixels, focused at 0.5 m (from the image plane), on
the axis:

| Depth | 450 mm | 480 mm | 500 mm | 520 mm | 550 mm | 600 mm |
|---|---|---|---|---|---|---|
| CoC | 7.02 px | 2.62 px | 0 | 2.41 px | 5.68 px | 10.36 px |

The blur grows faster on the near side than on the far side, so the in-focus range is not
centred on the focus distance.

## Diffraction: the Airy disc

Even a perfectly focused point is blurred by diffraction at the aperture. etendue takes the
diameter of the Airy disc to its first dark ring,

$$
a = 2.44\,\lambda\,N_w, \qquad N_w = N\left(1 + \frac{m_0}{m_p}\right),
$$

with $N_w$ the working f-number (see [Physical cameras](../physical_cameras.md)) and $\lambda$
the wavelength: the laser's for a triangulation head, 550 nm for a camera without a laser of its
own. For the example, $N_w = 4.22$ and the Airy disc is 1.16 px at 660 nm, 0.97 px at 550 nm.
It grows in proportion to the f-number: stopping down from f/4 to f/16 shrinks defocus by four
and grows diffraction by four. That trade is what the [head optimiser](heads.md) balances.

## The total

The two sources are combined as independent blurs, by root sum of squares:

$$
b = \sqrt{c^2 + a^2} .
$$

This is the **total blur** the Optics and Rigs workspaces show, map and limit (`blur_px` in the
kernel's output; `coc_px` and `airy_px` are its parts). A point counts as sharp when $b$ is at
most the **max blur** you set (2 px by default).

## Depth of field

Along the axis, the blur reaches a limit $c_{\max}$ at two depths. Solving $c = c_{\max}$ for
the image distance $s$ gives, with $s_{E'}$ the exit pupil's distance behind H′,

$$
s_{\text{near}} = \frac{D_x v_0 - c_{\max}\,s_{E'}}{D_x - c_{\max}}, \qquad
s_{\text{far}} = \frac{D_x v_0 + c_{\max}\,s_{E'}}{D_x + c_{\max}},
$$

and each image distance maps back to an object depth by the lens equation. When $s_{\text{far}}$
reaches the focal length the far limit is at infinity: the focus distance where that happens is
the **hyperfocal distance**.

For the example (depths from the image plane):

| Limit | Near | Far | Hyperfocal |
|---|---|---|---|
| 1 px | 492.2 mm | 508.1 mm | 26.7 m |
| 2 px | 484.6 mm | 516.5 mm | 13.4 m |

A rig judges sharpness with the total blur, so its sharp range along each camera's axis is the
depth of field for the CoC that leaves room for diffraction, $c_{\max} = \sqrt{b_{\max}^2 - a^2}$.
When diffraction alone exceeds the limit, the camera is sharp nowhere. The studio's default
stereo pair (IMX174 with a 25 mm lens at f/4, refocused on 400 mm, a 2 px limit) is sharp from
391 to 409 mm along each axis.

Try it: move the f-number and the focus, and watch the defocus curve narrow while the Airy floor
rises. The depth of field is where the total stays under the limit.

<div class="etendue-demo" data-demo="blur"></div>

## Blur as a Gaussian

etendue reports blur as a diameter. To use it as a Gaussian kernel (to blur an image by hand, or
to compare with a detector's tolerance), match the Gaussian's full width at half maximum to the
diameter:

$$
\sigma = \frac{b}{2\sqrt{2\ln 2}} \approx \frac{b}{2.355} .
$$

A 2 px blur is then a Gaussian with $\sigma \approx 0.85$ px.

## Assumptions and limits

- **Ideal lens.** No aberrations: spherical aberration, coma, astigmatism and field curvature
  would add blur that varies across the field. Real lenses are rarely diffraction-limited at
  wide apertures, so treat a predicted blur below about 1 px with suspicion.
- **Shapes are reduced to diameters.** The defocus disc and the Airy pattern are not convolved;
  the root sum of squares is an approximation that is good when one source dominates and
  reasonable in between.
- **Pixel integration** is not part of the budget. A pixel's area adds $1/12\ \text{px}^2$ to the
  variance of any spot, which matters for sub-pixel stripe widths.
- **Tilted sensors.** Off the plane of best focus, the blur's size is first order in the tilt:
  within $0.6\tan\theta$ of a traced ray bundle, about 5 % at 5°. The zero-blur plane itself is
  exact.
- **Synthetic images are sharp.** Renders use pinhole cameras, so the images etendue renders have
  no defocus or diffraction blur; the blur budget is a design tool, not part of the image model
  (see [Synthetic images](synthetic_images.md)).

API: `etendue-core` `PhysicalCamera::coc_px`, `airy_diameter_m`, `depth_of_field`;
`rig::sharpness`.
