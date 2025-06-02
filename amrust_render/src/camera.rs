use glam::{Mat4, Vec3};
use wgpu::util::DeviceExt;

//Camera Uniform
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct CameraUniform {
    view_proj: [[f32; 4]; 4],
}

pub trait CameraData {
    fn get_view_matrix(&self) -> Mat4;
    fn get_projection_matrix(&self) -> Mat4;
}

pub struct Camera {
    view_matrix: Mat4,
    projection_matrix: Mat4,
}

impl Camera {
    pub fn new<T: CameraData>(data: &T) -> Self {
        let view_matrix = data.get_view_matrix();
        let projection_matrix = data.get_projection_matrix();
        Self {
            view_matrix,
            projection_matrix,
        }
    }

    pub fn view_projection_matrix(&self) -> Mat4 {
        self.projection_matrix * self.view_matrix
    }

    pub fn create_gpu_resources(
        &self,
        device: &wgpu::Device,
    ) -> (wgpu::BindGroupLayout, wgpu::BindGroup, wgpu::Buffer) {
        let uniform_buffer = self.create_uniform_buffer(device);
        let bind_group_layout = Self::create_bind_group_layout(device);
        let bind_group = Self::create_bind_group(device, &bind_group_layout, &uniform_buffer);

        (bind_group_layout, bind_group, uniform_buffer)
    }

    fn create_uniform(&self) -> CameraUniform {
        CameraUniform {
            view_proj: self.view_projection_matrix().to_cols_array_2d(),
        }
    }

    fn create_uniform_buffer(&self, device: &wgpu::Device) -> wgpu::Buffer {
        let camera_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Camera buffer"),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            contents: bytemuck::cast_slice(&[self.create_uniform()]),
        });

        camera_buffer
    }

    pub fn create_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Camera bind group layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        })
    }

    fn create_bind_group(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        buffer: &wgpu::Buffer,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Camera bind group"),
            layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: buffer.as_entire_binding(),
            }],
        })
    }
}

pub struct OrthographicCameraData {
    pub eye_position: Vec3,
    pub target_position: Vec3,
    pub up_vector: Vec3,
    pub near: f32,
    pub far: f32,
    pub zoom: f32, //1.0 is the default zoom
}

pub enum CameraTransform {
    Zoom(f32),
    Pan(Vec3),
    Rotate {
        pivot: Vec3,
        rotation_axis: Vec3,
        angle: f32,
    },
}

impl OrthographicCameraData {
    pub fn transform(mut self, transform: CameraTransform) -> Self {
        match transform {
            CameraTransform::Zoom(value) => {
                self.zoom += value;
            }
            CameraTransform::Pan(value) => {
                self.eye_position += value;
                self.target_position += value;
            }
            CameraTransform::Rotate {
                pivot,
                rotation_axis,
                angle,
            } => {
                let (eye_position, target_position, up_vector) =
                    self.get_multi_rotation_data(pivot, rotation_axis, angle);

                self.eye_position = eye_position;
                self.target_position = target_position;
                self.up_vector = up_vector;
            }
        }

        self
    }

    fn get_multi_rotation_data(
        &self,
        pivot: Vec3,
        rotation_axis: Vec3,
        angle: f32,
    ) -> (Vec3, Vec3, Vec3) {
        let rotation_matrix = Mat4::from_axis_angle(rotation_axis, angle);

        //rotate the eye position relative to the pivot
        let relative_pos = self.eye_position - pivot;
        let rotated_pos = rotation_matrix.transform_vector3(relative_pos);
        let new_eye_position = pivot + rotated_pos;

        //rotate the target position relative to the pivot
        let relative_target = self.target_position - pivot;
        let rotated_target = rotation_matrix.transform_vector3(relative_target);
        let new_target = pivot + rotated_target;

        let rotated_up_vector = rotation_matrix.transform_vector3(self.up_vector);

        (new_eye_position, new_target, rotated_up_vector)
    }
}

impl CameraData for OrthographicCameraData {
    fn get_view_matrix(&self) -> Mat4 {
        Mat4::look_at_rh(self.eye_position, self.target_position, self.up_vector)
    }

    fn get_projection_matrix(&self) -> Mat4 {
        let (left, right, bottom, top) = get_bounds_from_zoom(self.zoom);
        Mat4::orthographic_rh(left, right, bottom, top, self.near, self.far)
    }
}

impl Default for OrthographicCameraData {
    fn default() -> Self {
        Self {
            eye_position: glam::Vec3 {
                x: 0.0,
                y: 0.0,
                z: 25.0,
            },
            target_position: glam::Vec3 {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
            up_vector: glam::Vec3 {
                x: 0.0,
                y: 1.0,
                z: 0.0,
            },
            near: -0.1,
            far: 100.0,
            zoom: 1.0,
        }
    }
}

//calculates and returns the clip bounds (left, right, bottom, top)
fn get_bounds_from_zoom(current_zoom: f32) -> (f32, f32, f32, f32) {
    let left = -1.0 / current_zoom;
    let right = 1.0 / current_zoom;
    let bottom = -1.0 / current_zoom;
    let top = 1.0 / current_zoom;

    (left, right, bottom, top)
}
