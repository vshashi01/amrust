@group(0) @binding(0)
var scene_tex: texture_2d<f32>;
@group(0) @binding(1)
var scene_samp: sampler;

@group(0) @binding(2)
var mask_tex: texture_2d<f32>;
@group(0) @binding(3)
var mask_samp: sampler;

@group(0) @binding(4)
var<uniform> texel_size: vec2<f32>; // (1/width, 1/height)

@fragment
fn fs_outline(@builtin(position) pos: vec4<f32>)
    -> @location(0) vec4<f32>
{
    let uv = pos.xy * texel_size;

    let center = textureSample(mask_tex, mask_samp, uv).r;

    // Not selected → just show scene
    if (center < 0.5) {
        return textureSample(scene_tex, scene_samp, uv);
    }

    // Neighbor samples (WGSL-legal)
    let left  = textureSample(mask_tex, mask_samp, uv + vec2(-texel_size.x, 0.0)).r;
    let right = textureSample(mask_tex, mask_samp, uv + vec2( texel_size.x, 0.0)).r;
    let up    = textureSample(mask_tex, mask_samp, uv + vec2(0.0, -texel_size.y)).r;
    let down  = textureSample(mask_tex, mask_samp, uv + vec2(0.0,  texel_size.y)).r;

    // Edge detection
    if (left < 0.5 || right < 0.5 || up < 0.5 || down < 0.5) {
        // Outline color
        return vec4<f32>(1.0, 0.8, 0.1, 1.0);
    }

    // Interior pixel
    return textureSample(scene_tex, scene_samp, uv);
}

