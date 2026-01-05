use crate::prelude::*;

pub trait VertexDescriptor {
    fn layout<const LOCATION: u32>() -> wgpu::VertexBufferLayout<'static>;
}

#[repr(transparent)]
#[derive(Debug, Copy, Clone, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Position(pub [f32; 3]);

impl VertexDescriptor for Position {
    fn layout<const LOCATION: u32>() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x3,
                offset: 0,
                shader_location: LOCATION,
            }],
        }
    }
}

#[repr(transparent)]
#[derive(Debug, Copy, Clone, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Color(pub [f32; 3]);

impl VertexDescriptor for Color {
    fn layout<const LOCATION: u32>() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x3,
                offset: 0,
                shader_location: LOCATION,
            }],
        }
    }
}

#[repr(transparent)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct TexCoords(pub [f32; 2]);

impl VertexDescriptor for TexCoords {
    fn layout<const LOCATION: u32>() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x2,
                offset: 0,
                shader_location: LOCATION,
            }],
        }
    }
}

#[repr(transparent)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct UseTexture(i32); // 0 for false, 1 for true

impl UseTexture {
    //todo: replace this weird API
    pub const fn no() -> Self {
        Self(-1)
    }

    pub const fn yes() -> Self {
        Self(0)
    }

    pub fn from_texture_index(texture_index: u32) -> Self {
        let texture_index = i32::try_from(texture_index);
        match texture_index {
            Ok(index) => UseTexture(index),
            Err(_) => UseTexture(-1),
        }
    }
}

impl VertexDescriptor for UseTexture {
    fn layout<const LOCATION: u32>() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Sint32,
                offset: 0,
                shader_location: LOCATION,
            }],
        }
    }
}
