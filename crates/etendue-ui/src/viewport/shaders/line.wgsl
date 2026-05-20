// Unlit colored primitives for the etendue 3D viewport.
//
// Drives three pipelines that share this shader: the ground grid and world
// axes (line-list), explicit wireframe edge geometry (line-list), and point
// markers (point-list). Each vertex carries its own RGB color; there is no
// lighting — these are overlay/reference geometry.
//
// A per-draw model uniform lets the same vertex buffer be re-posed; the grid
// and axes pass an identity model, future wireframe entities pass their pose.

// Camera uniforms shared with the mesh shader (bind group 0). The layout must
// match `Globals` in mesh.wgsl exactly — the same bind group is bound.
struct Globals {
    view_proj: mat4x4<f32>,
    light_dir: vec4<f32>,
    eye: vec4<f32>,
};

@group(0) @binding(0)
var<uniform> globals: Globals;

// Per-draw model transform + tint (bind group 1). Layout matches `Model` in
// mesh.wgsl so the same bind group layout is reused; `normal` is ignored here.
struct Model {
    model: mat4x4<f32>,
    normal: mat4x4<f32>,
    tint: vec4<f32>,
};

@group(1) @binding(0)
var<uniform> model: Model;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) color: vec3<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec3<f32>,
};

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    let world_pos = model.model * vec4<f32>(in.position, 1.0);
    out.clip_position = globals.view_proj * world_pos;
    out.color = in.color;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return vec4<f32>(in.color * model.tint.rgb, model.tint.a);
}
