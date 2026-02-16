struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec3<f32>,
    @location(1) world_pos: vec3<f32>,
    @location(2) world_normal: vec3<f32>,
    @location(3) use_lighting: i32,
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

struct LightUniform {
    direction: vec3<f32>,
    intensity: f32,
    color: vec3<f32>,
    ambient: f32,
}

@group(0) @binding(2)
var<uniform> light: LightUniform;

@group(1) @binding(0)
var<uniform> view_clip: ClipUniform;

@group(2) @binding(0)
var<uniform> mesh_clip: ClipUniform;

fn clip_pass(clip: ClipUniform, world_pos: vec3<f32>) -> bool {
    if (clip.enabled == 0u) {
        return true;
    }

    var axis_value: f32;
    if (clip.axis == 0u) {
        axis_value = world_pos.x;
    } else if (clip.axis == 1u) {
        axis_value = world_pos.y;
    } else {
        axis_value = world_pos.z;
    }

    let dist = clip.axis_sign * axis_value + clip.d;
    if (dist > 0.0) {
        return false;
    }

    if (clip.finite == 0u) {
        return true;
    }

    var uv: vec2<f32>;
    if (clip.axis == 0u) {
        uv = vec2(world_pos.y, world_pos.z);
    } else if (clip.axis == 1u) {
        uv = vec2(world_pos.x, world_pos.z);
    } else {
        uv = vec2(world_pos.x, world_pos.y);
    }

    return all(uv >= clip.bounds_min) && all(uv <= clip.bounds_max);
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    if (!clip_pass(view_clip, in.world_pos) || !clip_pass(mesh_clip, in.world_pos)) {
        discard;
    }

    let albedo = in.color;
    if (in.use_lighting == 0) {
        return vec4<f32>(albedo, 1.0);
    }

    let n = normalize(in.world_normal);
    // light.direction is the direction rays travel; use L = -direction for shading.
    let l = normalize(-light.direction);
    let diffuse = max(dot(n, l), 0.0);
    let lit_color = albedo * (light.ambient + light.intensity * diffuse * light.color);

    return vec4<f32>(lit_color, 1.0);
}
