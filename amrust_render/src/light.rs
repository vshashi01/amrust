use crate::prelude::*;

use crate::instance::InstanceFieldDescriptor;

pub struct LightData {
    pub direction: glam::Vec3,
    pub intensity: f32,
    pub color: glam::Vec3,
    pub ambient: f32,
    buffer: wgpu::Buffer,
}

impl LightData {
    pub fn new_directional_light(
        device: &wgpu::Device,
        direction: glam::Vec3,
        intensity: f32,
        color: glam::Vec3,
        ambient: f32,
    ) -> Self {
        let uniform = Self::create_uniform(&direction, &intensity, &color, &ambient);
        let buffer = Self::create_uniform_buffer(uniform, device);

        Self {
            direction,
            intensity,
            color,
            ambient,
            buffer,
        }
    }

    pub fn write_buffer(&self, queue: &wgpu::Queue) {
        let uniform =
            Self::create_uniform(&self.direction, &self.intensity, &self.color, &self.ambient);
        queue.write_buffer(&self.buffer, 0, bytemuck::cast_slice(&[uniform]));
    }

    fn create_uniform(
        direction: &glam::Vec3,
        intensity: &f32,
        color: &glam::Vec3,
        ambient: &f32,
    ) -> LightUniform {
        LightUniform {
            direction: [direction.x, direction.y, direction.z],
            intensity: *intensity,
            color: [color.x, color.y, color.z],
            ambient: *ambient,
        }
    }

    fn create_uniform_buffer(uniform: LightUniform, device: &wgpu::Device) -> wgpu::Buffer {
        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Screen Space buffer"),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            contents: bytemuck::cast_slice(&[uniform]),
        })
    }

    pub fn get_binding_resource(&self) -> wgpu::BindingResource<'_> {
        self.buffer.as_entire_binding()
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct LightUniform {
    // Direction the light points (light rays travel). In shading use L = -direction.
    pub direction: [f32; 3],
    pub intensity: f32,
    pub color: [f32; 3],
    pub ambient: f32,
}

impl LightUniform {
    pub fn get_size() -> usize {
        std::mem::size_of::<Self>()
    }
}

#[repr(transparent)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct UseLightingData(pub i32);

impl UseLightingData {
    pub const NO: Self = Self(0);
    pub const YES: Self = Self(1);
}

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct NormalMatrixData {
    pub col0: [f32; 4],
    pub col1: [f32; 4],
    pub col2: [f32; 4],
    pub use_lighting: UseLightingData,
    pub _pad: [i32; 3],
}

impl NormalMatrixData {
    pub fn from_mat3(normal_matrix: glam::Mat3, use_lighting: UseLightingData) -> Self {
        let cols = normal_matrix.to_cols_array_2d();
        Self {
            col0: [cols[0][0], cols[0][1], cols[0][2], 0.0],
            col1: [cols[1][0], cols[1][1], cols[1][2], 0.0],
            col2: [cols[2][0], cols[2][1], cols[2][2], 0.0],
            use_lighting,
            _pad: [0; 3],
        }
    }

    pub fn from_model_matrix(model_matrix: glam::Mat4, use_lighting: UseLightingData) -> Self {
        let normal_matrix: glam::Mat3 = Self::mat4_to_mat3(&model_matrix.inverse().transpose());
        Self::from_mat3(normal_matrix, use_lighting)
    }

    fn mat4_to_mat3(m4: &glam::Mat4) -> glam::Mat3 {
        glam::Mat3::from_cols(
            m4.x_axis.truncate(),
            m4.y_axis.truncate(),
            m4.z_axis.truncate(),
        )
    }
}

impl InstanceFieldDescriptor for NormalMatrixData {
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
                    format: wgpu::VertexFormat::Sint32,
                    offset: std::mem::size_of::<[f32; 12]>() as wgpu::BufferAddress,
                    shader_location: LOCATION + 3,
                },
            ],
        }
    }
}
