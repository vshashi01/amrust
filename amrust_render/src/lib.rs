use thiserror::Error;

mod prelude;
pub use prelude::*;

use crate::{gpu_mesh::GpuMesh, instance::GpuInstance};

//export module
pub mod bounding_box;
pub mod camera;
pub mod gpu_mesh;
pub mod instance;
pub mod material;
pub mod normalized_axis_gizmo;
pub mod normalized_box;
pub mod renderer;
pub mod screen_space;
pub mod texture;
pub mod transformation;
pub mod vertex;

// internal module
mod composite;
mod constants;
mod pipeline;
mod render_pass;

#[derive(Debug, Error)]
pub enum WgpuError {
    #[error("Something went wrong with the Device request")]
    DeviceError(#[from] wgpu::RequestDeviceError),
    // #[cfg(feature = "wgpu")]
    // #[error("When something goes wrong with the adapter")]
    // AdapterError(#[from] wgpu::RequestAdapterError),
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

pub struct RenderData3d<'a> {
    pub renderable: Renderable3d,
    pub mesh: &'a GpuMesh,
    pub instance: &'a GpuInstance,
    pub local_bind_groups: Vec<(&'a wgpu::BindGroup, u32)>,
}

#[cfg(test)]
mod tests {
    use std::{cmp::Ordering, path::PathBuf};

    use glam::{Mat4, Vec3};
    use nv_flip::DEFAULT_PIXELS_PER_DEGREE;

    use super::*;
    use crate::{
        camera::{CameraData, OrthographicCameraData},
        gpu_mesh::{GpuMesh, MeshBuilder},
        instance::InstanceDataBuilder,
        material::Material,
        screen_space::SizeInPixel,
        transformation::Transformation,
    };
    use normalized_box::{
        COLORS, INDEXED_POSITIONS_BOX_EDGE_INDICES, INDICES, ORDERED_POSITIONS,
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
                set_colored_mesh_object(&renderer.device, &mut render_db);

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
                set_colored_mesh_object(&renderer.device, &mut main_render_db);
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
            camera_data.copy_rotation_component(&get_camera_data(400, 400));
            subviewport_frame_view_data.update_viewport_size(400.0, 400.0, &camera_data);
            renderer.write_frame_view_data_to_gpu(&subviewport_frame_view_data);

            let views = [
                renderer::RenderView {
                    rect: None,
                    render_data: &main_render_data,
                    frame_view_data: &main_frame_view_data,
                    clear_color: wgpu::Color {
                        r: 0.1,
                        g: 0.2,
                        b: 0.3,
                        a: 1.0,
                    },
                },
                renderer::RenderView {
                    rect: Some(renderer::ViewportRect {
                        x: 0,
                        y: 0,
                        width: 400,
                        height: 400,
                    }),
                    render_data: &secondary_render_data,
                    frame_view_data: &subviewport_frame_view_data,
                    clear_color: wgpu::Color::RED,
                },
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

            image_buffer
                .save("tests/data/vertex_color_mesh_with_subviewport_actual.png")
                .unwrap();

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
                &renderer.texture_bind_group_layout,
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
                &renderer.texture_array_bind_group_layout,
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
        sampler: &wgpu::Sampler,
        layout: &wgpu::BindGroupLayout,
    ) -> (u32, u32) {
        let tex = texture::Texture::from_bytes(device, queue, bytes, path).unwrap();
        render_db.add_texture(tex, device, sampler, layout)
    }

    fn set_multi_tex_mesh_object(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        sampler: &wgpu::Sampler,
        layout: &wgpu::BindGroupLayout,
        render_db: &mut TestRenderDb,
    ) -> (u32, u32) {
        let (top_tex_id, _) = create_texture_and_texture_bind_group(
            include_bytes!("resources/top-tex.png"),
            "top-tex.png",
            device,
            queue,
            render_db,
            sampler,
            layout,
        );

        let (right_tex_id, _) = create_texture_and_texture_bind_group(
            include_bytes!("resources/right-tex.png"),
            "right-tex.png",
            device,
            queue,
            render_db,
            sampler,
            layout,
        );

        let (left_tex_id, _) = create_texture_and_texture_bind_group(
            include_bytes!("resources/left-tex.png"),
            "left-tex.png",
            device,
            queue,
            render_db,
            sampler,
            layout,
        );

        let (bottom_tex_id, _) = create_texture_and_texture_bind_group(
            include_bytes!("resources/bottom-tex.png"),
            "bottom-tex.png",
            device,
            queue,
            render_db,
            sampler,
            layout,
        );

        let (front_tex_id, _) = create_texture_and_texture_bind_group(
            include_bytes!("resources/front-tex.png"),
            "front-tex.png",
            device,
            queue,
            render_db,
            sampler,
            layout,
        );

        let (back_tex_id, _) = create_texture_and_texture_bind_group(
            include_bytes!("resources/back-tex.png"),
            "back-tex.png",
            device,
            queue,
            render_db,
            sampler,
            layout,
        );

        let texture_array_bind_group = render_db.create_texture_array(
            &[
                front_tex_id,
                bottom_tex_id,
                top_tex_id,
                back_tex_id,
                left_tex_id,
                right_tex_id,
            ],
            device,
            sampler,
            layout,
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
            local_resources: vec![(texture_array_bind_group, 1)],
        };
        let _multi_tex_mesh_object_id = render_db.add_object(multi_tex_mesh_object);

        let multi_tex_mesh_wireframe_object = RenderObject {
            renderable: Renderable3d::WireframeMesh,
            gpu_mesh_id: multi_tex_mesh_id,
            instance: InstanceDataBuilder::new()
                .add_instance_stream(&transformations)
                .add_instance_stream(&material_colors)
                .build(device),
            local_resources: vec![],
        };
        let _multi_tex_mesh_wireframe_object_id =
            render_db.add_object(multi_tex_mesh_wireframe_object);

        (
            _multi_tex_mesh_object_id,
            _multi_tex_mesh_wireframe_object_id,
        )
    }

    fn single_tex_mesh_object(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        sampler: &wgpu::Sampler,
        layout: &wgpu::BindGroupLayout,
        render_db: &mut TestRenderDb,
    ) -> (u32, u32) {
        let (_, happy_tree_bind_group_id) = create_texture_and_texture_bind_group(
            include_bytes!("resources/happy-tree.png"),
            "happy-tree",
            device,
            queue,
            render_db,
            sampler,
            layout,
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
            local_resources: vec![(happy_tree_bind_group_id, 1)],
        };
        let _single_tex_mesh_object_id = render_db.add_object(single_tex_mesh_object);

        let single_tex_mesh_wireframe_object = RenderObject {
            renderable: Renderable3d::WireframeMesh,
            gpu_mesh_id: single_tex_mesh_id,
            instance: InstanceDataBuilder::new()
                .add_instance_stream(&transformations)
                .add_instance_stream(&material_colors)
                .build(device),
            local_resources: vec![],
        };
        let _single_tex_mesh_wireframe_object_id =
            render_db.add_object(single_tex_mesh_wireframe_object);

        (
            _single_tex_mesh_object_id,
            _single_tex_mesh_wireframe_object_id,
        )
    }

    fn set_colored_mesh_object(device: &wgpu::Device, render_db: &mut TestRenderDb) -> (u32, u32) {
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
            local_resources: vec![],
        };
        let _colored_mesh_object_id = render_db.add_object(colored_mesh_object);

        let colored_mesh_wireframe_object = RenderObject {
            renderable: Renderable3d::WireframeMesh,
            gpu_mesh_id: colored_mesh_id,
            instance: InstanceDataBuilder::new()
                .add_instance_stream(&transformations)
                .add_instance_stream(&material_colors)
                .build(device),
            local_resources: vec![],
        };
        let _colored_mesh_wireframe_object_id = render_db.add_object(colored_mesh_wireframe_object);

        (_colored_mesh_object_id, _colored_mesh_wireframe_object_id)
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
            local_resources: vec![],
        };
        let _colored_mesh_object_id = render_db.add_object(colored_mesh_object);

        let colored_mesh_wireframe_object = RenderObject {
            renderable: Renderable3d::WireframeMesh,
            gpu_mesh_id: colored_mesh_id,
            instance: InstanceDataBuilder::new()
                .add_instance_stream(&transformations)
                .add_instance_stream(&material_colors)
                .build(device),
            local_resources: vec![],
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
            local_resources: vec![],
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
            local_resources: vec![],
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
            local_resources: vec![],
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
            local_resources: vec![],
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
            local_resources: vec![],
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
            local_resources: vec![],
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
            local_resources: vec![],
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
            local_resources: vec![],
        };

        render_db.add_object(simple_mesh_object)
    }

    pub struct RenderObject {
        pub renderable: Renderable3d,
        pub instance: instance::GpuInstance,
        pub gpu_mesh_id: u32,
        pub local_resources: Vec<(u32, u32)>, // (index to local_bind_group, slot_index)
    }

    pub struct TestRenderDb {
        textures: Vec<texture::Texture>,
        meshes: Vec<GpuMesh>,
        objects: Vec<RenderObject>,
        // invisible_objects: HashSet<usize>,
        local_bind_groups: Vec<wgpu::BindGroup>,
    }

    impl TestRenderDb {
        fn new(device: &wgpu::Device) -> Self {
            let mut _self = Self {
                textures: vec![],
                meshes: vec![],
                objects: vec![],
                // invisible_objects: HashSet::new(),
                local_bind_groups: vec![],
            };

            set_wcs_gizmo(device, &mut _self);

            _self
        }

        // returns the texture id and the bind group id
        fn add_texture(
            &mut self,
            texture: texture::Texture,
            device: &wgpu::Device,
            sampler: &wgpu::Sampler,
            layout: &wgpu::BindGroupLayout,
        ) -> (u32, u32) {
            let bind_group = texture::generate_basic_texture_bind_group::<0, 1>(
                device, &texture, sampler, layout,
            );

            self.textures.push(texture);
            let bind_group_id = self.add_local_bind_group(bind_group);
            let texture_id = (self.textures.len() - 1) as u32;

            (texture_id, bind_group_id)
        }

        // returns the bind group id for the texture array
        fn create_texture_array(
            &mut self,
            texture_ids: &[u32],
            device: &wgpu::Device,
            sampler: &wgpu::Sampler,
            layout: &wgpu::BindGroupLayout,
        ) -> u32 {
            let mut texture_views = Vec::<&wgpu::TextureView>::new();

            for texture_id in texture_ids {
                let texture = &self.textures[*texture_id as usize];
                texture_views.push(&texture.view);
            }

            let bind_group = texture::generate_texture_array_bind_group::<0, 1>(
                device,
                "Array 1",
                &texture_views,
                sampler,
                layout,
            );

            self.add_local_bind_group(bind_group)
        }

        fn add_mesh(&mut self, mesh: GpuMesh) -> u32 {
            self.meshes.push(mesh);

            (self.meshes.len() - 1) as u32
        }

        fn add_object(&mut self, object: RenderObject) -> u32 {
            self.objects.push(object);

            (self.objects.len() - 1) as u32
        }

        // fn clear_all(&mut self) {
        //     self.objects.clear();
        //     self.meshes.clear();
        //     self.local_bind_groups.clear();
        // }

        // fn make_object_invisible(&mut self, object_id: usize) {
        //     self.invisible_objects.insert(object_id);
        // }

        // fn make_object_visible(&mut self, object_id: &usize) {
        //     self.invisible_objects.remove(object_id);
        // }

        fn add_local_bind_group(&mut self, bind_group: wgpu::BindGroup) -> u32 {
            self.local_bind_groups.push(bind_group);

            (self.local_bind_groups.len() - 1) as u32
        }

        fn get_renderables<'a>(&'a self) -> impl Iterator<Item = RenderData3d<'a>> {
            self.objects.iter().map(|r| {
                let gpu_mesh = self.meshes.get(r.gpu_mesh_id as usize).unwrap();
                let local_resources = r
                    .local_resources
                    .iter()
                    .map(|resource| {
                        let bind_group = self.local_bind_groups.get(resource.0 as usize).unwrap();
                        (bind_group, resource.1)
                    })
                    .collect::<Vec<_>>();

                RenderData3d {
                    renderable: r.renderable.clone(),
                    mesh: gpu_mesh,
                    instance: &r.instance,
                    local_bind_groups: local_resources,
                }
            })
        }
    }
}
