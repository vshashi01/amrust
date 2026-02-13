
struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>, 
    @location(0) color: vec3<f32>,
    @location(1) world_pos: vec3<f32>,
}

struct ClipUniform {
    axis: u32,
    enabled: u32,
    finite: u32,
    _pad0: u32,
    axis_sign: f32,
    d: f32,
    _pad1: vec2<f32>,
    bounds_min: vec2<f32>,
    bounds_max: vec2<f32>,
}

@group(1) @binding(0)
var<uniform> view_clip: ClipUniform;

@group(2) @binding(0)
var<uniform> mesh_clip: ClipUniform;

// fn clip_pass(clip: ClipUniform, world_pos: vec3<f32>) -> bool {
//     if (clip.enabled == 0u) {
//         return true;
//     }

//     var axis_value: f32;
//     if (clip.axis == 0u) {
//         axis_value = world_pos.x;
//     } else if (clip.axis == 1u) {
//         axis_value = world_pos.y;
//     } else {
//         axis_value = world_pos.z;
//     }

//     let dist = clip.axis_sign * axis_value + clip.d;
//     if (dist > 0.0) {
//         return false;
//     }

//     if (clip.finite == 0u) {
//         return true;
//     }

//     var uv: vec2<f32>;
//     if (clip.axis == 0u) {
//         uv = vec2(world_pos.y, world_pos.z);
//     } else if (clip.axis == 1u) {
//         uv = vec2(world_pos.x, world_pos.z);
//     } else {
//         uv = vec2(world_pos.x, world_pos.y);
//     }

//     return all(uv >= clip.bounds_min) && all(uv <= clip.bounds_max);
// }

@fragment 
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // if (!clip_pass(view_clip, in.world_pos) || !clip_pass(mesh_clip, in.world_pos)) {
    //     discard;
    // }
    return vec4<f32>(in.color, 1.0);
}
