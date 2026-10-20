# Working volume and rig coverage

A measuring cell has a **working volume**: the region where it can actually measure. For one
triangulation head that region lies in its laser plane, and the head's map shows it (see
[Triangulation heads](heads.md)). A rig of several cameras, with or without lasers, measures in
3-D, and its working volume is where enough cameras see a point sharply. Stereo needs two views of
a point; a laser–camera pair needs its camera to see its own laser line there. etendue counts both
over a grid of voxels, so you can compare layouts by the volume they cover rather than by eye.

## Rig layouts

A rig's `layout` places `count` identical cameras on a ring around the rig's +Z axis (see
[Scenes and scenarios](../scenes.md) for the fields). Camera $i$ sits at

$$
\phi_i = \text{phase} + \frac{2\pi i}{\text{count}}, \qquad \text{radius } \rho, \text{ in the rig's } z = 0 \text{ plane},
$$

its axis tilted toward the rig axis by $\varphi$, by default $\varphi = \arctan(\rho / d)$, so that
every axis passes through the working point $(0, 0, d)$. A camera's image +X runs along the ring
(`tangential`), away from the axis (`radial`), or along the rig's +X (`common`).

- **Stereo.** Two cameras with `image_x: "common"` are a classical stereo pair: at $\pm\rho$ on
  the rig's x axis (a baseline of $2\rho$), their image rows lined up, and, with no tilt, their
  images parallel.
- **Lasers.** A layout may give each camera its own line laser. Either the lasers stand on the
  ring (at their own radius and an azimuth offset from their camera), aimed at the working point
  with the line across their camera's image, or each is fixed to its camera's body at a given pose,
  as a [triangulation head](heads.md) carries it.
- **Refocus.** In the Rigs workspace, a camera with optics can be refocused on the working point:
  its focus distance becomes $d / \cos\varphi$, the distance along its axis.

## What a voxel counts

The box is given in the rig's frame, with a number of voxels per axis (up to 512). For each voxel
centre $\mathbf{p}$:

- **A camera sees it** when $\mathbf{p}$ is in front of the camera and projects onto its sensor,
  and, for a camera with optics, the total blur there (circle of confusion and Airy disc, root sum
  of squares) is at most the **max blur** (see [Defocus and the blur budget](blur.md)). The Airy
  disc uses the wavelength of the camera's own laser, or 550 nm without one. A camera without
  optics is a pinhole and has no blur.
- **A pair measures it** when its laser's fan meets the voxel and its camera sees the voxel's
  centre. "Meets" means the fan's plane cuts the voxel (the centre is within half the voxel's
  extent across the plane, $\tfrac12\sum_i |n_i|\,\Delta_i$ for the plane's normal $\mathbf{n}$ and
  voxel edges $\Delta_i$), in front of the laser, within its fan angle and reach.

Each voxel gets two counts: **views**, the cameras that see it, and **lit**, the pairs that
measure it. The volume where at least $m$ cameras see and at least $k$ pairs measure is the
number of such voxels times the voxel's volume. The Rigs workspace reports the volume **seen by
1+**, **seen by 2+**, **seen by all** cameras, **measured** (by at least one pair) and **by 2+
pairs**, and draws the counts on slices through the box.

Each camera's **sharp range** along its own axis is its depth of field for the blur that leaves
room for diffraction, $\sqrt{b_{\max}^2 - a^2}$: the same test as the voxels, along one line. The
default stereo pair (an IMX174 with a 25 mm lens at f/4, cameras 120 mm apart converging at
400 mm, refocused there, a 2 px limit) is sharp from 391 to 409 mm along each axis, so its
two-view volume is a thin slab: a stereo rig with an f/4 lens is a sharp-focus instrument.

## How well a grid measures a volume

A voxel counts entirely or not at all, according to its centre, so the volume is right to about
half a voxel over the region's boundary. For a parallel pinhole pair (baseline 0.1 m, 1000 px
focal length, a 1000 × 800 image) both cameras see, between depths $z_0$ and $z_1$,

$$
V = \int_{z_0}^{z_1} (a z - B)\,c\,z \;dz, \qquad a = 1,\; c = 0.8,\; B = 0.1\ \text{m},
$$

which is 0.042667 m³ over 0.2–0.6 m. Counted on a 128³ grid it is 0.042957 m³, 0.68 % more.

**A ring of three heads.** Three IMX174 cameras with 25 mm lenses at f/4 on an 80 mm ring
converging at 0.35 m, each with a 660 nm laser on its body 100 mm off its axis, counted over a box
±150 mm wide and 0.2–0.5 m deep with a 3 px limit, on 64³ voxels (4.7 mm each): 1154 voxels are
seen by all three cameras, and 1974 are measured by at least one pair. The count takes about
13 ms in WebAssembly.

Try it: change the number of cameras, the ring radius, the working distance and the blur limit,
and watch the slice: how many cameras see each point sharply, and where the laser–camera pairs
measure.

<div class="etendue-demo" data-demo="coverage"></div>

The Rigs workspace also plots the volumes against the tilt $\varphi$ ("Coverage against the tilt
φ"), which shows how far the cameras should converge.

## Assumptions and limits

- **No occlusion.** A camera sees a voxel if the voxel projects onto its sensor; the part, the
  fixture or the robot in between are not considered.
- **No surface.** Coverage is a property of the rig and a volume. Whether a real surface faces
  the camera, or how a stripe looks on it, is for the head map, the ground truth and the
  renders.
- **Centres only.** Visibility and blur are judged at the voxel's centre; only the laser test
  uses the voxel's extent.
- **The camera models** are the cameras' own: distortion and Scheimpflug tilts are included,
  through the same projection as everywhere else.

API: `etendue-core` `rig::coverage`, `rig::sharpness`; `@etendue/wasm` `rigCoverage`,
`rigSharpness`, `expandRig`.
