// Example: screen-space sized (in-world) vertex shader
// - Keeps world rotation
// - Ignores any scale baked into model_matrix by normalizing axes
// - Scales mesh so it appears `screen_px` pixels tall/wide (approx) on screen
//
// Bind groups:
//   @group(0) binding(0) camera uniform (same as your other shaders)
//   @group(0) binding(1) screen params uniform (viewport size in pixels)
//
// Vertex inputs:
//   @location(0) position: vec3<f32>
// Instance inputs:
//   @location(5..8) model matrix columns
//   @location(9) material color (optional passthrough)
//   @location(12) desired screen size in pixels (per-instance)

struct CameraUniform {
    view_proj: mat4x4<f32>,
};

struct ScreenUniform {
    viewport: vec2<f32>, // (width_px, height_px)
   // _pad: vec2<f32>,
};

@group(0) @binding(0)
var<uniform> camera: CameraUniform;

@group(0) @binding(1)
var<uniform> screen: ScreenUniform;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) color: vec3<f32>,
};

struct InstanceInput {
    @location(5) model_matrix_0: vec4<f32>,
    @location(6) model_matrix_1: vec4<f32>,
    @location(7) model_matrix_2: vec4<f32>,
    @location(8) model_matrix_3: vec4<f32>,

    @location(9) material: vec3<f32>,
    @location(12) screen_px: f32,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec3<f32>,
};

fn safe_normalize(v: vec3<f32>) -> vec3<f32> {
    let m2 = max(dot(v, v), 1e-20);
    return v * inverseSqrt(m2);
}

@vertex
fn vs_main(v: VertexInput, i: InstanceInput) -> VertexOutput {
    var out: VertexOutput;

    let m = mat4x4<f32>(
        i.model_matrix_0,
        i.model_matrix_1,
        i.model_matrix_2,
        i.model_matrix_3,
    );

    // Rotation-only basis (strip scale, keep orientation)
    let x0 = m[0].xyz;
    let y0 = m[1].xyz;

    let x = safe_normalize(x0);
    let y = safe_normalize(y0 - dot(y0, x) * x); // Gram-Schmidt
    let z = cross(x, y);

    let t = m[3].xyz;

    // Project anchor and 1-unit steps in world to estimate pixels-per-world-unit
    let clip0 = camera.view_proj * vec4<f32>(t, 1.0);
    let clipx = camera.view_proj * vec4<f32>(t + x, 1.0);
    let clipy = camera.view_proj * vec4<f32>(t + y, 1.0);

    let ndc0 = clip0.xy / clip0.w;
    let ndcx = clipx.xy / clipx.w;
    let ndcy = clipy.xy / clipy.w;

    // NDC [-1..1] -> pixels
    let ndc_to_px = screen.viewport * 0.5;
    let dx_px = (ndcx - ndc0) * ndc_to_px;
    let dy_px = (ndcy - ndc0) * ndc_to_px;

    // Robust ppu estimate even when one axis points toward camera
    let ppu = max(length(dx_px), length(dy_px));
    let scale = i.screen_px / max(ppu, 1e-6);

    // Apply constant-screen-size scale in local space, then rotate+translate in world
    let local = v.position * scale;
    let world = t + local.x * x + local.y * y + local.z * z;

    out.clip_position = camera.view_proj * vec4<f32>(world, 1.0);
    out.color = i.material;
    return out;
}
