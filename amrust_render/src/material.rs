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
    pub const NO: Self = Self(-1);

    pub const YES: Self = Self(1);
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

/// Back face material for double-sided rendering
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct BackMaterialUniform {
    pub color: [f32; 3],
    pub enabled: i32, // -1 = disabled, 1 = enabled
}

impl BackMaterialUniform {
    pub const DISABLED: i32 = -1;
    pub const ENABLED: i32 = 1;

    pub fn new(color: [f32; 3], enabled: bool) -> Self {
        Self {
            color,
            enabled: if enabled {
                Self::ENABLED
            } else {
                Self::DISABLED
            },
        }
    }

    pub fn disabled() -> Self {
        Self {
            color: [0.0, 0.0, 0.0],
            enabled: Self::DISABLED,
        }
    }

    pub fn get_size() -> usize {
        std::mem::size_of::<Self>()
    }
}
