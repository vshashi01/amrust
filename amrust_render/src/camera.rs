use glam::{Mat4, Vec3};

use crate::{bounding_box::BoundingBox, prelude::*};

//Camera Uniform
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct CameraUniform {
    view_proj: [[f32; 4]; 4],
}

impl CameraUniform {
    pub fn get_size() -> usize {
        std::mem::size_of::<Self>()
    }
}

pub trait CameraData {
    fn get_view_matrix(&self) -> Mat4;
    fn get_projection_matrix(&self) -> Mat4;
    fn get_view_projection(&self) -> Mat4;

    fn transform(&mut self, transform: CameraTransform) -> &mut Self;

    fn get_frustum_planes(&self) -> Frustum {
        calculate_frustum_planes(self.get_projection_matrix(), self.get_view_matrix())
    }

    fn set_viewport_size(&mut self, width: f32, height: f32);
}

#[derive(Debug)]
pub struct Frustum([Plane; 6]);

impl Frustum {
    pub fn is_point_inside(&self, point: Vec3) -> bool {
        self.0
            .iter()
            .all(|p| p.normal.dot(point) + p.distance >= 0.0)
    }
}

#[derive(Debug)]
pub struct Plane {
    pub normal: Vec3,
    pub distance: f32,
}

pub struct Camera {
    view_matrix: Mat4,
    projection_matrix: Mat4,
    buffer: wgpu::Buffer,
    //pub bind_group: wgpu::BindGroup,
}

impl Camera {
    pub fn new(device: &wgpu::Device) -> Self {
        let view_matrix = Mat4::IDENTITY;
        let projection_matrix = Mat4::IDENTITY;
        let buffer = Self::create_uniform_buffer(
            Self::create_uniform(view_matrix, projection_matrix),
            device,
        );

        //let bind_group = Self::create_bind_group(&buffer, device);
        Self {
            view_matrix,
            projection_matrix,
            buffer,
            //bind_group,
        }
    }

    pub fn update<T: CameraData>(&mut self, data: &T) {
        self.view_matrix = data.get_view_matrix();
        self.projection_matrix = data.get_projection_matrix();
    }

    pub fn write_buffer(&self, queue: &wgpu::Queue) {
        let uniform = Self::create_uniform(self.view_matrix, self.projection_matrix);
        queue.write_buffer(&self.buffer, 0, bytemuck::cast_slice(&[uniform]));
    }

    pub fn get_binding_reosurce(&self) -> wgpu::BindingResource<'_> {
        self.buffer.as_entire_binding()
    }

    fn create_uniform(view_matrix: Mat4, projection_matrix: Mat4) -> CameraUniform {
        let view_proj_matrix = projection_matrix * view_matrix;
        CameraUniform {
            view_proj: view_proj_matrix.to_cols_array_2d(),
        }
    }

    fn create_uniform_buffer(uniform: CameraUniform, device: &wgpu::Device) -> wgpu::Buffer {
        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Camera buffer"),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            contents: bytemuck::cast_slice(&[uniform]),
        })
    }

    // pub fn create_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    //     device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
    //         label: Some("Camera bind group layout"),
    //         entries: &[wgpu::BindGroupLayoutEntry {
    //             binding: 0,
    //             visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
    //             ty: wgpu::BindingType::Buffer {
    //                 ty: wgpu::BufferBindingType::Uniform,
    //                 has_dynamic_offset: false,
    //                 min_binding_size: None,
    //             },
    //             count: None,
    //         }],
    //     })
    // }

    // fn create_bind_group(buffer: &wgpu::Buffer, device: &wgpu::Device) -> wgpu::BindGroup {
    //     let layout = Self::create_bind_group_layout(device);
    //     device.create_bind_group(&wgpu::BindGroupDescriptor {
    //         label: Some("Camera bind group"),
    //         layout: &layout,
    //         entries: &[wgpu::BindGroupEntry {
    //             binding: 0,
    //             resource: buffer.as_entire_binding(),
    //         }],
    //     })
    // }
}

#[derive(Clone, Debug, PartialEq)]
pub struct OrthographicCameraData {
    pub eye_position: Vec3,
    pub target_position: Vec3,
    pub up_vector: Vec3,
    pub near: f32,
    pub far: f32,
    pub zoom: f32,         //1.0 is the default zoom
    pub aspect_ratio: f32, // width/height
}

pub enum CameraTransform {
    Zoom(f32),
    ZoomTowards {
        target: Vec3,
        amount: f32,
    },
    Pan(Vec3),
    Rotate {
        pivot: Vec3,
        rotation_axis: Vec3,
        angle: f32,
    },
    SetView {
        eye_position: Vec3,
        target_position: Vec3,
        up_vector: Vec3,
    },
    FitToExtent {
        min: Vec3,
        max: Vec3,
    }, //it depends on how the independent camera system handle the input. Orthographic camera data takes in the min and maximum in WCS
}

impl OrthographicCameraData {
    pub fn set(mut self, eye_position: Vec3, target_position: Vec3, up_vector: Vec3) -> Self {
        self.eye_position = eye_position;
        self.target_position = target_position;
        self.up_vector = up_vector;
        self.zoom = 1.0; // Reset zoom to default
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

    pub fn copy_rotation_component(&mut self, src: &OrthographicCameraData) {
        let f = (src.target_position - src.eye_position).normalize(); // forward (world)
        let r = f.cross(src.up_vector).normalize(); // right (world) for RH look_at
        let u = r.cross(f).normalize(); // corrected up (world)

        let dist = (self.target_position - self.eye_position)
            .length()
            .max(1e-6);

        self.up_vector = u;
        self.target_position = self.eye_position + f * dist;
    }
}

impl CameraData for OrthographicCameraData {
    fn get_view_matrix(&self) -> Mat4 {
        Mat4::look_at_rh(self.eye_position, self.target_position, self.up_vector)
    }

    fn get_projection_matrix(&self) -> Mat4 {
        let (left, right, bottom, top) = get_bounds_from_zoom(self.zoom, self.aspect_ratio);
        Mat4::orthographic_rh(left, right, bottom, top, self.near, self.far)
    }

    fn get_view_projection(&self) -> Mat4 {
        self.get_projection_matrix() * self.get_view_matrix()
    }

    fn transform(&mut self, transform: CameraTransform) -> &mut Self {
        match transform {
            CameraTransform::Zoom(value) => {
                //ToDo: Fix the zoom to never become negative
                self.zoom += value;
                if self.zoom < 0.0 {
                    self.zoom = 0.0001;
                }
            }
            CameraTransform::ZoomTowards { target, amount } => {
                let old_zoom = self.zoom;

                self.zoom += amount;
                if self.zoom < 0.0 {
                    self.zoom = 0.0001;
                }

                let zoom_ratio = self.zoom / old_zoom;
                let to_target_eye = target - self.eye_position;
                let to_target_target = target - self.target_position;

                self.eye_position = target - to_target_eye * zoom_ratio;
                self.target_position = target - to_target_target * zoom_ratio;
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
            CameraTransform::SetView {
                eye_position,
                target_position,
                up_vector,
            } => {
                self.eye_position = eye_position;
                self.target_position = target_position;
                self.up_vector = up_vector;
            }
            CameraTransform::FitToExtent { min, max } => {
                let view_matrix = self.get_view_matrix();
                let max_extent =
                    calculate_max_extent_in_camera_space(&BoundingBox { min, max }, &view_matrix);

                if !max_extent.is_nan() && max_extent.is_sign_positive() && max_extent.is_finite() {
                    self.zoom = 2.0 / max_extent
                }
            }
        }

        self
    }

    fn set_viewport_size(&mut self, width: f32, height: f32) {
        self.aspect_ratio = width / height;
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
            far: 500.0,
            zoom: 1.0,
            aspect_ratio: 1.0,
        }
    }
}

//calculates and returns the clip bounds (left, right, bottom, top)
fn get_bounds_from_zoom(current_zoom: f32, aspect_ratio: f32) -> (f32, f32, f32, f32) {
    let half_width = 1.0 / current_zoom;
    let half_height = half_width / aspect_ratio;

    let left = -half_width;
    let right = half_width;
    let bottom = -half_height;
    let top = half_height;

    (left, right, bottom, top)
}

fn calculate_frustum_planes(projection_matrix: Mat4, view_matrix: Mat4) -> Frustum {
    let clip_space_matrix = projection_matrix * view_matrix;
    let matrix = clip_space_matrix.to_cols_array_2d();
    //[4][4]

    let plane = |i: usize, j: usize, sign: isize| {
        //gribb-hartmann method for extracting planes from clip space matrix
        // https://www.gamedevs.org/uploads/fast-extraction-viewing-frustum-planes-from-world-view-projection-matrix.pdf
        let normal = Vec3::new(
            matrix[0][i] + sign as f32 * matrix[0][j],
            matrix[1][i] + sign as f32 * matrix[1][j],
            matrix[2][i] + sign as f32 * matrix[2][j],
        );

        let distance = matrix[3][i] + sign as f32 * matrix[3][j];
        Plane {
            normal: normal.normalize(),
            distance: distance / normal.length(),
        }
    };

    let left = plane(3, 0, 1);
    let right = plane(3, 0, -1);
    let bottom = plane(3, 1, 1);
    let top = plane(3, 1, -1);
    let near = plane(2, 0, 0); //sign is 0 since its v*col3
    let far = plane(3, 2, -1);

    Frustum([left, right, bottom, top, near, far])
}

fn calculate_max_extent_in_camera_space(bbox: &BoundingBox, view_matrix: &Mat4) -> f32 {
    let corners_in_camera_space = bbox.corners().map(|corner| {
        let corner_vector = corner.extend(1.0);
        let transformed = view_matrix * corner_vector;
        transformed.truncate()
    });
    let mut min = corners_in_camera_space[0];
    let mut max = corners_in_camera_space[0];

    for v in &corners_in_camera_space[1..] {
        min = min.min(*v);
        max = max.max(*v);
    }

    let extent = max - min;

    extent.x.max(extent.y).max(extent.z)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_frustum_planes_identity_matrices() {
        let projection_matrix = Mat4::IDENTITY;
        let view_matrix = Mat4::IDENTITY;
        let frustum = calculate_frustum_planes(projection_matrix, view_matrix);
        assert_eq!(frustum.0[0].normal, Vec3::new(1.0, 0.0, 0.0));
        assert_eq!(frustum.0[1].normal, Vec3::new(-1.0, 0.0, 0.0));
        assert_eq!(frustum.0[2].normal, Vec3::new(0.0, 1.0, 0.0));
        assert_eq!(frustum.0[3].normal, Vec3::new(0.0, -1.0, 0.0));
        assert_eq!(frustum.0[4].normal, Vec3::new(0.0, 0.0, 1.0));
        assert_eq!(frustum.0[5].normal, Vec3::new(0.0, 0.0, -1.0));
    }
}
