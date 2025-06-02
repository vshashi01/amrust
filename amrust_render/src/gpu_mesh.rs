use wgpu::util::DeviceExt;

use crate::vertex::Vertex;

pub struct GpuMesh {
    pub vertex_buffer: wgpu::Buffer,
    pub index_buffer: Option<wgpu::Buffer>,
    pub buffer_length: u32,
    pub local_bind_resources: Vec<(u32, u32)>, // (local binding resource index, slot index)
}

impl GpuMesh {
    pub fn new_indexed_mesh<T: Vertex + bytemuck::Pod, U: bytemuck::Pod>(
        vertices: &[T],
        indices: &[U],
        buffer_length: u32,
        local_bind_resources: Vec<(u32, u32)>,
        device: &wgpu::Device,
    ) -> Self {
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Vertex Buffer"),
            contents: bytemuck::cast_slice(vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Index Buffer"),
            contents: bytemuck::cast_slice(indices),
            usage: wgpu::BufferUsages::INDEX,
        });

        Self {
            vertex_buffer,
            index_buffer: Some(index_buffer),
            buffer_length,
            local_bind_resources,
        }
    }

    pub fn new_mesh<T: Vertex + bytemuck::Pod>(
        vertices: &[T],
        buffer_length: u32,
        local_bind_resources: Vec<(u32, u32)>,
        device: &wgpu::Device,
    ) -> Self {
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Vertex Buffer"),
            contents: bytemuck::cast_slice(vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        Self {
            vertex_buffer,
            index_buffer: None,
            buffer_length,
            local_bind_resources,
        }
    }
}
