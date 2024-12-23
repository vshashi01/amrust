use std::sync::Arc;

use eframe::{
    egui_wgpu::{self, CallbackTrait, RenderState},
    wgpu::{
        self, util::DeviceExt, BindGroupLayout, BlendState, BufferUsages, ColorTargetState,
        ColorWrites, MultisampleState, ShaderSource, ShaderStages, SurfaceConfiguration,
        TextureFormat, VertexBufferLayout,
    },
};

use super::texture::*;
use super::viewport_resources::ViewportResources;
use super::{
    camera::{Camera, CameraUniform},
    viewport_resources::Dataset,
};

pub struct EViewport3d {
    pub height: f32,
    pub width: f32,
}

impl EViewport3d {
    pub fn new<'a>(render_state: &RenderState, (height, width): (f32, f32)) -> Self {
        let device = &render_state.device;
        // let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        //     label: None,
        //     source: wgpu::ShaderSource::Wgsl(include_str!("../custom3d_wgpu_shader.wgsl").into()),
        // });
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Vertex Buffer"),
            contents: bytemuck::cast_slice(VERTICES),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Index Buffer"),
            contents: bytemuck::cast_slice(INDICES),
            usage: wgpu::BufferUsages::INDEX,
        });

        let index_buffer_len = INDICES.len() as u32;

        // let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        //     label: None,
        //     source: wgpu::ShaderSource::Wgsl(include_str!("./simple_vertex.wgsl").into()),
        // });

        // let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        //     label: None,
        //     entries: &[wgpu::BindGroupLayoutEntry {
        //         binding: 0,
        //         visibility: wgpu::ShaderStages::VERTEX,
        //         ty: wgpu::BindingType::Buffer {
        //             ty: wgpu::BufferBindingType::Uniform,
        //             has_dynamic_offset: false,
        //             min_binding_size: None,
        //         },
        //         count: None,
        //     }],
        // });

        // let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        //     label: None,
        //     bind_group_layouts: &[&bind_group_layout],
        //     push_constant_ranges: &[],
        // });

        // let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        //     label: None,
        //     layout: Some(&pipeline_layout),
        //     vertex: wgpu::VertexState {
        //         module: &shader,
        //         entry_point: "vs_main",
        //         compilation_options: wgpu::PipelineCompilationOptions::default(),
        //         buffers: &[],
        //     },
        //     fragment: Some(wgpu::FragmentState {
        //         module: &shader,
        //         entry_point: "fs_main",
        //         compilation_options: wgpu::PipelineCompilationOptions::default(),
        //         targets: &[Some(ColorTargetState {
        //             format: texture_format,
        //             blend: None,
        //             write_mask: ColorWrites::all(),
        //         })],
        //     }),
        //     primitive: wgpu::PrimitiveState::default(),
        //     depth_stencil: None,
        //     multisample: wgpu::MultisampleState::default(),
        //     multiview: None,
        // });

        let camera = Camera {
            eye: (0.0, 1.0, 2.0).into(),
            target: (0.0, 0.0, 0.0).into(),
            up: cgmath::Vector3::unit_y(),
            fovy: 45.0,
            aspect: width / height,
            z_near: 0.1,
            z_far: 100.0,
        };

        // let camera_controller = CameraController::new(0.2);

        let camera_uniform = CameraUniform::new();

        // let camera_staging = CameraStaging::new(camera);
        // camera_staging.update_camera(&mut camera_uniform);

        let camera_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Camera buffer"),
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
            contents: bytemuck::cast_slice(&[camera_uniform]),
        });

        let camera_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Camera bind group layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Camera bind group"),
            layout: &camera_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buffer.as_entire_binding(),
            }],
        });

        let pipeline = generate_basic_render_pipeline(
            &device,
            render_state.target_format,
            "shader",
            ShaderSource::Wgsl((include_str!("simple_vertex.wgsl")).into()),
            &[
                SimpleVertex::desc(), /* instance::InstanceRaw::desc() */
            ],
            &[&camera_bind_group_layout],
            true,
        );

        // let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        //     label: None,
        //     contents: bytemuck::cast_slice(&[0.0]),
        //     usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::UNIFORM,
        // });

        // let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        //     label: None,
        //     layout: &bind_group_layout,
        //     entries: &[wgpu::BindGroupEntry {
        //         binding: 0,
        //         resource: uniform_buffer.as_entire_binding(),
        //     }],
        // });

        // Because the graphics pipeline must have the same lifetime as the egui render pass,
        // instead of storing the pipeline in our `Custom3D` struct, we insert it into the
        // `callback_resources` type map, which is stored alongside the render pass.
        render_state
            .renderer
            .write()
            .callback_resources
            .insert(ViewportResources {
                pipeline,
                vertex_buffer,
                index_buffer,
                index_buffer_len,
                camera,
                camera_bind_group,
                camera_uniform,
                camera_buffer,
                dataset: Dataset(),
            });

        Self { height, width }
    }

    pub fn custom_painting(&self, ui: &mut egui::Ui) {
        let (rect, _response) = ui.allocate_exact_size(
            egui::Vec2 {
                x: self.width,
                y: self.height,
            },
            egui::Sense::drag(),
        );

        // let new_angle = angle + response.drag_delta().x * 0.01;

        let cb = egui_wgpu::Callback::new_paint_callback(rect, EViewport3dCallback {});
        ui.painter().add(cb);
    }
}

pub struct EViewport3dCallback();

impl CallbackTrait for EViewport3dCallback {
    fn prepare(
        &self,
        _device: &wgpu::Device,
        _queue: &wgpu::Queue,
        _screen_descriptor: &eframe::egui_wgpu::ScreenDescriptor,
        _egui_encoder: &mut wgpu::CommandEncoder,
        _callback_resources: &mut eframe::egui_wgpu::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        Vec::new()
    }

    fn finish_prepare(
        &self,
        _device: &wgpu::Device,
        _queue: &wgpu::Queue,
        _egui_encoder: &mut wgpu::CommandEncoder,
        _callback_resources: &mut eframe::egui_wgpu::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        Vec::new()
    }

    fn paint<'a>(
        &'a self,
        info: egui::PaintCallbackInfo,
        render_pass: &mut wgpu::RenderPass<'static>,
        callback_resources: &'a eframe::egui_wgpu::CallbackResources,
    ) {
        let resources: &ViewportResources = callback_resources.get().unwrap();
        render_pass.set_pipeline(&resources.pipeline);
        render_pass.set_bind_group(0, &resources.camera_bind_group, &[]);
        render_pass.set_vertex_buffer(0, resources.vertex_buffer.slice(..));
        render_pass.set_index_buffer(resources.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
        render_pass.draw_indexed(0..resources.index_buffer_len, 0, 0..1);
    }
}

fn generate_basic_render_pipeline(
    device: &wgpu::Device,
    texture_format: wgpu::TextureFormat,
    shader_name: &str,
    source: ShaderSource,
    buffers: &[VertexBufferLayout<'static>],
    bind_group_layouts: &[&BindGroupLayout],
    need_depth_stencil: bool,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(shader_name),
        source: source,
    });

    let pipeline_layout_name = format!("Pipeline Layout {shader_name}");
    let render_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some(&pipeline_layout_name),
        bind_group_layouts: bind_group_layouts,
        push_constant_ranges: &[],
    });

    let render_pipeline_name = format!("Render Pipeline {shader_name}");
    let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(&render_pipeline_name),
        layout: Some(&render_pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            buffers: buffers,
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            targets: &[Some(ColorTargetState {
                format: texture_format,
                blend: Some(BlendState::REPLACE),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: Some(wgpu::Face::Back),
            unclipped_depth: false,
            polygon_mode: wgpu::PolygonMode::Fill,
            conservative: false,
        },
        depth_stencil: if need_depth_stencil {
            Some(wgpu::DepthStencilState {
                format: Texture::DEPTH_FORMAT,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::LessEqual,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            })
        } else {
            None
        },
        multisample: MultisampleState {
            count: 1,
            mask: !0,
            alpha_to_coverage_enabled: false,
        },
        multiview: None,
        cache: None,
    });

    render_pipeline
}

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct SimpleVertex {
    pub position: [f32; 3],
    pub color: [f32; 3],
}

impl SimpleVertex {
    fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<SimpleVertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x3,
                    offset: 0,
                    shader_location: 0,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x3,
                    offset: std::mem::size_of::<[f32; 3]>() as wgpu::BufferAddress,
                    shader_location: 1,
                },
            ],
        }
    }
}

// -----------(A)*-------------
//-----------*-----*-----------
//------(B)*--------*(E)-------
//----------*-----*-------------
//--------(C)*---*(D)-----------
pub const VERTICES: &[SimpleVertex] = &[
    SimpleVertex {
        position: [-0.0868241, 0.49240386, 0.0],
        color: [0.5, 0.0, 0.5],
    }, // A
    SimpleVertex {
        position: [-0.49513406, 0.06958647, 0.0],
        color: [0.5, 0.0, 0.5],
    }, // B
    SimpleVertex {
        position: [-0.21918549, -0.44939706, 0.0],
        color: [0.5, 0.0, 0.5],
    }, // C
    SimpleVertex {
        position: [0.35966998, -0.3473291, 0.0],
        color: [0.5, 0.0, 0.5],
    }, // D
    SimpleVertex {
        position: [0.44147372, 0.2347359, 0.0],
        color: [0.5, 0.0, 0.5],
    }, // E
];

pub const INDICES: &[u16] = &[0, 1, 4, 1, 2, 4, 2, 3, 4];
