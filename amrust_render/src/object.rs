use wgpu::{self, util::DeviceExt};

use crate::instance::{Instance, InstanceRaw};

pub enum Renderables {
    IndexedMesh(GpuMesh, bool),
    Mesh(GpuMesh),
}

pub struct GpuMesh {
    pub vertex_buffer: wgpu::Buffer,
    pub index_buffer: Option<wgpu::Buffer>, //probably wireframe does not need to be indexed
    pub buffer_length: u32,
}

impl GpuMesh {
    pub fn new_indexed_mesh(
        vertices: &[u8],
        indices: &[u8],
        buffer_length: u32,
        device: &wgpu::Device,
    ) -> Self {
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Vertex Buffer"),
            contents: vertices,
            usage: wgpu::BufferUsages::VERTEX,
        });

        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Index Buffer"),
            contents: indices,
            usage: wgpu::BufferUsages::INDEX,
        });

        Self {
            vertex_buffer,
            index_buffer: Some(index_buffer),
            buffer_length,
        }
    }

    pub fn new_mesh(vertices: &[u8], buffer_length: u32, device: &wgpu::Device) -> Self {
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Vertex Buffer"),
            contents: vertices,
            usage: wgpu::BufferUsages::VERTEX,
        });

        Self {
            vertex_buffer,
            index_buffer: None,
            buffer_length,
        }
    }
}

pub struct Object {
    pub renderable_id: usize,
    pub instances: Vec<InstanceRaw>,
    pub instance_buffer: wgpu::Buffer,
}

impl Object {
    pub fn new(renderable_id: usize, instances: Vec<Instance>, device: &wgpu::Device) -> Self {
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
        }
    }
}
