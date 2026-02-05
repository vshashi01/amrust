use image::{ImageBuffer, Rgba};

use crate::camera::CameraUniform;
use crate::composite::CompositeFragUniform;
use crate::screen_space::{self, ScreenSpace, ScreenSpaceUniform};
use crate::{Renderable, composite, constants, prelude::*};

use crate::{
    RenderData, WgpuError,
    camera::Camera,
    camera::CameraData,
    instance::InstanceFieldDescriptor,
    material, pipeline, render_pass, texture, transformation,
    vertex::{self, VertexDescriptor},
};

use std::{collections::HashMap, num::NonZero};

pub struct RenderTextureData {
    pub texture: wgpu::Texture,
    pub texture_view: wgpu::TextureView,
    pub texture_size: wgpu::Extent3d,
    pub texture_sampler: wgpu::Sampler,
}

pub struct Renderer {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,

    // properties related to the render surface
    output_buffer: Option<wgpu::Buffer>,
    texture_size: wgpu::Extent3d,
    texture_format: wgpu::TextureFormat, //stored for future dynamic render pipeline creation

    // internal rendering resources
    global_3d_pass_bind_group: wgpu::BindGroup,
    global_bind_groups: Vec<wgpu::BindGroup>,
    pub texture_bind_group_layout: wgpu::BindGroupLayout,
    pub texture_sampler: wgpu::Sampler,
    pub texture_array_bind_group_layout: wgpu::BindGroupLayout,
    camera: Camera,
    screen_space_data: ScreenSpace,
    composite_frag_uniform_buffer: wgpu::Buffer,
    composite_frag_uniform: CompositeFragUniform,

    // render pipelines
    render_pipeline_cache: HashMap<&'static str, wgpu::RenderPipeline>,
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
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
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
        //let camera_bind_group_layout = Camera::create_bind_group_layout(&device);
        let camera_bind_group_layout = create_global_3d_render_pass_bind_group_layout(&device);
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
            .build(&device, constants::TEXTURE_MESH_PIPELINE_KEY);

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
            .build(&device, constants::ARRAY_TEXTURE_MESH_PIPELINE_KEY);

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
            .build(&device, constants::VERTEX_COLORED_MESH_PIPELINE_KEY);

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
            .build(&device, constants::SOLID_COLORED_MESH_PIPELINE_KEY);

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
            .build(&device, constants::WIREFRAME_MESH_PIPELINE_KEY);

        let basic_vert_source =
            wgpu::ShaderSource::Wgsl((include_str!("shaders/basic_vert_shader.wgsl")).into());
        let silhoutte_frag_shader_source = wgpu::ShaderSource::Wgsl(
            include_str!("shaders/silhoutte_mask_frag_shader.wgsl").into(),
        );
        let silhoutte_render_pipeline = pipeline::PipelineBuilder::new()
            .set_vertex_source(basic_vert_source, None)
            .set_frag_source(silhoutte_frag_shader_source, None)
            .set_texture_format(wgpu::TextureFormat::R8Unorm)
            .add_vertex_buffer_layout(vertex::Position::layout::<0>())
            .add_vertex_buffer_layout(transformation::TransformationData::layout::<5>())
            .add_bind_group_layout(&camera_bind_group_layout)
            .set_depth_stencil(pipeline::create_depth_stencil_state())
            .build(&device, constants::MESH_SILHOUETTE_PIPELINE_KEY);

        let comp_bind_group_layout = composite::get_composite_pipeline_bind_group_layout(&device);
        let comp_vert_source =
            wgpu::ShaderSource::Wgsl((include_str!("shaders/composite_vert_shader.wgsl")).into());
        let comp_frag_shader_source =
            wgpu::ShaderSource::Wgsl(include_str!("shaders/composite_frag_shader.wgsl").into());
        let comp_render_pipeline = pipeline::PipelineBuilder::new()
            .set_vertex_source(comp_vert_source, None)
            .set_frag_source(comp_frag_shader_source, None)
            .set_texture_format(texture_format)
            .add_bind_group_layout(&comp_bind_group_layout)
            .build(&device, constants::COMPOSITE_PASS_PIPELINE);

        let screen_space_uniform = ScreenSpaceUniform::create_bind_group_layout(&device);
        let screen_space_vert_source = wgpu::ShaderSource::Wgsl(
            (include_str!("shaders/screen_space_vert_shader.wgsl")).into(),
        );
        let colored_frag_source =
            wgpu::ShaderSource::Wgsl(include_str!("shaders/colored_frag_shader.wgsl").into());
        let screen_space_colored_mesh_render_pipeline = pipeline::PipelineBuilder::new()
            .set_vertex_source(screen_space_vert_source, None)
            .set_frag_source(colored_frag_source, None)
            .set_texture_format(texture_format)
            .add_vertex_buffer_layout(vertex::Position::layout::<0>())
            .add_vertex_buffer_layout(vertex::Color::layout::<1>())
            .add_vertex_buffer_layout(transformation::TransformationData::layout::<5>())
            .add_vertex_buffer_layout(material::RgbMaterialData::layout::<9>())
            .add_vertex_buffer_layout(screen_space::SizeInPixel::layout::<12>())
            .add_bind_group_layout(&camera_bind_group_layout)
            .add_bind_group_layout(&screen_space_uniform)
            .set_depth_stencil(pipeline::create_depth_stencil_state())
            .build(&device, constants::SCREEN_SPACE_MESH_PIPELINE_KEY);

        let screen_space_vert_source = wgpu::ShaderSource::Wgsl(
            (include_str!("shaders/screen_space_vert_shader.wgsl")).into(),
        );
        let colored_frag_source =
            wgpu::ShaderSource::Wgsl(include_str!("shaders/colored_frag_shader.wgsl").into());
        let screen_space_wireframe_mesh_render_pipeline = pipeline::PipelineBuilder::new()
            .set_vertex_source(screen_space_vert_source, None)
            .set_frag_source(colored_frag_source, None)
            .set_texture_format(texture_format)
            .add_vertex_buffer_layout(vertex::Position::layout::<0>())
            .add_vertex_buffer_layout(vertex::Color::layout::<1>())
            .add_vertex_buffer_layout(transformation::TransformationData::layout::<5>())
            .add_vertex_buffer_layout(material::RgbMaterialData::layout::<9>())
            .add_vertex_buffer_layout(screen_space::SizeInPixel::layout::<12>())
            .add_bind_group_layout(&camera_bind_group_layout)
            .add_bind_group_layout(&screen_space_uniform)
            .set_topology(wgpu::PrimitiveTopology::LineList)
            .set_depth_stencil(pipeline::create_depth_stencil_state())
            .build(&device, constants::SCREEN_SPACE_WIREFRAME_PIPELINE_KEY);

        let mut render_pipeline_cache = HashMap::new();
        render_pipeline_cache.insert(
            constants::TEXTURE_MESH_PIPELINE_KEY,
            texture_surface_render_pipeline,
        );
        render_pipeline_cache.insert(
            constants::WIREFRAME_MESH_PIPELINE_KEY,
            wireframe_render_pipeline,
        );
        render_pipeline_cache.insert(
            constants::VERTEX_COLORED_MESH_PIPELINE_KEY,
            colored_surface_render_pipeline,
        );
        render_pipeline_cache.insert(
            constants::SOLID_COLORED_MESH_PIPELINE_KEY,
            solid_render_pipeline,
        );
        render_pipeline_cache.insert(
            constants::ARRAY_TEXTURE_MESH_PIPELINE_KEY,
            texture_array_surface_render_pipeline,
        );
        render_pipeline_cache.insert(
            constants::MESH_SILHOUETTE_PIPELINE_KEY,
            silhoutte_render_pipeline,
        );
        render_pipeline_cache.insert(constants::COMPOSITE_PASS_PIPELINE, comp_render_pipeline);
        render_pipeline_cache.insert(
            constants::SCREEN_SPACE_MESH_PIPELINE_KEY,
            screen_space_colored_mesh_render_pipeline,
        );
        render_pipeline_cache.insert(
            constants::SCREEN_SPACE_WIREFRAME_PIPELINE_KEY,
            screen_space_wireframe_mesh_render_pipeline,
        );

        let camera = Camera::new(&device);
        let screen_space_data = ScreenSpace::new(
            &device,
            texture_size.width as f32,
            texture_size.height as f32,
        );
        let global_bind_group = create_global_3d_render_pass_bind_group(
            &device,
            &camera_bind_group_layout,
            camera.get_binding_reosurce(),
            screen_space_data.get_binding_resource(),
        );

        let composite_frag_uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Highlight pixle size"),
            size: CompositeFragUniform::get_size() as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let composite_frag_uniform = CompositeFragUniform::new(10);

        queue.write_buffer(
            &composite_frag_uniform_buffer,
            0,
            bytemuck::cast_slice(&[composite_frag_uniform]),
        );

        Ok(Renderer {
            device,
            queue,
            output_buffer,
            texture_size,
            texture_format,
            global_bind_groups: Vec::new(),
            texture_bind_group_layout: basic_texture_bind_group_layout,
            texture_array_bind_group_layout,
            texture_sampler,
            camera,
            screen_space_data,
            global_3d_pass_bind_group: global_bind_group,
            composite_frag_uniform_buffer,
            composite_frag_uniform,
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
        let texture_sampler = self.device.create_sampler(&Default::default());

        RenderTextureData {
            texture,
            texture_view,
            texture_size: self.texture_size,
            texture_sampler,
        }
    }

    pub fn add_global_bind_group(&mut self, bind_group: wgpu::BindGroup) -> u32 {
        self.global_bind_groups.push(bind_group);

        (self.global_bind_groups.len() - 1) as u32
    }

    pub fn update_camera(&mut self, camera_data: &impl CameraData) {
        self.camera.update(camera_data);
        self.camera.write_buffer(&self.queue);
    }

    pub fn set_highlight_pixels(&mut self, px_thickness: u32) {
        self.composite_frag_uniform.highlight_pixel_size = px_thickness as i32;
        self.queue.write_buffer(
            &self.composite_frag_uniform_buffer,
            0,
            bytemuck::cast_slice(&[self.composite_frag_uniform]),
        );
    }

    pub async fn render_to_texture<'a>(
        &self,
        render_data: &[RenderData<'a>],
        render_texture_data: &RenderTextureData,
    ) -> Result<(), WgpuError> {
        //ToDO: validate the texture size
        Self::render_internal(self, render_data, Some(render_texture_data)).await
    }

    pub async fn render<'a>(&self, render_data: &[RenderData<'a>]) -> Result<(), WgpuError> {
        Self::render_internal(self, render_data, None).await
    }

    async fn render_internal<'a>(
        &self,
        render_data: &[RenderData<'a>],
        render_texture_data: Option<&RenderTextureData>,
    ) -> Result<(), WgpuError> {
        let final_texture_data = match render_texture_data {
            Some(data) => data,
            None => &self.create_render_texture_data(),
        };

        let depth_texture =
            texture::DepthTexture::create_depth_texture(&self.device, self.texture_size);

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

        let use_final_texture_data = render_data
            .iter()
            .all(|d| !matches!(d.renderable, Renderable::SilhouetteMesh));

        let color_data = if use_final_texture_data {
            final_texture_data
        } else {
            &create_texture_data(&self.device, self.texture_size, self.texture_format)
        };

        // standard render pass
        {
            let render_pass_desc = wgpu::RenderPassDescriptor {
                label: Some("Surface Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &color_data.texture_view,
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

            render_pass.set_bind_group(0, &self.global_3d_pass_bind_group, &[]);
            // set up global bind groups
            for (i, bind_group) in self.global_bind_groups.iter().enumerate() {
                render_pass.set_bind_group((i + 1) as u32, bind_group, &[]);
            }

            // let renderables = render_db.get_renderables().collect::<Vec<_>>();

            render_pass::solid_render_pass(
                render_data,
                &self.render_pipeline_cache,
                &mut render_pass,
            );

            render_pass::wireframe_render_pass(
                render_data,
                &self.render_pipeline_cache,
                &mut render_pass,
            );
        }

        //mask render pass for selected meshes
        if render_data
            .iter()
            .any(|d| matches!(d.renderable, crate::Renderable::SilhouetteMesh))
        {
            let selection_mask_tex = create_texture_data(
                &self.device,
                final_texture_data.texture_size,
                wgpu::TextureFormat::R8Unorm,
            );
            {
                let render_pass_desc = wgpu::RenderPassDescriptor {
                    label: Some("Silhoutte Mesh Pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &selection_mask_tex.texture_view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
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
                    }), //ToDo: revisit depth later
                    occlusion_query_set: None,
                    timestamp_writes: None,
                };
                let mut render_pass = encoder.begin_render_pass(&render_pass_desc);

                render_pass.set_bind_group(0, &self.global_3d_pass_bind_group, &[]);
                // set up global bind groups
                for (i, bind_group) in self.global_bind_groups.iter().enumerate() {
                    render_pass.set_bind_group((i + 1) as u32, bind_group, &[]);
                }

                // let renderables = render_db.get_renderables().collect::<Vec<_>>();

                render_pass::silhoutte_pass(
                    render_data,
                    &self.render_pipeline_cache,
                    &mut render_pass,
                );
            }

            // composite of textures
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("Composite Pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &final_texture_data.texture_view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    occlusion_query_set: None,
                    timestamp_writes: None,
                });

                pass.set_pipeline(
                    self.render_pipeline_cache
                        .get(constants::COMPOSITE_PASS_PIPELINE)
                        .unwrap(),
                );
                pass.set_bind_group(
                    0,
                    &composite::create_composite_bind_group(
                        &self.device,
                        &composite::get_composite_pipeline_bind_group_layout(&self.device),
                        &color_data.texture_view,
                        &color_data.texture_sampler,
                        &selection_mask_tex.texture_view,
                        &selection_mask_tex.texture_sampler,
                        self.composite_frag_uniform_buffer
                            .as_entire_buffer_binding(),
                    ),
                    &[],
                );
                pass.draw(0..3, 0..1);
            }
        }

        if let Some(buffer) = &self.output_buffer {
            let u32_size = std::mem::size_of::<u32>() as u32;
            encoder.copy_texture_to_buffer(
                wgpu::TexelCopyTextureInfo {
                    aspect: wgpu::TextureAspect::All,
                    texture: &final_texture_data.texture,
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

pub fn create_texture_data(
    device: &wgpu::Device,
    size: wgpu::Extent3d,
    format: wgpu::TextureFormat,
) -> RenderTextureData {
    let texture_desc = wgpu::TextureDescriptor {
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::TEXTURE_BINDING,
        label: None,
        view_formats: &[],
    };
    let texture = device.create_texture(&texture_desc);
    let texture_view = texture.create_view(&Default::default());
    let texture_sampler = device.create_sampler(&Default::default());

    RenderTextureData {
        texture,
        texture_view,
        texture_size: size,
        texture_sampler,
    }
}

fn create_global_3d_render_pass_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("Global Bind Group Layout"),
        entries: &[
            //camera uniform
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            //screen space size uniform
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
        ],
    })
}

fn create_global_3d_render_pass_bind_group(
    device: &wgpu::Device,
    bind_group_layout: &wgpu::BindGroupLayout,
    camera_resource: wgpu::BindingResource<'_>,
    screen_space_resource: wgpu::BindingResource<'_>,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("Global Bind Group"),
        layout: bind_group_layout,
        entries: &[
            //camera uniform
            wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_resource,
            },
            //screen space size uniform
            wgpu::BindGroupEntry {
                binding: 1,
                resource: screen_space_resource,
            },
        ],
    })
}
