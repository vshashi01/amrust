use std::ops::Range;

use wgpu::{self, util::DeviceExt};

use crate::instance::{Instance, InstanceRaw};

pub enum Renderables {
    IndexedMesh(GpuMesh),
    Mesh(GpuMesh),
}

impl Renderables {
    pub fn render(
        &self,
        render_pass: &mut wgpu::RenderPass<'_>,
        instance_len: u32,
        local_bind_groups: &[wgpu::BindGroup],
    ) {
        match self {
            Renderables::IndexedMesh(mesh) => {
                render_pass.set_vertex_buffer(0, mesh.vertex_buffer.slice(..));
                if let Some(index_buffer) = &mesh.index_buffer {
                    render_pass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint16);

                    for (index, slot) in &mesh.local_bind_resources {
                        if let Some(bind_group) = local_bind_groups.get(*index as usize) {
                            render_pass.set_bind_group(*slot, bind_group, &[]);
                        } else {
                            panic!("Local bind group not found for index {index}");
                        }
                    }

                    render_pass.draw_indexed(0..mesh.buffer_length, 0, 0..instance_len);
                } else {
                    panic!("Mesh does not have an index buffer");
                }
            }
            Renderables::Mesh(mesh) => {
                render_pass.set_vertex_buffer(0, mesh.vertex_buffer.slice(..));
                // render_pass.set_bind_group(1, &basic_diffuse_bind_group, &[]);
                for (index, slot) in &mesh.local_bind_resources {
                    if let Some(bind_group) = local_bind_groups.get(*index as usize) {
                        render_pass.set_bind_group(*slot, bind_group, &[]);
                    } else {
                        panic!("Local bind group not found for index {index}");
                    }
                }

                render_pass.draw(0..mesh.buffer_length, 0..instance_len);
            }
        }
    }
}

pub struct GpuMesh {
    pub vertex_buffer: wgpu::Buffer,
    pub index_buffer: Option<wgpu::Buffer>,
    pub buffer_length: u32,
    pub local_bind_resources: Vec<(u32, u32)>, // (local binding resource index, slot index)
}

impl GpuMesh {
    pub fn new_indexed_mesh(
        vertices: &[u8],
        indices: &[u8],
        buffer_length: u32,
        local_bind_resources: Vec<(u32, u32)>,
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
            local_bind_resources,
        }
    }

    pub fn new_mesh(
        vertices: &[u8],
        buffer_length: u32,
        local_bind_resources: Vec<(u32, u32)>,
        device: &wgpu::Device,
    ) -> Self {
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Vertex Buffer"),
            contents: vertices,
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
