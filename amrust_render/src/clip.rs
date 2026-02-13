use crate::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipPlaneAxis {
    X,
    Y,
    Z,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClipPlane {
    pub axis: ClipPlaneAxis,
    pub axis_sign: f32,
    pub d: f32,
    pub finite: bool,
    pub bounds_min: glam::Vec2,
    pub bounds_max: glam::Vec2,
}

impl ClipPlane {
    pub fn to_uniform(&self) -> ClipUniform {
        let axis = match self.axis {
            ClipPlaneAxis::X => 0,
            ClipPlaneAxis::Y => 1,
            ClipPlaneAxis::Z => 2,
        };

        ClipUniform {
            axis,
            enabled: 1,
            finite: self.finite as u32,
            _pad0: 0,
            axis_sign: if self.axis_sign >= 0.0 { 1.0 } else { -1.0 },
            d: self.d,
            _pad1: [0.0; 2],
            bounds_min: self.bounds_min.to_array(),
            bounds_max: self.bounds_max.to_array(),
        }
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Zeroable, bytemuck::Pod)]
pub struct ClipUniform {
    pub axis: u32,
    pub enabled: u32,
    pub finite: u32,
    pub _pad0: u32,
    pub axis_sign: f32,
    pub d: f32,
    pub _pad1: [f32; 2],
    pub bounds_min: [f32; 2],
    pub bounds_max: [f32; 2],
}

impl ClipUniform {
    pub fn disabled() -> Self {
        Self {
            axis: 0,
            enabled: 0,
            finite: 0,
            _pad0: 0,
            axis_sign: 1.0,
            d: 0.0,
            _pad1: [0.0; 2],
            bounds_min: [0.0; 2],
            bounds_max: [0.0; 2],
        }
    }

    pub const fn get_size() -> usize {
        std::mem::size_of::<Self>()
    }
}

pub fn create_clip_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("Clip Plane Bind Group Layout"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: Some(
                    std::num::NonZero::new(ClipUniform::get_size() as wgpu::BufferAddress).unwrap(),
                ),
            },
            count: None,
        }],
    })
}

pub fn create_clip_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    uniform: &ClipUniform,
) -> (wgpu::Buffer, wgpu::BindGroup) {
    let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Clip Plane buffer"),
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        contents: bytemuck::cast_slice(&[*uniform]),
    });

    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("Clip Plane Bind Group"),
        layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: buffer.as_entire_binding(),
        }],
    });

    (buffer, bind_group)
}
