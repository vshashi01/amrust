use crate::prelude::*;

use tinyvec::ArrayVec;
use wgpu::BindGroupLayoutEntry;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum ClipPlaneAxis {
    #[default]
    X,
    Y,
    Z,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ClipPlane {
    pub axis: ClipPlaneAxis,
    pub axis_sign: f32,
    pub d: f32,
    pub is_enabled: bool,
    pub is_finite: bool,
    pub bounds_min: glam::Vec2,
    pub bounds_max: glam::Vec2,
}

impl Default for ClipPlane {
    fn default() -> Self {
        Self {
            axis: Default::default(),
            axis_sign: 1.0,
            d: Default::default(),
            is_enabled: false,
            is_finite: false,
            bounds_min: Default::default(),
            bounds_max: Default::default(),
        }
    }
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
            enabled: self.is_enabled as u32,
            finite: self.is_finite as u32,
            _pad0: 0,
            axis_sign: if self.axis_sign >= 0.0 { 1.0 } else { -1.0 },
            d: self.d,
            _pad1: [0.0; 2],
            bounds_min: self.bounds_min.to_array(),
            bounds_max: self.bounds_max.to_array(),
        }
    }

    pub fn copy_from(&mut self, other: &ClipPlane) {
        self.axis = other.axis;
        self.is_enabled = other.is_enabled;
        self.is_finite = other.is_finite;
        self.axis_sign = other.axis_sign;
        self.bounds_min = other.bounds_min;
        self.bounds_max = other.bounds_max;
    }
}

#[derive(Debug, Clone)]
pub struct ClipPlanes<const COUNT: usize> {
    clip_planes: tinyvec::ArrayVec<[ClipPlane; COUNT]>,
    pub(crate) buffers: [wgpu::Buffer; COUNT],
}

impl<const COUNT: usize> ClipPlanes<COUNT> {
    pub fn new(device: &wgpu::Device) -> Self {
        let mut clip_planes: ArrayVec<[ClipPlane; COUNT]> = ArrayVec::new();

        for _i in 0..COUNT {
            let clip_plane = ClipPlane::default();
            clip_planes.push(clip_plane);
        }

        let buffers: [wgpu::Buffer; COUNT] = clip_planes
            .iter()
            .map(|c| create_buffer(device, &[c.to_uniform()]))
            .collect::<Vec<_>>()
            .try_into()
            .expect("Something wrong in Clip Planes length");

        Self {
            clip_planes,
            buffers,
        }
    }

    pub fn update_clip_planes(&mut self, planes: impl FnOnce(&mut ArrayVec<[ClipPlane; COUNT]>)) {
        planes(&mut self.clip_planes)
    }

    pub fn write_buffer(&self, queue: &wgpu::Queue) {
        for (index, clip_plane) in self.clip_planes.iter().enumerate() {
            let uniform = clip_plane.to_uniform();
            queue.write_buffer(
                self.buffers.get(index).unwrap(),
                0,
                bytemuck::cast_slice(&[uniform]),
            );
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

fn create_buffer(device: &wgpu::Device, uniforms: &[ClipUniform]) -> wgpu::Buffer {
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Clip Plane buffer"),
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        contents: bytemuck::cast_slice(uniforms),
    })
}

pub(crate) fn create_clip_bind_group_layout<const COUNT: usize, const BINDING_OFFSET: u32>(
    device: &wgpu::Device,
) -> wgpu::BindGroupLayout {
    let entries = create_clip_bind_group_layout_entries::<COUNT, BINDING_OFFSET>();
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("Clip Plane Bind Group Layout"),
        entries: &entries,
    })
}

pub fn create_clip_bind_group<const COUNT: usize, const BINDING_OFFSET: u32>(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    buffers: &[wgpu::Buffer; COUNT],
) -> wgpu::BindGroup {
    let entries = create_clip_bind_group_entries::<COUNT, BINDING_OFFSET>(buffers);
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("Clip Plane Bind Group"),
        layout,
        entries: &entries,
    })
}

pub fn create_clip_bind_group_entries<const COUNT: usize, const BINDING_OFFSET: u32>(
    buffers: &[wgpu::Buffer; COUNT],
) -> Vec<wgpu::BindGroupEntry<'_>> {
    if buffers.len() != COUNT {
        panic!("Clip plane bind resources does not match the clip plane count: {COUNT}");
    } else {
        buffers
            .iter()
            .enumerate()
            .map(|(index, buffer)| wgpu::BindGroupEntry {
                binding: (index as u32) + BINDING_OFFSET,
                resource: buffer.as_entire_binding(),
            })
            .collect::<Vec<_>>()
    }
}

pub fn create_clip_bind_group_layout_entries<const COUNT: usize, const BINDING_OFFSET: u32>()
-> Vec<wgpu::BindGroupLayoutEntry> {
    let mut entries: Vec<BindGroupLayoutEntry> = Vec::new();

    for index in 0..COUNT {
        let entry = wgpu::BindGroupLayoutEntry {
            binding: (index as u32) + BINDING_OFFSET,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: Some(
                    std::num::NonZero::new(ClipUniform::get_size() as wgpu::BufferAddress).unwrap(),
                ),
            },
            count: None,
        };

        entries.push(entry);
    }

    entries
}
