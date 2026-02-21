use image::{ImageBuffer, Rgba};

use crate::camera::{CameraData, CameraUniform};
use crate::clip::ClipPlanes;
use crate::composite::CompositeFragUniform;
use crate::light::{LightData, LightUniform};
use crate::screen_space::{self, ScreenSpace, ScreenSpaceUniform};
use crate::{Renderable3d, clip, composite, constants, light, prelude::*};

use crate::{
    RenderData3d, TriangleFaceMode, WgpuError,
    camera::Camera,
    instance::InstanceFieldDescriptor,
    material, pipeline, render_pass, texture, transformation,
    vertex::{self, VertexDescriptor},
};

use std::{collections::HashMap, num::NonZero};

pub const MAX_CLIP_PLANE_COUNT: usize = 1;

pub struct RenderTextureData {
    pub texture: wgpu::Texture,
    pub texture_view: wgpu::TextureView,
    pub texture_size: wgpu::Extent3d,
    pub texture_sampler: wgpu::Sampler,
    pub texture_format: wgpu::TextureFormat,
}

pub struct FrameViewData {
    bind_group: wgpu::BindGroup,
    pub camera: Camera,
    pub screen_space_data: ScreenSpace,
    pub clip_planes: ClipPlanes<MAX_CLIP_PLANE_COUNT>,
    pub light: LightData,
    pub triangle_face_mode: TriangleFaceMode,
}

impl FrameViewData {
    pub fn new(device: &wgpu::Device, initial_frame_width: f32, initial_frame_height: f32) -> Self {
        let camera = Camera::new(device);
        let screen_space_data = ScreenSpace::new(device, initial_frame_width, initial_frame_height);
        let clip_planes = ClipPlanes::new(device);
        let light = LightData::new_directional_light(
            device,
            glam::Vec3::new(0.0, -1.0, 0.0),
            1.0,
            glam::Vec3::new(1.0, 1.0, 1.0),
            0.2,
        );

        let bind_group = create_global_3d_render_pass_bind_group::<MAX_CLIP_PLANE_COUNT>(
            device,
            camera.get_binding_reosurce(),
            screen_space_data.get_binding_resource(),
            light.get_binding_resource(),
            &clip_planes.buffers,
        );

        Self {
            camera,
            screen_space_data,
            bind_group,
            clip_planes,
            light,
            triangle_face_mode: TriangleFaceMode::FrontOnly,
        }
    }

    pub fn update_viewport_size(
        &mut self,
        width: f32,
        height: f32,
        new_camera_data: &impl CameraData,
    ) {
        self.camera.update(new_camera_data);
        self.screen_space_data.update_size(width, height);
    }

    pub fn update_data_to_gpu(&self, queue: &wgpu::Queue) {
        self.camera.write_buffer(queue);
        self.screen_space_data.write_buffer(queue);
        self.clip_planes.write_buffer(queue);
        self.light.write_buffer(queue);
    }

    pub fn set_clip_plane(&mut self, clip_plane: &clip::ClipPlane) {
        self.clip_planes.update_clip_planes(|planes| {
            if let Some(plane) = planes.first_mut() {
                plane.copy_from(clip_plane);
            }
        });
    }

    // pub fn set_light(&mut self, light: LightUniform) {
    //     self.light_uniform = light;
    // }

    // pub fn clear_clip_plane(&mut self) {
    //     self.clip_plane = None;
    // }

    // fn clip_bind_group(&self) -> &wgpu::BindGroup {
    //     &self.clip_bind_group
    // }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ViewportRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

pub struct RenderView<'a, 'fv> {
    pub(crate) rect: Option<ViewportRect>,
    pub(crate) render_data: &'a [RenderData3d<'a>],
    pub(crate) frame_view_data: &'fv FrameViewData,
    pub(crate) clear_color: wgpu::Color,
}

impl<'a, 'fv> RenderView<'a, 'fv> {
    pub fn new_main_view(
        render_data: &'a [RenderData3d<'a>],
        frame_view_data: &'fv FrameViewData,
        bg_color: wgpu::Color,
    ) -> Self {
        Self {
            rect: None,
            render_data,
            frame_view_data,
            clear_color: bg_color,
        }
    }

    pub fn new_sub_viewport(
        rect: ViewportRect,
        render_data: &'a [RenderData3d<'a>],
        frame_view_data: &'fv FrameViewData,
    ) -> Self {
        Self {
            rect: Some(rect),
            render_data,
            frame_view_data,
            clear_color: wgpu::Color::TRANSPARENT,
        }
    }
}

enum RenderMode<'rm> {
    DrawToBuffer(&'rm wgpu::Buffer, wgpu::Extent3d),
    DrawToTexture(&'rm RenderTextureData),
    //DrawToWindow //for future
}

pub struct Renderer {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,

    // properties related to the render surface
    texture_format: wgpu::TextureFormat, //stored for future dynamic render pipeline creation

    // internal rendering resources
    global_bind_groups: Vec<wgpu::BindGroup>,
    pub texture_bind_group_layout: wgpu::BindGroupLayout,
    pub texture_sampler: wgpu::Sampler,
    pub texture_array_bind_group_layout: wgpu::BindGroupLayout,
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

        Self::setup_new_renderer(&device, &queue, texture_format)
    }

    pub async fn from_new_device() -> Result<Self, WgpuError> {
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

        let texture_format = wgpu::TextureFormat::Rgba8UnormSrgb;

        Self::setup_new_renderer(&device, &queue, texture_format)
    }

    fn setup_new_renderer(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        texture_format: wgpu::TextureFormat,
    ) -> Result<Self, WgpuError> {
        let global_bind_group_layout =
            create_global_3d_render_pass_bind_group_layout::<MAX_CLIP_PLANE_COUNT>(device);
        // All pipelines now use transparency-enabled layout since shaders expect it
        let mesh_local_clip_layout = crate::RenderDataLocalResources::clip_layout(device);
        let mesh_local_textured_layout = crate::RenderDataLocalResources::textured_layout(device);
        let mesh_local_array_textured_layout =
            crate::RenderDataLocalResources::array_textured_layout(
                device,
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
            .add_vertex_buffer_layout(vertex::Position3d::layout::<0>())
            .add_vertex_buffer_layout(vertex::Color::layout::<1>())
            .add_vertex_buffer_layout(vertex::TexCoords::layout::<2>())
            .add_vertex_buffer_layout(vertex::UseTexture::layout::<3>())
            .add_vertex_buffer_layout(transformation::TransformationData::layout::<5>())
            .add_vertex_buffer_layout(material::RgbMaterialData::layout::<9>())
            .add_bind_group_layout(&global_bind_group_layout)
            .add_bind_group_layout(&mesh_local_clip_layout)
            .add_bind_group_layout(&mesh_local_textured_layout)
            .set_depth_stencil(pipeline::create_depth_stencil_state())
            .build(device, constants::TEXTURE_MESH_PIPELINE_KEY);

        let standard_textured_vert_shader_source = wgpu::ShaderSource::Wgsl(
            (include_str!("shaders/textured_vert_shader_lit.wgsl")).into(),
        );
        let single_texture_frag_shader_source =
            wgpu::ShaderSource::Wgsl((include_str!("shaders/lit_texture_frag_shader.wgsl")).into());
        let texture_surface_render_pipeline_lit = pipeline::PipelineBuilder::new()
            .set_vertex_source(standard_textured_vert_shader_source, None)
            .set_frag_source(single_texture_frag_shader_source, None)
            .set_texture_format(texture_format)
            .add_vertex_buffer_layout(vertex::Position3d::layout::<0>())
            .add_vertex_buffer_layout(vertex::Color::layout::<1>())
            .add_vertex_buffer_layout(vertex::TexCoords::layout::<2>())
            .add_vertex_buffer_layout(vertex::UseTexture::layout::<3>())
            .add_vertex_buffer_layout(vertex::Normal::layout::<4>())
            .add_vertex_buffer_layout(transformation::TransformationData::layout::<5>())
            .add_vertex_buffer_layout(material::RgbMaterialData::layout::<9>())
            .add_vertex_buffer_layout(light::NormalMatrixData::layout::<10>())
            .add_bind_group_layout(&global_bind_group_layout)
            .add_bind_group_layout(&mesh_local_clip_layout)
            .add_bind_group_layout(&mesh_local_textured_layout)
            .set_depth_stencil(pipeline::create_depth_stencil_state())
            .build(device, constants::TEXTURE_MESH_PIPELINE_KEY_LIT);

        let standard_textured_vert_shader_source =
            wgpu::ShaderSource::Wgsl((include_str!("shaders/textured_vert_shader.wgsl")).into());
        let array_textures_frag_shader_source = wgpu::ShaderSource::Wgsl(
            (include_str!("shaders/array_textures_frag_shader.wgsl")).into(),
        );
        let texture_array_surface_render_pipeline = pipeline::PipelineBuilder::new()
            .set_vertex_source(standard_textured_vert_shader_source, None)
            .set_frag_source(array_textures_frag_shader_source, None)
            .set_texture_format(texture_format)
            .add_vertex_buffer_layout(vertex::Position3d::layout::<0>())
            .add_vertex_buffer_layout(vertex::Color::layout::<1>())
            .add_vertex_buffer_layout(vertex::TexCoords::layout::<2>())
            .add_vertex_buffer_layout(vertex::UseTexture::layout::<3>())
            .add_vertex_buffer_layout(transformation::TransformationData::layout::<5>())
            .add_vertex_buffer_layout(material::RgbMaterialData::layout::<9>())
            .add_bind_group_layout(&global_bind_group_layout)
            .add_bind_group_layout(&mesh_local_clip_layout)
            .add_bind_group_layout(&mesh_local_array_textured_layout)
            .set_depth_stencil(pipeline::create_depth_stencil_state())
            .build(device, constants::ARRAY_TEXTURE_MESH_PIPELINE_KEY);

        let standard_textured_vert_shader_source = wgpu::ShaderSource::Wgsl(
            (include_str!("shaders/textured_vert_shader_lit.wgsl")).into(),
        );
        let array_textures_frag_shader_source = wgpu::ShaderSource::Wgsl(
            (include_str!("shaders/lit_array_textures_frag_shader.wgsl")).into(),
        );
        let texture_array_surface_render_pipeline_lit = pipeline::PipelineBuilder::new()
            .set_vertex_source(standard_textured_vert_shader_source, None)
            .set_frag_source(array_textures_frag_shader_source, None)
            .set_texture_format(texture_format)
            .add_vertex_buffer_layout(vertex::Position3d::layout::<0>())
            .add_vertex_buffer_layout(vertex::Color::layout::<1>())
            .add_vertex_buffer_layout(vertex::TexCoords::layout::<2>())
            .add_vertex_buffer_layout(vertex::UseTexture::layout::<3>())
            .add_vertex_buffer_layout(vertex::Normal::layout::<4>())
            .add_vertex_buffer_layout(transformation::TransformationData::layout::<5>())
            .add_vertex_buffer_layout(material::RgbMaterialData::layout::<9>())
            .add_vertex_buffer_layout(light::NormalMatrixData::layout::<10>())
            .add_bind_group_layout(&global_bind_group_layout)
            .add_bind_group_layout(&mesh_local_clip_layout)
            .add_bind_group_layout(&mesh_local_array_textured_layout)
            .set_depth_stencil(pipeline::create_depth_stencil_state())
            .build(device, constants::ARRAY_TEXTURE_MESH_PIPELINE_KEY_LIT);

        // No-cull variants for textured meshes
        let standard_textured_vert_shader_source =
            wgpu::ShaderSource::Wgsl((include_str!("shaders/textured_vert_shader.wgsl")).into());
        let single_texture_frag_shader_source =
            wgpu::ShaderSource::Wgsl((include_str!("shaders/texture_frag_shader.wgsl")).into());
        let no_cull_texture_surface_render_pipeline = pipeline::PipelineBuilder::new()
            .set_vertex_source(standard_textured_vert_shader_source, None)
            .set_frag_source(single_texture_frag_shader_source, None)
            .set_texture_format(texture_format)
            .add_vertex_buffer_layout(vertex::Position3d::layout::<0>())
            .add_vertex_buffer_layout(vertex::Color::layout::<1>())
            .add_vertex_buffer_layout(vertex::TexCoords::layout::<2>())
            .add_vertex_buffer_layout(vertex::UseTexture::layout::<3>())
            .add_vertex_buffer_layout(transformation::TransformationData::layout::<5>())
            .add_vertex_buffer_layout(material::RgbMaterialData::layout::<9>())
            .add_bind_group_layout(&global_bind_group_layout)
            .add_bind_group_layout(&mesh_local_clip_layout)
            .add_bind_group_layout(&mesh_local_textured_layout)
            .set_depth_stencil(pipeline::create_depth_stencil_state())
            .set_cull_mode(TriangleFaceMode::FrontAndBack)
            .build(device, constants::NO_CULL_TEXTURE_MESH_PIPELINE_KEY);

        let standard_textured_vert_shader_source = wgpu::ShaderSource::Wgsl(
            (include_str!("shaders/textured_vert_shader_lit.wgsl")).into(),
        );
        let single_texture_frag_shader_source =
            wgpu::ShaderSource::Wgsl((include_str!("shaders/lit_texture_frag_shader.wgsl")).into());
        let no_cull_texture_surface_render_pipeline_lit = pipeline::PipelineBuilder::new()
            .set_vertex_source(standard_textured_vert_shader_source, None)
            .set_frag_source(single_texture_frag_shader_source, None)
            .set_texture_format(texture_format)
            .add_vertex_buffer_layout(vertex::Position3d::layout::<0>())
            .add_vertex_buffer_layout(vertex::Color::layout::<1>())
            .add_vertex_buffer_layout(vertex::TexCoords::layout::<2>())
            .add_vertex_buffer_layout(vertex::UseTexture::layout::<3>())
            .add_vertex_buffer_layout(vertex::Normal::layout::<4>())
            .add_vertex_buffer_layout(transformation::TransformationData::layout::<5>())
            .add_vertex_buffer_layout(material::RgbMaterialData::layout::<9>())
            .add_vertex_buffer_layout(light::NormalMatrixData::layout::<10>())
            .add_bind_group_layout(&global_bind_group_layout)
            .add_bind_group_layout(&mesh_local_clip_layout)
            .add_bind_group_layout(&mesh_local_textured_layout)
            .set_depth_stencil(pipeline::create_depth_stencil_state())
            .set_cull_mode(TriangleFaceMode::FrontAndBack)
            .build(device, constants::NO_CULL_TEXTURE_MESH_PIPELINE_KEY_LIT);

        let standard_textured_vert_shader_source =
            wgpu::ShaderSource::Wgsl((include_str!("shaders/textured_vert_shader.wgsl")).into());
        let array_textures_frag_shader_source = wgpu::ShaderSource::Wgsl(
            (include_str!("shaders/array_textures_frag_shader.wgsl")).into(),
        );
        let no_cull_texture_array_surface_render_pipeline = pipeline::PipelineBuilder::new()
            .set_vertex_source(standard_textured_vert_shader_source, None)
            .set_frag_source(array_textures_frag_shader_source, None)
            .set_texture_format(texture_format)
            .add_vertex_buffer_layout(vertex::Position3d::layout::<0>())
            .add_vertex_buffer_layout(vertex::Color::layout::<1>())
            .add_vertex_buffer_layout(vertex::TexCoords::layout::<2>())
            .add_vertex_buffer_layout(vertex::UseTexture::layout::<3>())
            .add_vertex_buffer_layout(transformation::TransformationData::layout::<5>())
            .add_vertex_buffer_layout(material::RgbMaterialData::layout::<9>())
            .add_bind_group_layout(&global_bind_group_layout)
            .add_bind_group_layout(&mesh_local_clip_layout)
            .add_bind_group_layout(&mesh_local_array_textured_layout)
            .set_depth_stencil(pipeline::create_depth_stencil_state())
            .set_cull_mode(TriangleFaceMode::FrontAndBack)
            .build(device, constants::NO_CULL_ARRAY_TEXTURE_MESH_PIPELINE_KEY);

        let standard_textured_vert_shader_source = wgpu::ShaderSource::Wgsl(
            (include_str!("shaders/textured_vert_shader_lit.wgsl")).into(),
        );
        let array_textures_frag_shader_source = wgpu::ShaderSource::Wgsl(
            (include_str!("shaders/lit_array_textures_frag_shader.wgsl")).into(),
        );
        let no_cull_texture_array_surface_render_pipeline_lit = pipeline::PipelineBuilder::new()
            .set_vertex_source(standard_textured_vert_shader_source, None)
            .set_frag_source(array_textures_frag_shader_source, None)
            .set_texture_format(texture_format)
            .add_vertex_buffer_layout(vertex::Position3d::layout::<0>())
            .add_vertex_buffer_layout(vertex::Color::layout::<1>())
            .add_vertex_buffer_layout(vertex::TexCoords::layout::<2>())
            .add_vertex_buffer_layout(vertex::UseTexture::layout::<3>())
            .add_vertex_buffer_layout(vertex::Normal::layout::<4>())
            .add_vertex_buffer_layout(transformation::TransformationData::layout::<5>())
            .add_vertex_buffer_layout(material::RgbMaterialData::layout::<9>())
            .add_vertex_buffer_layout(light::NormalMatrixData::layout::<10>())
            .add_bind_group_layout(&global_bind_group_layout)
            .add_bind_group_layout(&mesh_local_clip_layout)
            .add_bind_group_layout(&mesh_local_array_textured_layout)
            .set_depth_stencil(pipeline::create_depth_stencil_state())
            .set_cull_mode(TriangleFaceMode::FrontAndBack)
            .build(
                device,
                constants::NO_CULL_ARRAY_TEXTURE_MESH_PIPELINE_KEY_LIT,
            );

        let colored_vert_shader_source =
            wgpu::ShaderSource::Wgsl((include_str!("shaders/colored_vert_shader.wgsl")).into());
        let colored_frag_shader_source =
            wgpu::ShaderSource::Wgsl(include_str!("shaders/colored_frag_shader.wgsl").into());
        let colored_surface_render_pipeline = pipeline::PipelineBuilder::new()
            .set_vertex_source(colored_vert_shader_source, None)
            .set_frag_source(colored_frag_shader_source, None)
            .set_texture_format(texture_format)
            .add_vertex_buffer_layout(vertex::Position3d::layout::<0>())
            .add_vertex_buffer_layout(vertex::Color::layout::<1>())
            .add_vertex_buffer_layout(transformation::TransformationData::layout::<5>())
            .add_vertex_buffer_layout(material::RgbMaterialData::layout::<9>())
            .add_bind_group_layout(&global_bind_group_layout)
            .add_bind_group_layout(&mesh_local_clip_layout)
            .set_depth_stencil(pipeline::create_depth_stencil_state())
            .build(device, constants::VERTEX_COLORED_MESH_PIPELINE_KEY);

        let colored_vert_shader_source =
            wgpu::ShaderSource::Wgsl((include_str!("shaders/colored_vert_shader_lit.wgsl")).into());
        let colored_frag_shader_source =
            wgpu::ShaderSource::Wgsl(include_str!("shaders/lit_colored_frag_shader.wgsl").into());
        let colored_surface_render_pipeline_lit = pipeline::PipelineBuilder::new()
            .set_vertex_source(colored_vert_shader_source, None)
            .set_frag_source(colored_frag_shader_source, None)
            .set_texture_format(texture_format)
            .add_vertex_buffer_layout(vertex::Position3d::layout::<0>())
            .add_vertex_buffer_layout(vertex::Color::layout::<1>())
            .add_vertex_buffer_layout(vertex::Normal::layout::<4>())
            .add_vertex_buffer_layout(transformation::TransformationData::layout::<5>())
            .add_vertex_buffer_layout(material::RgbMaterialData::layout::<9>())
            .add_vertex_buffer_layout(light::NormalMatrixData::layout::<10>())
            .add_bind_group_layout(&global_bind_group_layout)
            .add_bind_group_layout(&mesh_local_clip_layout)
            .set_depth_stencil(pipeline::create_depth_stencil_state())
            .build(device, constants::VERTEX_COLORED_MESH_PIPELINE_KEY_LIT);

        let solid_source = wgpu::ShaderSource::Wgsl(
            (include_str!("shaders/material_color_vert_shader.wgsl")).into(),
        );
        let colored_frag_shader_source =
            wgpu::ShaderSource::Wgsl(include_str!("shaders/colored_frag_shader.wgsl").into());
        let solid_render_pipeline = pipeline::PipelineBuilder::new()
            .set_vertex_source(solid_source, None)
            .set_frag_source(colored_frag_shader_source, None)
            .set_texture_format(texture_format)
            .add_vertex_buffer_layout(vertex::Position3d::layout::<0>())
            .add_vertex_buffer_layout(transformation::TransformationData::layout::<5>())
            .add_vertex_buffer_layout(material::RgbMaterialData::layout::<9>())
            .add_bind_group_layout(&global_bind_group_layout)
            .add_bind_group_layout(&mesh_local_clip_layout)
            .set_depth_stencil(pipeline::create_depth_stencil_state())
            .build(device, constants::SOLID_COLORED_MESH_PIPELINE_KEY);

        let solid_source = wgpu::ShaderSource::Wgsl(
            (include_str!("shaders/material_color_vert_shader_lit.wgsl")).into(),
        );
        let colored_frag_shader_source =
            wgpu::ShaderSource::Wgsl(include_str!("shaders/lit_colored_frag_shader.wgsl").into());
        let solid_render_pipeline_lit = pipeline::PipelineBuilder::new()
            .set_vertex_source(solid_source, None)
            .set_frag_source(colored_frag_shader_source, None)
            .set_texture_format(texture_format)
            .add_vertex_buffer_layout(vertex::Position3d::layout::<0>())
            .add_vertex_buffer_layout(vertex::Normal::layout::<4>())
            .add_vertex_buffer_layout(transformation::TransformationData::layout::<5>())
            .add_vertex_buffer_layout(material::RgbMaterialData::layout::<9>())
            .add_vertex_buffer_layout(light::NormalMatrixData::layout::<10>())
            .add_bind_group_layout(&global_bind_group_layout)
            .add_bind_group_layout(&mesh_local_clip_layout)
            .set_depth_stencil(pipeline::create_depth_stencil_state())
            .build(device, constants::SOLID_COLORED_MESH_PIPELINE_KEY_LIT);

        let colored_vert_shader_source = wgpu::ShaderSource::Wgsl(
            (include_str!("shaders/material_color_vert_shader.wgsl")).into(),
        );
        let colored_frag_shader_source =
            wgpu::ShaderSource::Wgsl(include_str!("shaders/colored_frag_shader.wgsl").into());
        let wireframe_render_pipeline = pipeline::PipelineBuilder::new()
            .set_vertex_source(colored_vert_shader_source, None)
            .set_frag_source(colored_frag_shader_source, None)
            .set_texture_format(texture_format)
            .add_vertex_buffer_layout(vertex::Position3d::layout::<0>())
            .add_vertex_buffer_layout(transformation::TransformationData::layout::<5>())
            .add_vertex_buffer_layout(material::RgbMaterialData::layout::<9>())
            .add_bind_group_layout(&global_bind_group_layout)
            .add_bind_group_layout(&mesh_local_clip_layout)
            .set_topology(wgpu::PrimitiveTopology::LineList)
            .set_depth_stencil(pipeline::create_depth_stencil_state())
            .build(device, constants::WIREFRAME_MESH_PIPELINE_KEY);

        // No-cull variants for colored and solid meshes
        let colored_vert_shader_source =
            wgpu::ShaderSource::Wgsl((include_str!("shaders/colored_vert_shader.wgsl")).into());
        let colored_frag_shader_source =
            wgpu::ShaderSource::Wgsl(include_str!("shaders/colored_frag_shader.wgsl").into());
        let no_cull_colored_surface_render_pipeline = pipeline::PipelineBuilder::new()
            .set_vertex_source(colored_vert_shader_source, None)
            .set_frag_source(colored_frag_shader_source, None)
            .set_texture_format(texture_format)
            .add_vertex_buffer_layout(vertex::Position3d::layout::<0>())
            .add_vertex_buffer_layout(vertex::Color::layout::<1>())
            .add_vertex_buffer_layout(transformation::TransformationData::layout::<5>())
            .add_vertex_buffer_layout(material::RgbMaterialData::layout::<9>())
            .add_bind_group_layout(&global_bind_group_layout)
            .add_bind_group_layout(&mesh_local_clip_layout)
            .set_depth_stencil(pipeline::create_depth_stencil_state())
            .set_cull_mode(TriangleFaceMode::FrontAndBack)
            .build(device, constants::NO_CULL_VERTEX_COLORED_MESH_PIPELINE_KEY);

        let colored_vert_shader_source =
            wgpu::ShaderSource::Wgsl((include_str!("shaders/colored_vert_shader_lit.wgsl")).into());
        let colored_frag_shader_source =
            wgpu::ShaderSource::Wgsl(include_str!("shaders/lit_colored_frag_shader.wgsl").into());
        let no_cull_colored_surface_render_pipeline_lit = pipeline::PipelineBuilder::new()
            .set_vertex_source(colored_vert_shader_source, None)
            .set_frag_source(colored_frag_shader_source, None)
            .set_texture_format(texture_format)
            .add_vertex_buffer_layout(vertex::Position3d::layout::<0>())
            .add_vertex_buffer_layout(vertex::Color::layout::<1>())
            .add_vertex_buffer_layout(vertex::Normal::layout::<4>())
            .add_vertex_buffer_layout(transformation::TransformationData::layout::<5>())
            .add_vertex_buffer_layout(material::RgbMaterialData::layout::<9>())
            .add_vertex_buffer_layout(light::NormalMatrixData::layout::<10>())
            .add_bind_group_layout(&global_bind_group_layout)
            .add_bind_group_layout(&mesh_local_clip_layout)
            .set_depth_stencil(pipeline::create_depth_stencil_state())
            .set_cull_mode(TriangleFaceMode::FrontAndBack)
            .build(
                device,
                constants::NO_CULL_VERTEX_COLORED_MESH_PIPELINE_KEY_LIT,
            );

        let solid_source = wgpu::ShaderSource::Wgsl(
            (include_str!("shaders/material_color_vert_shader.wgsl")).into(),
        );
        let colored_frag_shader_source =
            wgpu::ShaderSource::Wgsl(include_str!("shaders/colored_frag_shader.wgsl").into());
        let no_cull_solid_render_pipeline = pipeline::PipelineBuilder::new()
            .set_vertex_source(solid_source, None)
            .set_frag_source(colored_frag_shader_source, None)
            .set_texture_format(texture_format)
            .add_vertex_buffer_layout(vertex::Position3d::layout::<0>())
            .add_vertex_buffer_layout(transformation::TransformationData::layout::<5>())
            .add_vertex_buffer_layout(material::RgbMaterialData::layout::<9>())
            .add_bind_group_layout(&global_bind_group_layout)
            .add_bind_group_layout(&mesh_local_clip_layout)
            .set_depth_stencil(pipeline::create_depth_stencil_state())
            .set_cull_mode(TriangleFaceMode::FrontAndBack)
            .build(device, constants::NO_CULL_SOLID_COLORED_MESH_PIPELINE_KEY);

        let solid_source = wgpu::ShaderSource::Wgsl(
            (include_str!("shaders/material_color_vert_shader_lit.wgsl")).into(),
        );
        let colored_frag_shader_source =
            wgpu::ShaderSource::Wgsl(include_str!("shaders/lit_colored_frag_shader.wgsl").into());
        let no_cull_solid_render_pipeline_lit = pipeline::PipelineBuilder::new()
            .set_vertex_source(solid_source, None)
            .set_frag_source(colored_frag_shader_source, None)
            .set_texture_format(texture_format)
            .add_vertex_buffer_layout(vertex::Position3d::layout::<0>())
            .add_vertex_buffer_layout(vertex::Normal::layout::<4>())
            .add_vertex_buffer_layout(transformation::TransformationData::layout::<5>())
            .add_vertex_buffer_layout(material::RgbMaterialData::layout::<9>())
            .add_vertex_buffer_layout(light::NormalMatrixData::layout::<10>())
            .add_bind_group_layout(&global_bind_group_layout)
            .add_bind_group_layout(&mesh_local_clip_layout)
            .set_depth_stencil(pipeline::create_depth_stencil_state())
            .set_cull_mode(TriangleFaceMode::FrontAndBack)
            .build(
                device,
                constants::NO_CULL_SOLID_COLORED_MESH_PIPELINE_KEY_LIT,
            );

        // Transparent pipeline variants
        // Recreate shader sources since they were moved
        let standard_textured_vert_shader_source_transparent =
            wgpu::ShaderSource::Wgsl((include_str!("shaders/textured_vert_shader.wgsl")).into());
        let single_texture_frag_shader_source_transparent =
            wgpu::ShaderSource::Wgsl((include_str!("shaders/texture_frag_shader.wgsl")).into());
        let standard_textured_vert_shader_source_lit_transparent = wgpu::ShaderSource::Wgsl(
            (include_str!("shaders/textured_vert_shader_lit.wgsl")).into(),
        );
        let single_texture_frag_shader_source_lit_transparent =
            wgpu::ShaderSource::Wgsl((include_str!("shaders/lit_texture_frag_shader.wgsl")).into());
        let array_textures_frag_shader_source_transparent = wgpu::ShaderSource::Wgsl(
            (include_str!("shaders/array_textures_frag_shader.wgsl")).into(),
        );
        let array_textures_frag_shader_source_lit_transparent = wgpu::ShaderSource::Wgsl(
            (include_str!("shaders/lit_array_textures_frag_shader.wgsl")).into(),
        );
        let colored_vert_shader_source_transparent =
            wgpu::ShaderSource::Wgsl((include_str!("shaders/colored_vert_shader.wgsl")).into());
        let colored_frag_shader_source_transparent =
            wgpu::ShaderSource::Wgsl(include_str!("shaders/colored_frag_shader.wgsl").into());
        let colored_vert_shader_source_lit_transparent =
            wgpu::ShaderSource::Wgsl((include_str!("shaders/colored_vert_shader_lit.wgsl")).into());
        let colored_frag_shader_source_lit_transparent =
            wgpu::ShaderSource::Wgsl(include_str!("shaders/lit_colored_frag_shader.wgsl").into());
        let solid_vert_shader_source_transparent = wgpu::ShaderSource::Wgsl(
            (include_str!("shaders/material_color_vert_shader.wgsl")).into(),
        );
        let solid_vert_shader_source_lit_transparent = wgpu::ShaderSource::Wgsl(
            (include_str!("shaders/material_color_vert_shader_lit.wgsl")).into(),
        );

        let transparent_texture_surface_render_pipeline = pipeline::PipelineBuilder::new()
            .set_vertex_source(standard_textured_vert_shader_source_transparent, None)
            .set_frag_source(single_texture_frag_shader_source_transparent, None)
            .set_texture_format(texture_format)
            .add_vertex_buffer_layout(vertex::Position3d::layout::<0>())
            .add_vertex_buffer_layout(vertex::Color::layout::<1>())
            .add_vertex_buffer_layout(vertex::TexCoords::layout::<2>())
            .add_vertex_buffer_layout(vertex::UseTexture::layout::<3>())
            .add_vertex_buffer_layout(transformation::TransformationData::layout::<5>())
            .add_vertex_buffer_layout(material::RgbMaterialData::layout::<9>())
            .add_bind_group_layout(&global_bind_group_layout)
            .add_bind_group_layout(&mesh_local_clip_layout)
            .add_bind_group_layout(&mesh_local_textured_layout)
            .set_depth_stencil(pipeline::create_depth_stencil_state_transparent())
            .set_blend_state(pipeline::create_alpha_blend_state())
            .build(device, constants::TRANSPARENT_TEXTURE_MESH_PIPELINE_KEY);

        let transparent_texture_surface_render_pipeline_lit = pipeline::PipelineBuilder::new()
            .set_vertex_source(standard_textured_vert_shader_source_lit_transparent, None)
            .set_frag_source(single_texture_frag_shader_source_lit_transparent, None)
            .set_texture_format(texture_format)
            .add_vertex_buffer_layout(vertex::Position3d::layout::<0>())
            .add_vertex_buffer_layout(vertex::Color::layout::<1>())
            .add_vertex_buffer_layout(vertex::TexCoords::layout::<2>())
            .add_vertex_buffer_layout(vertex::UseTexture::layout::<3>())
            .add_vertex_buffer_layout(vertex::Normal::layout::<4>())
            .add_vertex_buffer_layout(transformation::TransformationData::layout::<5>())
            .add_vertex_buffer_layout(material::RgbMaterialData::layout::<9>())
            .add_vertex_buffer_layout(light::NormalMatrixData::layout::<10>())
            .add_bind_group_layout(&global_bind_group_layout)
            .add_bind_group_layout(&mesh_local_clip_layout)
            .add_bind_group_layout(&mesh_local_textured_layout)
            .set_depth_stencil(pipeline::create_depth_stencil_state_transparent())
            .set_blend_state(pipeline::create_alpha_blend_state())
            .build(device, constants::TRANSPARENT_TEXTURE_MESH_PIPELINE_KEY_LIT);

        let transparent_texture_array_surface_render_pipeline = pipeline::PipelineBuilder::new()
            .set_vertex_source(
                wgpu::ShaderSource::Wgsl(
                    (include_str!("shaders/textured_vert_shader.wgsl")).into(),
                ),
                None,
            )
            .set_frag_source(array_textures_frag_shader_source_transparent, None)
            .set_texture_format(texture_format)
            .add_vertex_buffer_layout(vertex::Position3d::layout::<0>())
            .add_vertex_buffer_layout(vertex::Color::layout::<1>())
            .add_vertex_buffer_layout(vertex::TexCoords::layout::<2>())
            .add_vertex_buffer_layout(vertex::UseTexture::layout::<3>())
            .add_vertex_buffer_layout(transformation::TransformationData::layout::<5>())
            .add_vertex_buffer_layout(material::RgbMaterialData::layout::<9>())
            .add_bind_group_layout(&global_bind_group_layout)
            .add_bind_group_layout(&mesh_local_clip_layout)
            .add_bind_group_layout(&mesh_local_array_textured_layout)
            .set_depth_stencil(pipeline::create_depth_stencil_state_transparent())
            .set_blend_state(pipeline::create_alpha_blend_state())
            .build(
                device,
                constants::TRANSPARENT_ARRAY_TEXTURE_MESH_PIPELINE_KEY,
            );

        let transparent_texture_array_surface_render_pipeline_lit =
            pipeline::PipelineBuilder::new()
                .set_vertex_source(
                    wgpu::ShaderSource::Wgsl(
                        (include_str!("shaders/textured_vert_shader_lit.wgsl")).into(),
                    ),
                    None,
                )
                .set_frag_source(array_textures_frag_shader_source_lit_transparent, None)
                .set_texture_format(texture_format)
                .add_vertex_buffer_layout(vertex::Position3d::layout::<0>())
                .add_vertex_buffer_layout(vertex::Color::layout::<1>())
                .add_vertex_buffer_layout(vertex::TexCoords::layout::<2>())
                .add_vertex_buffer_layout(vertex::UseTexture::layout::<3>())
                .add_vertex_buffer_layout(vertex::Normal::layout::<4>())
                .add_vertex_buffer_layout(transformation::TransformationData::layout::<5>())
                .add_vertex_buffer_layout(material::RgbMaterialData::layout::<9>())
                .add_vertex_buffer_layout(light::NormalMatrixData::layout::<10>())
                .add_bind_group_layout(&global_bind_group_layout)
                .add_bind_group_layout(&mesh_local_clip_layout)
                .add_bind_group_layout(&mesh_local_array_textured_layout)
                .set_depth_stencil(pipeline::create_depth_stencil_state_transparent())
                .set_blend_state(pipeline::create_alpha_blend_state())
                .build(
                    device,
                    constants::TRANSPARENT_ARRAY_TEXTURE_MESH_PIPELINE_KEY_LIT,
                );

        let transparent_colored_surface_render_pipeline = pipeline::PipelineBuilder::new()
            .set_vertex_source(colored_vert_shader_source_transparent.clone(), None)
            .set_frag_source(colored_frag_shader_source_transparent.clone(), None)
            .set_texture_format(texture_format)
            .add_vertex_buffer_layout(vertex::Position3d::layout::<0>())
            .add_vertex_buffer_layout(vertex::Color::layout::<1>())
            .add_vertex_buffer_layout(transformation::TransformationData::layout::<5>())
            .add_vertex_buffer_layout(material::RgbMaterialData::layout::<9>())
            .add_bind_group_layout(&global_bind_group_layout)
            .add_bind_group_layout(&mesh_local_clip_layout)
            .set_depth_stencil(pipeline::create_depth_stencil_state_transparent())
            .set_blend_state(pipeline::create_alpha_blend_state())
            .build(
                device,
                constants::TRANSPARENT_VERTEX_COLORED_MESH_PIPELINE_KEY,
            );

        let transparent_colored_surface_render_pipeline_lit = pipeline::PipelineBuilder::new()
            .set_vertex_source(colored_vert_shader_source_lit_transparent.clone(), None)
            .set_frag_source(colored_frag_shader_source_lit_transparent.clone(), None)
            .set_texture_format(texture_format)
            .add_vertex_buffer_layout(vertex::Position3d::layout::<0>())
            .add_vertex_buffer_layout(vertex::Color::layout::<1>())
            .add_vertex_buffer_layout(vertex::Normal::layout::<4>())
            .add_vertex_buffer_layout(transformation::TransformationData::layout::<5>())
            .add_vertex_buffer_layout(material::RgbMaterialData::layout::<9>())
            .add_vertex_buffer_layout(light::NormalMatrixData::layout::<10>())
            .add_bind_group_layout(&global_bind_group_layout)
            .add_bind_group_layout(&mesh_local_clip_layout)
            .set_depth_stencil(pipeline::create_depth_stencil_state_transparent())
            .set_blend_state(pipeline::create_alpha_blend_state())
            .build(
                device,
                constants::TRANSPARENT_VERTEX_COLORED_MESH_PIPELINE_KEY_LIT,
            );

        let transparent_solid_render_pipeline = pipeline::PipelineBuilder::new()
            .set_vertex_source(solid_vert_shader_source_transparent.clone(), None)
            .set_frag_source(colored_frag_shader_source_transparent.clone(), None)
            .set_texture_format(texture_format)
            .add_vertex_buffer_layout(vertex::Position3d::layout::<0>())
            .add_vertex_buffer_layout(transformation::TransformationData::layout::<5>())
            .add_vertex_buffer_layout(material::RgbMaterialData::layout::<9>())
            .add_bind_group_layout(&global_bind_group_layout)
            .add_bind_group_layout(&mesh_local_clip_layout)
            .set_depth_stencil(pipeline::create_depth_stencil_state_transparent())
            .set_blend_state(pipeline::create_alpha_blend_state())
            .build(
                device,
                constants::TRANSPARENT_SOLID_COLORED_MESH_PIPELINE_KEY,
            );

        let transparent_solid_render_pipeline_lit = pipeline::PipelineBuilder::new()
            .set_vertex_source(solid_vert_shader_source_lit_transparent, None)
            .set_frag_source(colored_frag_shader_source_lit_transparent.clone(), None)
            .set_texture_format(texture_format)
            .add_vertex_buffer_layout(vertex::Position3d::layout::<0>())
            .add_vertex_buffer_layout(vertex::Normal::layout::<4>())
            .add_vertex_buffer_layout(transformation::TransformationData::layout::<5>())
            .add_vertex_buffer_layout(material::RgbMaterialData::layout::<9>())
            .add_vertex_buffer_layout(light::NormalMatrixData::layout::<10>())
            .add_bind_group_layout(&global_bind_group_layout)
            .add_bind_group_layout(&mesh_local_clip_layout)
            .set_depth_stencil(pipeline::create_depth_stencil_state_transparent())
            .set_blend_state(pipeline::create_alpha_blend_state())
            .build(
                device,
                constants::TRANSPARENT_SOLID_COLORED_MESH_PIPELINE_KEY_LIT,
            );

        // Recreate material color shader sources for transparent wireframe
        let material_color_vert_source_transparent = wgpu::ShaderSource::Wgsl(
            (include_str!("shaders/material_color_vert_shader.wgsl")).into(),
        );

        let transparent_wireframe_render_pipeline = pipeline::PipelineBuilder::new()
            .set_vertex_source(material_color_vert_source_transparent, None)
            .set_frag_source(colored_frag_shader_source_transparent.clone(), None)
            .set_texture_format(texture_format)
            .add_vertex_buffer_layout(vertex::Position3d::layout::<0>())
            .add_vertex_buffer_layout(transformation::TransformationData::layout::<5>())
            .add_vertex_buffer_layout(material::RgbMaterialData::layout::<9>())
            .add_bind_group_layout(&global_bind_group_layout)
            .add_bind_group_layout(&mesh_local_clip_layout)
            .set_topology(wgpu::PrimitiveTopology::LineList)
            .set_depth_stencil(pipeline::create_depth_stencil_state_transparent())
            .set_blend_state(pipeline::create_alpha_blend_state())
            .build(device, constants::TRANSPARENT_WIREFRAME_MESH_PIPELINE_KEY);

        // No-cull transparent pipeline variants
        let standard_textured_vert_shader_source_no_cull =
            wgpu::ShaderSource::Wgsl((include_str!("shaders/textured_vert_shader.wgsl")).into());
        let single_texture_frag_shader_source_no_cull =
            wgpu::ShaderSource::Wgsl((include_str!("shaders/texture_frag_shader.wgsl")).into());
        let no_cull_transparent_texture_surface_render_pipeline = pipeline::PipelineBuilder::new()
            .set_vertex_source(standard_textured_vert_shader_source_no_cull, None)
            .set_frag_source(single_texture_frag_shader_source_no_cull, None)
            .set_texture_format(texture_format)
            .add_vertex_buffer_layout(vertex::Position3d::layout::<0>())
            .add_vertex_buffer_layout(vertex::Color::layout::<1>())
            .add_vertex_buffer_layout(vertex::TexCoords::layout::<2>())
            .add_vertex_buffer_layout(vertex::UseTexture::layout::<3>())
            .add_vertex_buffer_layout(transformation::TransformationData::layout::<5>())
            .add_vertex_buffer_layout(material::RgbMaterialData::layout::<9>())
            .add_bind_group_layout(&global_bind_group_layout)
            .add_bind_group_layout(&mesh_local_clip_layout)
            .add_bind_group_layout(&mesh_local_textured_layout)
            .set_depth_stencil(pipeline::create_depth_stencil_state_transparent())
            .set_blend_state(pipeline::create_alpha_blend_state())
            .set_cull_mode(TriangleFaceMode::FrontAndBack)
            .build(
                device,
                constants::NO_CULL_TRANSPARENT_TEXTURE_MESH_PIPELINE_KEY,
            );

        let standard_textured_vert_shader_source_lit_no_cull = wgpu::ShaderSource::Wgsl(
            (include_str!("shaders/textured_vert_shader_lit.wgsl")).into(),
        );
        let single_texture_frag_shader_source_lit_no_cull =
            wgpu::ShaderSource::Wgsl((include_str!("shaders/lit_texture_frag_shader.wgsl")).into());
        let no_cull_transparent_texture_surface_render_pipeline_lit =
            pipeline::PipelineBuilder::new()
                .set_vertex_source(standard_textured_vert_shader_source_lit_no_cull, None)
                .set_frag_source(single_texture_frag_shader_source_lit_no_cull, None)
                .set_texture_format(texture_format)
                .add_vertex_buffer_layout(vertex::Position3d::layout::<0>())
                .add_vertex_buffer_layout(vertex::Color::layout::<1>())
                .add_vertex_buffer_layout(vertex::TexCoords::layout::<2>())
                .add_vertex_buffer_layout(vertex::UseTexture::layout::<3>())
                .add_vertex_buffer_layout(vertex::Normal::layout::<4>())
                .add_vertex_buffer_layout(transformation::TransformationData::layout::<5>())
                .add_vertex_buffer_layout(material::RgbMaterialData::layout::<9>())
                .add_vertex_buffer_layout(light::NormalMatrixData::layout::<10>())
                .add_bind_group_layout(&global_bind_group_layout)
                .add_bind_group_layout(&mesh_local_clip_layout)
                .add_bind_group_layout(&mesh_local_textured_layout)
                .set_depth_stencil(pipeline::create_depth_stencil_state_transparent())
                .set_blend_state(pipeline::create_alpha_blend_state())
                .set_cull_mode(TriangleFaceMode::FrontAndBack)
                .build(
                    device,
                    constants::NO_CULL_TRANSPARENT_TEXTURE_MESH_PIPELINE_KEY_LIT,
                );

        let array_textures_frag_shader_source_no_cull = wgpu::ShaderSource::Wgsl(
            (include_str!("shaders/array_textures_frag_shader.wgsl")).into(),
        );
        let no_cull_transparent_texture_array_surface_render_pipeline =
            pipeline::PipelineBuilder::new()
                .set_vertex_source(
                    wgpu::ShaderSource::Wgsl(
                        (include_str!("shaders/textured_vert_shader.wgsl")).into(),
                    ),
                    None,
                )
                .set_frag_source(array_textures_frag_shader_source_no_cull, None)
                .set_texture_format(texture_format)
                .add_vertex_buffer_layout(vertex::Position3d::layout::<0>())
                .add_vertex_buffer_layout(vertex::Color::layout::<1>())
                .add_vertex_buffer_layout(vertex::TexCoords::layout::<2>())
                .add_vertex_buffer_layout(vertex::UseTexture::layout::<3>())
                .add_vertex_buffer_layout(transformation::TransformationData::layout::<5>())
                .add_vertex_buffer_layout(material::RgbMaterialData::layout::<9>())
                .add_bind_group_layout(&global_bind_group_layout)
                .add_bind_group_layout(&mesh_local_clip_layout)
                .add_bind_group_layout(&mesh_local_array_textured_layout)
                .set_depth_stencil(pipeline::create_depth_stencil_state_transparent())
                .set_blend_state(pipeline::create_alpha_blend_state())
                .set_cull_mode(TriangleFaceMode::FrontAndBack)
                .build(
                    device,
                    constants::NO_CULL_TRANSPARENT_ARRAY_TEXTURE_MESH_PIPELINE_KEY,
                );

        let array_textures_frag_shader_source_lit_no_cull = wgpu::ShaderSource::Wgsl(
            (include_str!("shaders/lit_array_textures_frag_shader.wgsl")).into(),
        );
        let no_cull_transparent_texture_array_surface_render_pipeline_lit =
            pipeline::PipelineBuilder::new()
                .set_vertex_source(
                    wgpu::ShaderSource::Wgsl(
                        (include_str!("shaders/textured_vert_shader_lit.wgsl")).into(),
                    ),
                    None,
                )
                .set_frag_source(array_textures_frag_shader_source_lit_no_cull, None)
                .set_texture_format(texture_format)
                .add_vertex_buffer_layout(vertex::Position3d::layout::<0>())
                .add_vertex_buffer_layout(vertex::Color::layout::<1>())
                .add_vertex_buffer_layout(vertex::TexCoords::layout::<2>())
                .add_vertex_buffer_layout(vertex::UseTexture::layout::<3>())
                .add_vertex_buffer_layout(vertex::Normal::layout::<4>())
                .add_vertex_buffer_layout(transformation::TransformationData::layout::<5>())
                .add_vertex_buffer_layout(material::RgbMaterialData::layout::<9>())
                .add_vertex_buffer_layout(light::NormalMatrixData::layout::<10>())
                .add_bind_group_layout(&global_bind_group_layout)
                .add_bind_group_layout(&mesh_local_clip_layout)
                .add_bind_group_layout(&mesh_local_array_textured_layout)
                .set_depth_stencil(pipeline::create_depth_stencil_state_transparent())
                .set_blend_state(pipeline::create_alpha_blend_state())
                .set_cull_mode(TriangleFaceMode::FrontAndBack)
                .build(
                    device,
                    constants::NO_CULL_TRANSPARENT_ARRAY_TEXTURE_MESH_PIPELINE_KEY_LIT,
                );

        let colored_vert_shader_source_no_cull =
            wgpu::ShaderSource::Wgsl((include_str!("shaders/colored_vert_shader.wgsl")).into());
        let colored_frag_shader_source_no_cull =
            wgpu::ShaderSource::Wgsl(include_str!("shaders/colored_frag_shader.wgsl").into());
        let no_cull_transparent_colored_surface_render_pipeline = pipeline::PipelineBuilder::new()
            .set_vertex_source(colored_vert_shader_source_no_cull, None)
            .set_frag_source(colored_frag_shader_source_no_cull, None)
            .set_texture_format(texture_format)
            .add_vertex_buffer_layout(vertex::Position3d::layout::<0>())
            .add_vertex_buffer_layout(vertex::Color::layout::<1>())
            .add_vertex_buffer_layout(transformation::TransformationData::layout::<5>())
            .add_vertex_buffer_layout(material::RgbMaterialData::layout::<9>())
            .add_bind_group_layout(&global_bind_group_layout)
            .add_bind_group_layout(&mesh_local_clip_layout)
            .set_depth_stencil(pipeline::create_depth_stencil_state_transparent())
            .set_blend_state(pipeline::create_alpha_blend_state())
            .set_cull_mode(TriangleFaceMode::FrontAndBack)
            .build(
                device,
                constants::NO_CULL_TRANSPARENT_VERTEX_COLORED_MESH_PIPELINE_KEY,
            );

        let colored_vert_shader_source_lit_no_cull =
            wgpu::ShaderSource::Wgsl((include_str!("shaders/colored_vert_shader_lit.wgsl")).into());
        let colored_frag_shader_source_lit_no_cull =
            wgpu::ShaderSource::Wgsl(include_str!("shaders/lit_colored_frag_shader.wgsl").into());
        let no_cull_transparent_colored_surface_render_pipeline_lit =
            pipeline::PipelineBuilder::new()
                .set_vertex_source(colored_vert_shader_source_lit_no_cull, None)
                .set_frag_source(colored_frag_shader_source_lit_no_cull, None)
                .set_texture_format(texture_format)
                .add_vertex_buffer_layout(vertex::Position3d::layout::<0>())
                .add_vertex_buffer_layout(vertex::Color::layout::<1>())
                .add_vertex_buffer_layout(vertex::Normal::layout::<4>())
                .add_vertex_buffer_layout(transformation::TransformationData::layout::<5>())
                .add_vertex_buffer_layout(material::RgbMaterialData::layout::<9>())
                .add_vertex_buffer_layout(light::NormalMatrixData::layout::<10>())
                .add_bind_group_layout(&global_bind_group_layout)
                .add_bind_group_layout(&mesh_local_clip_layout)
                .set_depth_stencil(pipeline::create_depth_stencil_state_transparent())
                .set_blend_state(pipeline::create_alpha_blend_state())
                .set_cull_mode(TriangleFaceMode::FrontAndBack)
                .build(
                    device,
                    constants::NO_CULL_TRANSPARENT_VERTEX_COLORED_MESH_PIPELINE_KEY_LIT,
                );

        let solid_vert_shader_source_no_cull = wgpu::ShaderSource::Wgsl(
            (include_str!("shaders/material_color_vert_shader.wgsl")).into(),
        );
        let solid_frag_shader_source_no_cull =
            wgpu::ShaderSource::Wgsl(include_str!("shaders/colored_frag_shader.wgsl").into());
        let no_cull_transparent_solid_render_pipeline = pipeline::PipelineBuilder::new()
            .set_vertex_source(solid_vert_shader_source_no_cull, None)
            .set_frag_source(solid_frag_shader_source_no_cull, None)
            .set_texture_format(texture_format)
            .add_vertex_buffer_layout(vertex::Position3d::layout::<0>())
            .add_vertex_buffer_layout(transformation::TransformationData::layout::<5>())
            .add_vertex_buffer_layout(material::RgbMaterialData::layout::<9>())
            .add_bind_group_layout(&global_bind_group_layout)
            .add_bind_group_layout(&mesh_local_clip_layout)
            .set_depth_stencil(pipeline::create_depth_stencil_state_transparent())
            .set_blend_state(pipeline::create_alpha_blend_state())
            .set_cull_mode(TriangleFaceMode::FrontAndBack)
            .build(
                device,
                constants::NO_CULL_TRANSPARENT_SOLID_COLORED_MESH_PIPELINE_KEY,
            );

        let solid_vert_shader_source_lit_no_cull = wgpu::ShaderSource::Wgsl(
            (include_str!("shaders/material_color_vert_shader_lit.wgsl")).into(),
        );
        let solid_frag_shader_source_lit_no_cull =
            wgpu::ShaderSource::Wgsl(include_str!("shaders/lit_colored_frag_shader.wgsl").into());
        let no_cull_transparent_solid_render_pipeline_lit = pipeline::PipelineBuilder::new()
            .set_vertex_source(solid_vert_shader_source_lit_no_cull, None)
            .set_frag_source(solid_frag_shader_source_lit_no_cull, None)
            .set_texture_format(texture_format)
            .add_vertex_buffer_layout(vertex::Position3d::layout::<0>())
            .add_vertex_buffer_layout(vertex::Normal::layout::<4>())
            .add_vertex_buffer_layout(transformation::TransformationData::layout::<5>())
            .add_vertex_buffer_layout(material::RgbMaterialData::layout::<9>())
            .add_vertex_buffer_layout(light::NormalMatrixData::layout::<10>())
            .add_bind_group_layout(&global_bind_group_layout)
            .add_bind_group_layout(&mesh_local_clip_layout)
            .set_depth_stencil(pipeline::create_depth_stencil_state_transparent())
            .set_blend_state(pipeline::create_alpha_blend_state())
            .set_cull_mode(TriangleFaceMode::FrontAndBack)
            .build(
                device,
                constants::NO_CULL_TRANSPARENT_SOLID_COLORED_MESH_PIPELINE_KEY_LIT,
            );

        let basic_vert_source =
            wgpu::ShaderSource::Wgsl((include_str!("shaders/basic_vert_shader.wgsl")).into());
        let silhoutte_frag_shader_source = wgpu::ShaderSource::Wgsl(
            include_str!("shaders/silhoutte_mask_frag_shader.wgsl").into(),
        );
        let silhoutte_render_pipeline = pipeline::PipelineBuilder::new()
            .set_vertex_source(basic_vert_source, None)
            .set_frag_source(silhoutte_frag_shader_source, None)
            .set_texture_format(wgpu::TextureFormat::R8Unorm)
            .add_vertex_buffer_layout(vertex::Position3d::layout::<0>())
            .add_vertex_buffer_layout(transformation::TransformationData::layout::<5>())
            .add_bind_group_layout(&global_bind_group_layout)
            .add_bind_group_layout(&mesh_local_clip_layout)
            .set_depth_stencil(pipeline::create_depth_stencil_state())
            .build(device, constants::MESH_SILHOUETTE_PIPELINE_KEY);

        let comp_bind_group_layout = composite::get_composite_pipeline_bind_group_layout(device);
        let comp_vert_source =
            wgpu::ShaderSource::Wgsl((include_str!("shaders/composite_vert_shader.wgsl")).into());
        let comp_frag_shader_source =
            wgpu::ShaderSource::Wgsl(include_str!("shaders/composite_frag_shader.wgsl").into());
        let comp_render_pipeline = pipeline::PipelineBuilder::new()
            .set_vertex_source(comp_vert_source, None)
            .set_frag_source(comp_frag_shader_source, None)
            .set_texture_format(texture_format)
            .add_bind_group_layout(&comp_bind_group_layout)
            .build(device, constants::COMPOSITE_PASS_PIPELINE);

        let screen_space_vert_source = wgpu::ShaderSource::Wgsl(
            (include_str!("shaders/screen_space_vert_shader.wgsl")).into(),
        );
        let colored_frag_source =
            wgpu::ShaderSource::Wgsl(include_str!("shaders/colored_frag_shader.wgsl").into());
        let screen_space_colored_mesh_render_pipeline = pipeline::PipelineBuilder::new()
            .set_vertex_source(screen_space_vert_source, None)
            .set_frag_source(colored_frag_source, None)
            .set_texture_format(texture_format)
            .add_vertex_buffer_layout(vertex::Position3d::layout::<0>())
            .add_vertex_buffer_layout(vertex::Color::layout::<1>())
            .add_vertex_buffer_layout(transformation::TransformationData::layout::<5>())
            .add_vertex_buffer_layout(material::RgbMaterialData::layout::<9>())
            .add_vertex_buffer_layout(material::UseMaterialData::layout::<12>())
            .add_vertex_buffer_layout(screen_space::SizeInPixel::layout::<13>())
            .add_bind_group_layout(&global_bind_group_layout)
            .add_bind_group_layout(&mesh_local_clip_layout)
            .set_depth_stencil(pipeline::create_depth_stencil_state())
            .build(device, constants::SCREEN_SPACE_MESH_PIPELINE_KEY);

        let screen_space_vert_source = wgpu::ShaderSource::Wgsl(
            (include_str!("shaders/screen_space_vert_shader.wgsl")).into(),
        );
        let colored_frag_source =
            wgpu::ShaderSource::Wgsl(include_str!("shaders/colored_frag_shader.wgsl").into());
        let screen_space_wireframe_mesh_render_pipeline = pipeline::PipelineBuilder::new()
            .set_vertex_source(screen_space_vert_source, None)
            .set_frag_source(colored_frag_source, None)
            .set_texture_format(texture_format)
            .add_vertex_buffer_layout(vertex::Position3d::layout::<0>())
            .add_vertex_buffer_layout(vertex::Color::layout::<1>())
            .add_vertex_buffer_layout(transformation::TransformationData::layout::<5>())
            .add_vertex_buffer_layout(material::RgbMaterialData::layout::<9>())
            .add_vertex_buffer_layout(material::UseMaterialData::layout::<12>())
            .add_vertex_buffer_layout(screen_space::SizeInPixel::layout::<13>())
            .add_bind_group_layout(&global_bind_group_layout)
            .add_bind_group_layout(&mesh_local_clip_layout)
            .set_topology(wgpu::PrimitiveTopology::LineList)
            .set_depth_stencil(pipeline::create_depth_stencil_state())
            .build(device, constants::SCREEN_SPACE_WIREFRAME_PIPELINE_KEY);

        let screen_space_vert_source = wgpu::ShaderSource::Wgsl(
            (include_str!("shaders/screen_space_vert_shader.wgsl")).into(),
        );
        let colored_frag_source = wgpu::ShaderSource::Wgsl(
            include_str!("shaders/screen_space_colored_frag_shader.wgsl").into(),
        );
        let screen_space_colored_mesh_render_pipeline_without_depth =
            pipeline::PipelineBuilder::new()
                .set_vertex_source(screen_space_vert_source, None)
                .set_frag_source(colored_frag_source, None)
                .set_texture_format(texture_format)
                .add_vertex_buffer_layout(vertex::Position3d::layout::<0>())
                .add_vertex_buffer_layout(vertex::Color::layout::<1>())
                .add_vertex_buffer_layout(transformation::TransformationData::layout::<5>())
                .add_vertex_buffer_layout(material::RgbMaterialData::layout::<9>())
                .add_vertex_buffer_layout(material::UseMaterialData::layout::<12>())
                .add_vertex_buffer_layout(screen_space::SizeInPixel::layout::<13>())
                .add_bind_group_layout(&global_bind_group_layout)
                .add_bind_group_layout(&mesh_local_clip_layout)
                .build(
                    device,
                    constants::SCREEN_SPACE_MESH_WITHOUT_DEPTH_PIPELINE_KEY,
                );

        let screen_space_vert_source = wgpu::ShaderSource::Wgsl(
            (include_str!("shaders/screen_space_vert_shader.wgsl")).into(),
        );
        let colored_frag_source = wgpu::ShaderSource::Wgsl(
            include_str!("shaders/screen_space_colored_frag_shader.wgsl").into(),
        );
        let screen_space_wireframe_mesh_render_pipeline_without_depth =
            pipeline::PipelineBuilder::new()
                .set_vertex_source(screen_space_vert_source, None)
                .set_frag_source(colored_frag_source, None)
                .set_texture_format(texture_format)
                .add_vertex_buffer_layout(vertex::Position3d::layout::<0>())
                .add_vertex_buffer_layout(vertex::Color::layout::<1>())
                .add_vertex_buffer_layout(transformation::TransformationData::layout::<5>())
                .add_vertex_buffer_layout(material::RgbMaterialData::layout::<9>())
                .add_vertex_buffer_layout(material::UseMaterialData::layout::<12>())
                .add_vertex_buffer_layout(screen_space::SizeInPixel::layout::<13>())
                .add_bind_group_layout(&global_bind_group_layout)
                .add_bind_group_layout(&mesh_local_clip_layout)
                .set_topology(wgpu::PrimitiveTopology::LineList)
                .build(
                    device,
                    constants::SCREEN_SPACE_WIREFRAME_WITHOUT_DEPTH_PIPELINE_KEY,
                );

        let mut render_pipeline_cache = HashMap::new();
        render_pipeline_cache.insert(
            constants::TEXTURE_MESH_PIPELINE_KEY,
            texture_surface_render_pipeline,
        );
        render_pipeline_cache.insert(
            constants::TEXTURE_MESH_PIPELINE_KEY_LIT,
            texture_surface_render_pipeline_lit,
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
            constants::VERTEX_COLORED_MESH_PIPELINE_KEY_LIT,
            colored_surface_render_pipeline_lit,
        );
        render_pipeline_cache.insert(
            constants::SOLID_COLORED_MESH_PIPELINE_KEY,
            solid_render_pipeline,
        );
        render_pipeline_cache.insert(
            constants::SOLID_COLORED_MESH_PIPELINE_KEY_LIT,
            solid_render_pipeline_lit,
        );
        render_pipeline_cache.insert(
            constants::ARRAY_TEXTURE_MESH_PIPELINE_KEY,
            texture_array_surface_render_pipeline,
        );
        render_pipeline_cache.insert(
            constants::ARRAY_TEXTURE_MESH_PIPELINE_KEY_LIT,
            texture_array_surface_render_pipeline_lit,
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
        render_pipeline_cache.insert(
            constants::SCREEN_SPACE_MESH_WITHOUT_DEPTH_PIPELINE_KEY,
            screen_space_colored_mesh_render_pipeline_without_depth,
        );
        render_pipeline_cache.insert(
            constants::SCREEN_SPACE_WIREFRAME_WITHOUT_DEPTH_PIPELINE_KEY,
            screen_space_wireframe_mesh_render_pipeline_without_depth,
        );

        // Insert transparent pipelines into cache
        render_pipeline_cache.insert(
            constants::TRANSPARENT_TEXTURE_MESH_PIPELINE_KEY,
            transparent_texture_surface_render_pipeline,
        );
        render_pipeline_cache.insert(
            constants::TRANSPARENT_TEXTURE_MESH_PIPELINE_KEY_LIT,
            transparent_texture_surface_render_pipeline_lit,
        );
        render_pipeline_cache.insert(
            constants::TRANSPARENT_ARRAY_TEXTURE_MESH_PIPELINE_KEY,
            transparent_texture_array_surface_render_pipeline,
        );
        render_pipeline_cache.insert(
            constants::TRANSPARENT_ARRAY_TEXTURE_MESH_PIPELINE_KEY_LIT,
            transparent_texture_array_surface_render_pipeline_lit,
        );
        render_pipeline_cache.insert(
            constants::TRANSPARENT_VERTEX_COLORED_MESH_PIPELINE_KEY,
            transparent_colored_surface_render_pipeline,
        );
        render_pipeline_cache.insert(
            constants::TRANSPARENT_VERTEX_COLORED_MESH_PIPELINE_KEY_LIT,
            transparent_colored_surface_render_pipeline_lit,
        );
        render_pipeline_cache.insert(
            constants::TRANSPARENT_SOLID_COLORED_MESH_PIPELINE_KEY,
            transparent_solid_render_pipeline,
        );
        render_pipeline_cache.insert(
            constants::TRANSPARENT_SOLID_COLORED_MESH_PIPELINE_KEY_LIT,
            transparent_solid_render_pipeline_lit,
        );
        render_pipeline_cache.insert(
            constants::TRANSPARENT_WIREFRAME_MESH_PIPELINE_KEY,
            transparent_wireframe_render_pipeline,
        );

        // Insert no-cull pipelines into cache
        render_pipeline_cache.insert(
            constants::NO_CULL_TEXTURE_MESH_PIPELINE_KEY,
            no_cull_texture_surface_render_pipeline,
        );
        render_pipeline_cache.insert(
            constants::NO_CULL_TEXTURE_MESH_PIPELINE_KEY_LIT,
            no_cull_texture_surface_render_pipeline_lit,
        );
        render_pipeline_cache.insert(
            constants::NO_CULL_ARRAY_TEXTURE_MESH_PIPELINE_KEY,
            no_cull_texture_array_surface_render_pipeline,
        );
        render_pipeline_cache.insert(
            constants::NO_CULL_ARRAY_TEXTURE_MESH_PIPELINE_KEY_LIT,
            no_cull_texture_array_surface_render_pipeline_lit,
        );
        render_pipeline_cache.insert(
            constants::NO_CULL_VERTEX_COLORED_MESH_PIPELINE_KEY,
            no_cull_colored_surface_render_pipeline,
        );
        render_pipeline_cache.insert(
            constants::NO_CULL_VERTEX_COLORED_MESH_PIPELINE_KEY_LIT,
            no_cull_colored_surface_render_pipeline_lit,
        );
        render_pipeline_cache.insert(
            constants::NO_CULL_SOLID_COLORED_MESH_PIPELINE_KEY,
            no_cull_solid_render_pipeline,
        );
        render_pipeline_cache.insert(
            constants::NO_CULL_SOLID_COLORED_MESH_PIPELINE_KEY_LIT,
            no_cull_solid_render_pipeline_lit,
        );
        render_pipeline_cache.insert(
            constants::NO_CULL_TRANSPARENT_TEXTURE_MESH_PIPELINE_KEY,
            no_cull_transparent_texture_surface_render_pipeline,
        );
        render_pipeline_cache.insert(
            constants::NO_CULL_TRANSPARENT_TEXTURE_MESH_PIPELINE_KEY_LIT,
            no_cull_transparent_texture_surface_render_pipeline_lit,
        );
        render_pipeline_cache.insert(
            constants::NO_CULL_TRANSPARENT_ARRAY_TEXTURE_MESH_PIPELINE_KEY,
            no_cull_transparent_texture_array_surface_render_pipeline,
        );
        render_pipeline_cache.insert(
            constants::NO_CULL_TRANSPARENT_ARRAY_TEXTURE_MESH_PIPELINE_KEY_LIT,
            no_cull_transparent_texture_array_surface_render_pipeline_lit,
        );
        render_pipeline_cache.insert(
            constants::NO_CULL_TRANSPARENT_VERTEX_COLORED_MESH_PIPELINE_KEY,
            no_cull_transparent_colored_surface_render_pipeline,
        );
        render_pipeline_cache.insert(
            constants::NO_CULL_TRANSPARENT_VERTEX_COLORED_MESH_PIPELINE_KEY_LIT,
            no_cull_transparent_colored_surface_render_pipeline_lit,
        );
        render_pipeline_cache.insert(
            constants::NO_CULL_TRANSPARENT_SOLID_COLORED_MESH_PIPELINE_KEY,
            no_cull_transparent_solid_render_pipeline,
        );
        render_pipeline_cache.insert(
            constants::NO_CULL_TRANSPARENT_SOLID_COLORED_MESH_PIPELINE_KEY_LIT,
            no_cull_transparent_solid_render_pipeline_lit,
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

        let texture_sampler = texture::create_tex_sampler(device);

        Ok(Renderer {
            device: device.clone(),
            queue: queue.clone(),
            texture_format,
            global_bind_groups: Vec::new(),
            texture_bind_group_layout: mesh_local_textured_layout,
            texture_array_bind_group_layout: mesh_local_array_textured_layout,
            texture_sampler,
            composite_frag_uniform_buffer,
            composite_frag_uniform,
            render_pipeline_cache,
        })
    }

    pub fn set_highlight_pixels(&mut self, px_thickness: u32) {
        self.composite_frag_uniform.highlight_pixel_size = px_thickness as i32;
        self.queue.write_buffer(
            &self.composite_frag_uniform_buffer,
            0,
            bytemuck::cast_slice(&[self.composite_frag_uniform]),
        );
    }

    pub fn create_frame_view_data(&self, width: u32, height: u32) -> FrameViewData {
        FrameViewData::new(&self.device, width as f32, height as f32)
    }

    pub fn write_frame_view_data_to_gpu(&self, frame_view_data: &FrameViewData) {
        frame_view_data.update_data_to_gpu(&self.queue);
    }

    pub async fn render_to_texture<'a>(
        &self,
        render_data: &[RenderData3d<'a>],
        render_texture_data: &RenderTextureData,
        frame_view_data: &FrameViewData,
    ) -> Result<(), WgpuError> {
        let view = RenderView::new_main_view(
            render_data,
            frame_view_data,
            wgpu::Color {
                r: 0.1,
                g: 0.2,
                b: 0.3,
                a: 1.0,
            },
        );
        self.render_views_internal(
            std::slice::from_ref(&view),
            RenderMode::DrawToTexture(render_texture_data),
        )
        .await
    }

    pub async fn render_views_to_texture<'a, 'fv>(
        &self,
        views: &[RenderView<'a, 'fv>],
        render_texture_data: &RenderTextureData,
    ) -> Result<(), WgpuError> {
        self.render_views_internal(views, RenderMode::DrawToTexture(render_texture_data))
            .await
    }

    pub async fn render_and_return_as_image_buffer<'a>(
        &self,
        render_data: &[RenderData3d<'a>],
        output_buffer: &wgpu::Buffer,
        size: wgpu::Extent3d,
        frame_view_data: &FrameViewData,
    ) -> Result<ImageBuffer<Rgba<u8>, Vec<u8>>, WgpuError> {
        let view = RenderView::new_main_view(
            render_data,
            frame_view_data,
            wgpu::Color {
                r: 0.1,
                g: 0.2,
                b: 0.3,
                a: 1.0,
            },
        );

        match self
            .render_views_internal(
                std::slice::from_ref(&view),
                RenderMode::DrawToBuffer(output_buffer, size),
            )
            .await
        {
            Ok(_) => Ok(self.present(output_buffer, size).await),
            Err(err) => Err(err),
        }
    }

    pub async fn render_views_and_return_as_image_buffer<'a, 'fv>(
        &self,
        views: &[RenderView<'a, 'fv>],
        output_buffer: &wgpu::Buffer,
        size: wgpu::Extent3d,
    ) -> Result<ImageBuffer<Rgba<u8>, Vec<u8>>, WgpuError> {
        match self
            .render_views_internal(views, RenderMode::DrawToBuffer(output_buffer, size))
            .await
        {
            Ok(_) => Ok(self.present(output_buffer, size).await),
            Err(err) => Err(err),
        }
    }

    async fn render_views_internal<'a, 'fv, 'rm>(
        &self,
        views: &[RenderView<'a, 'fv>],
        render_mode: RenderMode<'rm>,
    ) -> Result<(), WgpuError> {
        let final_texture_data = match &render_mode {
            RenderMode::DrawToBuffer(_, extent3d) => {
                &create_texture_data(&self.device, *extent3d, self.texture_format)
            }
            RenderMode::DrawToTexture(render_texture_data) => render_texture_data,
        };

        let depth_texture = texture::DepthTexture::create_depth_texture(
            &self.device,
            final_texture_data.texture_size,
        );

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

        let any_silhouette = views.iter().any(|v| {
            v.render_data
                .iter()
                .any(|d| matches!(d.renderable, Renderable3d::SilhouetteMesh))
        });

        let intermediate_color_data = if any_silhouette {
            Some(create_texture_data(
                &self.device,
                final_texture_data.texture_size,
                final_texture_data.texture_format,
            ))
        } else {
            None
        };

        for (view_index, view) in views.iter().enumerate() {
            let has_silhouette = view
                .render_data
                .iter()
                .any(|d| matches!(d.renderable, Renderable3d::SilhouetteMesh));

            if has_silhouette {
                let color_data = intermediate_color_data.as_ref().unwrap();

                // standard render pass into intermediate texture
                self.main_render_pass(
                    view,
                    &depth_texture,
                    &mut encoder,
                    color_data,
                    view.frame_view_data,
                    wgpu::LoadOp::Clear(view.clear_color),
                );

                // screen space pass without depth
                if view.render_data.iter().any(|d| {
                    if let Renderable3d::ScreenSpaceWireframeMesh { depth_testing, .. } =
                        d.renderable
                    {
                        !depth_testing
                    } else if let Renderable3d::ScreenSpaceColoredMesh { depth_testing, .. } =
                        d.renderable
                    {
                        !depth_testing
                    } else {
                        false
                    }
                }) {
                    self.screen_space_render_pass(
                        view,
                        &mut encoder,
                        color_data,
                        &view.frame_view_data.bind_group,
                    );
                }

                // mask render pass for selected meshes + composite into final
                self.silhouette_and_composite_mask(
                    view,
                    final_texture_data,
                    &depth_texture,
                    &mut encoder,
                    color_data,
                    view.frame_view_data,
                );
            } else {
                let color_load = if view_index == 0 {
                    wgpu::LoadOp::Clear(view.clear_color)
                } else {
                    wgpu::LoadOp::Load
                };

                // standard render pass directly into final texture
                self.main_render_pass(
                    view,
                    &depth_texture,
                    &mut encoder,
                    final_texture_data,
                    view.frame_view_data,
                    color_load,
                );

                // screen space pass without depth
                if view.render_data.iter().any(|d| {
                    if let Renderable3d::ScreenSpaceWireframeMesh { depth_testing, .. } =
                        d.renderable
                    {
                        !depth_testing
                    } else if let Renderable3d::ScreenSpaceColoredMesh { depth_testing, .. } =
                        d.renderable
                    {
                        !depth_testing
                    } else {
                        false
                    }
                }) {
                    self.screen_space_render_pass(
                        view,
                        &mut encoder,
                        final_texture_data,
                        &view.frame_view_data.bind_group,
                    );
                }
            }
        }

        if let RenderMode::DrawToBuffer(buffer, size) = &render_mode {
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
                        bytes_per_row: Some(u32_size * size.width),
                        rows_per_image: Some(size.height),
                    },
                },
                *size,
            );
        }

        self.queue.submit(Some(encoder.finish()));

        Ok(())
    }

    fn silhouette_and_composite_mask<'a, 'fv>(
        &self,
        render_view: &RenderView<'a, 'fv>,
        final_texture_data: &RenderTextureData,
        depth_texture: &texture::DepthTexture,
        encoder: &mut wgpu::CommandEncoder,
        color_data: &RenderTextureData,
        frame_view_data: &FrameViewData,
    ) {
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

            Self::apply_viewport_rect(&mut render_pass, render_view.rect);

            render_pass.set_bind_group(0, &frame_view_data.bind_group, &[]);

            render_pass::silhoutte_pass(
                render_view.render_data,
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
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
            });

            Self::apply_viewport_rect(&mut pass, render_view.rect);

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

    fn screen_space_render_pass<'a, 'fv>(
        &self,
        render_view: &RenderView<'a, 'fv>,
        encoder: &mut wgpu::CommandEncoder,
        color_data: &RenderTextureData,
        global_bind_group: &wgpu::BindGroup,
    ) {
        let render_pass_desc = wgpu::RenderPassDescriptor {
            label: Some("Screen Space Render Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &color_data.texture_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            occlusion_query_set: None,
            timestamp_writes: None,
        };
        let mut render_pass = encoder.begin_render_pass(&render_pass_desc);

        Self::apply_viewport_rect(&mut render_pass, render_view.rect);

        render_pass.set_bind_group(0, global_bind_group, &[]);
        // set up global bind groups
        for (i, bind_group) in self.global_bind_groups.iter().enumerate() {
            render_pass.set_bind_group((i + 3) as u32, bind_group, &[]);
        }

        render_pass::screen_space_colored_mesh_pass(
            render_view.render_data,
            &self.render_pipeline_cache,
            &mut render_pass,
            false,
        );

        render_pass::screen_space_wireframe_pass(
            render_view.render_data,
            &self.render_pipeline_cache,
            &mut render_pass,
            false,
        );
    }

    fn main_render_pass<'a, 'fv>(
        &self,
        render_view: &RenderView<'a, 'fv>,
        depth_texture: &texture::DepthTexture,
        encoder: &mut wgpu::CommandEncoder,
        color_data: &RenderTextureData,
        frame_view_data: &FrameViewData,
        color_load: wgpu::LoadOp<wgpu::Color>,
    ) {
        // Separate opaque and transparent renderables
        let opaque_renderables: Vec<_> = render_view
            .render_data
            .iter()
            .filter(|d| !d.local_resources.transparency.enabled)
            .cloned()
            .collect();

        let transparent_renderables: Vec<_> = render_view
            .render_data
            .iter()
            .filter(|d| d.local_resources.transparency.enabled)
            .cloned()
            .collect();

        // Opaque pass - always runs to clear the screen and depth buffer
        // Render pass is created unconditionally to ensure clear happens
        {
            let render_pass_desc = wgpu::RenderPassDescriptor {
                label: Some("Opaque Surface Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &color_data.texture_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: color_load,
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

            Self::apply_viewport_rect(&mut render_pass, render_view.rect);
            render_pass.set_bind_group(0, &frame_view_data.bind_group, &[]);
            // set up global bind groups
            for (i, bind_group) in self.global_bind_groups.iter().enumerate() {
                render_pass.set_bind_group((i + 3) as u32, bind_group, &[]);
            }

            // Only draw opaque objects if they exist
            if !opaque_renderables.is_empty() {
                render_pass::surface_3d_render_pass_with_depth(
                    &opaque_renderables,
                    &self.render_pipeline_cache,
                    &mut render_pass,
                    frame_view_data.triangle_face_mode,
                );
                render_pass::wireframe_3d_render_pass(
                    &opaque_renderables,
                    &self.render_pipeline_cache,
                    &mut render_pass,
                );
            }
        } // render_pass is dropped here, releasing the borrow on encoder

        // Transparent pass - reads depth but doesn't write, with alpha blending
        if !transparent_renderables.is_empty() {
            let render_pass_desc = wgpu::RenderPassDescriptor {
                label: Some("Transparent Surface Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &color_data.texture_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load, // Load from opaque pass
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &depth_texture.view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Load, // Load existing depth
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                occlusion_query_set: None,
                timestamp_writes: None,
            };
            let mut render_pass = encoder.begin_render_pass(&render_pass_desc);

            Self::apply_viewport_rect(&mut render_pass, render_view.rect);
            render_pass.set_bind_group(0, &frame_view_data.bind_group, &[]);
            // set up global bind groups
            for (i, bind_group) in self.global_bind_groups.iter().enumerate() {
                render_pass.set_bind_group((i + 3) as u32, bind_group, &[]);
            }

            render_pass::transparent_mesh_pass(
                &transparent_renderables,
                &self.render_pipeline_cache,
                &mut render_pass,
                frame_view_data.triangle_face_mode,
            );
        }
    }

    fn apply_viewport_rect(render_pass: &mut wgpu::RenderPass<'_>, rect: Option<ViewportRect>) {
        let Some(rect) = rect else {
            return;
        };

        render_pass.set_viewport(
            rect.x as f32,
            rect.y as f32,
            rect.width as f32,
            rect.height as f32,
            0.0,
            1.0,
        );
        render_pass.set_scissor_rect(rect.x, rect.y, rect.width, rect.height);
    }

    async fn present(
        &self,
        output_buffer: &wgpu::Buffer,
        size: wgpu::Extent3d,
    ) -> ImageBuffer<Rgba<u8>, Vec<u8>> {
        let image_buffer = {
            let buffer_slice = output_buffer.slice(..);

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

                        ImageBuffer::<Rgba<u8>, _>::from_raw(size.width, size.height, data.to_vec())
                            .unwrap()
                    } else {
                        panic!("Polling GPU never returned");
                    }
                }
                Err(e) => panic!("{}", e),
            }
        };
        output_buffer.unmap();

        image_buffer
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
        texture_format: format,
    }
}

pub fn create_global_3d_render_pass_bind_group_layout<const CLIP_PLANE_COUNT: usize>(
    device: &wgpu::Device,
) -> wgpu::BindGroupLayout {
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
                    min_binding_size: Some(
                        NonZero::new(CameraUniform::get_size() as wgpu::BufferAddress).unwrap(),
                    ),
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
                    min_binding_size: Some(
                        NonZero::new(ScreenSpaceUniform::get_size() as wgpu::BufferAddress)
                            .unwrap(),
                    ),
                },
                count: None,
            },
            //light uniform
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: Some(
                        NonZero::new(LightUniform::get_size() as wgpu::BufferAddress).unwrap(),
                    ),
                },
                count: None,
            },
            //global clip uniform
            wgpu::BindGroupLayoutEntry {
                binding: 3,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: Some(
                        NonZero::new(clip::ClipUniform::get_size() as wgpu::BufferAddress).unwrap(),
                    ),
                },
                count: None,
            },
        ],
    })
}

pub fn create_global_3d_render_pass_bind_group<const CLIP_PLANE_COUNT: usize>(
    device: &wgpu::Device,
    camera_resource: wgpu::BindingResource<'_>,
    screen_space_resource: wgpu::BindingResource<'_>,
    light_resource: wgpu::BindingResource<'_>,
    clip_plane_buffers: &[wgpu::Buffer; CLIP_PLANE_COUNT],
) -> wgpu::BindGroup {
    if clip_plane_buffers.len() != CLIP_PLANE_COUNT {
        panic!(
            "Clip Plane resources do not match the expected clip plane count: {CLIP_PLANE_COUNT}"
        );
    } else {
        let mut entries = vec![
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
            //light uniform
            wgpu::BindGroupEntry {
                binding: 2,
                resource: light_resource,
            },
        ];
        let mut clip_entries =
            clip::create_clip_bind_group_entries::<CLIP_PLANE_COUNT, 3>(clip_plane_buffers);

        entries.append(&mut clip_entries);

        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Global Bind Group"),
            layout: &create_global_3d_render_pass_bind_group_layout::<CLIP_PLANE_COUNT>(device),
            entries: &entries,
        })
    }
}

pub fn create_read_buffer(device: &wgpu::Device, width: u32, height: u32) -> wgpu::Buffer {
    let u32_size = std::mem::size_of::<u32>() as u32;
    let output_buffer_size = (u32_size * width * height) as wgpu::BufferAddress;
    let output_buffer_desc = wgpu::BufferDescriptor {
        size: output_buffer_size,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        label: None,
        mapped_at_creation: false,
    };

    device.create_buffer(&output_buffer_desc)
}
