use thiserror::Error;

//export module
pub mod camera;
pub mod gpu_mesh;
pub mod instance;
pub mod material;
pub mod renderer;
pub mod texture;
pub mod transformation;
pub mod vertex;

// internal module
mod normalized_box;
mod pipeline;
mod render_pass;

#[derive(Debug, Error)]
pub enum WgpuError {
    #[error("Something went wrong with the Device request")]
    DeviceError(#[from] wgpu::RequestDeviceError),

    #[error("When something goes wrong with the adapter")]
    AdapterError(#[from] wgpu::RequestAdapterError),
}

pub struct RenderObject {
    pub renderable: Renderable,
    pub instance: instance::GpuInstance,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Renderable {
    ColoredMesh(u32),
    TexturedMesh(u32, Vec<(u32, u32)>), // (local binding resource index, slot index)
    ArrayTexturedMesh(u32, Vec<(u32, u32)>),
    Mesh(u32),
    WireframeMesh(u32),
}

#[cfg(test)]
mod tests {
    use std::{cmp::Ordering, path::PathBuf};

    use glam::{Mat4, Vec3};
    use nv_flip::DEFAULT_PIXELS_PER_DEGREE;

    use super::*;
    use crate::{
        camera::{Camera, OrthographicCameraData},
        gpu_mesh::MeshBuilder,
        instance::InstanceDataBuilder,
        material::Material,
        transformation::Transformation,
    };
    use normalized_box::{
        COLORS, INDEXED_POSITIONS_BOX_EDGE_INDICES, INDICES, ORDERED_POSITIONS,
        ORDERED_POSITIONS_BOX_EDGE_INDICES, POSITIONS, TEX_COORDS, TRI_EDGE_INDICES, USE_TEXTURE,
    };

    const TEXTURE_WIDTH: u32 = 512;
    const TEXTURE_HEIGHT: u32 = 512;
    const FLIP_MEAN_ERROR: f32 = 0.0;

    #[test]
    fn test_box_wireframe_only() {
        pollster::block_on(async {
            let mut renderer = renderer::Renderer::new_texture_based(TEXTURE_WIDTH, TEXTURE_HEIGHT)
                .await
                .unwrap();
            let _wireframe_object_id = set_wireframe_mesh_object(&mut renderer);

            let camera_bind_group = get_camera_bind_group(&renderer.device);
            let _ = renderer.render(&camera_bind_group).await;
            let image_buffer = renderer.present().await;
            // image_buffer.save("tests/data/wireframe_mesh.png").unwrap();

            let ref_image_data = image::open(PathBuf::from("tests/data/wireframe_mesh.png"))
                .unwrap()
                .into_rgba8();

            let ref_image =
                nv_flip::FlipImageRgb8::with_data(TEXTURE_WIDTH, TEXTURE_HEIGHT, &ref_image_data);
            let test_image =
                nv_flip::FlipImageRgb8::with_data(TEXTURE_WIDTH, TEXTURE_HEIGHT, &image_buffer);

            let error_map = nv_flip::flip(ref_image, test_image, DEFAULT_PIXELS_PER_DEGREE);
            let pool = nv_flip::FlipPool::from_image(&error_map);
            if let Some(Ordering::Greater) = pool.mean().partial_cmp(&FLIP_MEAN_ERROR) {
                println!("Mean error {}", pool.mean());
                panic!("Something is wrong with the Wireframes")
            }
        });
    }

    #[test]
    fn test_box_solid_color_only() {
        pollster::block_on(async {
            let mut renderer = renderer::Renderer::new_texture_based(TEXTURE_WIDTH, TEXTURE_HEIGHT)
                .await
                .unwrap();
            let (_mesh_object_id, _wireframe_object_id) = set_solid_mesh(&mut renderer);

            let camera_bind_group = get_camera_bind_group(&renderer.device);
            let _ = renderer.render(&camera_bind_group).await;
            let image_buffer = renderer.present().await;
            // image_buffer.save("tests/data/solid_color_mesh.png").unwrap();

            let ref_image_data = image::open(PathBuf::from("tests/data/solid_color_mesh.png"))
                .unwrap()
                .into_rgba8();

            let ref_image =
                nv_flip::FlipImageRgb8::with_data(TEXTURE_WIDTH, TEXTURE_HEIGHT, &ref_image_data);
            let test_image =
                nv_flip::FlipImageRgb8::with_data(TEXTURE_WIDTH, TEXTURE_HEIGHT, &image_buffer);

            let error_map = nv_flip::flip(ref_image, test_image, DEFAULT_PIXELS_PER_DEGREE);
            let pool = nv_flip::FlipPool::from_image(&error_map);
            if let Some(Ordering::Greater) = pool.mean().partial_cmp(&FLIP_MEAN_ERROR) {
                println!("Mean error {}", pool.mean());
                panic!("Something is wrong with the Solid colors")
            }
        });
    }

    #[test]
    fn test_box_with_vertex_color_only() {
        pollster::block_on(async {
            let mut renderer = renderer::Renderer::new_texture_based(TEXTURE_WIDTH, TEXTURE_HEIGHT)
                .await
                .unwrap();
            let (_mesh_object_id, _wireframe_object_id) = set_colored_mesh_object(&mut renderer);

            let camera_bind_group = get_camera_bind_group(&renderer.device);
            let _ = renderer.render(&camera_bind_group).await;
            let image_buffer = renderer.present().await;
            // image_buffer
            //     .save("tests/data/vertex_color_mesh.png")
            //     .unwrap();

            let ref_image_data = image::open(PathBuf::from("tests/data/vertex_color_mesh.png"))
                .unwrap()
                .into_rgba8();

            let ref_image =
                nv_flip::FlipImageRgb8::with_data(TEXTURE_WIDTH, TEXTURE_HEIGHT, &ref_image_data);
            let test_image =
                nv_flip::FlipImageRgb8::with_data(TEXTURE_WIDTH, TEXTURE_HEIGHT, &image_buffer);

            let error_map = nv_flip::flip(ref_image, test_image, DEFAULT_PIXELS_PER_DEGREE);
            let pool = nv_flip::FlipPool::from_image(&error_map);
            if let Some(Ordering::Greater) = pool.mean().partial_cmp(&FLIP_MEAN_ERROR) {
                println!("Mean error {}", pool.mean());
                panic!("Something is wrong with the Vertex colors")
            }
        });
    }

    #[test]
    fn test_box_with_single_texture_and_vertex_colors() {
        pollster::block_on(async {
            let mut renderer = renderer::Renderer::new_texture_based(TEXTURE_WIDTH, TEXTURE_HEIGHT)
                .await
                .unwrap();
            let (_mesh_object_id, _wireframe_object_id) = single_tex_mesh_object(&mut renderer);

            let camera_bind_group = get_camera_bind_group(&renderer.device);
            let _ = renderer.render(&camera_bind_group).await;
            let image_buffer = renderer.present().await;
            // image_buffer.save("tests/data/single_tex_mesh.png").unwrap();

            let ref_image_data = image::open(PathBuf::from("tests/data/single_tex_mesh.png"))
                .unwrap()
                .into_rgba8();

            let ref_image =
                nv_flip::FlipImageRgb8::with_data(TEXTURE_WIDTH, TEXTURE_HEIGHT, &ref_image_data);
            let test_image =
                nv_flip::FlipImageRgb8::with_data(TEXTURE_WIDTH, TEXTURE_HEIGHT, &image_buffer);

            let error_map = nv_flip::flip(ref_image, test_image, DEFAULT_PIXELS_PER_DEGREE);
            let pool = nv_flip::FlipPool::from_image(&error_map);
            if let Some(Ordering::Greater) = pool.mean().partial_cmp(&FLIP_MEAN_ERROR) {
                println!("Mean error {}", pool.mean());
                panic!("Something is wrong with the Single Texture mesh rendering")
            }
        });
    }

    #[test]
    fn test_box_with_multiple_textures_and_vertex_colors() {
        pollster::block_on(async {
            let mut renderer = renderer::Renderer::new_texture_based(TEXTURE_WIDTH, TEXTURE_HEIGHT)
                .await
                .unwrap();
            let (_mesh_object_id, _wireframe_object_id) = set_multi_tex_mesh_object(&mut renderer);

            let camera_bind_group = get_camera_bind_group(&renderer.device);
            let _ = renderer.render(&camera_bind_group).await;
            let image_buffer = renderer.present().await;
            // image_buffer
            //     .save("tests/data/array_tex_mesh_new.png")
            //     .unwrap();

            let ref_image_data = image::open(PathBuf::from("tests/data/array_tex_mesh.png"))
                .unwrap()
                .into_rgba8();

            let ref_image =
                nv_flip::FlipImageRgb8::with_data(TEXTURE_WIDTH, TEXTURE_HEIGHT, &ref_image_data);
            let test_image =
                nv_flip::FlipImageRgb8::with_data(TEXTURE_WIDTH, TEXTURE_HEIGHT, &image_buffer);

            let error_map = nv_flip::flip(ref_image, test_image, DEFAULT_PIXELS_PER_DEGREE);
            let pool = nv_flip::FlipPool::from_image(&error_map);
            if let Some(Ordering::Greater) = pool.mean().partial_cmp(&FLIP_MEAN_ERROR) {
                println!("Mean error {}", pool.mean());
                panic!("Something is wrong with the Arrayed Texture mesh rendering")
            }
        });
    }

    fn get_camera_bind_group(device: &wgpu::Device) -> wgpu::BindGroup {
        let camera_data = OrthographicCameraData::default()
            .transform(camera::CameraTransform::Zoom(-0.80))
            .transform(camera::CameraTransform::Pan(Vec3 {
                x: 0.5,
                y: 0.5,
                z: 0.0,
            }))
            .transform(camera::CameraTransform::Rotate {
                pivot: Vec3 {
                    x: 0.0,
                    y: 0.0,
                    z: 0.0,
                },
                rotation_axis: Vec3::NEG_X,
                angle: std::f32::consts::FRAC_2_PI,
            })
            .transform(camera::CameraTransform::Rotate {
                pivot: Vec3 {
                    x: 0.0,
                    y: 0.0,
                    z: 0.0,
                },
                rotation_axis: Vec3::NEG_Y,
                angle: -std::f32::consts::FRAC_2_SQRT_PI * 1.5,
            })
            .transform(camera::CameraTransform::Rotate {
                pivot: Vec3 {
                    x: 0.0,
                    y: 0.0,
                    z: 0.0,
                },
                rotation_axis: Vec3::X,
                angle: std::f32::consts::FRAC_2_SQRT_PI,
            })
            .transform(camera::CameraTransform::Pan(Vec3 {
                x: 1.0,
                y: 1.0,
                z: 0.0,
            }));

        let camera = Camera::new(&camera_data);

        camera.create_bind_group(device)
    }

    fn create_texture_and_texture_bind_group(
        bytes: &[u8],
        path: &str,
        renderer: &mut renderer::Renderer,
    ) -> (u32, u32) {
        let tex =
            texture::Texture::from_bytes(&renderer.device, &renderer.queue, bytes, path).unwrap();
        renderer.add_texture(tex)
    }

    fn set_multi_tex_mesh_object(renderer: &mut renderer::Renderer) -> (u32, u32) {
        let (top_tex_id, _) = create_texture_and_texture_bind_group(
            include_bytes!("resources/top-tex.png"),
            "top-tex.png",
            renderer,
        );

        let (right_tex_id, _) = create_texture_and_texture_bind_group(
            include_bytes!("resources/right-tex.png"),
            "right-tex.png",
            renderer,
        );

        let (left_tex_id, _) = create_texture_and_texture_bind_group(
            include_bytes!("resources/left-tex.png"),
            "left-tex.png",
            renderer,
        );

        let (bottom_tex_id, _) = create_texture_and_texture_bind_group(
            include_bytes!("resources/bottom-tex.png"),
            "bottom-tex.png",
            renderer,
        );

        let (front_tex_id, _) = create_texture_and_texture_bind_group(
            include_bytes!("resources/front-tex.png"),
            "front-tex.png",
            renderer,
        );

        let (back_tex_id, _) = create_texture_and_texture_bind_group(
            include_bytes!("resources/back-tex.png"),
            "back-tex.png",
            renderer,
        );

        let texture_array_bind_group = renderer.create_texture_array(&[
            front_tex_id,
            bottom_tex_id,
            front_tex_id,
            back_tex_id,
            left_tex_id,
            right_tex_id,
        ]);

        let multi_tex_mesh = MeshBuilder::new()
            .add_vertex_stream(POSITIONS)
            .add_vertex_stream(COLORS)
            .add_vertex_stream(TEX_COORDS)
            .add_vertex_stream(&normalized_box::get_use_texture_vertices(
                vertex::UseTexture::from_texture_index(back_tex_id),
                vertex::UseTexture::from_texture_index(front_tex_id),
                vertex::UseTexture::from_texture_index(bottom_tex_id),
                vertex::UseTexture::no(),
                vertex::UseTexture::from_texture_index(top_tex_id),
                vertex::UseTexture::from_texture_index(right_tex_id),
                // vertex::UseTexture::from_texture_index(left_tex_id),
            ))
            .add_mesh_index_stream(INDICES)
            .add_wireframe_index_stream(INDEXED_POSITIONS_BOX_EDGE_INDICES)
            .build(&renderer.device);
        let multi_tex_mesh_id = renderer.add_mesh(multi_tex_mesh);

        let transformations = [
            Transformation(Mat4::from_translation((0.0, 5.0, 0.0).into())).to_data(),
            Transformation(Mat4::from_axis_angle(
                Vec3 {
                    x: 0.0,
                    y: 1.0,
                    z: 0.0,
                },
                45.0_f32.to_radians(),
            ))
            .to_data(),
        ];

        let material_colors = [
            Material::new(0.0, 0.0, 1.0).to_data(),
            Material::new(0.0, 0.0, 1.0).to_data(),
        ];

        let multi_tex_mesh_instance_buffer = InstanceDataBuilder::new()
            .add_instance_stream(&transformations)
            .add_instance_stream(&material_colors)
            .build(&renderer.device);

        let multi_tex_mesh_object = RenderObject {
            renderable: Renderable::ArrayTexturedMesh(
                multi_tex_mesh_id,
                vec![(texture_array_bind_group, 1)],
            ),
            instance: multi_tex_mesh_instance_buffer,
        };
        let _multi_tex_mesh_object_id = renderer.add_object(multi_tex_mesh_object);

        let multi_tex_mesh_wireframe_object = RenderObject {
            renderable: Renderable::WireframeMesh(multi_tex_mesh_id),
            instance: InstanceDataBuilder::new()
                .add_instance_stream(&transformations)
                .add_instance_stream(&material_colors)
                .build(&renderer.device),
        };
        let _multi_tex_mesh_wireframe_object_id =
            renderer.add_object(multi_tex_mesh_wireframe_object);

        (
            _multi_tex_mesh_object_id,
            _multi_tex_mesh_wireframe_object_id,
        )
    }

    fn single_tex_mesh_object(renderer: &mut renderer::Renderer) -> (u32, u32) {
        let (_, happy_tree_bind_group_id) = create_texture_and_texture_bind_group(
            include_bytes!("resources/happy-tree.png"),
            "happy-tree",
            renderer,
        );

        let single_tex_mesh = MeshBuilder::new()
            .add_vertex_stream(POSITIONS)
            .add_vertex_stream(COLORS)
            .add_vertex_stream(TEX_COORDS)
            .add_vertex_stream(USE_TEXTURE)
            .add_mesh_index_stream(INDICES)
            .add_wireframe_index_stream(TRI_EDGE_INDICES)
            .build(&renderer.device);
        let single_tex_mesh_id = renderer.add_mesh(single_tex_mesh);

        let transformations = [
            Transformation(Mat4::from_translation((0.0, 5.0, 0.0).into())).to_data(),
            Transformation(Mat4::from_axis_angle(
                Vec3 {
                    x: 0.0,
                    y: 1.0,
                    z: 0.0,
                },
                45.0_f32.to_radians(),
            ))
            .to_data(),
        ];

        let material_colors = [
            Material::new(0.0, 0.0, 1.0).to_data(),
            Material::new(0.0, 0.0, 1.0).to_data(),
        ];

        let single_tex_mesh_instance_buffer = InstanceDataBuilder::new()
            .add_instance_stream(&transformations)
            .add_instance_stream(&material_colors)
            .build(&renderer.device);

        let single_tex_mesh_object = RenderObject {
            renderable: Renderable::TexturedMesh(
                single_tex_mesh_id,
                vec![(happy_tree_bind_group_id, 1)],
            ),
            instance: single_tex_mesh_instance_buffer,
        };
        let _single_tex_mesh_object_id = renderer.add_object(single_tex_mesh_object);

        let single_tex_mesh_wireframe_object = RenderObject {
            renderable: Renderable::WireframeMesh(single_tex_mesh_id),
            instance: InstanceDataBuilder::new()
                .add_instance_stream(&transformations)
                .add_instance_stream(&material_colors)
                .build(&renderer.device),
        };
        let _single_tex_mesh_wireframe_object_id =
            renderer.add_object(single_tex_mesh_wireframe_object);

        (
            _single_tex_mesh_object_id,
            _single_tex_mesh_wireframe_object_id,
        )
    }

    fn set_colored_mesh_object(renderer: &mut renderer::Renderer) -> (u32, u32) {
        let colored_mesh = MeshBuilder::new()
            .add_vertex_stream(POSITIONS)
            .add_vertex_stream(COLORS)
            .add_mesh_index_stream(INDICES)
            .add_wireframe_index_stream(INDEXED_POSITIONS_BOX_EDGE_INDICES)
            .build(&renderer.device);
        let colored_mesh_id = renderer.add_mesh(colored_mesh);

        let transformations = [
            Transformation(Mat4::from_translation((0.0, 5.0, 0.0).into())).to_data(),
            Transformation(Mat4::from_axis_angle(
                Vec3 {
                    x: 0.0,
                    y: 1.0,
                    z: 0.0,
                },
                45.0_f32.to_radians(),
            ))
            .to_data(),
        ];

        let material_colors = [
            Material::new(0.0, 0.0, 1.0).to_data(),
            Material::new(0.0, 0.0, 1.0).to_data(),
        ];

        let colored_mesh_instance_buffer = InstanceDataBuilder::new()
            .add_instance_stream(&transformations)
            .add_instance_stream(&material_colors)
            .build(&renderer.device);

        let colored_mesh_object = RenderObject {
            renderable: Renderable::ColoredMesh(colored_mesh_id),
            instance: colored_mesh_instance_buffer,
        };
        let _colored_mesh_object_id = renderer.add_object(colored_mesh_object);

        let colored_mesh_wireframe_object = RenderObject {
            renderable: Renderable::WireframeMesh(colored_mesh_id),
            instance: InstanceDataBuilder::new()
                .add_instance_stream(&transformations)
                .add_instance_stream(&material_colors)
                .build(&renderer.device),
        };
        let _colored_mesh_wireframe_object_id = renderer.add_object(colored_mesh_wireframe_object);

        (_colored_mesh_object_id, _colored_mesh_wireframe_object_id)
    }

    fn set_solid_mesh(renderer: &mut renderer::Renderer) -> (u32, u32) {
        let simple_mesh = MeshBuilder::new()
            .add_vertex_stream(ORDERED_POSITIONS)
            .add_wireframe_index_stream(ORDERED_POSITIONS_BOX_EDGE_INDICES)
            .build(&renderer.device);

        let simple_mesh_id = renderer.add_mesh(simple_mesh);

        let transformations = [
            Transformation(Mat4::from_translation((0.0, 5.0, 0.0).into())).to_data(),
            Transformation(Mat4::from_axis_angle(
                Vec3 {
                    x: 0.0,
                    y: 1.0,
                    z: 0.0,
                },
                45.0_f32.to_radians(),
            ))
            .to_data(),
        ];

        let simple_mesh_instance_buffer = InstanceDataBuilder::new()
            .add_instance_stream(&transformations)
            .add_instance_stream(&[
                Material::new(0.75, 0.05, 0.5).to_data(),
                Material::new(1.0, 0.0, 1.0).to_data(),
            ])
            .build(&renderer.device);

        let simple_mesh_object = RenderObject {
            renderable: Renderable::Mesh(simple_mesh_id),
            instance: simple_mesh_instance_buffer,
        };
        let _simple_mesh_object_id = renderer.add_object(simple_mesh_object);

        let simple_mesh_wireframe_object = RenderObject {
            renderable: Renderable::WireframeMesh(simple_mesh_id),
            instance: InstanceDataBuilder::new()
                .add_instance_stream(&transformations)
                .add_instance_stream(&[
                    Material::new(0.0, 0.0, 1.0).to_data(),
                    Material::new(0.0, 0.0, 1.0).to_data(),
                ])
                .build(&renderer.device),
        };
        let _simple_mesh_wireframe_object_id = renderer.add_object(simple_mesh_wireframe_object);

        (_simple_mesh_object_id, _simple_mesh_wireframe_object_id)
    }

    fn set_wireframe_mesh_object(renderer: &mut renderer::Renderer) -> u32 {
        let simple_mesh = MeshBuilder::new()
            .add_vertex_stream(ORDERED_POSITIONS)
            .add_wireframe_index_stream(ORDERED_POSITIONS_BOX_EDGE_INDICES)
            .build(&renderer.device);

        let simple_mesh_id = renderer.add_mesh(simple_mesh);

        let transformations = [
            Transformation(Mat4::from_translation((0.0, 5.0, 0.0).into())).to_data(),
            Transformation(Mat4::from_axis_angle(
                Vec3 {
                    x: 0.0,
                    y: 1.0,
                    z: 0.0,
                },
                45.0_f32.to_radians(),
            ))
            .to_data(),
        ];

        let simple_mesh_wireframe_object = RenderObject {
            renderable: Renderable::WireframeMesh(simple_mesh_id),
            instance: InstanceDataBuilder::new()
                .add_instance_stream(&transformations)
                .add_instance_stream(&[
                    Material::new(0.0, 0.0, 1.0).to_data(),
                    Material::new(0.0, 0.0, 1.0).to_data(),
                ])
                .build(&renderer.device),
        };

        renderer.add_object(simple_mesh_wireframe_object)
    }
}
