use wgpu::{self, util::DeviceExt};

use crate::instance::{Instance, InstanceRaw};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderModes {
    Solid,
    Wireframe,
}

pub struct Object {
    pub renderable_id: usize,
    pub instances: Vec<InstanceRaw>,
    pub instance_buffer: wgpu::Buffer,
    pub render_modes: RenderModes,
}

impl Object {
    pub fn new(
        renderable_id: usize,
        instances: Vec<Instance>,
        render_modes: RenderModes,
        device: &wgpu::Device,
    ) -> Self {
        let instances_data = instances.iter().map(Instance::to_raw).collect::<Vec<_>>();
        let instance_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Instance buffer"),
            contents: bytemuck::cast_slice(&instances_data),
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        });

        Self {
            renderable_id,
            instances: instances_data,
            instance_buffer,
            render_modes,
        }
    }
}
