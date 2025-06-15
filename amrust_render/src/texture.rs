use std::num::NonZero;

use anyhow::Result;

pub const MAX_TEXTURE_SIZE: u32 = 8192;
pub const MAX_BINDING_ARRAY_ELEMENTS_PER_SHADER_STAGE: u32 = 6; // Maximum number of texture bindings per shader stage
pub const MAX_BINDING_ARRAY_SAMPLERS_PER_SHADER_STAGE: u32 = 1; // Maximum number of sampler bindings per shader stage

pub struct Texture {
    #[allow(dead_code)]
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub label: String,
}

impl Texture {
    pub fn from_bytes(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        bytes: &[u8],
        label: &str,
    ) -> Result<Self> {
        let img = image::load_from_memory(bytes)?;
        Self::from_image(device, queue, &img, label)
    }

    pub fn from_image(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        img: &image::DynamicImage,
        label: &str,
    ) -> Result<Self> {
        let img_rgba8 = img.to_rgba8();
        let dimensions = img_rgba8.dimensions();

        let size = wgpu::Extent3d {
            width: dimensions.0,
            height: dimensions.1,
            depth_or_array_layers: 1,
        };

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        queue.write_texture(
            texture.as_image_copy(),
            &img_rgba8,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4 * dimensions.0),
                rows_per_image: Some(dimensions.1),
            },
            size,
        );

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        Ok(Self {
            texture,
            view,
            label: label.to_string(),
        })
    }
}

pub struct DepthTexture {
    #[allow(dead_code)]
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    #[allow(dead_code)]
    pub label: String,
}

impl DepthTexture {
    pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

    pub fn create_depth_texture(device: &wgpu::Device, size: wgpu::Extent3d) -> Self {
        let desc = wgpu::TextureDescriptor {
            label: Some("Depth Texture"),
            mip_level_count: 1,
            size,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: Self::DEPTH_FORMAT,
            usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[], // not needed since this is not for sampling into an image buffer
        };

        let texture = device.create_texture(&desc);
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        Self {
            texture,
            view,
            label: "Depth texture".to_string(),
        }
    }
}

pub fn generate_basic_texture_sampler(device: &wgpu::Device) -> wgpu::Sampler {
    device.create_sampler(&wgpu::SamplerDescriptor {
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        address_mode_w: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Nearest,
        mipmap_filter: wgpu::FilterMode::Nearest,
        ..Default::default()
    })
}

pub fn generate_basic_texture_bind_group<const TEX_BINDING: u32, const SAMPLER_BINDING: u32>(
    device: &wgpu::Device,
    texture: &Texture,
    sampler: &wgpu::Sampler,
    layout: &wgpu::BindGroupLayout,
) -> wgpu::BindGroup {
    let bind_group_label = texture.label.clone() + "Bind Group";
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some(bind_group_label.as_str()),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: TEX_BINDING,
                resource: wgpu::BindingResource::TextureView(&texture.view),
            },
            wgpu::BindGroupEntry {
                binding: SAMPLER_BINDING,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
        ],
    })
}

pub fn generate_texture_bind_group_layout<const TEX_BINDING: u32, const SAMPLER_BINDING: u32>(
    device: &wgpu::Device,
    label: &str,
    filterable: bool,
) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: TEX_BINDING,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: SAMPLER_BINDING,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(if filterable {
                    wgpu::SamplerBindingType::Filtering
                } else {
                    wgpu::SamplerBindingType::NonFiltering
                }),
                count: None,
            },
        ],
        label: Some(label),
    })
}

pub fn generate_texture_array_bind_group<const TEX_BINDING: u32, const SAMPLER_BINDING: u32>(
    device: &wgpu::Device,
    label: &str,
    texture_views: &[&wgpu::TextureView],
    sampler: &wgpu::Sampler,
    layout: &wgpu::BindGroupLayout,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some(label),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: TEX_BINDING,
                resource: wgpu::BindingResource::TextureViewArray(texture_views),
            },
            wgpu::BindGroupEntry {
                binding: SAMPLER_BINDING,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
        ],
    })
}

pub fn generate_texture_array_bind_group_layout<
    const TEX_BINDING: u32,
    const SAMPLER_BINDING: u32,
>(
    device: &wgpu::Device,
    label: &str,
    filterable: bool,
    texture_count: NonZero<u32>,
) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: TEX_BINDING,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: Some(texture_count),
            },
            wgpu::BindGroupLayoutEntry {
                binding: SAMPLER_BINDING,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(if filterable {
                    wgpu::SamplerBindingType::Filtering
                } else {
                    wgpu::SamplerBindingType::NonFiltering
                }),
                count: None,
            },
        ],
        label: Some(label),
    })
}
