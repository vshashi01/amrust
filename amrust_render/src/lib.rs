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
use std::collections::HashMap;

use camera::{Camera, OrthographicCameraData};
use glam::{Mat4, Vec3};
use image::{ImageBuffer, Rgba};
use material::Material;
use normalized_box::INDICES;
use texture::Texture;
use thiserror::Error;
use vertex::VertexDescriptor;
use wgpu::{DepthStencilState, RenderPassDepthStencilAttachment};

use crate::{
    gpu_mesh::{GpuMesh, MeshBuilder},
    instance::{InstanceDataBuilder, InstanceFieldDescriptor},
    normalized_box::{COLORS, ORDERED_POSITIONS, POSITIONS, TEX_COORDS, USE_TEXTURE},
    object::RenderObject,
    renderables::Renderable,
    transformation::Transformation,
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
    meshes: Vec<GpuMesh>,
    objects: Vec<RenderObject>,
    local_bind_groups: Vec<wgpu::BindGroup>,

    // internal rendering resources
    global_bind_groups: Vec<wgpu::BindGroup>,

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
        let (device, queue) = adapter.request_device(&Default::default()).await.unwrap();

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
        let basic_texture_bind_group_layout =
            texture::Texture::create_bind_group_layout(&device, "Basic Texture Bind Group Layout");

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

        Ok(Renderer {
            device,
            queue,
            output_buffer,
            texture_size,
            texture_format,
            texture,
            texture_view,
            depth_texture,
            meshes: Vec::new(),
            objects: Vec::new(),
            local_bind_groups: Vec::new(),
            global_bind_groups: Vec::new(),
            render_pipeline_cache,
        })
    }

    pub fn add_mesh(&mut self, mesh: GpuMesh) -> u32 {
        self.meshes.push(mesh);

        (self.meshes.len() - 1) as u32
    }

    pub fn add_object(&mut self, object: RenderObject) -> u32 {
        self.objects.push(object);

        (self.objects.len() - 1) as u32
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

            let textured_objects = self.objects.iter().filter_map(|o| {
                if let Renderable::TexturedMesh(mesh_id, local_bind_groups_list) = &o.renderable {
                    Some((mesh_id, &o.instance, local_bind_groups_list))
                } else {
                    None
                }
            });

            for (mesh_id, instance, bind_groups_list) in textured_objects {
                let mesh = self.meshes.get(*mesh_id as usize).unwrap();
                render_pass
                    .set_pipeline(self.render_pipeline_cache.get("Textured Surface").unwrap());
                let position_buffer = mesh.vertex_slice::<vertex::Position>();
                let color_buffer = mesh.vertex_slice::<vertex::Color>();
                let tex_coord_buffer = mesh.vertex_slice::<vertex::TexCoords>();
                let use_texture_buffer = mesh.vertex_slice::<vertex::UseTexture>();

                let transformation_buffer =
                    instance.vertex_slice::<transformation::TransformationData>();
                let material_buffer = instance.vertex_slice::<material::RgbMaterialData>();

                render_pass.set_vertex_buffer(0, position_buffer);
                render_pass.set_vertex_buffer(1, color_buffer);
                render_pass.set_vertex_buffer(2, tex_coord_buffer);
                render_pass.set_vertex_buffer(3, use_texture_buffer);
                render_pass.set_vertex_buffer(4, transformation_buffer);
                render_pass.set_vertex_buffer(5, material_buffer);

                for pair in bind_groups_list.iter() {
                    let local_bind_group = self.local_bind_groups.get(pair.0 as usize).unwrap();
                    render_pass.set_bind_group(pair.1, local_bind_group, &[]);
                }

                if let Some(index_stream) = &mesh.index_stream {
                    let index_buffer = mesh.buffer.slice(index_stream.offset..index_stream.end);
                    render_pass.set_index_buffer(index_buffer, index_stream.format);

                    render_pass.draw_indexed(
                        0..index_stream.index_count,
                        0,
                        0..instance.instance_count,
                    );
                } else {
                    panic!("Mesh does not have an index buffer");
                }
            }

            let colored_objects = self.objects.iter().filter_map(|o| {
                if let Renderable::ColoredMesh(mesh_id) = &o.renderable {
                    Some((mesh_id, &o.instance))
                } else {
                    None
                }
            });

            for (mesh_id, instance) in colored_objects {
                let mesh = self.meshes.get(*mesh_id as usize).unwrap();
                render_pass
                    .set_pipeline(self.render_pipeline_cache.get("Colored Surface").unwrap());
                let position_buffer = mesh.vertex_slice::<vertex::Position>();
                let color_buffer = mesh.vertex_slice::<vertex::Color>();

                let transformation_buffer =
                    instance.vertex_slice::<transformation::TransformationData>();
                let material_buffer = instance.vertex_slice::<material::RgbMaterialData>();

                render_pass.set_vertex_buffer(0, position_buffer);
                render_pass.set_vertex_buffer(1, color_buffer);
                render_pass.set_vertex_buffer(2, transformation_buffer);
                render_pass.set_vertex_buffer(3, material_buffer);

                if let Some(index_stream) = &mesh.index_stream {
                    let index_buffer = mesh.buffer.slice(index_stream.offset..index_stream.end);
                    render_pass.set_index_buffer(index_buffer, index_stream.format);

                    render_pass.draw_indexed(
                        0..index_stream.index_count,
                        0,
                        0..instance.instance_count,
                    );
                } else {
                    panic!("Mesh does not have an index buffer");
                }
            }

            let simple_objects = self.objects.iter().filter_map(|o| {
                if let Renderable::Mesh(mesh_id) = &o.renderable {
                    Some((mesh_id, &o.instance))
                } else {
                    None
                }
            });

            for (meshid, instance) in simple_objects {
                let mesh = self.meshes.get(*meshid as usize).unwrap();
                render_pass.set_pipeline(
                    self.render_pipeline_cache
                        .get("Uniform Solid Surface")
                        .unwrap(),
                );
                let position_buffer = mesh.vertex_slice::<vertex::Position>();
                let transformation_buffer =
                    instance.vertex_slice::<transformation::TransformationData>();
                let material_buffer = instance.vertex_slice::<material::RgbMaterialData>();

                render_pass.set_vertex_buffer(0, position_buffer);
                render_pass.set_vertex_buffer(1, transformation_buffer);
                render_pass.set_vertex_buffer(2, material_buffer);

                render_pass.draw(0..mesh.vertex_count, 0..instance.instance_count);
            }

            let wireframe_objects = self.objects.iter().filter_map(|o| {
                if let Renderable::WireframeMesh(mesh_id) = &o.renderable {
                    Some((mesh_id, &o.instance))
                } else {
                    None
                }
            });

            for (mesh_id, instance) in wireframe_objects {
                let mesh = self.meshes.get(*mesh_id as usize).unwrap();
                render_pass.set_pipeline(self.render_pipeline_cache.get("Wireframe").unwrap());

                let position_buffer = mesh.vertex_slice::<vertex::Position>();
                let transformation_buffer =
                    instance.vertex_slice::<transformation::TransformationData>();
                let material_buffer = instance.vertex_slice::<material::RgbMaterialData>();

                render_pass.set_vertex_buffer(0, position_buffer);
                render_pass.set_vertex_buffer(1, transformation_buffer);
                render_pass.set_vertex_buffer(2, material_buffer);

                if let Some(index_stream) = &mesh.index_stream {
                    let index_buffer = mesh.buffer.slice(index_stream.offset..index_stream.end);
                    render_pass.set_index_buffer(index_buffer, index_stream.format);

                    render_pass.draw_indexed(
                        0..index_stream.index_count,
                        0,
                        0..instance.instance_count,
                    );
                } else {
                    render_pass.draw(0..mesh.vertex_count, 0..instance.instance_count);
                }
            }
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
        });

    let camera = Camera::new(&camera_data);
    let camera_bind_group = camera.create_bind_group(&renderer.device);

    let happy_tree_bytes = include_bytes!("happy-tree.png");
    let basic_diffuse_texture = Texture::from_bytes(
        &renderer.device,
        &renderer.queue,
        happy_tree_bytes,
        "Happy-tree.png",
    )
    .unwrap();
    let happy_tree_bind_group = basic_diffuse_texture.create_bind_group(&renderer.device);
    let happy_tree_bind_group_id = renderer.add_local_bind_group(happy_tree_bind_group);

    let tex_mesh = MeshBuilder::new()
        .add_vertex_stream(POSITIONS)
        .add_vertex_stream(COLORS)
        .add_vertex_stream(TEX_COORDS)
        .add_vertex_stream(USE_TEXTURE)
        .add_index_stream(INDICES)
        .build(&renderer.device);
    let tex_mesh_id = renderer.add_mesh(tex_mesh);

    let tex_mesh_instance_buffer = InstanceDataBuilder::new()
        .add_instance_stream(&[
            Transformation(Mat4::IDENTITY).to_data(),
            Transformation(Mat4::from_translation((5.0, 0.0, 0.0).into())).to_data(),
        ])
        .add_instance_stream(&[
            Material::new(1.0, 0.0, 0.0).to_data(),
            Material::new(1.0, 0.0, 0.0).to_data(),
        ])
        .build(&renderer.device);

    let tex_mesh_object = RenderObject {
        renderable: Renderable::TexturedMesh(tex_mesh_id, vec![(happy_tree_bind_group_id, 1)]),
        instance: tex_mesh_instance_buffer,
    };
    let _tex_mesh_object_id = renderer.add_object(tex_mesh_object);

    let tex_mesh_wireframe_object = RenderObject {
        renderable: Renderable::WireframeMesh(tex_mesh_id),
        instance: InstanceDataBuilder::new()
            .add_instance_stream(&[
                Transformation(Mat4::IDENTITY).to_data(),
                Transformation(Mat4::from_translation((5.0, 0.0, 0.0).into())).to_data(),
            ])
            .add_instance_stream(&[
                Material::new(0.0, 0.0, 1.0).to_data(),
                Material::new(0.0, 0.0, 1.0).to_data(),
            ])
            .build(&renderer.device),
    };
    let _tex_mesh_wireframe_object_id = renderer.add_object(tex_mesh_wireframe_object);

    let colored_mesh = MeshBuilder::new()
        .add_vertex_stream(POSITIONS)
        .add_vertex_stream(COLORS)
        .add_index_stream(INDICES)
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

    let image_buffer = renderer.render(&camera_bind_group).await;
    image_buffer.save("image.png").unwrap();
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
