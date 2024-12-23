use eframe::wgpu::{self};

use super::camera::{Camera, CameraUniform};

pub struct ViewportResources {
    pub pipeline: wgpu::RenderPipeline,
    pub vertex_buffer: wgpu::Buffer,
    pub index_buffer: wgpu::Buffer,
    pub index_buffer_len: u32,

    // pub bind_group: wgpu::BindGroup,
    pub camera: Camera,
    pub camera_bind_group: wgpu::BindGroup,
    pub camera_uniform: CameraUniform,
    pub camera_buffer: wgpu::Buffer,
    pub dataset: Dataset,
}

pub struct Dataset();

impl ViewportResources {}
