@vertex
fn vs_fullscreen(@builtin(vertex_index) i: u32)
    -> @builtin(position) vec4<f32>
{
    var pos: vec2<f32>;

    if (i == 0u) {
        pos = vec2(-1.0, -1.0);
    } else if (i == 1u) {
        pos = vec2( 3.0, -1.0);
    } else {
        pos = vec2(-1.0,  3.0);
    }

    return vec4<f32>(pos, 0.0, 1.0);
}
