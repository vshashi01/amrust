use crate::egui_tools::EguiRenderer;
use amrust_render::camera::{self, Camera, OrthographicCameraData};
use amrust_render::gpu_mesh::MeshBuilder;
use amrust_render::instance::InstanceDataBuilder;
use amrust_render::material::Material;
use amrust_render::normalized_box::{ORDERED_POSITIONS, ORDERED_POSITIONS_BOX_EDGE_INDICES};
use amrust_render::renderer::RenderTextureData;
use amrust_render::transformation::Transformation;
use egui::{Image, Vec2, epaint};
use egui_wgpu::wgpu::SurfaceError;
use egui_wgpu::wgpu::core::device;
use egui_wgpu::{ScreenDescriptor, wgpu};
use glam::{Mat4, Vec3};
use std::sync::Arc;
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::window::{Window, WindowId};

use amrust_render::{RenderObject, Renderable, renderer};

pub struct AppState {
    pub device: Arc<wgpu::Device>,
    pub queue: Arc<wgpu::Queue>,
    pub surface_config: wgpu::SurfaceConfiguration,
    pub surface: wgpu::Surface<'static>,
    pub scale_factor: f32,
    pub egui_renderer: EguiRenderer,
    pub renderer_3d: renderer::Renderer,
    pub texture_id: epaint::TextureId,
}

impl AppState {
    async fn new(
        instance: &wgpu::Instance,
        surface: wgpu::Surface<'static>,
        window: &Window,
        width: u32,
        height: u32,
    ) -> Self {
        let power_pref = wgpu::PowerPreference::default();
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: power_pref,
                force_fallback_adapter: false,
                compatible_surface: Some(&surface),
            })
            .await
            .expect("Failed to find an appropriate adapter");

        // let features = wgpu::Features::empty();
        let features = renderer::DEVICE_FEATURES
            .iter()
            .fold(wgpu::Features::empty(), |acc, &feature| acc | feature);
        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: None,
                    required_features: features,
                    required_limits: renderer::DEVICE_LIMITS,
                    memory_hints: Default::default(),
                },
                None,
            )
            .await
            .expect("Failed to create device");

        let swapchain_capabilities = surface.get_capabilities(&adapter);
        let selected_format = wgpu::TextureFormat::Bgra8UnormSrgb;
        let swapchain_format = swapchain_capabilities
            .formats
            .iter()
            .find(|d| **d == selected_format)
            .expect("failed to select proper surface texture format!");

        let surface_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: *swapchain_format,
            width,
            height,
            present_mode: wgpu::PresentMode::AutoVsync,
            desired_maximum_frame_latency: 0,
            alpha_mode: swapchain_capabilities.alpha_modes[0],
            view_formats: vec![],
        };

        surface.configure(&device, &surface_config);

        let mut egui_renderer = EguiRenderer::new(&device, surface_config.format, None, 1, window);

        let scale_factor = 1.0;

        let mut renderer_3d = renderer::Renderer::from_existing_device_and_queue(
            device.clone(),
            queue.clone(),
            surface_config.format,
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        )
        .await
        .unwrap();

        let texture_data = renderer_3d.create_render_texture_data();

        let texture_id = egui_renderer.register_texture(&device, &texture_data.texture_view);

        test_box_solid_color_only(&mut renderer_3d, &device, &texture_data).await;

        Self {
            device: Arc::new(device),
            queue: Arc::new(queue),
            surface,
            surface_config,
            egui_renderer,
            scale_factor,
            renderer_3d,
            texture_id,
        }
    }

    fn resize_surface(&mut self, width: u32, height: u32) {
        self.surface_config.width = width;
        self.surface_config.height = height;
        self.surface.configure(&self.device, &self.surface_config);
    }
}

pub struct App {
    instance: wgpu::Instance,
    state: Option<AppState>,
    window: Option<Arc<Window>>,
}

impl App {
    pub fn new() -> Self {
        let instance = egui_wgpu::wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        Self {
            instance,
            state: None,
            window: None,
        }
    }

    async fn set_window(&mut self, window: Window) {
        let window = Arc::new(window);
        let initial_width = 1360;
        let initial_height = 768;

        let _ = window.request_inner_size(PhysicalSize::new(initial_width, initial_height));

        let surface = self
            .instance
            .create_surface(window.clone())
            .expect("Failed to create surface!");

        let state = AppState::new(
            &self.instance,
            surface,
            &window,
            initial_width,
            initial_width,
        )
        .await;

        self.window.get_or_insert(window);
        self.state.get_or_insert(state);
    }

    fn handle_resized(&mut self, width: u32, height: u32) {
        if width > 0 && height > 0 {
            self.state.as_mut().unwrap().resize_surface(width, height);
        }
    }

    fn handle_redraw(&mut self) {
        // Attempt to handle minimizing window
        if let Some(window) = self.window.as_ref() {
            if let Some(min) = window.is_minimized() {
                if min {
                    println!("Window is minimized");
                    return;
                }
            }
        }

        let state = self.state.as_mut().unwrap();

        let screen_descriptor = ScreenDescriptor {
            size_in_pixels: [state.surface_config.width, state.surface_config.height],
            pixels_per_point: self.window.as_ref().unwrap().scale_factor() as f32
                * state.scale_factor,
        };

        let surface_texture = state.surface.get_current_texture();

        match surface_texture {
            Err(SurfaceError::Outdated) => {
                // Ignoring outdated to allow resizing and minimization
                println!("wgpu surface outdated");
                return;
            }
            Err(_) => {
                surface_texture.expect("Failed to acquire next swap chain texture");
                return;
            }
            Ok(_) => {}
        };

        let surface_texture = surface_texture.unwrap();

        let surface_view = surface_texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = state
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

        let window = self.window.as_ref().unwrap();

        {
            state.egui_renderer.begin_frame(window);

            egui::Window::new("winit + egui + wgpu says hello!")
                .resizable(true)
                .vscroll(true)
                .default_open(false)
                .show(state.egui_renderer.context(), |ui| {
                    ui.label("Label!");

                    if ui.button("Button!").clicked() {
                        println!("boom!")
                    }

                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.label(format!(
                            "Pixels per point: {}",
                            state.egui_renderer.context().pixels_per_point()
                        ));
                        if ui.button("-").clicked() {
                            state.scale_factor = (state.scale_factor - 0.1).max(0.3);
                        }
                        if ui.button("+").clicked() {
                            state.scale_factor = (state.scale_factor + 0.1).min(3.0);
                        }
                    });
                });

            let image_texture = Image::new((
                state.texture_id,
                Vec2::new(TEXTURE_WIDTH as f32, TEXTURE_HEIGHT as f32),
            ));

            egui::Window::new("3D Renderer")
                .resizable(true)
                .vscroll(true)
                .default_open(true)
                .show(state.egui_renderer.context(), |ui| {
                    // ui.label("3D Renderer is ready!");
                    // if ui.button("Render Triangle").clicked() {}
                    ui.image(image_texture.source(state.egui_renderer.context()));
                });

            state.egui_renderer.end_frame_and_draw(
                &state.device,
                &state.queue,
                &mut encoder,
                window,
                &surface_view,
                screen_descriptor,
            );
        }

        state.queue.submit(Some(encoder.finish()));
        surface_texture.present();
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let window = event_loop
            .create_window(Window::default_attributes())
            .unwrap();
        pollster::block_on(self.set_window(window));
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        // let egui render to process the event first
        self.state
            .as_mut()
            .unwrap()
            .egui_renderer
            .handle_input(self.window.as_ref().unwrap(), &event);

        match event {
            WindowEvent::CloseRequested => {
                println!("The close button was pressed; stopping");
                event_loop.exit();
            }
            WindowEvent::RedrawRequested => {
                self.handle_redraw();

                self.window.as_ref().unwrap().request_redraw();
            }
            WindowEvent::Resized(new_size) => {
                self.handle_resized(new_size.width, new_size.height);
            }
            _ => (),
        }
    }
}

const TEXTURE_WIDTH: u32 = 512;
const TEXTURE_HEIGHT: u32 = 512;

async fn test_box_solid_color_only(
    renderer: &mut renderer::Renderer,
    device: &wgpu::Device,
    render_texture_data: &RenderTextureData,
) {
    // let mut renderer = renderer::Renderer::from_new_device(TEXTURE_WIDTH, TEXTURE_HEIGHT)
    //     .await
    //     .unwrap();
    let (_mesh_object_id, _wireframe_object_id) = set_solid_mesh(renderer);

    let camera_bind_group = get_camera_bind_group(device);
    let _ = renderer
        .render(Some(render_texture_data), &camera_bind_group)
        .await;
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

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    position: [f32; 3],
    color: [f32; 3],
}

impl Vertex {
    fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x3,
                },
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 3]>() as wgpu::BufferAddress,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x3,
                },
            ],
        }
    }
}

const VERTICES: &[Vertex] = &[
    Vertex {
        position: [0.0, 0.5, 0.0],
        color: [1.0, 0.0, 0.0],
    },
    Vertex {
        position: [-0.5, -0.5, 0.0],
        color: [0.0, 1.0, 0.0],
    },
    Vertex {
        position: [0.5, -0.5, 0.0],
        color: [0.0, 0.0, 1.0],
    },
];

const INDICES: &[u16] = &[0, 1, 2];
