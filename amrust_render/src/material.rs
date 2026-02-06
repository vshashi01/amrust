use crate::prelude::*;

use crate::instance::InstanceFieldDescriptor;

#[derive(Debug, Clone, Copy)]
pub struct Material {
    pub red: f32,
    pub green: f32,
    pub blue: f32,
}

impl Material {
    pub fn new(red: f32, green: f32, blue: f32) -> Self {
        Self { red, green, blue }
    }

    pub fn to_data(&self) -> RgbMaterialData {
        RgbMaterialData([self.red, self.green, self.blue])
    }
}

#[repr(transparent)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct RgbMaterialData(pub [f32; 3]);

impl InstanceFieldDescriptor for RgbMaterialData {
    fn layout<const LOCATION: u32>() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &[wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x3,
                offset: 0,
                shader_location: LOCATION,
            }],
        }
    }
}

#[repr(transparent)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct UseMaterialData(i32);

impl UseMaterialData {
    pub fn new(use_material_data: bool) -> Self {
        if use_material_data { Self(1) } else { Self(0) }
    }
}

impl InstanceFieldDescriptor for UseMaterialData {
    fn layout<const LOCATION: u32>() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &[wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Sint32,
                offset: 0,
                shader_location: LOCATION,
            }],
        }
    }
}
