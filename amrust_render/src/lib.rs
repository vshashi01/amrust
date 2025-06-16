use glam::{Mat4, Vec3};
use image::{ImageBuffer, Rgba};
use thiserror::Error;
use wgpu::{DepthStencilState, Limits, RenderPassDepthStencilAttachment, wgt::DeviceDescriptor};

mod camera;
mod gpu_mesh;
mod instance;
mod material;
mod normalized_box;
mod object;
mod render_pass;
mod renderables;
mod texture;
mod transformation;
mod vertex;

use crate::{
    camera::{Camera, OrthographicCameraData},
    gpu_mesh::{GpuMesh, MeshBuilder},
    instance::{InstanceDataBuilder, InstanceFieldDescriptor},
    material::Material,
    normalized_box::{
        COLORS, INDEXED_POSITIONS_BOX_EDGE_INDICES, INDICES, ORDERED_POSITIONS,
        ORDERED_POSITIONS_BOX_EDGE_INDICES, POSITIONS, TEX_COORDS, TRI_EDGE_INDICES, USE_TEXTURE,
    },
    object::RenderObject,
    render_pass::{solid_render_pass, wireframe_render_pass},
    renderables::Renderable,
    texture::{
        MAX_BINDING_ARRAY_ELEMENTS_PER_SHADER_STAGE, MAX_BINDING_ARRAY_SAMPLERS_PER_SHADER_STAGE,
        MAX_TEXTURE_SIZE, Texture, generate_basic_texture_bind_group,
        generate_texture_array_bind_group, generate_texture_array_bind_group_layout,
    },
    transformation::Transformation,
    vertex::{UseTexture, VertexDescriptor},
};

use std::{
    collections::{HashMap, HashSet},
    num::NonZero,
};

#[derive(Debug, Error)]
pub enum WgpuError {
    #[error("Something went wrong with the Device request")]
    DeviceError(#[from] wgpu::RequestDeviceError),

    #[error("When something goes wrong with the adapter")]
    AdapterError(#[from] wgpu::RequestAdapterError),
}

pub struct Renderer {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,

    // properties related to the render surface
    output_buffer: wgpu::Buffer,
    texture_size: wgpu::Extent3d,
    texture_format: wgpu::TextureFormat, //stored for future dynamic render pipeline creation
    texture: wgpu::Texture,
    texture_view: wgpu::TextureView,
    depth_texture: texture::DepthTexture,

    // external rendering resources
    // consider splitting these to separate struct to be managed by the app
    textures: Vec<Texture>,
    meshes: Vec<GpuMesh>,
    objects: Vec<RenderObject>,
    invisible_objects: HashSet<usize>,
    local_bind_groups: Vec<wgpu::BindGroup>,

    // internal rendering resources
    global_bind_groups: Vec<wgpu::BindGroup>,
    texture_bind_group_layout: wgpu::BindGroupLayout,
    texture_sampler: wgpu::Sampler,
    texture_array_bind_group_layout: wgpu::BindGroupLayout,

    // render pipelines
    render_pipeline_cache: HashMap<String, wgpu::RenderPipeline>,
}

impl Renderer {
    pub async fn new_texture_based(width: u32, height: u32) -> Result<Self, WgpuError> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::VULKAN,
            ..Default::default()
        });
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::default(),
                compatible_surface: None,
                force_fallback_adapter: false,
            })
            .await
            .unwrap();

        println!("{:?}", adapter.get_info());

        adapter
            .features()
            .contains(wgpu::Features::TEXTURE_BINDING_ARRAY)
            .then(|| {
                println!("Adapter supports TEXTURE_BINDING_ARRAY feature");
            })
            .unwrap_or_else(|| {
                println!("Adapter does not support TEXTURE_BINDING_ARRAY feature");
            });

        let (device, queue) = adapter
            .request_device(&DeviceDescriptor {
                label: Some("Gpu Device"),
                required_features: wgpu::Features::TEXTURE_BINDING_ARRAY
                    | wgpu::Features::SAMPLED_TEXTURE_AND_STORAGE_BUFFER_ARRAY_NON_UNIFORM_INDEXING,
                required_limits: wgpu::Limits {
                    max_binding_array_elements_per_shader_stage:
                        MAX_BINDING_ARRAY_ELEMENTS_PER_SHADER_STAGE,
                    max_binding_array_sampler_elements_per_shader_stage:
                        MAX_BINDING_ARRAY_SAMPLERS_PER_SHADER_STAGE,
                    max_texture_dimension_2d: MAX_TEXTURE_SIZE,
                    ..Limits::downlevel_defaults()
                },
                memory_hints: wgpu::MemoryHints::Performance,
                trace: wgpu::Trace::Off,
            })
            .await
            .unwrap();

        let texture_size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };

        let texture_format = wgpu::TextureFormat::Rgba8UnormSrgb;

        let texture_desc = wgpu::TextureDescriptor {
            size: texture_size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: texture_format,
            usage: wgpu::TextureUsages::COPY_SRC | wgpu::TextureUsages::RENDER_ATTACHMENT,
            label: None,
            view_formats: &[],
        };
        let texture = device.create_texture(&texture_desc);
        let texture_view = texture.create_view(&Default::default());

        let u32_size = std::mem::size_of::<u32>() as u32;
        let output_buffer_size = (u32_size * width * height) as wgpu::BufferAddress;
        let output_buffer_desc = wgpu::BufferDescriptor {
            size: output_buffer_size,
            usage: wgpu::BufferUsages::COPY_DST
                // this tells wpgu that we want to read this buffer from the cpu
                | wgpu::BufferUsages::MAP_READ,
            label: None,
            mapped_at_creation: false,
        };
        let output_buffer = device.create_buffer(&output_buffer_desc);

        let camera_bind_group_layout = Camera::create_bind_group_layout(&device);
        let basic_texture_bind_group_layout = texture::generate_texture_bind_group_layout::<0, 1>(
            &device,
            "Basic Texture Bind Group Layout",
            true,
        );
        let texture_sampler = texture::generate_basic_texture_sampler(&device);
        let texture_array_bind_group_layout = generate_texture_array_bind_group_layout::<0, 1>(
            &device,
            "Texture Array Layout",
            true,
            NonZero::new(MAX_BINDING_ARRAY_ELEMENTS_PER_SHADER_STAGE).unwrap(),
        );

        let depth_texture = texture::DepthTexture::create_depth_texture(&device, texture_size);

        let source =
            wgpu::ShaderSource::Wgsl((include_str!("shaders/textured_vertex.wgsl")).into());
        let texture_surface_render_pipeline = create_render_pipeline(
            &device,
            "Textured Surface",
            source,
            texture_desc.format,
            &[
                vertex::Position::layout::<0>(),
                vertex::Color::layout::<1>(),
                vertex::TexCoords::layout::<2>(),
                vertex::UseTexture::layout::<3>(),
                transformation::TransformationData::layout::<5>(),
                material::RgbMaterialData::layout::<9>(),
            ],
            &[&camera_bind_group_layout, &basic_texture_bind_group_layout],
            wgpu::PrimitiveTopology::TriangleList,
        );

        let source =
            wgpu::ShaderSource::Wgsl((include_str!("shaders/array_texture_mesh.wgsl")).into());
        let texture_array_surface_render_pipeline = create_render_pipeline(
            &device,
            "Texture Array Surface",
            source,
            texture_desc.format,
            &[
                vertex::Position::layout::<0>(),
                vertex::Color::layout::<1>(),
                vertex::TexCoords::layout::<2>(),
                vertex::UseTexture::layout::<3>(),
                transformation::TransformationData::layout::<5>(),
                material::RgbMaterialData::layout::<9>(),
            ],
            &[&camera_bind_group_layout, &texture_array_bind_group_layout],
            wgpu::PrimitiveTopology::TriangleList,
        );

        let source = wgpu::ShaderSource::Wgsl((include_str!("shaders/colored_vertex.wgsl")).into());
        let colored_surface_render_pipeline = create_render_pipeline(
            &device,
            "Colored Surface",
            source,
            texture_desc.format,
            &[
                vertex::Position::layout::<0>(),
                vertex::Color::layout::<1>(),
                transformation::TransformationData::layout::<5>(),
                material::RgbMaterialData::layout::<9>(),
            ],
            &[&camera_bind_group_layout],
            wgpu::PrimitiveTopology::TriangleList,
        );

        let solid_source =
            wgpu::ShaderSource::Wgsl((include_str!("shaders/uniform_color_vertex.wgsl")).into());

        let solid_render_pipeline = create_render_pipeline(
            &device,
            "Solid uniform",
            solid_source,
            texture_desc.format,
            &[
                vertex::Position::layout::<0>(),
                transformation::TransformationData::layout::<5>(),
                material::RgbMaterialData::layout::<9>(),
            ],
            &[&camera_bind_group_layout],
            wgpu::PrimitiveTopology::TriangleList,
        );

        let wireframe_source =
            wgpu::ShaderSource::Wgsl((include_str!("shaders/uniform_color_vertex.wgsl")).into());

        let wireframe_render_pipeline = create_render_pipeline(
            &device,
            "Wireframe",
            wireframe_source,
            texture_desc.format,
            &[
                vertex::Position::layout::<0>(),
                transformation::TransformationData::layout::<5>(),
                material::RgbMaterialData::layout::<9>(),
            ],
            &[&camera_bind_group_layout],
            wgpu::PrimitiveTopology::LineList,
        );

        let mut render_pipeline_cache = HashMap::new();
        render_pipeline_cache.insert(
            "Textured Surface".to_string(),
            texture_surface_render_pipeline,
        );
        render_pipeline_cache.insert("Wireframe".to_string(), wireframe_render_pipeline);
        render_pipeline_cache.insert(
            "Colored Surface".to_string(),
            colored_surface_render_pipeline,
        );
        render_pipeline_cache.insert("Uniform Solid Surface".to_string(), solid_render_pipeline);
        render_pipeline_cache.insert(
            "Texture Array Surface".to_owned(),
            texture_array_surface_render_pipeline,
        );

        Ok(Renderer {
            device,
            queue,
            output_buffer,
            texture_size,
            texture_format,
            texture,
            texture_view,
            depth_texture,
            textures: Vec::new(),
            meshes: Vec::new(),
            objects: Vec::new(),
            invisible_objects: HashSet::new(),
            local_bind_groups: Vec::new(),
            global_bind_groups: Vec::new(),
            texture_bind_group_layout: basic_texture_bind_group_layout,
            texture_array_bind_group_layout,
            texture_sampler,
            render_pipeline_cache,
        })
    }

    // returns the texture id and the bind group id
    pub fn add_texture(&mut self, texture: Texture) -> (u32, u32) {
        let bind_group = generate_basic_texture_bind_group::<0, 1>(
            &self.device,
            &texture,
            &self.texture_sampler,
            &self.texture_bind_group_layout,
        );

        self.textures.push(texture);
        let bind_group_id = self.add_local_bind_group(bind_group);
        let texture_id = (self.textures.len() - 1) as u32;

        (texture_id, bind_group_id)
    }

    // returns the bind group id for the texture array
    pub fn create_texture_array(&mut self, texture_ids: &[u32]) -> u32 {
        let mut texture_views = Vec::<&wgpu::TextureView>::new();

        for texture_id in texture_ids {
            let texture = &self.textures[*texture_id as usize];
            texture_views.push(&texture.view);
        }

        let bind_group = generate_texture_array_bind_group::<0, 1>(
            &self.device,
            "Array 1",
            &texture_views,
            &self.texture_sampler,
            &self.texture_array_bind_group_layout,
        );

        self.add_local_bind_group(bind_group)
    }

    pub fn add_mesh(&mut self, mesh: GpuMesh) -> u32 {
        self.meshes.push(mesh);

        (self.meshes.len() - 1) as u32
    }

    pub fn add_object(&mut self, object: RenderObject) -> u32 {
        self.objects.push(object);

        (self.objects.len() - 1) as u32
    }

    pub fn make_object_invisible(&mut self, object_id: usize) {
        self.invisible_objects.insert(object_id);
    }

    pub fn make_object_visible(&mut self, object_id: &usize) {
        self.invisible_objects.remove(object_id);
    }

    pub fn add_global_bind_group(&mut self, bind_group: wgpu::BindGroup) -> u32 {
        self.global_bind_groups.push(bind_group);

        (self.global_bind_groups.len() - 1) as u32
    }

    pub fn add_local_bind_group(&mut self, bind_group: wgpu::BindGroup) -> u32 {
        self.local_bind_groups.push(bind_group);

        (self.local_bind_groups.len() - 1) as u32
    }

    pub async fn render(
        &self,
        primary_camera_bind_group: &wgpu::BindGroup,
    ) -> ImageBuffer<Rgba<u8>, Vec<u8>> {
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

        {
            let render_pass_desc = wgpu::RenderPassDescriptor {
                label: Some("Surface Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.texture_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.1,
                            g: 0.2,
                            b: 0.3,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(RenderPassDepthStencilAttachment {
                    view: &self.depth_texture.view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                occlusion_query_set: None,
                timestamp_writes: None,
            };
            let mut render_pass = encoder.begin_render_pass(&render_pass_desc);

            render_pass.set_bind_group(0, primary_camera_bind_group, &[]);
            // set up global bind groups
            for (i, bind_group) in self.global_bind_groups.iter().enumerate() {
                render_pass.set_bind_group((i + 1) as u32, bind_group, &[]);
            }

            let visible_objects = self
                .objects
                .iter()
                .enumerate()
                .filter(|(id, _)| !self.invisible_objects.contains(id))
                .map(|(_, object)| object)
                .collect::<Vec<_>>();

            solid_render_pass(
                &visible_objects,
                &self.meshes,
                &self.local_bind_groups,
                &self.render_pipeline_cache,
                &mut render_pass,
            );

            wireframe_render_pass(
                &visible_objects,
                &self.meshes,
                &self.local_bind_groups,
                &self.render_pipeline_cache,
                &mut render_pass,
            );
        }

        let u32_size = std::mem::size_of::<u32>() as u32;
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                aspect: wgpu::TextureAspect::All,
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &self.output_buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(u32_size * self.texture_size.width),
                    rows_per_image: Some(self.texture_size.height),
                },
            },
            self.texture_size,
        );

        self.queue.submit(Some(encoder.finish()));

        // We need to scope the mapping variables so that we can
        // unmap the buffer
        let image_buffer = {
            let buffer_slice = self.output_buffer.slice(..);

            // NOTE: We have to create the mapping THEN device.poll() before await
            // the future. Otherwise the application will freeze.
            let (tx, rx) = futures_intrusive::channel::shared::oneshot_channel();
            buffer_slice.map_async(wgpu::MapMode::Read, move |result| {
                tx.send(result).unwrap();
            });
            self.device.poll(wgpu::PollType::Wait).unwrap();
            rx.receive().await.unwrap().unwrap();

            let data = buffer_slice.get_mapped_range();

            use image::{ImageBuffer, Rgba};

            ImageBuffer::<Rgba<u8>, _>::from_raw(
                self.texture_size.width,
                self.texture_size.height,
                data.to_vec(),
            )
            .unwrap()
        };
        self.output_buffer.unmap();

        image_buffer
    }
}

pub async fn run() {
    let texture_size = 512u32;
    let mut renderer = Renderer::new_texture_based(texture_size, texture_size)
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
            UseTexture::from_texture_index(back_tex_id),
            UseTexture::from_texture_index(front_tex_id),
            UseTexture::from_texture_index(bottom_tex_id),
            UseTexture::from_texture_index(top_tex_id),
            UseTexture::from_texture_index(right_tex_id),
            UseTexture::from_texture_index(left_tex_id),
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
    let _multi_tex_mesh_wireframe_object_id = renderer.add_object(multi_tex_mesh_wireframe_object);

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
            .add_instance_stream(&[
                Transformation(Mat4::from_translation((-5.0, 0.0, 0.0).into())).to_data(),
            ])
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
            .add_instance_stream(&[
                Transformation(Mat4::from_translation((0.0, 5.0, 0.0).into())).to_data(),
            ])
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
    renderer: &mut Renderer,
) -> (u32, u32) {
    let tex = Texture::from_bytes(&renderer.device, &renderer.queue, bytes, path).unwrap();
    renderer.add_texture(tex)
}

fn create_render_pipeline(
    device: &wgpu::Device,
    shader_name: &str,
    source: wgpu::ShaderSource,
    texture_format: wgpu::TextureFormat,
    buffers: &[wgpu::VertexBufferLayout<'static>],
    bind_group_layouts: &[&wgpu::BindGroupLayout],
    topology: wgpu::PrimitiveTopology,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(shader_name),
        source,
    });

    let pipeline_layout_name = format!("Pipeline Layout {shader_name}");
    let render_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some(&pipeline_layout_name),
        bind_group_layouts,
        push_constant_ranges: &[],
    });

    let render_pipeline_name = format!("Render Pipeline: {shader_name}");

    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(&render_pipeline_name),
        layout: Some(&render_pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            buffers,
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            targets: &[Some(wgpu::ColorTargetState {
                format: texture_format,
                blend: Some(wgpu::BlendState {
                    alpha: wgpu::BlendComponent::REPLACE,
                    color: wgpu::BlendComponent::REPLACE,
                }),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: Default::default(),
        }),
        primitive: wgpu::PrimitiveState {
            topology,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: Some(wgpu::Face::Back),
            // Setting this to anything other than Fill requires Features::NON_FILL_POLYGON_MODE
            polygon_mode: wgpu::PolygonMode::Fill,
            // Requires Features::DEPTH_CLIP_CONTROL
            unclipped_depth: false,
            // Requires Features::CONSERVATIVE_RASTERIZATION
            conservative: false,
        },
        depth_stencil: Some(DepthStencilState {
            format: texture::DepthTexture::DEPTH_FORMAT,
            depth_write_enabled: true,
            depth_compare: wgpu::CompareFunction::Less,
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState {
                constant: 0,
                slope_scale: 0.0,
                clamp: 0.0,
            },
        }),
        multisample: wgpu::MultisampleState {
            count: 1,
            mask: !0,
            alpha_to_coverage_enabled: false,
        },
        // If the pipeline will be used with a multiview render pass, this
        // indicates how many array layers the attachments will have.
        multiview: None,
        cache: None,
    })
}

#[cfg(test)]
pub mod tests {
    use super::*;

    #[test]
    fn test_run() {
        env_logger::init();
        pollster::block_on(run());
    }
}
