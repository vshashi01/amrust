use wgpu::util::DeviceExt;

use std::any::TypeId;

pub trait InstanceFieldDescriptor {
    fn layout<const LOCATION: u32>() -> wgpu::VertexBufferLayout<'static>;
}

pub struct GpuInstance {
    pub buffer: wgpu::Buffer,
    pub instance_data_stream: Vec<InstanceDataStream>,
    pub instance_count: u32,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct InstanceDataStream {
    pub type_id: TypeId,
    pub offset: wgpu::BufferAddress,
    pub stride: wgpu::BufferAddress,
}

impl GpuInstance {
    pub fn instance_data_stream<T: 'static>(&self) -> Option<&InstanceDataStream> {
        self.instance_data_stream
            .iter()
            .find(|vs| vs.type_id == TypeId::of::<T>())
    }

    pub fn vertex_slice<T: 'static>(&self) -> wgpu::BufferSlice {
        let stream = self.instance_data_stream::<T>().unwrap();
        self.buffer.slice(stream.offset..)
    }

    pub fn clone_with_buffer(&self, device: &wgpu::Device) -> Self {
        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Instance Buffer Clone"),
            contents: &self.buffer.slice(..).get_mapped_range(),
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        });

        Self {
            buffer,
            instance_data_stream: self.instance_data_stream.clone(),
            instance_count: self.instance_count,
        }
    }
}

pub struct InstanceDataBuilder {
    pub instance_data_stream: Vec<InstanceDataStream>,
    pub data: Vec<u8>,
    pub instance_count: u32,
}

impl InstanceDataBuilder {
    pub fn new() -> Self {
        Self {
            instance_data_stream: Vec::new(),
            data: Vec::new(),
            instance_count: 0,
        }
    }

    fn append<T: bytemuck::Pod + 'static>(&mut self, data: &[T]) -> wgpu::BufferAddress {
        let offset = self.data.len() as wgpu::BufferAddress;

        let bytes = bytemuck::cast_slice(data);
        self.data.extend_from_slice(bytes);
        offset
    }

    pub fn add_instance_stream<T: InstanceFieldDescriptor + bytemuck::Pod + 'static>(
        &mut self,
        data: &[T],
    ) -> &mut Self {
        if self.instance_count < 1 {
            //the first vertex stream defines the vertex count
            self.instance_count = data.len() as u32;
        } else {
            assert!(
                self.instance_count == data.len() as u32,
                "Vertex count mismatch: expected {}, got {}",
                self.instance_count,
                data.len()
            );
        }

        let offset = self.append(data);
        let stride = std::mem::size_of::<T>() as wgpu::BufferAddress;
        self.instance_data_stream.push(InstanceDataStream {
            type_id: TypeId::of::<T>(),
            offset,
            stride,
        });

        self
    }

    pub fn build(&mut self, device: &wgpu::Device) -> GpuInstance {
        let usage = wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST;
        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Instance Buffer"),
            contents: &self.data,
            usage,
        });

        GpuInstance {
            buffer,
            instance_data_stream: self.instance_data_stream.clone(),
            instance_count: self.instance_count,
        }
    }
}
