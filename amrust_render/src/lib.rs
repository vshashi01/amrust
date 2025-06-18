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
    use glam::{Mat4, Vec3};

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

    pub async fn run() {
        let texture_size = 512u32;
        let mut renderer = renderer::Renderer::new_texture_based(texture_size, texture_size)
            .await
            .unwrap();

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
        let camera_bind_group = camera.create_bind_group(&renderer.device);

        let (_, happy_tree_bind_group_id) = create_texture_and_texture_bind_group(
            include_bytes!("resources/happy-tree.png"),
            "happy-tree",
            &mut renderer,
        );

        let (top_tex_id, _) = create_texture_and_texture_bind_group(
            include_bytes!("resources/top-tex.png"),
            "top-tex.png",
            &mut renderer,
        );

        let (right_tex_id, _) = create_texture_and_texture_bind_group(
            include_bytes!("resources/right-tex.png"),
            "right-tex.png",
            &mut renderer,
        );

        let (left_tex_id, _) = create_texture_and_texture_bind_group(
            include_bytes!("resources/left-tex.png"),
            "left-tex.png",
            &mut renderer,
        );

        let (bottom_tex_id, _) = create_texture_and_texture_bind_group(
            include_bytes!("resources/bottom-tex.png"),
            "bottom-tex.png",
            &mut renderer,
        );

        let (front_tex_id, _) = create_texture_and_texture_bind_group(
            include_bytes!("resources/front-tex.png"),
            "front-tex.png",
            &mut renderer,
        );

        let (back_tex_id, _) = create_texture_and_texture_bind_group(
            include_bytes!("resources/back-tex.png"),
            "back-tex.png",
            &mut renderer,
        );

        let texture_array_bind_group = renderer.create_texture_array(&[
            front_tex_id,
            bottom_tex_id,
            front_tex_id,
            back_tex_id,
            left_tex_id,
            right_tex_id,
        ]);

        let single_tex_mesh = MeshBuilder::new()
            .add_vertex_stream(POSITIONS)
            .add_vertex_stream(COLORS)
            .add_vertex_stream(TEX_COORDS)
            .add_vertex_stream(USE_TEXTURE)
            .add_mesh_index_stream(INDICES)
            .add_wireframe_index_stream(TRI_EDGE_INDICES)
            .build(&renderer.device);
        let single_tex_mesh_id = renderer.add_mesh(single_tex_mesh);

        let single_tex_mesh_instance_buffer = InstanceDataBuilder::new()
            .add_instance_stream(&[
                Transformation(Mat4::IDENTITY).to_data(),
                //Transformation(Mat4::from_translation((5.0, 0.0, 0.0).into())).to_data(),
            ])
            .add_instance_stream(&[
                Material::new(1.0, 0.0, 0.0).to_data(),
                //Material::new(1.0, 0.0, 0.0).to_data(),
            ])
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
                .add_instance_stream(&[
                    Transformation(Mat4::IDENTITY).to_data(),
                    //Transformation(Mat4::from_translation((5.0, 0.0, 0.0).into())).to_data(),
                ])
                .add_instance_stream(&[
                    Material::new(0.0, 0.0, 1.0).to_data(),
                    // Material::new(0.0, 0.0, 1.0).to_data(),
                ])
                .build(&renderer.device),
        };
        let _single_tex_mesh_wireframe_object_id =
            renderer.add_object(single_tex_mesh_wireframe_object);

        let multi_tex_mesh = MeshBuilder::new()
            .add_vertex_stream(POSITIONS)
            .add_vertex_stream(COLORS)
            .add_vertex_stream(TEX_COORDS)
            .add_vertex_stream(&normalized_box::get_use_texture_vertices(
                vertex::UseTexture::from_texture_index(back_tex_id),
                vertex::UseTexture::from_texture_index(front_tex_id),
                vertex::UseTexture::from_texture_index(bottom_tex_id),
                vertex::UseTexture::from_texture_index(top_tex_id),
                vertex::UseTexture::from_texture_index(right_tex_id),
                vertex::UseTexture::from_texture_index(left_tex_id),
            ))
            .add_mesh_index_stream(INDICES)
            .add_wireframe_index_stream(INDEXED_POSITIONS_BOX_EDGE_INDICES)
            .build(&renderer.device);
        let multi_tex_mesh_id = renderer.add_mesh(multi_tex_mesh);

        let multi_tex_mesh_instance_buffer = InstanceDataBuilder::new()
            .add_instance_stream(&[
                //Transformation(Mat4::IDENTITY).to_data(),
                Transformation(Mat4::from_translation((5.0, 0.0, 0.0).into())).to_data(),
            ])
            .add_instance_stream(&[
                // Material::new(1.0, 0.0, 0.0).to_data(),
                Material::new(1.0, 0.0, 0.0).to_data(),
            ])
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
                .add_instance_stream(&[
                    //Transformation(Mat4::IDENTITY).to_data(),
                    Transformation(Mat4::from_translation((5.0, 0.0, 0.0).into())).to_data(),
                ])
                .add_instance_stream(&[
                    // Material::new(0.0, 0.0, 1.0).to_data(),
                    Material::new(0.0, 0.0, 1.0).to_data(),
                ])
                .build(&renderer.device),
        };
        let _multi_tex_mesh_wireframe_object_id =
            renderer.add_object(multi_tex_mesh_wireframe_object);

        let colored_mesh = MeshBuilder::new()
            .add_vertex_stream(POSITIONS)
            .add_vertex_stream(COLORS)
            .add_mesh_index_stream(INDICES)
            .add_wireframe_index_stream(INDEXED_POSITIONS_BOX_EDGE_INDICES)
            .build(&renderer.device);
        let colored_mesh_id = renderer.add_mesh(colored_mesh);

        let colored_mesh_instance_buffer = InstanceDataBuilder::new()
            .add_instance_stream(&[
                Transformation(Mat4::from_translation((-5.0, 0.0, 0.0).into())).to_data(),
            ])
            .add_instance_stream(&[Material::new(1.0, 0.0, 0.0).to_data()])
            .build(&renderer.device);

        let colored_mesh_object = RenderObject {
            renderable: Renderable::ColoredMesh(colored_mesh_id),
            instance: colored_mesh_instance_buffer,
        };
        let _colored_mesh_object_id = renderer.add_object(colored_mesh_object);

        let colored_mesh_wireframe_object = RenderObject {
            renderable: Renderable::WireframeMesh(colored_mesh_id),
            instance: InstanceDataBuilder::new()
                .add_instance_stream(&[Transformation(Mat4::from_translation(
                    (-5.0, 0.0, 0.0).into(),
                ))
                .to_data()])
                .add_instance_stream(&[Material::new(0.0, 0.0, 1.0).to_data()])
                .build(&renderer.device),
        };
        let _colored_mesh_wireframe_object_id = renderer.add_object(colored_mesh_wireframe_object);

        let simple_mesh = MeshBuilder::new()
            .add_vertex_stream(ORDERED_POSITIONS)
            .add_wireframe_index_stream(ORDERED_POSITIONS_BOX_EDGE_INDICES)
            .build(&renderer.device);

        let simple_mesh_id = renderer.add_mesh(simple_mesh);

        let simple_mesh_instance_buffer = InstanceDataBuilder::new()
            .add_instance_stream(&[
                Transformation(Mat4::from_translation((0.0, 5.0, 0.0).into())).to_data(),
            ])
            .add_instance_stream(&[Material::new(1.0, 0.0, 0.0).to_data()])
            .build(&renderer.device);

        let simple_mesh_object = RenderObject {
            renderable: Renderable::Mesh(simple_mesh_id),
            instance: simple_mesh_instance_buffer,
        };
        let _simple_mesh_object_id = renderer.add_object(simple_mesh_object);

        let simple_mesh_wireframe_object = RenderObject {
            renderable: Renderable::WireframeMesh(simple_mesh_id),
            instance: InstanceDataBuilder::new()
                .add_instance_stream(&[Transformation(Mat4::from_translation(
                    (0.0, 5.0, 0.0).into(),
                ))
                .to_data()])
                .add_instance_stream(&[Material::new(0.0, 0.0, 1.0).to_data()])
                .build(&renderer.device),
        };
        let _simple_mesh_wireframe_object_id = renderer.add_object(simple_mesh_wireframe_object);

        // renderer.make_object_invisible(_single_tex_mesh_object_id as usize);
        // renderer.make_object_invisible(_multi_tex_mesh_object_id as usize);
        // renderer.make_object_invisible(_colored_mesh_object_id as usize);
        // renderer.make_object_invisible(_simple_mesh_object_id as usize);

        let image_buffer = renderer.render(&camera_bind_group).await;
        image_buffer.save("image.png").unwrap();
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

    #[test]
    fn test_run() {
        env_logger::init();
        pollster::block_on(run());
    }

    fn test_box_with_front_texture() {}

    fn test_box_with_back_texture() {}

    fn test_box_wireframe_only() {}

    fn test_box_solid_color_only() {}

    fn test_box_with_vertex_color_only() {}

    fn test_box_with_vertex_color_and_texture() {}
}
