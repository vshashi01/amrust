use thiserror::Error;

mod prelude;
pub use prelude::*;

use crate::{gpu_mesh::GpuMesh, instance::GpuInstance};

//export module
pub mod bounding_box;
pub mod camera;
pub mod clip;
pub mod gpu_mesh;
pub mod instance;
pub mod material;
pub mod normalized_axis_gizmo;
pub mod normalized_box;
pub mod render_data_3d;
pub mod renderer;
pub mod screen_space;
pub mod texture;
pub mod transformation;
pub mod transparency;
pub mod vertex;

// internal module
mod composite;
mod constants;
mod light;
mod pipeline;
mod render_pass;

pub use render_data_3d::RenderDataLocalResources;

#[derive(Debug, Error)]
pub enum WgpuError {
    #[error("Something went wrong with the Device request")]
    DeviceError(#[from] wgpu::RequestDeviceError),
    // #[cfg(feature = "wgpu")]
    // #[error("When something goes wrong with the adapter")]
    // AdapterError(#[from] wgpu::RequestAdapterError),
}

#[derive(Clone)]
pub struct RenderData3d<'a> {
    pub renderable: Renderable3d,
    pub mesh: &'a GpuMesh,
    pub instance: &'a GpuInstance,
    pub local_resources: &'a RenderDataLocalResources,
    pub triangle_face_mode: Option<TriangleFaceMode>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Renderable3d {
    ColoredMesh,
    TexturedMesh,
    ArrayTexturedMesh,
    Mesh,
    WireframeMesh,
    SilhouetteMesh,
    ScreenSpaceColoredMesh { depth_testing: bool, order: u8 },
    ScreenSpaceWireframeMesh { depth_testing: bool, order: u8 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TriangleFaceMode {
    FrontAndBack,
    FrontOnly,
}

#[cfg(test)]
mod tests {
    use std::{cmp::Ordering, path::PathBuf};

    use glam::{Mat4, Vec3};
    use nv_flip::DEFAULT_PIXELS_PER_DEGREE;

    use super::*;
    use crate::{
        camera::{CameraData, OrthographicCameraData},
        clip::ClipPlanes,
        gpu_mesh::{GpuMesh, MeshBuilder},
        instance::InstanceDataBuilder,
        light::{NormalMatrixData, UseLightingData},
        material::Material,
        screen_space::SizeInPixel,
        transformation::Transformation,
        transparency::Transparency,
    };
    use normalized_box::{
        COLORS, INDEXED_POSITIONS_BOX_EDGE_INDICES, INDICES, NORMALS, ORDERED_POSITIONS,
        ORDERED_POSITIONS_BOX_EDGE_INDICES, POSITIONS, TEX_COORDS, TRI_EDGE_INDICES, USE_TEXTURE,
    };

    const TEXTURE_WIDTH: u32 = 512;
    const TEXTURE_HEIGHT: u32 = 512;
    const FLIP_MEAN_ERROR: f32 = 0.00;

    #[test]
    fn test_box_wireframe_only() {
        pollster::block_on(async {
            let renderer = renderer::Renderer::from_new_device().await.unwrap();

            let mut frame_view_data =
                renderer.create_frame_view_data(TEXTURE_WIDTH, TEXTURE_HEIGHT);
            frame_view_data
                .camera
                .update(&get_camera_data(TEXTURE_WIDTH, TEXTURE_HEIGHT));
            renderer.write_frame_view_data_to_gpu(&frame_view_data);

            let read_buffer =
                renderer::create_read_buffer(&renderer.device, TEXTURE_WIDTH, TEXTURE_HEIGHT);

            let mut render_db = TestRenderDb::new(&renderer.device);
            let _wireframe_object_id = set_wireframe_mesh_object(&renderer.device, &mut render_db);

            let render_data = render_db.get_renderables().collect::<Vec<_>>();
            let image_buffer = renderer
                .render_and_return_as_image_buffer(
                    &render_data,
                    &read_buffer,
                    wgpu::Extent3d {
                        width: TEXTURE_WIDTH,
                        height: TEXTURE_HEIGHT,
                        depth_or_array_layers: 1,
                    },
                    &frame_view_data,
                )
                .await
                .unwrap();
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
    fn test_colored_mesh_mixed_lighting_instances() {
        pollster::block_on(async {
            let renderer = renderer::Renderer::from_new_device().await.unwrap();

            let mut frame_view_data =
                renderer.create_frame_view_data(TEXTURE_WIDTH, TEXTURE_HEIGHT);
            frame_view_data
                .camera
                .update(&get_camera_data(TEXTURE_WIDTH, TEXTURE_HEIGHT));
            renderer.write_frame_view_data_to_gpu(&frame_view_data);

            let read_buffer =
                renderer::create_read_buffer(&renderer.device, TEXTURE_WIDTH, TEXTURE_HEIGHT);

            let mut render_db = TestRenderDb::new(&renderer.device);
            let colored_mesh = MeshBuilder::new()
                .add_vertex_stream(POSITIONS)
                .add_vertex_stream(COLORS)
                .add_vertex_stream(NORMALS)
                .add_mesh_index_stream(INDICES)
                .build(&renderer.device);
            let colored_mesh_id = render_db.add_mesh(colored_mesh);

            let transformations = [
                Transformation(Mat4::from_translation((0.0, 5.0, 0.0).into())).to_data(),
                Transformation(Mat4::from_translation((3.0, 5.0, 0.0).into())).to_data(),
            ];

            let normal_matrices = [
                NormalMatrixData::from_model_matrix(
                    Mat4::from_translation((0.0, 5.0, 0.0).into()),
                    UseLightingData::YES,
                ),
                NormalMatrixData::from_model_matrix(
                    Mat4::from_translation((3.0, 5.0, 0.0).into()),
                    UseLightingData::NO,
                ),
            ];

            let material_colors = [
                Material::new(0.0, 0.8, 0.2).to_data(),
                Material::new(0.2, 0.2, 1.0).to_data(),
            ];

            let colored_mesh_instance_buffer = InstanceDataBuilder::new()
                .add_instance_stream(&transformations)
                .add_instance_stream(&material_colors)
                .add_instance_stream(&normal_matrices)
                .build(&renderer.device);

            let colored_mesh_object = RenderObject {
                renderable: Renderable3d::ColoredMesh,
                gpu_mesh_id: colored_mesh_id,
                instance: colored_mesh_instance_buffer,
                mesh_local: RenderDataLocalResources::new_colored(
                    &renderer.device,
                    ClipPlanes::new(&renderer.device),
                    Transparency::opaque(&renderer.device),
                ),
                cull_mode: None,
            };

            let _colored_mesh_object_id = render_db.add_object(colored_mesh_object);

            let render_data = render_db.get_renderables().collect::<Vec<_>>();
            let image_buffer = renderer
                .render_and_return_as_image_buffer(
                    &render_data,
                    &read_buffer,
                    wgpu::Extent3d {
                        width: TEXTURE_WIDTH,
                        height: TEXTURE_HEIGHT,
                        depth_or_array_layers: 1,
                    },
                    &frame_view_data,
                )
                .await
                .unwrap();

            // image_buffer
            //     .save("tests/data/colored_mesh_mixed_lighting.png")
            //     .unwrap();

            let ref_image_data =
                image::open(PathBuf::from("tests/data/colored_mesh_mixed_lighting.png"))
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
                panic!("Something is wrong with the mixed lighting colors")
            }
        });
    }

    #[test]
    fn test_box_solid_color_only() {
        pollster::block_on(async {
            let renderer = renderer::Renderer::from_new_device().await.unwrap();

            let mut frame_view_data =
                renderer.create_frame_view_data(TEXTURE_WIDTH, TEXTURE_HEIGHT);
            frame_view_data
                .camera
                .update(&get_camera_data(TEXTURE_WIDTH, TEXTURE_HEIGHT));
            renderer.write_frame_view_data_to_gpu(&frame_view_data);

            let read_buffer =
                renderer::create_read_buffer(&renderer.device, TEXTURE_WIDTH, TEXTURE_HEIGHT);

            let mut render_db = TestRenderDb::new(&renderer.device);
            let (_mesh_object_id, _wireframe_object_id) =
                set_solid_mesh(&renderer.device, &mut render_db);

            let render_data = render_db.get_renderables().collect::<Vec<_>>();
            let image_buffer = renderer
                .render_and_return_as_image_buffer(
                    &render_data,
                    &read_buffer,
                    wgpu::Extent3d {
                        width: TEXTURE_WIDTH,
                        height: TEXTURE_HEIGHT,
                        depth_or_array_layers: 1,
                    },
                    &frame_view_data,
                )
                .await
                .unwrap();
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
            let renderer = renderer::Renderer::from_new_device().await.unwrap();
            let mut frame_view_data =
                renderer.create_frame_view_data(TEXTURE_WIDTH, TEXTURE_HEIGHT);
            frame_view_data
                .camera
                .update(&get_camera_data(TEXTURE_WIDTH, TEXTURE_HEIGHT));
            renderer.write_frame_view_data_to_gpu(&frame_view_data);

            let read_buffer =
                renderer::create_read_buffer(&renderer.device, TEXTURE_WIDTH, TEXTURE_HEIGHT);

            let mut render_db = TestRenderDb::new(&renderer.device);
            let (_mesh_object_id, _wireframe_object_id) =
                set_colored_mesh_object(&renderer.device, None, &mut render_db);

            let render_data = render_db.get_renderables().collect::<Vec<_>>();
            let image_buffer = renderer
                .render_and_return_as_image_buffer(
                    &render_data,
                    &read_buffer,
                    wgpu::Extent3d {
                        width: TEXTURE_WIDTH,
                        height: TEXTURE_HEIGHT,
                        depth_or_array_layers: 1,
                    },
                    &frame_view_data,
                )
                .await
                .unwrap();
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
    fn test_box_with_vertex_color_with_sub_viewport() {
        pollster::block_on(async {
            let renderer = renderer::Renderer::from_new_device().await.unwrap();

            let mut main_frame_view_data =
                renderer.create_frame_view_data(TEXTURE_WIDTH, TEXTURE_HEIGHT);
            main_frame_view_data
                .camera
                .update(&get_camera_data(TEXTURE_WIDTH, TEXTURE_HEIGHT));
            renderer.write_frame_view_data_to_gpu(&main_frame_view_data);

            let read_buffer =
                renderer::create_read_buffer(&renderer.device, TEXTURE_WIDTH, TEXTURE_HEIGHT);

            let mut main_render_db = TestRenderDb::new(&renderer.device);
            let (_mesh_object_id, _wireframe_object_id) =
                set_colored_mesh_object(&renderer.device, None, &mut main_render_db);
            let main_render_data = main_render_db.get_renderables().collect::<Vec<_>>();

            let mut secondary_render_db = TestRenderDb::new(&renderer.device);
            set_simple_mesh(&renderer.device, &mut secondary_render_db);
            set_wcs_gizmo(&renderer.device, &mut secondary_render_db);
            let secondary_render_data = secondary_render_db.get_renderables().collect::<Vec<_>>();

            let mut subviewport_frame_view_data = renderer.create_frame_view_data(400, 400);
            let mut camera_data = OrthographicCameraData {
                aspect_ratio: 400_f32 / 400_f32,
                ..Default::default()
            };
            camera_data
                .transform(camera::CameraTransform::SetView {
                    eye_position: glam::Vec3::new(0.0, 0.0, 25.0),
                    target_position: glam::Vec3::new(0.0, 0.0, 0.0),
                    up_vector: glam::Vec3::Y,
                })
                .transform(camera::CameraTransform::Zoom(-0.6));
            subviewport_frame_view_data.update_viewport_size(400.0, 400.0, &camera_data);
            renderer.write_frame_view_data_to_gpu(&subviewport_frame_view_data);

            let views = [
                renderer::RenderView::new_main_view(
                    &main_render_data,
                    &main_frame_view_data,
                    wgpu::Color {
                        r: 0.1,
                        g: 0.2,
                        b: 0.3,
                        a: 1.0,
                    },
                ),
                renderer::RenderView::new_sub_viewport(
                    renderer::ViewportRect {
                        x: 0,
                        y: 0,
                        width: 400,
                        height: 400,
                    },
                    &secondary_render_data,
                    &subviewport_frame_view_data,
                ),
            ];

            let image_buffer = renderer
                .render_views_and_return_as_image_buffer(
                    &views,
                    &read_buffer,
                    wgpu::Extent3d {
                        width: TEXTURE_WIDTH,
                        height: TEXTURE_HEIGHT,
                        depth_or_array_layers: 1,
                    },
                )
                .await
                .unwrap();

            // image_buffer
            //     .save("tests/data/vertex_color_mesh_with_subviewport_actual.png")
            //     .unwrap();

            let ref_image_data = image::open(PathBuf::from(
                "tests/data/vertex_color_mesh_with_subviewport.png",
            ))
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
                panic!("Something is wrong with the Subviewport rendering")
            }
        });
    }

    #[test]
    fn test_box_with_vertex_color_global_clip_plane() {
        pollster::block_on(async {
            let renderer = renderer::Renderer::from_new_device().await.unwrap();

            let mut frame_view_data =
                renderer.create_frame_view_data(TEXTURE_WIDTH, TEXTURE_HEIGHT);
            frame_view_data
                .camera
                .update(&get_camera_data(TEXTURE_WIDTH, TEXTURE_HEIGHT));
            frame_view_data.set_clip_plane(&clip::ClipPlane {
                axis: clip::ClipPlaneAxis::X,
                axis_sign: 1.0,
                is_enabled: true,
                d: 0.0,
                is_finite: false,
                bounds_min: glam::Vec2::ZERO,
                bounds_max: glam::Vec2::ZERO,
            });
            renderer.write_frame_view_data_to_gpu(&frame_view_data);

            let read_buffer =
                renderer::create_read_buffer(&renderer.device, TEXTURE_WIDTH, TEXTURE_HEIGHT);

            let mut render_db = TestRenderDb::new(&renderer.device);
            let (_mesh_object_id, _wireframe_object_id) =
                set_colored_mesh_object(&renderer.device, None, &mut render_db);

            let render_data = render_db.get_renderables().collect::<Vec<_>>();
            let image_buffer = renderer
                .render_and_return_as_image_buffer(
                    &render_data,
                    &read_buffer,
                    wgpu::Extent3d {
                        width: TEXTURE_WIDTH,
                        height: TEXTURE_HEIGHT,
                        depth_or_array_layers: 1,
                    },
                    &frame_view_data,
                )
                .await
                .unwrap();
            // image_buffer
            //     .save("tests/data/vertex_color_mesh_with_global_clip_plane_actual.png")
            //     .unwrap();

            let ref_image_data = image::open(PathBuf::from(
                "tests/data/vertex_color_mesh_with_global_clip_plane.png",
            ))
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
                panic!("Something is wrong with global clipping")
            }
        });
    }

    #[test]
    fn test_box_with_vertex_color_global_and_local_clip_plane() {
        pollster::block_on(async {
            let renderer = renderer::Renderer::from_new_device().await.unwrap();

            let mut frame_view_data =
                renderer.create_frame_view_data(TEXTURE_WIDTH, TEXTURE_HEIGHT);
            frame_view_data
                .camera
                .update(&get_camera_data(TEXTURE_WIDTH, TEXTURE_HEIGHT));
            frame_view_data.set_clip_plane(&clip::ClipPlane {
                axis: clip::ClipPlaneAxis::Y,
                axis_sign: -1.0,
                is_enabled: true,
                d: -3.0,
                is_finite: false,
                bounds_min: glam::Vec2::ZERO,
                bounds_max: glam::Vec2::ZERO,
            });
            renderer.write_frame_view_data_to_gpu(&frame_view_data);

            let read_buffer =
                renderer::create_read_buffer(&renderer.device, TEXTURE_WIDTH, TEXTURE_HEIGHT);
            let mut render_db = TestRenderDb::new(&renderer.device);
            let mut clip_plane = ClipPlanes::new(&renderer.device);
            clip_plane.update_clip_planes(|planes| {
                if let Some(plane) = planes.first_mut() {
                    plane.axis = clip::ClipPlaneAxis::X;
                    plane.axis_sign = -1.0;
                    plane.is_enabled = true;
                    plane.d = 0.0;
                    plane.is_finite = false;
                }
            });
            clip_plane.write_buffer(&renderer.queue);
            let (_mesh_a_id, _mesh_b_id) =
                set_two_colored_mesh_objects(&renderer.device, &mut render_db, Some(clip_plane));

            let render_data = render_db.get_renderables().collect::<Vec<_>>();
            let image_buffer = renderer
                .render_and_return_as_image_buffer(
                    &render_data,
                    &read_buffer,
                    wgpu::Extent3d {
                        width: TEXTURE_WIDTH,
                        height: TEXTURE_HEIGHT,
                        depth_or_array_layers: 1,
                    },
                    &frame_view_data,
                )
                .await
                .unwrap();
            // image_buffer
            //     .save("tests/data/vertex_color_mesh_with_global_and_local_clip_plane_actual.png")
            //     .unwrap();

            let ref_image_data = image::open(PathBuf::from(
                "tests/data/vertex_color_mesh_with_global_and_local_clip_plane.png",
            ))
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
                panic!("Something is wrong with global and local clipping")
            }
        });
    }

    #[test]
    fn test_box_with_global_no_cull_and_global_clip_plane() {
        pollster::block_on(async {
            let renderer = renderer::Renderer::from_new_device().await.unwrap();

            let mut frame_view_data =
                renderer.create_frame_view_data(TEXTURE_WIDTH, TEXTURE_HEIGHT);

            // Set no cull mode - this is the key test feature
            frame_view_data.triangle_face_mode = TriangleFaceMode::FrontAndBack;

            frame_view_data
                .camera
                .update(&get_camera_data(TEXTURE_WIDTH, TEXTURE_HEIGHT));
            frame_view_data.set_clip_plane(&clip::ClipPlane {
                axis: clip::ClipPlaneAxis::X,
                axis_sign: 1.0,
                is_enabled: true,
                d: 0.0,
                is_finite: false,
                bounds_min: glam::Vec2::ZERO,
                bounds_max: glam::Vec2::ZERO,
            });
            renderer.write_frame_view_data_to_gpu(&frame_view_data);

            let read_buffer =
                renderer::create_read_buffer(&renderer.device, TEXTURE_WIDTH, TEXTURE_HEIGHT);

            let mut render_db = TestRenderDb::new(&renderer.device);
            let (_mesh_object_id, _wireframe_object_id) =
                set_colored_mesh_object(&renderer.device, None, &mut render_db);

            let render_data = render_db.get_renderables().collect::<Vec<_>>();
            let image_buffer = renderer
                .render_and_return_as_image_buffer(
                    &render_data,
                    &read_buffer,
                    wgpu::Extent3d {
                        width: TEXTURE_WIDTH,
                        height: TEXTURE_HEIGHT,
                        depth_or_array_layers: 1,
                    },
                    &frame_view_data,
                )
                .await
                .unwrap();
            // Uncomment to save reference image
            // image_buffer
            //     .save("tests/data/vertex_color_mesh_with_no_cull_and_global_clip_plane_actual.png")
            //     .unwrap();

            let ref_image_data = image::open(PathBuf::from(
                "tests/data/vertex_color_mesh_with_no_cull_and_global_clip_plane.png",
            ))
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
                panic!("Something is wrong with no culling and global clipping")
            }
        });
    }

    #[test]
    fn test_box_with_local_no_cull_and_global_clip_plane() {
        pollster::block_on(async {
            let renderer = renderer::Renderer::from_new_device().await.unwrap();

            let mut frame_view_data =
                renderer.create_frame_view_data(TEXTURE_WIDTH, TEXTURE_HEIGHT);
            frame_view_data.triangle_face_mode = TriangleFaceMode::FrontOnly;

            frame_view_data
                .camera
                .update(&get_camera_data(TEXTURE_WIDTH, TEXTURE_HEIGHT));
            frame_view_data.set_clip_plane(&clip::ClipPlane {
                axis: clip::ClipPlaneAxis::X,
                axis_sign: 1.0,
                is_enabled: true,
                d: 0.0,
                is_finite: false,
                bounds_min: glam::Vec2::ZERO,
                bounds_max: glam::Vec2::ZERO,
            });
            renderer.write_frame_view_data_to_gpu(&frame_view_data);

            let read_buffer =
                renderer::create_read_buffer(&renderer.device, TEXTURE_WIDTH, TEXTURE_HEIGHT);

            let mut render_db = TestRenderDb::new(&renderer.device);
            let (_mesh_object_id, _wireframe_object_id) =
                set_colored_mesh_object(&renderer.device, Some(true), &mut render_db);

            let render_data = render_db.get_renderables().collect::<Vec<_>>();
            let image_buffer = renderer
                .render_and_return_as_image_buffer(
                    &render_data,
                    &read_buffer,
                    wgpu::Extent3d {
                        width: TEXTURE_WIDTH,
                        height: TEXTURE_HEIGHT,
                        depth_or_array_layers: 1,
                    },
                    &frame_view_data,
                )
                .await
                .unwrap();
            // Uncomment to save reference image
            // image_buffer
            //     .save("tests/data/vertex_color_mesh_with_no_cull_and_global_clip_plane_actual.png")
            //     .unwrap();

            let ref_image_data = image::open(PathBuf::from(
                "tests/data/vertex_color_mesh_with_no_cull_and_global_clip_plane.png",
            ))
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
                panic!("Something is wrong with no culling and global clipping")
            }
        });
    }

    #[test]
    fn test_box_with_vertex_color_and_silhoutte() {
        pollster::block_on(async {
            let mut renderer = renderer::Renderer::from_new_device().await.unwrap();
            let mut frame_view_data =
                renderer.create_frame_view_data(TEXTURE_WIDTH, TEXTURE_HEIGHT);
            frame_view_data
                .camera
                .update(&get_camera_data(TEXTURE_WIDTH, TEXTURE_HEIGHT));
            renderer.write_frame_view_data_to_gpu(&frame_view_data);
            renderer.set_highlight_pixels(4);

            let read_buffer =
                renderer::create_read_buffer(&renderer.device, TEXTURE_WIDTH, TEXTURE_HEIGHT);

            let mut render_db = TestRenderDb::new(&renderer.device);
            let (_mesh_object_id, _wireframe_object_id, _silhoutte_mesh_object_id) =
                set_colored_mesh_object_with_silhoutte(&renderer.device, &mut render_db);

            let render_data = render_db.get_renderables().collect::<Vec<_>>();
            let image_buffer = renderer
                .render_and_return_as_image_buffer(
                    &render_data,
                    &read_buffer,
                    wgpu::Extent3d {
                        width: TEXTURE_WIDTH,
                        height: TEXTURE_HEIGHT,
                        depth_or_array_layers: 1,
                    },
                    &frame_view_data,
                )
                .await
                .unwrap();
            // image_buffer
            //     .save("tests/data/vertex_color_mesh_with_silhouette_actual.png")
            //     .unwrap();

            let ref_image_data = image::open(PathBuf::from(
                "tests/data/vertex_color_mesh_with_silhouette.png",
            ))
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
                panic!("Something is wrong with the Silhoutte")
            }
        });
    }

    #[test]
    fn test_box_in_screen_space_size_with_depth() {
        pollster::block_on(async {
            let renderer = renderer::Renderer::from_new_device().await.unwrap();
            let mut frame_view_data =
                renderer.create_frame_view_data(TEXTURE_WIDTH, TEXTURE_HEIGHT);
            frame_view_data
                .camera
                .update(&get_camera_data(TEXTURE_WIDTH, TEXTURE_HEIGHT));
            renderer.write_frame_view_data_to_gpu(&frame_view_data);

            let read_buffer =
                renderer::create_read_buffer(&renderer.device, TEXTURE_WIDTH, TEXTURE_HEIGHT);

            let mut render_db = TestRenderDb::new(&renderer.device);

            let (_mesh_object_id, _wireframe_object_id) =
                set_screen_space_mesh_and_wireframe(&renderer.device, &mut render_db, true);

            let render_data = render_db.get_renderables().collect::<Vec<_>>();
            let image_buffer = renderer
                .render_and_return_as_image_buffer(
                    &render_data,
                    &read_buffer,
                    wgpu::Extent3d {
                        width: TEXTURE_WIDTH,
                        height: TEXTURE_HEIGHT,
                        depth_or_array_layers: 1,
                    },
                    &frame_view_data,
                )
                .await
                .unwrap();
            // image_buffer
            //     .save("tests/data/screen_space_boxes_with_depth_actual.png")
            //     .unwrap();

            let ref_image_data = image::open(PathBuf::from(
                "tests/data/screen_space_boxes_with_depth.png",
            ))
            .unwrap()
            .into_rgba8();

            let ref_image =
                nv_flip::FlipImageRgb8::with_data(TEXTURE_WIDTH, TEXTURE_HEIGHT, &ref_image_data);
            let test_image =
                nv_flip::FlipImageRgb8::with_data(TEXTURE_WIDTH, TEXTURE_HEIGHT, &image_buffer);

            let error_map = nv_flip::flip(ref_image, test_image, DEFAULT_PIXELS_PER_DEGREE);
            let pool = nv_flip::FlipPool::from_image(&error_map);
            if let Some(Ordering::Greater) = pool.mean().partial_cmp(&0.0091) {
                println!("Mean error {}", pool.mean());
                panic!("Something is wrong with the Screen Space Mesh with Depth")
            }
        });
    }

    //ToDo:: Update this to test with other objects as well
    #[test]
    fn test_box_in_screen_space_size_without_depth() {
        pollster::block_on(async {
            let renderer = renderer::Renderer::from_new_device().await.unwrap();
            let mut frame_view_data =
                renderer.create_frame_view_data(TEXTURE_WIDTH, TEXTURE_HEIGHT);
            frame_view_data
                .camera
                .update(&get_camera_data(TEXTURE_WIDTH, TEXTURE_HEIGHT));
            renderer.write_frame_view_data_to_gpu(&frame_view_data);

            let read_buffer =
                renderer::create_read_buffer(&renderer.device, TEXTURE_WIDTH, TEXTURE_HEIGHT);

            let mut render_db = TestRenderDb::new(&renderer.device);
            let (_mesh_object_id, _wireframe_object_id) =
                set_screen_space_mesh_and_wireframe(&renderer.device, &mut render_db, false);

            let render_data = render_db.get_renderables().collect::<Vec<_>>();
            let image_buffer = renderer
                .render_and_return_as_image_buffer(
                    &render_data,
                    &read_buffer,
                    wgpu::Extent3d {
                        width: TEXTURE_WIDTH,
                        height: TEXTURE_HEIGHT,
                        depth_or_array_layers: 1,
                    },
                    &frame_view_data,
                )
                .await
                .unwrap();
            // image_buffer
            //     .save("tests/data/screen_space_boxes_without_depth.png")
            //     .unwrap();

            let ref_image_data = image::open(PathBuf::from(
                "tests/data/screen_space_boxes_without_depth.png",
            ))
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
                panic!("Something is wrong with the Screen Space Mesh without Depth")
            }
        });
    }

    #[test]
    fn test_box_with_single_texture_and_vertex_colors() {
        pollster::block_on(async {
            let renderer = renderer::Renderer::from_new_device().await.unwrap();
            let mut frame_view_data =
                renderer.create_frame_view_data(TEXTURE_WIDTH, TEXTURE_HEIGHT);
            frame_view_data
                .camera
                .update(&get_camera_data(TEXTURE_WIDTH, TEXTURE_HEIGHT));
            renderer.write_frame_view_data_to_gpu(&frame_view_data);

            let read_buffer =
                renderer::create_read_buffer(&renderer.device, TEXTURE_WIDTH, TEXTURE_HEIGHT);

            let mut render_db = TestRenderDb::new(&renderer.device);
            let (_mesh_object_id, _wireframe_object_id) = single_tex_mesh_object(
                &renderer.device,
                &renderer.queue,
                &renderer.texture_sampler,
                &mut render_db,
            );

            let render_data = render_db.get_renderables().collect::<Vec<_>>();
            let image_buffer = renderer
                .render_and_return_as_image_buffer(
                    &render_data,
                    &read_buffer,
                    wgpu::Extent3d {
                        width: TEXTURE_WIDTH,
                        height: TEXTURE_HEIGHT,
                        depth_or_array_layers: 1,
                    },
                    &frame_view_data,
                )
                .await
                .unwrap();
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
            let renderer = renderer::Renderer::from_new_device().await.unwrap();
            let mut frame_view_data =
                renderer.create_frame_view_data(TEXTURE_WIDTH, TEXTURE_HEIGHT);
            frame_view_data
                .camera
                .update(&get_camera_data(TEXTURE_WIDTH, TEXTURE_HEIGHT));
            renderer.write_frame_view_data_to_gpu(&frame_view_data);

            let read_buffer =
                renderer::create_read_buffer(&renderer.device, TEXTURE_WIDTH, TEXTURE_HEIGHT);

            let mut render_db = TestRenderDb::new(&renderer.device);
            let (_mesh_object_id, _wireframe_object_id) = set_multi_tex_mesh_object(
                &renderer.device,
                &renderer.queue,
                &renderer.texture_sampler,
                &mut render_db,
            );

            let render_data = render_db.get_renderables().collect::<Vec<_>>();
            let image_buffer = renderer
                .render_and_return_as_image_buffer(
                    &render_data,
                    &read_buffer,
                    wgpu::Extent3d {
                        width: TEXTURE_WIDTH,
                        height: TEXTURE_HEIGHT,
                        depth_or_array_layers: 1,
                    },
                    &frame_view_data,
                )
                .await
                .unwrap();
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

    #[test]
    fn test_transparent_colored_mesh() {
        pollster::block_on(async {
            let renderer = renderer::Renderer::from_new_device().await.unwrap();

            let mut frame_view_data =
                renderer.create_frame_view_data(TEXTURE_WIDTH, TEXTURE_HEIGHT);
            frame_view_data
                .camera
                .update(&get_camera_data(TEXTURE_WIDTH, TEXTURE_HEIGHT));
            renderer.write_frame_view_data_to_gpu(&frame_view_data);

            let read_buffer =
                renderer::create_read_buffer(&renderer.device, TEXTURE_WIDTH, TEXTURE_HEIGHT);

            let mut render_db = TestRenderDb::new(&renderer.device);

            // Create an opaque mesh in the background
            let _opaque_mesh_id = set_simple_mesh(&renderer.device, &mut render_db);

            // Create a transparent mesh in front with 50% opacity
            let _transparent_mesh_id =
                set_transparent_colored_mesh(&renderer.device, &mut render_db, 0.5);

            let render_data = render_db.get_renderables().collect::<Vec<_>>();

            let image_buffer = renderer
                .render_and_return_as_image_buffer(
                    &render_data,
                    &read_buffer,
                    wgpu::Extent3d {
                        width: TEXTURE_WIDTH,
                        height: TEXTURE_HEIGHT,
                        depth_or_array_layers: 1,
                    },
                    &frame_view_data,
                )
                .await
                .unwrap();
            // image_buffer
            //     .save("tests/data/transparent_colored_mesh_new.png")
            //     .unwrap();

            let ref_image_data =
                image::open(PathBuf::from("tests/data/transparent_colored_mesh.png"))
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
                panic!("Something is wrong with the Transparent Colored Mesh rendering")
            }
        });
    }

    #[test]
    fn test_transparent_mesh_z_order_sorting() {
        // Test that transparent meshes are correctly sorted by z_order
        pollster::block_on(async {
            let renderer = renderer::Renderer::from_new_device().await.unwrap();
            let mut render_db = TestRenderDb::new(&renderer.device);

            // Create two transparent meshes with different z_orders
            let _ = set_transparent_colored_mesh_with_z_order(
                &renderer.device,
                &mut render_db,
                0.5,
                200,                          // Higher z_order (should render last/on top)
                Material::new(1.0, 0.0, 0.0), // Red
            );

            let _ = set_transparent_colored_mesh_with_z_order(
                &renderer.device,
                &mut render_db,
                0.5,
                100,                          // Lower z_order (should render first/bottom)
                Material::new(0.0, 0.0, 1.0), // Blue
            );

            let render_data: Vec<_> = render_db.get_renderables().collect();

            // Verify both meshes are marked as transparent
            assert_eq!(
                render_data
                    .iter()
                    .filter(|d| d.local_resources.transparency.enabled)
                    .count(),
                2,
                "Should have 2 transparent renderables"
            );

            // Verify z_orders are set correctly
            let z_orders: Vec<u8> = render_data
                .iter()
                .filter(|d| d.local_resources.transparency.enabled)
                .map(|d| d.local_resources.transparency.z_order)
                .collect();

            assert!(z_orders.contains(&200), "Should have mesh with z_order 200");
            assert!(z_orders.contains(&100), "Should have mesh with z_order 100");

            // Verify the render pass would sort them correctly
            // (higher z_order should come after lower z_order in the sorted list)
            let mut sorted_data = render_data.clone();
            sorted_data.sort_by_key(|d| std::cmp::Reverse(d.local_resources.transparency.z_order));

            // After sorting by Reverse(z_order), the first should be z_order 200
            let first_transparent = sorted_data
                .iter()
                .find(|d| d.local_resources.transparency.enabled)
                .unwrap();
            assert_eq!(
                first_transparent.local_resources.transparency.z_order, 200,
                "Higher z_order should come first after Reverse sorting"
            );
        });
    }

    fn set_transparent_colored_mesh_with_z_order(
        device: &wgpu::Device,
        render_db: &mut TestRenderDb,
        opacity: f32,
        z_order: u8,
        material: Material,
    ) -> u32 {
        let colored_mesh = MeshBuilder::new()
            .add_vertex_stream(POSITIONS)
            .add_vertex_stream(COLORS)
            .add_mesh_index_stream(INDICES)
            .build(device);
        let colored_mesh_id = render_db.add_mesh(colored_mesh);

        let transformations =
            [Transformation(Mat4::from_translation((0.0, 0.0, 1.0).into())).to_data()];

        let material_colors = [material.to_data()];

        let colored_mesh_instance_buffer = InstanceDataBuilder::new()
            .add_instance_stream(&transformations)
            .add_instance_stream(&material_colors)
            .build(device);

        let transparency = Transparency::new(device, true, z_order, opacity);

        let colored_mesh_object = RenderObject {
            renderable: Renderable3d::ColoredMesh,
            gpu_mesh_id: colored_mesh_id,
            instance: colored_mesh_instance_buffer,
            mesh_local: RenderDataLocalResources::new_colored(
                device,
                ClipPlanes::new(device),
                transparency,
            ),
            cull_mode: None,
        };
        render_db.add_object(colored_mesh_object)
    }

    fn get_camera_data(width: u32, height: u32) -> OrthographicCameraData {
        let mut camera = OrthographicCameraData {
            aspect_ratio: width as f32 / height as f32,
            ..Default::default()
        };
        camera
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

        camera
    }

    fn create_texture_and_texture_bind_group(
        bytes: &[u8],
        path: &str,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        render_db: &mut TestRenderDb,
    ) -> u32 {
        let tex = texture::Texture::from_bytes(device, queue, bytes, path).unwrap();
        render_db.add_texture(tex)
    }

    fn single_tex_mesh_object(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        sampler: &wgpu::Sampler,
        render_db: &mut TestRenderDb,
    ) -> (u32, u32) {
        let happy_tree_id = create_texture_and_texture_bind_group(
            include_bytes!("resources/happy-tree.png"),
            "happy-tree",
            device,
            queue,
            render_db,
        );

        let single_tex_mesh = MeshBuilder::new()
            .add_vertex_stream(POSITIONS)
            .add_vertex_stream(COLORS)
            .add_vertex_stream(TEX_COORDS)
            .add_vertex_stream(USE_TEXTURE)
            .add_mesh_index_stream(INDICES)
            .add_wireframe_index_stream(TRI_EDGE_INDICES)
            .build(device);
        let single_tex_mesh_id = render_db.add_mesh(single_tex_mesh);

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
            .build(device);

        let single_tex_mesh_object = RenderObject {
            renderable: Renderable3d::TexturedMesh,
            gpu_mesh_id: single_tex_mesh_id,
            instance: single_tex_mesh_instance_buffer,
            mesh_local: RenderDataLocalResources::new_textured(
                device,
                ClipPlanes::new(device),
                render_db.get_texture_view(happy_tree_id),
                sampler,
                Transparency::opaque(device),
            ),
            cull_mode: None,
        };
        let _single_tex_mesh_object_id = render_db.add_object(single_tex_mesh_object);

        let single_tex_mesh_wireframe_object = RenderObject {
            renderable: Renderable3d::WireframeMesh,
            gpu_mesh_id: single_tex_mesh_id,
            instance: InstanceDataBuilder::new()
                .add_instance_stream(&transformations)
                .add_instance_stream(&material_colors)
                .build(device),
            mesh_local: RenderDataLocalResources::new_colored(
                device,
                ClipPlanes::new(device),
                Transparency::opaque(device),
            ),
            cull_mode: None,
        };
        let _single_tex_mesh_wireframe_object_id =
            render_db.add_object(single_tex_mesh_wireframe_object);

        (
            _single_tex_mesh_object_id,
            _single_tex_mesh_wireframe_object_id,
        )
    }

    fn set_multi_tex_mesh_object(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        sampler: &wgpu::Sampler,
        render_db: &mut TestRenderDb,
    ) -> (u32, u32) {
        let top_tex_id = create_texture_and_texture_bind_group(
            include_bytes!("resources/top-tex.png"),
            "top-tex.png",
            device,
            queue,
            render_db,
        );

        let right_tex_id = create_texture_and_texture_bind_group(
            include_bytes!("resources/right-tex.png"),
            "right-tex.png",
            device,
            queue,
            render_db,
        );

        let left_tex_id = create_texture_and_texture_bind_group(
            include_bytes!("resources/left-tex.png"),
            "left-tex.png",
            device,
            queue,
            render_db,
        );

        let bottom_tex_id = create_texture_and_texture_bind_group(
            include_bytes!("resources/bottom-tex.png"),
            "bottom-tex.png",
            device,
            queue,
            render_db,
        );

        let front_tex_id = create_texture_and_texture_bind_group(
            include_bytes!("resources/front-tex.png"),
            "front-tex.png",
            device,
            queue,
            render_db,
        );

        let back_tex_id = create_texture_and_texture_bind_group(
            include_bytes!("resources/back-tex.png"),
            "back-tex.png",
            device,
            queue,
            render_db,
        );

        let multi_tex_mesh = MeshBuilder::new()
            .add_vertex_stream(POSITIONS)
            .add_vertex_stream(COLORS)
            .add_vertex_stream(TEX_COORDS)
            .add_vertex_stream(&normalized_box::get_use_texture_vertices(
                vertex::UseTexture::from_texture_index(3),
                vertex::UseTexture::from_texture_index(0),
                vertex::UseTexture::from_texture_index(1),
                vertex::UseTexture::NO, //2
                vertex::UseTexture::from_texture_index(5),
                vertex::UseTexture::from_texture_index(4),
                // vertex::UseTexture::from_texture_index(left_tex_id),
            ))
            .add_mesh_index_stream(INDICES)
            .add_wireframe_index_stream(INDEXED_POSITIONS_BOX_EDGE_INDICES)
            .build(device);
        let multi_tex_mesh_id = render_db.add_mesh(multi_tex_mesh);

        let texture_views = [
            render_db.get_texture_view(front_tex_id),
            render_db.get_texture_view(bottom_tex_id),
            render_db.get_texture_view(top_tex_id),
            render_db.get_texture_view(back_tex_id),
            render_db.get_texture_view(left_tex_id),
            render_db.get_texture_view(right_tex_id),
        ];

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
            .build(device);

        let multi_tex_mesh_object = RenderObject {
            renderable: Renderable3d::ArrayTexturedMesh,
            gpu_mesh_id: multi_tex_mesh_id,
            instance: multi_tex_mesh_instance_buffer,
            mesh_local: RenderDataLocalResources::new_array_textured::<6>(
                device,
                ClipPlanes::new(device),
                &texture_views,
                sampler,
                Transparency::opaque(device),
            ),
            cull_mode: None,
        };
        let _multi_tex_mesh_object_id = render_db.add_object(multi_tex_mesh_object);

        let multi_tex_mesh_wireframe_object = RenderObject {
            renderable: Renderable3d::WireframeMesh,
            gpu_mesh_id: multi_tex_mesh_id,
            instance: InstanceDataBuilder::new()
                .add_instance_stream(&transformations)
                .add_instance_stream(&material_colors)
                .build(device),
            mesh_local: RenderDataLocalResources::new_colored(
                device,
                ClipPlanes::new(device),
                Transparency::opaque(device),
            ),
            cull_mode: None,
        };
        let _multi_tex_mesh_wireframe_object_id =
            render_db.add_object(multi_tex_mesh_wireframe_object);

        (
            _multi_tex_mesh_object_id,
            _multi_tex_mesh_wireframe_object_id,
        )
    }

    fn set_colored_mesh_object(
        device: &wgpu::Device,
        set_show_back_face_local: Option<bool>,
        render_db: &mut TestRenderDb,
    ) -> (u32, u32) {
        let colored_mesh = MeshBuilder::new()
            .add_vertex_stream(POSITIONS)
            .add_vertex_stream(COLORS)
            .add_mesh_index_stream(INDICES)
            .add_wireframe_index_stream(INDEXED_POSITIONS_BOX_EDGE_INDICES)
            .build(device);
        let colored_mesh_id = render_db.add_mesh(colored_mesh);

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
            .build(device);

        let colored_mesh_object = RenderObject {
            renderable: Renderable3d::ColoredMesh,
            gpu_mesh_id: colored_mesh_id,
            instance: colored_mesh_instance_buffer,
            mesh_local: RenderDataLocalResources::new_colored(
                device,
                ClipPlanes::new(device),
                Transparency::opaque(device),
            ),
            cull_mode: if let Some(show_back_face_local) = set_show_back_face_local {
                if show_back_face_local {
                    Some(TriangleFaceMode::FrontAndBack)
                } else {
                    None
                }
            } else {
                None
            },
        };
        let _colored_mesh_object_id = render_db.add_object(colored_mesh_object);

        let colored_mesh_wireframe_object = RenderObject {
            renderable: Renderable3d::WireframeMesh,
            gpu_mesh_id: colored_mesh_id,
            instance: InstanceDataBuilder::new()
                .add_instance_stream(&transformations)
                .add_instance_stream(&material_colors)
                .build(device),
            mesh_local: RenderDataLocalResources::new_colored(
                device,
                ClipPlanes::new(device),
                Transparency::opaque(device),
            ),
            cull_mode: None,
        };
        let _colored_mesh_wireframe_object_id = render_db.add_object(colored_mesh_wireframe_object);

        (_colored_mesh_object_id, _colored_mesh_wireframe_object_id)
    }

    fn set_transparent_colored_mesh(
        device: &wgpu::Device,
        render_db: &mut TestRenderDb,
        opacity: f32,
    ) -> u32 {
        let colored_mesh = MeshBuilder::new()
            .add_vertex_stream(POSITIONS)
            .add_vertex_stream(COLORS)
            .add_mesh_index_stream(INDICES)
            .build(device);
        let colored_mesh_id = render_db.add_mesh(colored_mesh);

        // Place transparent mesh in front (closer to camera, positive Z)
        let transformations =
            [Transformation(Mat4::from_translation((0.0, 0.0, 2.0).into())).to_data()];

        let material_colors = [Material::new(1.0, 0.0, 0.0).to_data()]; // Red color

        let colored_mesh_instance_buffer = InstanceDataBuilder::new()
            .add_instance_stream(&transformations)
            .add_instance_stream(&material_colors)
            .build(device);

        let transparency = Transparency::transparent(device, 128, opacity);

        let colored_mesh_object = RenderObject {
            renderable: Renderable3d::ColoredMesh,
            gpu_mesh_id: colored_mesh_id,
            instance: colored_mesh_instance_buffer,
            mesh_local: RenderDataLocalResources::new_colored(
                device,
                ClipPlanes::new(device),
                transparency,
            ),
            cull_mode: None,
        };
        render_db.add_object(colored_mesh_object)
    }

    fn set_two_colored_mesh_objects(
        device: &wgpu::Device,
        render_db: &mut TestRenderDb,
        local_clip_planes: Option<ClipPlanes<1>>,
    ) -> ((u32, u32), (u32, u32)) {
        let colored_mesh = MeshBuilder::new()
            .add_vertex_stream(POSITIONS)
            .add_vertex_stream(COLORS)
            .add_wireframe_index_stream(INDEXED_POSITIONS_BOX_EDGE_INDICES)
            .add_mesh_index_stream(INDICES)
            .build(device);
        let colored_mesh_id = render_db.add_mesh(colored_mesh);

        let material_color = [Material::new(0.0, 0.0, 1.0).to_data()];

        let instance_a_surface = InstanceDataBuilder::new()
            .add_instance_stream(&[Transformation(Mat4::IDENTITY).to_data()])
            .add_instance_stream(&material_color)
            .build(device);

        let instance_a_wireframe = InstanceDataBuilder::new()
            .add_instance_stream(&[Transformation(Mat4::IDENTITY).to_data()])
            .add_instance_stream(&material_color)
            .build(device);

        let instance_b_surface = InstanceDataBuilder::new()
            .add_instance_stream(&[
                Transformation(Mat4::from_translation((3.0, 3.0, 0.0).into())).to_data(),
            ])
            .add_instance_stream(&material_color)
            .build(device);

        let instance_b_wireframe = InstanceDataBuilder::new()
            .add_instance_stream(&[
                Transformation(Mat4::from_translation((3.0, 3.0, 0.0).into())).to_data(),
            ])
            .add_instance_stream(&material_color)
            .build(device);

        let clip_planes = local_clip_planes.unwrap_or(ClipPlanes::new(device));

        let mesh_a = RenderObject {
            renderable: Renderable3d::ColoredMesh,
            gpu_mesh_id: colored_mesh_id,
            instance: instance_a_surface,
            mesh_local: RenderDataLocalResources::new_colored(
                device,
                clip_planes.clone(),
                Transparency::opaque(device),
            ),
            cull_mode: None,
        };

        let wireframe_a = RenderObject {
            renderable: Renderable3d::WireframeMesh,
            gpu_mesh_id: colored_mesh_id,
            instance: instance_a_wireframe,
            mesh_local: RenderDataLocalResources::new_colored(
                device,
                clip_planes.clone(),
                Transparency::opaque(device),
            ),
            cull_mode: None,
        };

        let mesh_b = RenderObject {
            renderable: Renderable3d::ColoredMesh,
            gpu_mesh_id: colored_mesh_id,
            instance: instance_b_surface,
            mesh_local: RenderDataLocalResources::new_colored(
                device,
                clip_planes.clone(),
                Transparency::opaque(device),
            ),
            cull_mode: None,
        };

        let wireframe_b = RenderObject {
            renderable: Renderable3d::WireframeMesh,
            gpu_mesh_id: colored_mesh_id,
            instance: instance_b_wireframe,
            mesh_local: RenderDataLocalResources::new_colored(
                device,
                clip_planes.clone(),
                Transparency::opaque(device),
            ),
            cull_mode: None,
        };

        let mesh_a_id = render_db.add_object(mesh_a);
        let wireframe_a_id = render_db.add_object(wireframe_a);
        let mesh_b_id = render_db.add_object(mesh_b);
        let wireframe_b_id = render_db.add_object(wireframe_b);

        ((mesh_a_id, wireframe_a_id), (mesh_b_id, wireframe_b_id))
    }

    fn set_colored_mesh_object_with_silhoutte(
        device: &wgpu::Device,
        render_db: &mut TestRenderDb,
    ) -> (u32, u32, u32) {
        //(mesh id, wireframe id, silhoutte id
        let colored_mesh = MeshBuilder::new()
            .add_vertex_stream(POSITIONS)
            .add_vertex_stream(COLORS)
            .add_mesh_index_stream(INDICES)
            .add_wireframe_index_stream(INDEXED_POSITIONS_BOX_EDGE_INDICES)
            .build(device);
        let colored_mesh_id = render_db.add_mesh(colored_mesh);

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
            .build(device);

        let colored_mesh_object = RenderObject {
            renderable: Renderable3d::ColoredMesh,
            gpu_mesh_id: colored_mesh_id,
            instance: colored_mesh_instance_buffer,
            mesh_local: RenderDataLocalResources::new_colored(
                device,
                ClipPlanes::new(device),
                Transparency::opaque(device),
            ),
            cull_mode: None,
        };
        let _colored_mesh_object_id = render_db.add_object(colored_mesh_object);

        let colored_mesh_wireframe_object = RenderObject {
            renderable: Renderable3d::WireframeMesh,
            gpu_mesh_id: colored_mesh_id,
            instance: InstanceDataBuilder::new()
                .add_instance_stream(&transformations)
                .add_instance_stream(&material_colors)
                .build(device),
            mesh_local: RenderDataLocalResources::new_colored(
                device,
                ClipPlanes::new(device),
                Transparency::opaque(device),
            ),
            cull_mode: None,
        };
        let _colored_mesh_wireframe_object_id = render_db.add_object(colored_mesh_wireframe_object);

        let silhoutte_mesh_object = RenderObject {
            renderable: Renderable3d::SilhouetteMesh,
            gpu_mesh_id: colored_mesh_id,
            instance: InstanceDataBuilder::new()
                .add_instance_stream(&[Transformation(Mat4::from_translation(
                    (0.0, 5.0, 0.0).into(),
                ))
                .to_data()])
                .build(device),
            mesh_local: RenderDataLocalResources::new_colored(
                device,
                ClipPlanes::new(device),
                Transparency::opaque(device),
            ),
            cull_mode: None,
        };

        let _silhoutte_mesh_object_id = render_db.add_object(silhoutte_mesh_object);
        (
            _colored_mesh_object_id,
            _colored_mesh_wireframe_object_id,
            _silhoutte_mesh_object_id,
        )
    }

    fn set_screen_space_mesh_and_wireframe(
        device: &wgpu::Device,
        render_db: &mut TestRenderDb,
        with_depth_testing: bool,
    ) -> (u32, u32) {
        //(mesh id, wireframe id
        let colored_mesh = MeshBuilder::new()
            .add_vertex_stream(POSITIONS)
            .add_vertex_stream(COLORS)
            .add_mesh_index_stream(INDICES)
            .add_wireframe_index_stream(INDEXED_POSITIONS_BOX_EDGE_INDICES)
            .build(device);
        let colored_mesh_id = render_db.add_mesh(colored_mesh);

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

        let surface_material_colors = [
            Material::new(0.0, 0.0, 1.0).to_data(),
            Material::new(0.0, 0.0, 1.0).to_data(),
        ];

        let surface_pixel_sizes = [SizeInPixel(15.0), SizeInPixel(100.0)];

        let colored_mesh_instance_buffer = InstanceDataBuilder::new()
            .add_instance_stream(&transformations)
            .add_instance_stream(&surface_material_colors)
            .add_instance_stream(&[
                material::UseMaterialData::YES,
                material::UseMaterialData::NO,
            ])
            .add_instance_stream(&surface_pixel_sizes)
            .build(device);

        let colored_mesh_object = RenderObject {
            renderable: Renderable3d::ScreenSpaceColoredMesh {
                depth_testing: with_depth_testing,
                order: if with_depth_testing { 1 } else { 2 }, //The surface to go behind the wireframe
            },
            gpu_mesh_id: colored_mesh_id,
            instance: colored_mesh_instance_buffer,
            mesh_local: RenderDataLocalResources::new_colored(
                device,
                ClipPlanes::new(device),
                Transparency::opaque(device),
            ),
            cull_mode: None,
        };
        let _colored_mesh_object_id = render_db.add_object(colored_mesh_object);

        let wireframe_material_colors = [
            Material::new(1.0, 0.0, 0.0).to_data(),
            Material::new(1.0, 0.0, 0.0).to_data(),
        ];

        let wireframe_pixel_sizes = [SizeInPixel(25.0), SizeInPixel(105.0)];

        let colored_mesh_wireframe_object = RenderObject {
            renderable: Renderable3d::ScreenSpaceWireframeMesh {
                depth_testing: with_depth_testing,
                order: 1,
            },
            gpu_mesh_id: colored_mesh_id,
            instance: InstanceDataBuilder::new()
                .add_instance_stream(&transformations)
                .add_instance_stream(&wireframe_material_colors)
                .add_instance_stream(&wireframe_pixel_sizes)
                .add_instance_stream(&[
                    material::UseMaterialData::YES,
                    material::UseMaterialData::YES,
                ])
                .build(device),
            mesh_local: RenderDataLocalResources::new_colored(
                device,
                ClipPlanes::new(device),
                Transparency::opaque(device),
            ),
            cull_mode: None,
        };

        let _colored_mesh_wireframe_object_id = render_db.add_object(colored_mesh_wireframe_object);

        (_colored_mesh_object_id, _colored_mesh_wireframe_object_id)
    }

    fn set_solid_mesh(device: &wgpu::Device, render_db: &mut TestRenderDb) -> (u32, u32) {
        let simple_mesh = MeshBuilder::new()
            .add_vertex_stream(ORDERED_POSITIONS)
            .add_wireframe_index_stream(ORDERED_POSITIONS_BOX_EDGE_INDICES)
            .build(device);

        let simple_mesh_id = render_db.add_mesh(simple_mesh);

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
            .build(device);

        let simple_mesh_object = RenderObject {
            renderable: Renderable3d::Mesh,
            gpu_mesh_id: simple_mesh_id,
            instance: simple_mesh_instance_buffer,
            mesh_local: RenderDataLocalResources::new_colored(
                device,
                ClipPlanes::new(device),
                Transparency::opaque(device),
            ),
            cull_mode: None,
        };
        let _simple_mesh_object_id = render_db.add_object(simple_mesh_object);

        let simple_mesh_wireframe_object = RenderObject {
            renderable: Renderable3d::WireframeMesh,
            gpu_mesh_id: simple_mesh_id,
            instance: InstanceDataBuilder::new()
                .add_instance_stream(&transformations)
                .add_instance_stream(&[
                    Material::new(0.0, 0.0, 1.0).to_data(),
                    Material::new(0.0, 0.0, 1.0).to_data(),
                ])
                .build(device),
            mesh_local: RenderDataLocalResources::new_colored(
                device,
                ClipPlanes::new(device),
                Transparency::opaque(device),
            ),
            cull_mode: None,
        };

        let _simple_mesh_wireframe_object_id = render_db.add_object(simple_mesh_wireframe_object);

        (_simple_mesh_object_id, _simple_mesh_wireframe_object_id)
    }

    fn set_wireframe_mesh_object(device: &wgpu::Device, render_db: &mut TestRenderDb) -> u32 {
        let simple_mesh = MeshBuilder::new()
            .add_vertex_stream(ORDERED_POSITIONS)
            .add_wireframe_index_stream(ORDERED_POSITIONS_BOX_EDGE_INDICES)
            .build(device);

        let simple_mesh_id = render_db.add_mesh(simple_mesh);

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
            renderable: Renderable3d::WireframeMesh,
            gpu_mesh_id: simple_mesh_id,
            instance: InstanceDataBuilder::new()
                .add_instance_stream(&transformations)
                .add_instance_stream(&[
                    Material::new(0.0, 0.0, 1.0).to_data(),
                    Material::new(0.0, 0.0, 1.0).to_data(),
                ])
                .build(device),
            mesh_local: RenderDataLocalResources::new_colored(
                device,
                ClipPlanes::new(device),
                Transparency::opaque(device),
            ),
            cull_mode: None,
        };

        render_db.add_object(simple_mesh_wireframe_object)
    }

    fn set_wcs_gizmo(device: &wgpu::Device, render_db: &mut TestRenderDb) -> u32 {
        let (pos, color, indices) =
            normalized_axis_gizmo::generate_axis_gizmo_prism_mesh(1.0, 0.02);
        let simple_mesh = MeshBuilder::new()
            .add_vertex_stream(&pos)
            .add_vertex_stream(&color)
            .add_mesh_index_stream(&indices)
            .build(device);

        let simple_mesh_id = render_db.add_mesh(simple_mesh);

        let transformations = [Transformation(Mat4::IDENTITY).to_data()];

        let simple_mesh_instance_buffer = InstanceDataBuilder::new()
            .add_instance_stream(&transformations)
            .add_instance_stream(&[Material::new(0.75, 0.05, 0.5).to_data()])
            .add_instance_stream(&[SizeInPixel(100.0)])
            .add_instance_stream(&[material::UseMaterialData::NO])
            .build(device);

        let gizmo_object = RenderObject {
            renderable: Renderable3d::ScreenSpaceColoredMesh {
                depth_testing: false,
                order: 0,
            },
            gpu_mesh_id: simple_mesh_id,
            instance: simple_mesh_instance_buffer,
            mesh_local: RenderDataLocalResources::new_colored(
                device,
                ClipPlanes::new(device),
                Transparency::opaque(device),
            ),
            cull_mode: None,
        };

        render_db.add_object(gizmo_object)
    }

    fn set_simple_mesh(device: &wgpu::Device, render_db: &mut TestRenderDb) -> u32 {
        let simple_mesh = MeshBuilder::new()
            .add_vertex_stream(ORDERED_POSITIONS)
            .add_wireframe_index_stream(ORDERED_POSITIONS_BOX_EDGE_INDICES)
            .build(device);

        let simple_mesh_id = render_db.add_mesh(simple_mesh);
        let transformations = [Transformation(Mat4::IDENTITY).to_data()];

        let simple_mesh_instance_buffer = InstanceDataBuilder::new()
            .add_instance_stream(&transformations)
            .add_instance_stream(&[Material::new(1.0, 1.0, 0.0).to_data()])
            .build(device);

        let simple_mesh_object = RenderObject {
            renderable: Renderable3d::Mesh,
            gpu_mesh_id: simple_mesh_id,
            instance: simple_mesh_instance_buffer,
            mesh_local: RenderDataLocalResources::new_colored(
                device,
                ClipPlanes::new(device),
                Transparency::opaque(device),
            ),
            cull_mode: None,
        };

        render_db.add_object(simple_mesh_object)
    }

    pub struct RenderObject {
        pub renderable: Renderable3d,
        pub instance: instance::GpuInstance,
        pub gpu_mesh_id: u32,
        pub mesh_local: RenderDataLocalResources,
        pub cull_mode: Option<TriangleFaceMode>,
    }

    pub struct TestRenderDb {
        textures: Vec<texture::Texture>,
        meshes: Vec<GpuMesh>,
        objects: Vec<RenderObject>,
    }

    impl TestRenderDb {
        fn new(device: &wgpu::Device) -> Self {
            let mut _self = Self {
                textures: vec![],
                meshes: vec![],
                objects: vec![],
            };

            set_wcs_gizmo(device, &mut _self);

            _self
        }

        // returns the texture id
        fn add_texture(&mut self, texture: texture::Texture) -> u32 {
            self.textures.push(texture);
            (self.textures.len() - 1) as u32
        }

        fn get_texture_view(&self, id: u32) -> &wgpu::TextureView {
            &self.textures[id as usize].view
        }

        fn add_mesh(&mut self, mesh: GpuMesh) -> u32 {
            self.meshes.push(mesh);

            (self.meshes.len() - 1) as u32
        }

        fn add_object(&mut self, object: RenderObject) -> u32 {
            self.objects.push(object);

            (self.objects.len() - 1) as u32
        }

        fn get_renderables<'a>(&'a self) -> impl Iterator<Item = RenderData3d<'a>> {
            self.objects.iter().map(|r| {
                let gpu_mesh = self.meshes.get(r.gpu_mesh_id as usize).unwrap();

                RenderData3d {
                    renderable: r.renderable.clone(),
                    mesh: gpu_mesh,
                    instance: &r.instance,
                    local_resources: &r.mesh_local,
                    triangle_face_mode: r.cull_mode,
                }
            })
        }
    }
}
