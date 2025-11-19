use crate::amrust_db::{Db, add_render_items_from_db};
use crate::egui_tools::EguiRenderer;
use crate::save_3mf::save;
use amrust_render::bounding_box::BoundingBox;
// use amrust_lib::widgets::dropped_files::DroppedFilesWidget;
use amrust_render::camera::{self, CameraData, OrthographicCameraData};
use amrust_render::gpu_mesh::MeshBuilder;
use amrust_render::instance::InstanceDataBuilder;
use amrust_render::material::Material;
use amrust_render::normalized_box::{ORDERED_POSITIONS, ORDERED_POSITIONS_BOX_EDGE_INDICES};
use amrust_render::renderer::RenderTextureData;
use amrust_render::transformation::Transformation;
use egui::{Frame, Id, Image, Margin, Vec2, epaint};
use egui_file_dialog::FileDialog;
use egui_wgpu::wgpu::SurfaceError;
use egui_wgpu::{ScreenDescriptor, wgpu};
use glam::{Mat4, Vec3};
use std::path::PathBuf;
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
    pub dpi_factor: f32,
    pub egui_renderer: EguiRenderer,
    pub renderer_3d: renderer::Renderer,
    pub camera_data: OrthographicCameraData,
    pub render_texture_data: renderer::RenderTextureData,
    pub texture_id: epaint::TextureId,
    // pub dropped_files: DroppedFilesWidget,
    pub load_file_dlg: FileDialog,
    pub save_file_dlg: FileDialog,
    pub picked_file: Option<PathBuf>,
    pub scene_bbox: Option<BoundingBox>,
    pub db: Db,
}

impl AppState {
    async fn new(
        instance: &wgpu::Instance,
        surface: wgpu::Surface<'static>,
        window: &Window,
        width: u32,
        height: u32,
        dpi_factor: f32,
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

        let features = renderer::DEVICE_FEATURES
            .iter()
            .fold(wgpu::Features::empty(), |acc, &feature| acc | feature);
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: None,
                required_features: features,
                required_limits: renderer::DEVICE_LIMITS,
                memory_hints: Default::default(),
                trace: wgpu::Trace::Off,
            })
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

        let mut renderer_3d = renderer::Renderer::from_existing_device_and_queue(
            device.clone(),
            queue.clone(),
            surface_config.format,
            width,
            height,
        )
        .await
        .unwrap();

        //when the size change we need to create a new render texture data and register a new egui texture.
        let render_texture_data = renderer_3d.create_render_texture_data();
        let texture_id = egui_renderer.register_texture(&device, &render_texture_data.texture_view);
        let camera_data = get_camera_data();
        render_frame(&mut renderer_3d, &camera_data, &render_texture_data).await;

        // let dropped_files_widget = DroppedFilesWidget::new();

        let load_file_dlg = FileDialog::new()
            .add_file_filter_extensions("3MF", vec!["3mf"])
            .default_file_filter("3MF");

        let save_file_dlg = FileDialog::new()
            .add_save_extension("3MF file", "3mf")
            .default_save_extension("3MF file");

        Self {
            device: Arc::new(device),
            queue: Arc::new(queue),
            surface,
            surface_config,
            dpi_factor,
            egui_renderer,
            renderer_3d,
            camera_data,
            render_texture_data,
            texture_id,
            // dropped_files: dropped_files_widget,
            load_file_dlg,
            save_file_dlg,
            picked_file: None,
            scene_bbox: None,
            db: Db::new(),
        }
    }

    fn resize_surface(&mut self, width: u32, height: u32) {
        self.surface_config.width = width;
        self.surface_config.height = height;
        self.surface.configure(&self.device, &self.surface_config);

        self.renderer_3d.set_size(width, height);
        let render_texture_data = self.renderer_3d.create_render_texture_data();
        let texture_id = self
            .egui_renderer
            .register_texture(&self.device, &render_texture_data.texture_view);
        self.render_texture_data = render_texture_data;
        self.texture_id = texture_id;
    }

    fn handle_redraw(&mut self) {
        pollster::block_on(async {
            render_frame(
                &mut self.renderer_3d,
                &self.camera_data,
                &self.render_texture_data,
            )
            .await
        });
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
        let scale_factor = window.scale_factor();

        let surface = self
            .instance
            .create_surface(window.clone())
            .expect("Failed to create surface!");

        let state = AppState::new(
            &self.instance,
            surface,
            &window,
            initial_width,
            initial_height,
            scale_factor as f32,
        )
        .await;

        self.window.get_or_insert(window);
        self.state.get_or_insert(state);
    }

    fn handle_resized(&mut self, width: u32, height: u32) {
        if width > 0 && height > 0 {
            let state = self.state.as_mut().unwrap();
            state.resize_surface(width, height);
            state
                .camera_data
                .set_viewport_size(width as f32, height as f32);
        }
    }

    fn handle_dpi_changed(&mut self, scale_factor: f64) {
        let state = self.state.as_mut().unwrap();
        state.dpi_factor = scale_factor as f32;
        state.egui_renderer.context().request_repaint();
    }

    fn handle_redraw(&mut self) {
        // Attempt to handle minimizing window
        if let Some(window) = self.window.as_ref()
            && let Some(min) = window.is_minimized()
            && min
        {
            // println!("Window is minimized");
            return;
        }

        let state = self.state.as_mut().unwrap();

        let screen_descriptor = ScreenDescriptor {
            size_in_pixels: [state.surface_config.width, state.surface_config.height],
            pixels_per_point: state.dpi_factor,
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
            Ok(_) => {
                state.handle_redraw();
            }
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

            let default_bbox = BoundingBox::default();
            let bbox = state.scene_bbox.as_ref().unwrap_or(&default_bbox);
            egui::TopBottomPanel::top("top panel")
                .resizable(false)
                .show(state.egui_renderer.context(), |ui| {
                    egui::MenuBar::new().ui(ui, |ui| {
                        if ui.button("Import Part").clicked() {
                            state.load_file_dlg.pick_file();
                        }

                        // if ui.button("Add Test Mesh").clicked() {
                        //     set_solid_mesh(&mut state.renderer_3d);
                        //     state.egui_renderer.context().request_repaint();
                        // }

                        ui.add_enabled_ui(!state.db.is_scene_empty(), |ui| {
                            if ui.button("Unzoom Scene").clicked() {
                                unzoom_bbox(&mut state.camera_data, bbox);
                                state.egui_renderer.context().request_repaint();
                            }

                            if ui.button("Clear All").clicked() {
                                state.renderer_3d.clear_all();
                                state.db.clear_all();
                                state.egui_renderer.context().request_repaint();
                            }

                            if ui.button("Save to 3mf").clicked() {
                                state.save_file_dlg.save_file();
                            }
                        })
                    });
                });

            // egui::SidePanel::left(Id::new("object list")).show(
            //     state.egui_renderer.context(),
            //     |ui| {
            //         ui.label("Left Panel");
            //     },
            // );

            egui::CentralPanel::default().show(state.egui_renderer.context(), |ui| {
                let dpi_factor = state.egui_renderer.context().pixels_per_point();
                let image_texture = Image::new((
                    state.texture_id,
                    Vec2::new(
                        (state.surface_config.width as f32 / dpi_factor) - 10.0,
                        (state.surface_config.height as f32 / dpi_factor) - 10.0,
                    ),
                ));

                let response = ui.interact(
                    ui.max_rect(),
                    ui.id().with("3d_viewport"),
                    egui::Sense::drag(),
                );

                if response.dragged() {
                    let delta = response.drag_delta();
                    state
                        .camera_data
                        .transform(camera::CameraTransform::Rotate {
                            pivot: bbox.center(),
                            rotation_axis: glam::Vec3::Y,
                            angle: delta.x * 0.01, // adjust sensitivity as needed
                        });
                    state
                        .camera_data
                        .transform(camera::CameraTransform::Rotate {
                            pivot: bbox.center(),
                            rotation_axis: glam::Vec3::X,
                            angle: delta.y * 0.01,
                        });
                    state.egui_renderer.context().request_repaint();
                }

                // Track scroll for zoom
                if ui.ctx().input(|i| i.raw_scroll_delta.y != 0.0) {
                    let scroll = ui.ctx().input(|i| i.raw_scroll_delta.y);
                    state
                        .camera_data
                        .transform(camera::CameraTransform::Zoom(scroll * 0.001));
                    state.egui_renderer.context().request_repaint();
                }

                Frame::new()
                    .inner_margin(Margin::symmetric(5, 5))
                    .show(ui, |ui| {
                        ui.image(image_texture.source(state.egui_renderer.context()));
                    });
            });

            // state
            //     .dropped_files
            //     .run(state.egui_renderer.context(), &|test| false);

            state.load_file_dlg.update(state.egui_renderer.context());
            if let Some(path) = state.load_file_dlg.take_picked() {
                println!("File picked is: {:?}", path);

                if let Some(ext) = path.extension()
                    && let Some("3mf") = ext.to_str()
                {
                    let threemf = std::fs::File::open(path).unwrap();
                    let db = crate::load_3mf::load(threemf);
                    match db {
                        Ok(db) => {
                            println!("Db contains: {:?}", db);
                            let appended = state.db.append(db);
                            match appended {
                                Ok(_) => {
                                    state.renderer_3d.clear_all();
                                    match add_render_items_from_db(
                                        &mut state.renderer_3d,
                                        &state.db,
                                    ) {
                                        Ok(new_bbox) => {
                                            // println!("New Bounding Box is {:?}", new_bbox);
                                            let bbox = match &mut state.scene_bbox {
                                                Some(current_bbox) => {
                                                    current_bbox.unite(&new_bbox);
                                                    *current_bbox
                                                }
                                                None => {
                                                    //ToDo:: Fix this properly for the clear mesh case
                                                    let bbox =
                                                        &mut state.scene_bbox.insert(new_bbox);
                                                    **bbox
                                                }
                                            };

                                            unzoom_bbox(&mut state.camera_data, &bbox);
                                            state.egui_renderer.context().request_repaint();
                                        }
                                        Err(err) => {
                                            println!("Error: {:?}", err);
                                        }
                                    }
                                }
                                Err(err) => {
                                    println!("{err:?}")
                                }
                            }
                        }
                        Err(err) => println!("Error:{:?}", err),
                    }
                }
            }

            state.save_file_dlg.update(state.egui_renderer.context());
            if let Some(save_file_path) = state.save_file_dlg.take_picked() {
                println!("File path to save to is {save_file_path:?}");

                let file = std::fs::File::create_new(save_file_path);

                match file {
                    Ok(file) => {
                        if let Err(err) = save(&state.db, file) {
                            println!("{err:?}");
                        }
                    }
                    Err(err) => println!("{err:?}"),
                }
            }

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
            WindowEvent::ScaleFactorChanged {
                scale_factor,
                inner_size_writer: _,
            } => {
                self.handle_dpi_changed(scale_factor);
            }
            // WindowEvent::HoveredFileCancelled => {
            //     println!("Hovered file cancelled")
            // }
            // WindowEvent::HoveredFile(filepath) => {
            //     println!("File hovered");
            // }
            // WindowEvent::DroppedFile(filepath) => {
            //     println!("File dropped");
            // }
            _ => (),
        }
    }
}

async fn render_frame(
    renderer: &mut renderer::Renderer,
    camera_data: &impl CameraData,
    render_texture_data: &RenderTextureData,
) {
    renderer.update_camera(camera_data);
    let _ = renderer.render_to_texture(render_texture_data).await;
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

fn get_camera_data() -> OrthographicCameraData {
    let mut camera_data = OrthographicCameraData {
        far: 5000.0,
        ..Default::default()
    };
    camera_data
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

    camera_data
}

pub fn unzoom_bbox(camera: &mut impl CameraData, total_bbox: &BoundingBox) {
    let top_left_corner = Vec3::new(total_bbox.min.x, total_bbox.min.y, total_bbox.max.z);
    // println!("The top left corner is: {}", top_left_corner);
    camera
        .transform(amrust_render::camera::CameraTransform::SetView {
            eye_position: top_left_corner,
            target_position: total_bbox.center(),
            up_vector: Vec3::Z,
        })
        .transform(amrust_render::camera::CameraTransform::FitToExtent {
            min: total_bbox.min,
            max: total_bbox.max,
        });
}
