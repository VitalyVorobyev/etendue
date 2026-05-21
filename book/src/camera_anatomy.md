# Camera Anatomy (M8)

M8 added a translucent overlay that shows the internal structure of a
`CameraEntity` inside the 3D viewport: the image sensor plane, the two
principal planes `H` and `H'`, and the lens aperture ring.

## Motivation

The camera entity was already a frustum wireframe. Without the internal
overlay, a camera with Scheimpflug tilt or a wide principal-plane gap looks
identical to a thin-lens camera at the same pose — the frustum wires tell
you where the camera *looks*, but not how the lens is focused or where the
sensor sits. M8 makes those invisible parameters visible.

## What is rendered

All anatomy geometry is generated in **camera-local** coordinates by helpers
in `viewport::scene`, then placed in the world by the camera's `pose` matrix
(the same model-matrix pattern every other entity uses).

### Sensor plane

A translucent rectangle sized `resolution × pixel_pitch_m` placed at
`z = −v₀`, where `v₀ = sensor_distance(s_o, g, f)` is the image-side
conjugate of the focus plane. For a well-focused camera this is very close
to the rear focal plane; as the focus slider moves the sensor plane follows.

The rectangle is **fronto-parallel** — Scheimpflug tilt is not visualised in
the anatomy overlay (the tilt is captured in the projection model and the M4
plane of best focus; visualising the tilt here is a follow-up).

A `None` return from `sensor_distance` (object inside the front focal point)
simply adds no sensor drawable — the entity contributes no broken geometry.

### Principal planes `H` and `H'`

`H'` (the rear principal plane) sits at the camera-local origin `z = 0`;
`H` (the front principal plane) at `z = −g`, where `g` is
`PhysicalOptics::principal_gap_m`. For a thin lens `g = 0` and the two
planes coincide.

Each plane is a square `1.4 × aperture_diameter` on a side, drawn as a
double-sided translucent quad (two coplanar triangles per face, facing
`±z`, so the quad shades correctly from either side of the plane under the
directional light).

### Aperture ring

A 48-segment polygon approximating a circle of diameter
`effective_focal_length_m / f_number` (the geometric entrance pupil), drawn
as an opaque line list at `z = −g/2` — midway between `H` and `H'`, where
the physical aperture stop of a thick lens conventionally sits. For a thin
lens the ring lies in the coincident principal-plane.

## Alpha values

| element | alpha | pipeline |
|---|---|---|
| sensor plane | 0.35 | `mesh_translucent` |
| `H` plane | 0.35 | `mesh_translucent` |
| `H'` plane | 0.35 | `mesh_translucent` |
| aperture ring | 1.0 (opaque) | `lines` |

The anatomy alpha is lower than the laser fan (0.30) so anatomy quads never
dominate the scene visually.

## Colours

| element | RGB |
|---|---|
| sensor plane | cool blue `[0.30, 0.50, 0.85]` |
| `H` plane | pale cyan `[0.55, 0.85, 0.95]` |
| `H'` plane | pale lavender `[0.80, 0.78, 0.95]` |
| aperture ring | warm gold `[0.95, 0.85, 0.40]` |

The two principal planes use distinct colours so a non-zero gap reads as two
separate planes when the inter-principal slider is non-zero.

## Interaction with the parameter panel

Dragging any physical-optics slider in the camera section causes a full
`rebuild_scene`, so all anatomy geometry is regenerated each time. The
anatomy is always shown — there is no toggle in v0.1.0.
