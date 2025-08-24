use image::{ImageBuffer, Rgba};

use crate::camera::CameraData;
use crate::prelude::*;

use crate::{
    RenderObject, WgpuError,
    camera::Camera,
    gpu_mesh::GpuMesh,
    instance::InstanceFieldDescriptor,
    material, pipeline, render_pass, texture, transformation,
    vertex::{self, VertexDescriptor},
};

use std::{
    collections::{HashMap, HashSet},
    num::NonZero,
};

pub struct RenderTextureData {
    pub texture: wgpu::Texture,
    pub texture_view: wgpu::TextureView,
    pub texture_size: wgpu::Extent3d,
}

pub struct Renderer {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,

    // properties related to the render surface
    output_buffer: Option<wgpu::Buffer>,
    texture_size: wgpu::Extent3d,
    texture_format: wgpu::TextureFormat, //stored for future dynamic render pipeline creation

    // external rendering resources
    // consider splitting these to separate struct to be managed by the app
    textures: Vec<texture::Texture>,
    meshes: Vec<GpuMesh>,
    objects: Vec<RenderObject>,
    invisible_objects: HashSet<usize>,
    local_bind_groups: Vec<wgpu::BindGroup>,

    // internal rendering resources
    global_bind_groups: Vec<wgpu::BindGroup>,
    texture_bind_group_layout: wgpu::BindGroupLayout,
    texture_sampler: wgpu::Sampler,
    texture_array_bind_group_layout: wgpu::BindGroupLayout,
    camera: Camera,

    // render pipelines
    render_pipeline_cache: HashMap<String, wgpu::RenderPipeline>,
}

pub const DEVICE_FEATURES: [wgpu::Features; 2] = [
    wgpu::Features::TEXTURE_BINDING_ARRAY,
    wgpu::Features::SAMPLED_TEXTURE_AND_STORAGE_BUFFER_ARRAY_NON_UNIFORM_INDEXING,
];

// #[cfg(feature = "wgpu")]
pub const DEVICE_LIMITS: wgpu::Limits = wgpu::Limits {
    max_binding_array_elements_per_shader_stage:
        texture::MAX_BINDING_ARRAY_ELEMENTS_PER_SHADER_STAGE,
    max_binding_array_sampler_elements_per_shader_stage:
        texture::MAX_BINDING_ARRAY_SAMPLERS_PER_SHADER_STAGE,
    max_texture_dimension_2d: texture::MAX_TEXTURE_SIZE,
    ..wgpu::Limits::downlevel_defaults()
};

// #[cfg(feature = "egui_wgpu")]
// pub const DEVICE_LIMITS: wgpu::Limits = wgpu::Limits {
//     max_texture_dimension_2d: texture::MAX_TEXTURE_SIZE,
//     ..wgpu::Limits::downlevel_defaults()
// };

impl Renderer {
    pub async fn from_existing_device_and_queue(
        device: wgpu::Device,
        queue: wgpu::Queue,
        texture_format: wgpu::TextureFormat,
        width: u32,
        height: u32,
    ) -> Result<Self, WgpuError> {
        for features in DEVICE_FEATURES {
            if !device.features().contains(features) {
                panic!(
                    "Required features {:?} are not supported by the device",
                    features
                );
            }
        }

        let limits_satisfied = device.limits().check_limits(&DEVICE_LIMITS);

        if !limits_satisfied {
            panic!(
                "Device does not support required limits: {:?}",
                DEVICE_LIMITS
            );
        }

        let texture_size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };

        Self::setup_new_renderer(device, queue, texture_format, texture_size, None)
    }

    pub async fn from_new_device(width: u32, height: u32) -> Result<Self, WgpuError> {
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

        let required_features = DEVICE_FEATURES
            .iter()
            .fold(wgpu::Features::empty(), |acc, &f| acc | f);

        let device_descriptor = wgpu::DeviceDescriptor {
            label: Some("Gpu Device"),
            required_features,
            required_limits: DEVICE_LIMITS,
            memory_hints: wgpu::MemoryHints::Performance,
            trace: wgpu::Trace::Off,
        };

        let (device, queue) = adapter.request_device(&device_descriptor).await.unwrap();

        let texture_size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };

        let texture_format = wgpu::TextureFormat::Rgba8UnormSrgb;

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

        Self::setup_new_renderer(
            device,
            queue,
            texture_format,
            texture_size,
            Some(output_buffer),
        )
    }

    fn setup_new_renderer(
        device: wgpu::Device,
        queue: wgpu::Queue,
        texture_format: wgpu::TextureFormat,
        texture_size: wgpu::Extent3d,
        output_buffer: Option<wgpu::Buffer>,
    ) -> Result<Self, WgpuError> {
        let camera_bind_group_layout = Camera::create_bind_group_layout(&device);
        let basic_texture_bind_group_layout = texture::generate_texture_bind_group_layout::<0, 1>(
            &device,
            "Basic Texture Bind Group Layout",
            true,
        );
        let texture_sampler = texture::generate_basic_texture_sampler(&device);
        let texture_array_bind_group_layout =
            texture::generate_texture_array_bind_group_layout::<0, 1>(
                &device,
                "Texture Array Layout",
                true,
                NonZero::new(texture::MAX_BINDING_ARRAY_ELEMENTS_PER_SHADER_STAGE).unwrap(),
            );

        let standard_textured_vert_shader_source =
            wgpu::ShaderSource::Wgsl((include_str!("shaders/textured_vert_shader.wgsl")).into());
        let single_texture_frag_shader_source =
            wgpu::ShaderSource::Wgsl((include_str!("shaders/texture_frag_shader.wgsl")).into());
        let texture_surface_render_pipeline = pipeline::PipelineBuilder::new()
            .set_vertex_source(standard_textured_vert_shader_source, None)
            .set_frag_source(single_texture_frag_shader_source, None)
            .set_texture_format(texture_format)
            .add_vertex_buffer_layout(vertex::Position::layout::<0>())
            .add_vertex_buffer_layout(vertex::Color::layout::<1>())
            .add_vertex_buffer_layout(vertex::TexCoords::layout::<2>())
            .add_vertex_buffer_layout(vertex::UseTexture::layout::<3>())
            .add_vertex_buffer_layout(transformation::TransformationData::layout::<5>())
            .add_vertex_buffer_layout(material::RgbMaterialData::layout::<9>())
            .add_bind_group_layout(&camera_bind_group_layout)
            .add_bind_group_layout(&basic_texture_bind_group_layout)
            .set_depth_stencil(pipeline::create_depth_stencil_state())
            .build(&device, "Textured Surface");

        let standard_textured_vert_shader_source =
            wgpu::ShaderSource::Wgsl((include_str!("shaders/textured_vert_shader.wgsl")).into());
        let array_textures_frag_shader_source = wgpu::ShaderSource::Wgsl(
            (include_str!("shaders/array_textures_frag_shader.wgsl")).into(),
        );
        let texture_array_surface_render_pipeline = pipeline::PipelineBuilder::new()
            .set_vertex_source(standard_textured_vert_shader_source, None)
            .set_frag_source(array_textures_frag_shader_source, None)
            .set_texture_format(texture_format)
            .add_vertex_buffer_layout(vertex::Position::layout::<0>())
            .add_vertex_buffer_layout(vertex::Color::layout::<1>())
            .add_vertex_buffer_layout(vertex::TexCoords::layout::<2>())
            .add_vertex_buffer_layout(vertex::UseTexture::layout::<3>())
            .add_vertex_buffer_layout(transformation::TransformationData::layout::<5>())
            .add_vertex_buffer_layout(material::RgbMaterialData::layout::<9>())
            .add_bind_group_layout(&camera_bind_group_layout)
            .add_bind_group_layout(&texture_array_bind_group_layout)
            .set_depth_stencil(pipeline::create_depth_stencil_state())
            .build(&device, "Texture Array Surface");

        let colored_vert_shader_source =
            wgpu::ShaderSource::Wgsl((include_str!("shaders/colored_vert_shader.wgsl")).into());
        let colored_frag_shader_source =
            wgpu::ShaderSource::Wgsl(include_str!("shaders/colored_frag_shader.wgsl").into());
        let colored_surface_render_pipeline = pipeline::PipelineBuilder::new()
            .set_vertex_source(colored_vert_shader_source, None)
            .set_frag_source(colored_frag_shader_source, None)
            .set_texture_format(texture_format)
            .add_vertex_buffer_layout(vertex::Position::layout::<0>())
            .add_vertex_buffer_layout(vertex::Color::layout::<1>())
            .add_vertex_buffer_layout(transformation::TransformationData::layout::<5>())
            .add_vertex_buffer_layout(material::RgbMaterialData::layout::<9>())
            .add_bind_group_layout(&camera_bind_group_layout)
            .set_depth_stencil(pipeline::create_depth_stencil_state())
            .build(&device, "Colored Surface");

        let solid_source = wgpu::ShaderSource::Wgsl(
            (include_str!("shaders/material_color_vert_shader.wgsl")).into(),
        );
        let colored_frag_shader_source =
            wgpu::ShaderSource::Wgsl(include_str!("shaders/colored_frag_shader.wgsl").into());
        let solid_render_pipeline = pipeline::PipelineBuilder::new()
            .set_vertex_source(solid_source, None)
            .set_frag_source(colored_frag_shader_source, None)
            .set_texture_format(texture_format)
            .add_vertex_buffer_layout(vertex::Position::layout::<0>())
            .add_vertex_buffer_layout(transformation::TransformationData::layout::<5>())
            .add_vertex_buffer_layout(material::RgbMaterialData::layout::<9>())
            .add_bind_group_layout(&camera_bind_group_layout)
            .set_depth_stencil(pipeline::create_depth_stencil_state())
            .build(&device, "Solid uniform");

        let colored_vert_shader_source = wgpu::ShaderSource::Wgsl(
            (include_str!("shaders/material_color_vert_shader.wgsl")).into(),
        );
        let colored_frag_shader_source =
            wgpu::ShaderSource::Wgsl(include_str!("shaders/colored_frag_shader.wgsl").into());
        let wireframe_render_pipeline = pipeline::PipelineBuilder::new()
            .set_vertex_source(colored_vert_shader_source, None)
            .set_frag_source(colored_frag_shader_source, None)
            .set_texture_format(texture_format)
            .add_vertex_buffer_layout(vertex::Position::layout::<0>())
            .add_vertex_buffer_layout(transformation::TransformationData::layout::<5>())
            .add_vertex_buffer_layout(material::RgbMaterialData::layout::<9>())
            .add_bind_group_layout(&camera_bind_group_layout)
            .set_topology(wgpu::PrimitiveTopology::LineList)
            .set_depth_stencil(pipeline::create_depth_stencil_state())
            .build(&device, "Solid Wireframe");

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

        let camera = Camera::new(&device);

        Ok(Renderer {
            device,
            queue,
            output_buffer,
            texture_size,
            texture_format,
            textures: Vec::new(),
            meshes: Vec::new(),
            objects: Vec::new(),
            invisible_objects: HashSet::new(),
            local_bind_groups: Vec::new(),
            global_bind_groups: Vec::new(),
            texture_bind_group_layout: basic_texture_bind_group_layout,
            texture_array_bind_group_layout,
            texture_sampler,
            camera,
            render_pipeline_cache,
        })
    }

    pub fn set_size(&mut self, width: u32, height: u32) {
        let texture_size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };

        self.texture_size = texture_size;
    }

    pub fn create_render_texture_data(&self) -> RenderTextureData {
        let texture_desc = wgpu::TextureDescriptor {
            size: self.texture_size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.texture_format,
            usage: wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING,
            label: None,
            view_formats: &[],
        };
        let texture = self.device.create_texture(&texture_desc);
        let texture_view = texture.create_view(&Default::default());

        RenderTextureData {
            texture,
            texture_view,
            texture_size: self.texture_size,
        }
    }

    // returns the texture id and the bind group id
    pub fn add_texture(&mut self, texture: texture::Texture) -> (u32, u32) {
        let bind_group = texture::generate_basic_texture_bind_group::<0, 1>(
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

        let bind_group = texture::generate_texture_array_bind_group::<0, 1>(
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

    pub fn update_camera(&mut self, camera_data: &impl CameraData) {
        self.camera.update(camera_data);
        self.camera.write_buffer(&self.queue);
    }

    pub async fn render_to_texture(
        &self,
        render_texture_data: &RenderTextureData,
    ) -> Result<(), WgpuError> {
        //ToDO: validate the texture size
        Self::render_internal(self, Some(render_texture_data)).await
    }

    pub async fn render(&self) -> Result<(), WgpuError> {
        Self::render_internal(self, None).await
    }

    async fn render_internal(
        &self,
        render_texture_data: Option<&RenderTextureData>,
    ) -> Result<(), WgpuError> {
        let texture_data = match render_texture_data {
            Some(data) => data,
            None => &self.create_render_texture_data(),
        };
        let depth_texture =
            texture::DepthTexture::create_depth_texture(&self.device, self.texture_size);

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

        {
            let render_pass_desc = wgpu::RenderPassDescriptor {
                label: Some("Surface Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &texture_data.texture_view,
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
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &depth_texture.view,
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

            render_pass.set_bind_group(0, &self.camera.bind_group, &[]);
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

            render_pass::solid_render_pass(
                &visible_objects,
                &self.meshes,
                &self.local_bind_groups,
                &self.render_pipeline_cache,
                &mut render_pass,
            );

            render_pass::wireframe_render_pass(
                &visible_objects,
                &self.meshes,
                &self.local_bind_groups,
                &self.render_pipeline_cache,
                &mut render_pass,
            );
        }

        if let Some(buffer) = &self.output_buffer {
            let u32_size = std::mem::size_of::<u32>() as u32;
            encoder.copy_texture_to_buffer(
                wgpu::TexelCopyTextureInfo {
                    aspect: wgpu::TextureAspect::All,
                    texture: &texture_data.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                },
                wgpu::TexelCopyBufferInfo {
                    buffer,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(u32_size * self.texture_size.width),
                        rows_per_image: Some(self.texture_size.height),
                    },
                },
                self.texture_size,
            );
        }

        self.queue.submit(Some(encoder.finish()));

        Ok(())
    }

    pub async fn present(&mut self) -> ImageBuffer<Rgba<u8>, Vec<u8>> {
        match &self.output_buffer {
            Some(buffer) => {
                // We need to scope the mapping variables so that we can
                // unmap the buffer
                let image_buffer = {
                    let buffer_slice = buffer.slice(..);

                    // NOTE: We have to create the mapping THEN device.poll() before await
                    // the future. Otherwise the application will freeze.
                    let (tx, rx) = futures_intrusive::channel::shared::oneshot_channel();
                    buffer_slice.map_async(wgpu::MapMode::Read, move |result| {
                        tx.send(result).unwrap();
                    });

                    let result = self.device.poll(wgpu::PollType::Wait);

                    match result {
                        Ok(status) => {
                            if status.wait_finished() {
                                rx.receive().await.unwrap().unwrap();

                                let data = buffer_slice.get_mapped_range();

                                use image::{ImageBuffer, Rgba};

                                ImageBuffer::<Rgba<u8>, _>::from_raw(
                                    self.texture_size.width,
                                    self.texture_size.height,
                                    data.to_vec(),
                                )
                                .unwrap()
                            } else {
                                panic!("Polling GPU never returned");
                            }
                        }
                        Err(e) => panic!("{}", e),
                    }
                };
                buffer.unmap();

                image_buffer
            }
            None => panic!("Output buffer is not set!"),
        }
    }
}
