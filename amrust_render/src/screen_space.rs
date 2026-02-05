use std::num::NonZero;

use crate::instance::InstanceFieldDescriptor;

#[repr(transparent)]
#[derive(Debug, Clone, Copy, bytemuck::Zeroable, bytemuck::Pod)]
pub struct ScreenSpaceUniform {
    pub screen_size: [f32; 2],
}

impl ScreenSpaceUniform {
    pub fn new(width: f32, height: f32) -> Self {
        Self {
            screen_size: [width, height],
        }
    }

    pub const fn get_size() -> usize {
        // only 1 size for now
        std::mem::size_of::<Self>()
    }

    pub fn create_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Camera bind group layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        })
    }
}

pub struct SizeInPixel(f32);

impl InstanceFieldDescriptor for SizeInPixel {
    fn layout<const LOCATION: u32>() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &[wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x3,
                offset: 0,
                shader_location: LOCATION,
            }],
        }
    }
}

pub fn get_screen_space_uniform_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("Composite Bind Group Layout"),
        entries: &[],
    })
}

pub fn create_screen_space_mesh_bind_group(
    device: &wgpu::Device,
    composite_bind_group_layout: &wgpu::BindGroupLayout,
    main_texture_view: &wgpu::TextureView,
    main_texture_sampler: &wgpu::Sampler,
    silhoutte_texture_view: &wgpu::TextureView,
    silhoutte_texture_sampler: &wgpu::Sampler,
    screen_space_frag_uniform: wgpu::BufferBinding,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("Composite Bind Group"),
        layout: composite_bind_group_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(main_texture_view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(main_texture_sampler),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::TextureView(silhoutte_texture_view),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::Sampler(silhoutte_texture_sampler),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: wgpu::BindingResource::Buffer(screen_space_frag_uniform),
            },
        ],
    })
}
