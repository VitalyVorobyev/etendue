# Optical System Designer — Handoff to Claude Code

## Purpose of this document

A personal R&D conversation with Claude reached a workable concept for an interactive design tool for industrial inspection optical systems (primarily laser triangulation 3D sensors, also multi-camera rigs). This document is the handoff to Claude Code for the **planning** phase. **Do not start coding from this document.** First read the existing code, then propose a crate layout and surface architectural decisions for the user to review.

## What the tool is

A desktop application that helps a mechanical/optical designer plan sensor prototypes by interactively exploring:

- Camera and laser geometry in a 3D scene
- Working volume and triangulation resolution
- Focus and defocus across the target
- Multi-camera overlap regions (for stereo / multi-view geometry)
- Parameter optimization for a given target distance and depth range

Primary user is the developer (PhD physics, deep Rust, computer-vision background, M-series Mac, no CUDA). This is a hobby / personal-research project, not a commercial product. Scope must stay finite.

## Existing foundation: `calibration-rs`

Located at `/Users/vitalyvorobyev/vision/calibration-rs` on the user's machine. **Read the actual source before designing anything.** A composable camera pipeline already exists with these pieces.

**Traits** (all generic over `S: RealField + Copy`):

- `ProjectionModel<S>`: `project_dir`, `unproject_dir`
- `DistortionModel<S>`: `distort`, `undistort`
- `SensorModel<S>`: `normalized_to_sensor`, `sensor_to_normalized`
- `IntrinsicsModel<S>`: `sensor_to_pixel`, `pixel_to_sensor`
- `CameraProject` (in `camera.rs`): thin object-safe trait used by downstream consumers

**Composition**: `Camera<S, P, D, Sm, K>` runs `pixel = intrinsics(sensor(distortion(projection(dir))))`.

**Concrete implementations already present**:

- `Pinhole` projection
- `NoDistortion`, `BrownConrady5<S>` (k1,k2,k3,p1,p2 with iterative undistort, default 8 iters)
- `IdentitySensor`, `HomographySensor<S>`, `ScheimpflugParams` (OpenCV-compatible tilt; compiles to homography)
- `FxFyCxCySkew<S>` intrinsics

**Serialization**: `CameraParams` with tagged enums per stage, built into a concrete `CameraModel` through type-erased `AnyProjection / AnyDistortion / AnySensor / AnyIntrinsics`. Uses `serde` with `flatten` and aliases (`tau_x` ↔ `tilt_x`).

**Architectural rules to preserve**:

- The new project depends on `calibration-rs`, does not vendor or fork it.
- Any addition to `calibration-rs` (thick lens, aperture) is to be upstreamed, with the user's review, not duplicated downstream.
- Reuse the existing serde conventions (tagged enums, flattened param structs, type-erased `Any*` wrappers) for any new component types.
- Keep generics over `S: RealField + Copy` for new traits where the existing pipeline does.

## What needs to be added

### Likely additions to `calibration-rs` — discuss with user before implementing

The existing pipeline is a pure geometric mapping. It says nothing about depth of field, aperture, or where the in-focus plane is. The design tool needs all three.

Three possible architectures for the thick-lens / aperture / focus model — Claude Code should propose a recommendation and let the user choose:

1. **New trait sibling** of projection: `ApertureModel<S>` with methods like `circle_of_confusion(depth, off_axis) -> S`. Keeps the existing pipeline pure-geometric. New trait gets its own `Any*` wrapper.
2. **Extend `ProjectionModel`** with optional defocus methods. Cleanest call site but pollutes a currently-minimal trait.
3. **`ThickLens<S>` wrapper struct** outside `calibration-rs`, in the new design crate. Holds a `Camera` plus principal-plane separation, f-number, focus distance. Computes defocus separately. Calibration-rs stays untouched.

Option 3 has the lowest coupling but means thick-lens info isn't first-class in the camera params file. Option 1 is probably the cleanest long-term answer. Discuss before deciding.

The parameters that need representing somewhere (probably in `calibration-rs` if the model lives there, otherwise in the new crate):

- Principal plane separation H–H' (or equivalently: effective focal length + back focal distance)
- Aperture / f-number (or entrance pupil diameter)
- Object-side focus distance
- Sensor distance (derivable from focus + thin-lens or thick-lens equation)

Defocus PSF model — start with geometric circle-of-confusion diameter. Optionally upgrade to a depth-dependent Gaussian later. Scheimpflug tilts the plane of best focus; the math is well-known and should be derived once in a comment / doc block, not pattern-matched from OpenCV.

### New crate: `optical-design-core` (kernel, no UI)

Geometric and physical kernel. Reusable outside the design tool — e.g. for `genicam-rs`-driven calibration pipelines or post-acquisition analysis.

**Laser line module**:

- `LaserPlane` struct: SE(3) pose, fan direction vectors, half-angle, wavelength
- Thickness function `w(d, θ)` — parametric. Start with Gaussian-beam-style waist evolution (`w(d) = w₀ √(1 + (d/zR)²)`), allow user override per-component. The `θ` axis along the fan can use a separate width function. Powell-lens behaviour is captured by the *resulting* line geometry, not by modelling the lens.
- Optional intensity `I(d, θ)` — Gaussian or top-hat along/across fan
- Intersection with arbitrary triangle mesh → 3D curve + per-point cross-section width
- Projection of intersected curve through a `Camera` → 2D curve in pixel space + per-point pixel width (geometric ⊕ defocus blur, added in quadrature)

**Scene module**:

- World poses for cameras and lasers (SE(3) using `nalgebra::Isometry3`)
- Target geometry: mesh, parametric primitives (plane, sphere, cylinder, step), calibration boards
- Working volume primitive: axis-aligned box, oriented box, or frustum

**Analysis module**:

- Defocus map over a target surface: per-point blur diameter in pixels using thick-lens + Scheimpflug
- Working volume voxelization with per-voxel: visibility from each camera, illumination by each laser, triangulation angle, projected pixel resolution
- Multi-camera overlap: which voxels are visible from N cameras (for stereo / multi-view reconstruction working volume)
- Triangulation resolution: dZ per pixel error along the laser plane (or per stereo correspondence)

**Optimization module** (deferred from MVP, but design with this in mind):

- Given target working distance and depth range, optimize: focal length, aperture, Scheimpflug tilt, baseline
- Objective is a weighted combination of resolution, depth-of-focus coverage, occlusion margin
- Black-box optimizer over a parameter struct (`argmin` crate is the obvious choice)

**Component bank**:

- Format spec — JSON or YAML schema for: image sensors (pixel pitch, resolution, well depth, read noise, QE — minimal at first), lenses (focal length, max aperture, mount, FoV, MTF placeholder), lasers (wavelength, fan angle, power, parametric line-width model)
- "Compatible with standard specifications" — needs research. The user mentioned this; I (Claude) don't know of a universal standard for full optical system component specs. Possibilities to investigate: Zemax glass catalog format (overkill, glass-only), GenICam GenApi (sensor-side, the user has background here from `genicam-rs`), or a clean custom schema with planned importers. Claude Code should ask the user what they had in mind before assuming a format.
- Seed bank: a small curated set entered by hand from datasheets (a few Sony Pregius sensors, a few Edmund / Thorlabs lenses, a few common machine-vision lasers like Z-Laser, Coherent StingRay)

### New crate: `optical-design-ui` (frontend)

Pure Rust per user decision. **Open question to resolve before coding**: `egui` + `wgpu` vs. `bevy`.

- **`egui` + `wgpu`** via `egui-wgpu` integration: simpler, full control of the render loop, lower learning curve given the user's existing Rust depth. Good 2D plots (egui_plot), immediate-mode UI for parameter panels.
- **`bevy`**: ECS-based scene graph is a natural fit for "many cameras, lasers, target meshes, gizmos." More structure but more framework to learn, and the UI integration (`bevy_egui` or `bevy_ui`) adds another moving part.

Recommendation: prototype a minimal scene (camera frustum + laser fan + working volume box + orbit camera) in both for ~half a day each before committing. The 3D viewport requirements are unexotic: orbit/pan/zoom, wireframe + shaded meshes, translucent volumes, lines/points/axes, picking. All standard wgpu territory either way.

**Screens** (in build order):

1. *System view* — 3D scene + parameter panel + real-time updates on slider drag. (MVP)
2. *Defocus map* — heatmap on target surface showing blur diameter in pixels.
3. *Working-volume analysis* — per-voxel coverage / triangulation angle / resolution as colored 3D voxels or slice plots.
4. *Multi-camera overlap* — N-view overlap region visualization.
5. *Component picker* — browse component bank, drag onto scene.
6. *Optimization* — set target distance and range, view optimization result (deferred).

## Constraints

- Pure Rust front-to-back. No Tauri / React / Vite / Bun.
- M-series Mac primary dev machine; Metal backend via wgpu.
- No CUDA, no NVIDIA-specific code paths.
- Simulation only — no hardware acquisition (that is `genicam-rs`'s job).
- No Blender dependency. Considered and dropped earlier in the discussion.

## Explicitly out of scope

- Realistic synthetic image rendering: path tracing, BRDFs, speckle, full sensor noise stack. The user explicitly does not want this. Geometric line projection + parametric width + defocus blur is sufficient for the design questions this tool answers.
- Component-level optical design (Zemax territory): ray tracing through prescribed surfaces, aspherics, wavefront analysis, Seidel / Zernike aberrations. Components in this tool are **parametric** — thick lens with f-number and focus, laser characterized by its output line geometry. Powell lenses are not modeled; their effect on the laser line is.
- Wavelength-dependent effects beyond a single nominal wavelength
- Polarization
- Coherent diffraction effects
- Speckle modeling
- Any database of real lens prescriptions

## MVP definition (kill criterion: does this help size a real triangulation sensor?)

Single screen:

1. One camera + one laser + one target plane in a 3D scene
2. Parameter sliders for: camera focal length, aperture, focus distance, Scheimpflug tilts (τx, τy); laser pose, fan angle, beam waist
3. Real-time visualization of:
   - Working volume (region where target is simultaneously in focus, illuminated by the laser, and visible to the camera)
   - Projected laser line on the simulated image (geometric width ⊕ defocus blur in pixels)
   - Defocus heatmap on target

If this is genuinely useful for sizing a real sensor, the project earns the right to grow into multi-camera rigs, the component bank, and optimization.

## What Claude Code should do first

In order:

1. Read `calibration-rs` source. Confirm understanding of the trait decomposition and serialization patterns before proposing anything.
2. Propose a workspace layout: where `calibration-rs` lives relative to the new crates, whether to use a Cargo workspace, what the crate names should be.
3. Surface the architectural decisions for user review:
   - Where the thick-lens / aperture model lives (the three options above)
   - `egui+wgpu` vs. `bevy` — recommend a prototyping approach, don't pick blindly
   - Component bank format — ask the user what "compatible with standard specifications" means concretely
   - Defocus PSF: geometric circle-of-confusion only, or depth-dependent Gaussian
4. Turn the MVP into a concrete milestone list with explicit kill criteria.
5. **Wait for user sign-off before writing any code.**

## User preferences to respect

- Precise technical distinctions over loose summaries. The user is a domain expert; do not over-explain physics or Rust basics.
- No speculative "fixes" to existing code without verifying they are wanted. The user has explicitly flagged this preference. Same caution applies to speculative scaffolding in new code.
- Strong preference for building from scratch with full control over implementation, vs. depending on high-level frameworks or commercial dependencies.
- Existing tooling already configured: `cargo-sweep`, global `target-dir` in `~/.cargo/config.toml`, `debug = 1` in dev profile. Don't disturb these.

## Pointers to related work in the user's ecosystem

- `genicam-rs` — GigE Vision client in Rust; will eventually feed real images into calibration pipelines that share the camera models with this tool. Awareness only; not a direct dependency.
- ChArUco reconstruction crates (`chess-corners`, `calibration-targets`) — separate project; same calibration-rs base.
- Lagrangian visual-RL project — unrelated to this tool, mentioned only to flag that the user already has several concurrent projects; do not let this one sprawl.