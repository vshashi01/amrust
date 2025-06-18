
struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>, 
    @location(0) color: vec3<f32>,
    @location(1) tex_coords: vec2<f32>,
    @location(2) use_texture: i32, // 0 or 1
}

@group(1) @binding(0)
var t_diffuse: texture_2d<f32>;
@group(1) @binding(1)
var s_diffuse: sampler;

@fragment 
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    if (in.use_texture >= 0) {
        return textureSample(t_diffuse, s_diffuse, in.tex_coords);
    } else {
        return vec4<f32>(in.color, 1.0);
    }
}