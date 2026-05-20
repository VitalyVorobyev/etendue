// Flat-shaded triangle meshes for the etendue 3D viewport.
//
// One directional light plus a constant ambient term — Lambert diffuse, no
// specular. The same shader serves both the opaque and the translucent pass;
// the alpha that decides between them comes from the per-instance tint, and
// the pass difference (blend state, depth-write) lives entirely on the Rust
// pipeline objects, not here.

// Camera + lighting uniforms, shared by every pass (bind group 0).
struct Globals {
    // Combined view-projection matrix (world space -> clip space).
    view_proj: mat4x4<f32>,
    // World-space light direction (points *toward* the light), xyz; w unused.
    light_dir: vec4<f32>,
    // World-space camera/eye position, xyz; w unused.
    eye: vec4<f32>,
};

@group(0) @binding(0)
var<uniform> globals: Globals;

// Per-draw uniform: a model transform plus an RGBA tint (bind group 1).
struct Model {
    // Model -> world transform.
    model: mat4x4<f32>,
    // Normal matrix (inverse-transpose of the model upper 3x3), stored as a
    // mat4x4 for std140-friendly 16-byte alignment; only the upper 3x3 is used.
    normal: mat4x4<f32>,
    // Flat RGBA tint multiplied into the vertex color. Alpha < 1 routes the
    // mesh through the translucent pass.
    tint: vec4<f32>,
};

@group(1) @binding(0)
var<uniform> model: Model;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec3<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_normal: vec3<f32>,
    @location(1) color: vec3<f32>,
};

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    let world_pos = model.model * vec4<f32>(in.position, 1.0);
    out.clip_position = globals.view_proj * world_pos;
    // Transform the normal by the normal matrix's upper 3x3.
    let n = (model.normal * vec4<f32>(in.normal, 0.0)).xyz;
    out.world_normal = n;
    out.color = in.color;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // Renormalize after interpolation; flat meshes still benefit from it.
    let n = normalize(in.world_normal);
    let l = normalize(globals.light_dir.xyz);
    // Two-sided lighting: faces pointing away from the light are still lit by
    // the back lobe, which keeps the translucent test quad readable from both
    // sides without a dedicated back-face pass.
    let lambert = abs(dot(n, l));
    let ambient = 0.28;
    let intensity = ambient + (1.0 - ambient) * lambert;
    let rgb = in.color * model.tint.rgb * intensity;
    return vec4<f32>(rgb, model.tint.a);
}
