struct CameraUniform {
    view_proj: mat4x4<f32>,
};

@group(0) @binding(0)
var<uniform> camera: CameraUniform;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) color: vec3<f32>,
    @location(2) tex_coords: vec2<f32>,
    @location(3) use_texture: i32, // 0 or 1
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>, 
    @location(0) color: vec3<f32>,
    @location(1) tex_coords: vec2<f32>,
    @location(2) use_texture: i32, // 0 or 1
    @location(3) world_pos: vec3<f32>,
}


struct InstanceInput {
    @location(5) model_matrix_0: vec4<f32>,
    @location(6) model_matrix_1: vec4<f32>,
    @location(7) model_matrix_2: vec4<f32>,
    @location(8) model_matrix_3: vec4<f32>,
    @location(9) material: vec3<f32>,
    // @location(9) normal_matrix_0: vec3<f32>,
    // @location(10) normal_matrix_1: vec3<f32>,
    // @location(11) normal_matrix_2: vec3<f32>,
}


@vertex
fn vs_main(model: VertexInput, instance: InstanceInput) -> VertexOutput {
    var out: VertexOutput;
    let model_matrix = mat4x4<f32>(instance.model_matrix_0, instance.model_matrix_1, instance.model_matrix_2, instance.model_matrix_3,);
    out.color = model.color;
    out.tex_coords = model.tex_coords;
    out.use_texture = model.use_texture;
    let world_pos = (model_matrix * vec4<f32>(model.position, 1.0)).xyz;
    out.world_pos = world_pos;
    out.clip_position = camera.view_proj * vec4<f32>(world_pos, 1.0);

    return out;
}
