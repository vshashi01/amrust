use wgpu::util::DeviceExt;

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

pub struct ScreenSpace {
    width: f32,
    height: f32,
    buffer: wgpu::Buffer,
}

impl ScreenSpace {
    pub fn new(device: &wgpu::Device, width: f32, height: f32) -> Self {
        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Screen Space buffer"),
            contents: bytemuck::cast_slice(&[ScreenSpaceUniform {
                screen_size: [width, height],
            }]),
            usage: wgpu::BufferUsages::UNIFORM,
        });

        Self {
            width,
            height,
            buffer,
        }
    }

    pub fn get_binding_resource(&self) -> wgpu::BindingResource<'_> {
        self.buffer.as_entire_binding()
    }
}

#[repr(transparent)]
#[derive(Debug, Clone, Copy, bytemuck::Zeroable, bytemuck::Pod)]
pub struct SizeInPixel(pub f32);

impl InstanceFieldDescriptor for SizeInPixel {
    fn layout<const LOCATION: u32>() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &[wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32,
                offset: 0,
                shader_location: LOCATION,
            }],
        }
    }
}
