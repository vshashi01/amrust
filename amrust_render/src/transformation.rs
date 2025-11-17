use glam::Mat4;

use crate::prelude::*;

use crate::instance::InstanceFieldDescriptor;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Transformation(pub Mat4);

impl Transformation {
    pub fn to_data(&self) -> TransformationData {
        TransformationData(self.0.to_cols_array_2d())
    }
}

#[repr(transparent)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct TransformationData(pub [[f32; 4]; 4]);

impl InstanceFieldDescriptor for TransformationData {
    fn layout<const LOCATION: u32>() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &[
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x4,
                    offset: 0,
                    shader_location: LOCATION,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x4,
                    offset: std::mem::size_of::<[f32; 4]>() as wgpu::BufferAddress,
                    shader_location: LOCATION + 1,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x4,
                    offset: std::mem::size_of::<[f32; 8]>() as wgpu::BufferAddress,
                    shader_location: LOCATION + 2,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x4,
                    offset: std::mem::size_of::<[f32; 12]>() as wgpu::BufferAddress,
                    shader_location: LOCATION + 3,
                },
            ],
        }
    }
}
