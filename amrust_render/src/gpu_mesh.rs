use wgpu::util::DeviceExt;

use crate::vertex::VertexDescriptor;

use std::any::TypeId;

pub struct GpuMesh {
    pub buffer: wgpu::Buffer,
    pub vertex_count: u32,
    pub vertex_streams: Vec<VertexStream>,
    pub mesh_index_stream: Option<IndexStream>,
    pub wireframe_index_stream: Option<IndexStream>,
}

impl GpuMesh {
    pub fn vertex_stream<T: 'static>(&self) -> Option<&VertexStream> {
        self.vertex_streams
            .iter()
            .find(|vs| vs.type_id == TypeId::of::<T>())
    }

    pub fn vertex_slice<T: 'static>(&self) -> wgpu::BufferSlice {
        let stream = self.vertex_stream::<T>().unwrap();
        self.buffer.slice(stream.offset..stream.end)
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct VertexStream {
    pub type_id: TypeId,
    pub offset: wgpu::BufferAddress,
    pub end: wgpu::BufferAddress,
    pub stride: wgpu::BufferAddress,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct IndexStream {
    pub offset: wgpu::BufferAddress,
    pub end: wgpu::BufferAddress,
    pub index_count: u32,
    pub format: wgpu::IndexFormat,
}

pub struct MeshBuilder {
    pub vertex_streams: Vec<VertexStream>,
    pub mesh_index_stream: Option<IndexStream>,
    pub wireframe_index_stream: Option<IndexStream>,
    pub data: Vec<u8>,
    pub vertex_count: u32,
}

impl MeshBuilder {
    pub fn new() -> Self {
        Self {
            vertex_streams: Vec::new(),
            mesh_index_stream: None,
            wireframe_index_stream: None,
            data: Vec::new(),
            vertex_count: 0,
        }
    }

    fn append<T: bytemuck::Pod + 'static>(&mut self, data: &[T]) -> wgpu::BufferAddress {
        let offset = self.data.len() as wgpu::BufferAddress;

        let bytes = bytemuck::cast_slice(data);
        self.data.extend(bytes);
        offset
    }

    pub fn add_vertex_stream<T: VertexDescriptor + bytemuck::Pod + 'static>(
        &mut self,
        data: &[T],
    ) -> &mut Self {
        if self.vertex_count < 1 {
            //the first vertex stream defines the vertex count
            self.vertex_count = data.len() as u32;
        } else {
            assert!(
                self.vertex_count == data.len() as u32,
                "Vertex count mismatch: expected {}, got {}",
                self.vertex_count,
                data.len()
            );
        }

        let offset = self.append(data);
        let end = self.data.len() as wgpu::BufferAddress;
        let stride = std::mem::size_of::<T>() as wgpu::BufferAddress;
        self.vertex_streams.push(VertexStream {
            type_id: TypeId::of::<T>(),
            offset,
            end,
            stride,
        });

        self
    }

    pub fn add_mesh_index_stream(&mut self, data: &[u16]) -> &mut Self {
        assert!(
            self.mesh_index_stream.is_none(),
            "Index stream already set, cannot set again"
        );

        let offset = self.append(data);
        let format = wgpu::IndexFormat::Uint16;
        let end = self.data.len() as wgpu::BufferAddress;
        let index_count = data.len() as u32;

        self.mesh_index_stream = Some(IndexStream {
            offset,
            end,
            index_count,
            format,
        });

        self
    }

    pub fn add_wireframe_index_stream(&mut self, data: &[u16]) -> &mut Self {
        assert!(
            self.wireframe_index_stream.is_none(),
            "Index stream already set, cannot set again"
        );

        let offset = self.append(data);
        let format = wgpu::IndexFormat::Uint16;
        let end = self.data.len() as wgpu::BufferAddress;
        let index_count = data.len() as u32;

        self.wireframe_index_stream = Some(IndexStream {
            offset,
            end,
            index_count,
            format,
        });

        self
    }

    pub fn build(&mut self, device: &wgpu::Device) -> GpuMesh {
        let mut usage = wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST;
        usage.set(
            wgpu::BufferUsages::INDEX,
            self.mesh_index_stream.is_some() || self.wireframe_index_stream.is_some(),
        );
        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Mesh Buffer"),
            contents: &self.data,
            usage,
        });

        GpuMesh {
            buffer,
            vertex_count: self.vertex_count,
            vertex_streams: self.vertex_streams.clone(),
            mesh_index_stream: self.mesh_index_stream,
            wireframe_index_stream: self.wireframe_index_stream,
        }
    }
}

impl Default for MeshBuilder {
    fn default() -> Self {
        Self::new()
    }
}
