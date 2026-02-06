use wgpu::util::DeviceExt;

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

    pub fn update_size(&mut self, width: f32, height: f32) {
        self.width = width;
        self.height = height;
    }

    pub fn write_buffer(&self, queue: &wgpu::Queue) {
        queue.write_buffer(
            &self.buffer,
            0,
            bytemuck::cast_slice(&[ScreenSpaceUniform {
                screen_size: [self.width, self.height],
            }]),
        );
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
