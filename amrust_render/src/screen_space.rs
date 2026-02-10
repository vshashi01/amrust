use crate::prelude::*;

use crate::instance::InstanceFieldDescriptor;

#[repr(transparent)]
#[derive(Debug, Clone, Copy, bytemuck::Zeroable, bytemuck::Pod)]
pub struct ScreenSpaceUniform {
    screen_size: [f32; 2],
}

pub struct ScreenSpace {
    width: f32,
    height: f32,
    buffer: wgpu::Buffer,
}

impl ScreenSpace {
    pub fn new(device: &wgpu::Device, width: f32, height: f32) -> Self {
        let uniform = Self::create_uniform(width, height);
        let buffer = Self::create_uniform_buffer(uniform, device);

        Self {
            width,
            height,
            buffer,
        }
    }

    pub fn update_size(&mut self, width: f32, height: f32) {
        self.width = width;
        self.height = height;
    }

    pub fn write_buffer(&self, queue: &wgpu::Queue) {
        let uniform = Self::create_uniform(self.width, self.height);
        queue.write_buffer(&self.buffer, 0, bytemuck::cast_slice(&[uniform]));
    }

    fn create_uniform(width: f32, height: f32) -> ScreenSpaceUniform {
        ScreenSpaceUniform {
            screen_size: [width, height],
        }
    }

    fn create_uniform_buffer(uniform: ScreenSpaceUniform, device: &wgpu::Device) -> wgpu::Buffer {
        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Screen Space buffer"),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            contents: bytemuck::cast_slice(&[uniform]),
        })
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
