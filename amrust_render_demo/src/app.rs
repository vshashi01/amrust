use crate::amrust_db::{
    Db, Mesh, add_render_items_from_scene, add_render_items_from_unique_parts,
    create_build_items_list, create_object_tree_from_identifiable, create_objects_list,
    create_scene_tree_items_by_unique_parts,
};
use crate::app_mode::AppMode;
use crate::clear_db::ClearDbOps;
use crate::egui_tools::EguiRenderer;
use crate::operation_manager::{OperationManager, OperationMessage, OperationResponse};
use crate::part_list::PartList;
use crate::render_db::RenderDb;
use crate::render_worker::{RenderMessage, RenderResponse, RenderWorker, RendererSettings};
use crate::save_3mf::save;
use crate::toolsheets::{self, Toolsheets};
use crate::tree_item_viewer::TreeItemViewer;
use crate::viewport::Viewport3D;
use crate::{load_3mf, save_3mf};
use amrust_render::bounding_box::BoundingBox;
// use amrust_lib::widgets::dropped_files::DroppedFilesWidget;
use amrust_render::camera::{self, CameraData, OrthographicCameraData};
use amrust_render::normalized_box::{ORDERED_POSITIONS, ORDERED_POSITIONS_TRI_EDGE_INDICES};
use amrust_render::transformation::Transformation;
use egui::{Id, Layout, epaint};
use egui_dock::{DockArea, DockState, NodeIndex};
use egui_file_dialog::FileDialog;
use egui_wgpu::wgpu::SurfaceError;
use egui_wgpu::{ScreenDescriptor, wgpu};
use glam::{Mat4, Vec3};
use smol::channel::{Receiver, Sender, TryRecvError};
use smol::lock::RwLock;
use smol::{Executor, channel};
use std::path::PathBuf;
use std::sync::Arc;
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::window::{Window, WindowId};

use amrust_render::{RenderDatabase, renderer};

struct AppState {
    pub device: Arc<wgpu::Device>,
    pub queue: Arc<wgpu::Queue>,
    pub surface_config: wgpu::SurfaceConfiguration,
    pub surface: wgpu::Surface<'static>,
    pub dpi_factor: f32,
    pub egui_renderer: EguiRenderer,
    pub camera_data: OrthographicCameraData,
    pub texture_id: Option<epaint::TextureId>,
    // pub dropped_files: DroppedFilesWidget,
    pub load_file_dlg: FileDialog,
    pub save_file_dlg: FileDialog,
    pub picked_file: Option<PathBuf>,
    pub scene_bbox: Option<BoundingBox>,
    pub db: Arc<RwLock<Db>>,
    pub toolsheets: Option<Toolsheets>,
    pub viewport_3d: Viewport3D,
    pub current_app_mode: AppMode,
    pub current_render_mode: AppMode,
    pub need_viewport_update: bool,

    pub executor: Arc<Executor<'static>>,
    pub render_message_tx: Sender<RenderMessage>,
    pub render_response_rx: Receiver<RenderResponse>,
    pub render_db: Arc<RwLock<RenderDb>>,

    pub operation_manager: OperationManager,
    pub operation_queue_tx: Sender<OperationMessage>,
    pub operation_response_rx: Receiver<OperationResponse>,
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

        let egui_renderer = EguiRenderer::new(&device, surface_config.format, None, 1, window);

        let camera_data = get_camera_data();

        // let dropped_files_widget = DroppedFilesWidget::new();

        //create async executor and run it
        let executor = Arc::new(Executor::new());
        {
            let exec = executor.clone();
            std::thread::Builder::new()
                .name("smol-executor".to_owned())
                .spawn(move || {
                    smol::block_on(async move {
                        println!("Trying to run executor");
                        exec.run(async move {
                            loop {
                                smol::Timer::after(std::time::Duration::from_millis(1)).await;
                            }
                        })
                        .await;
                    });
                })
                .expect("failed to spawn executor thread");
        }

        let render_db = Arc::new(RwLock::new(RenderDb::new()));

        //create new render worker
        let (render_message_tx, render_message_rx) = channel::unbounded();
        let (render_response_tx, render_response_rx) = channel::unbounded();

        let renderer_settings = RendererSettings {
            device: device.clone(),
            queue: queue.clone(),
            format: surface_config.format,
            width,
            height,
            initial_camera_data: camera_data.clone(),
        };

        {
            let internal_render_db = render_db.clone();
            executor
                .spawn(async {
                    println!("Attempted to println in separate thread");
                    let mut render_worker = RenderWorker::new(
                        renderer_settings,
                        internal_render_db,
                        render_message_rx,
                        render_response_tx,
                    )
                    .await;

                    render_worker.run().await;
                })
                .detach();
        }

        let load_file_dlg = FileDialog::new()
            .add_file_filter_extensions("3MF", vec!["3mf"])
            .default_file_filter("3MF");

        let save_file_dlg = FileDialog::new()
            .add_save_extension("3MF file", "3mf")
            .default_save_extension("3MF file");

        //create operation manager and its channels
        let (operation_queue_tx, operation_queue_rx) = channel::unbounded();
        let (operation_response_tx, operation_response_rx) = channel::unbounded();
        let operation_manager = OperationManager::new(operation_queue_rx, operation_response_tx);

        Self {
            device: Arc::new(device),
            queue: Arc::new(queue),
            surface,
            surface_config,
            dpi_factor,
            egui_renderer,
            camera_data,
            texture_id: None,
            // dropped_files: dropped_files_widget,
            load_file_dlg,
            save_file_dlg,
            picked_file: None,
            scene_bbox: None,
            db: Arc::new(RwLock::new(Db::new())),
            toolsheets: None,
            viewport_3d: Viewport3D {},
            current_app_mode: AppMode::Build,
            current_render_mode: AppMode::Build,
            need_viewport_update: false,
            executor,
            render_message_tx,
            render_response_rx,
            render_db,

            operation_manager,
            operation_queue_tx,
            operation_response_rx,
        }
    }

    fn resize_surface(&mut self, width: u32, height: u32) {
        self.surface_config.width = width;
        self.surface_config.height = height;
        self.surface.configure(&self.device, &self.surface_config);

        // resize the viewport.
        self.render_message_tx
            .send_blocking(RenderMessage::ResizeViewport(width, height))
            .unwrap();
    }

    fn handle_redraw(&mut self) {
        self.render_message_tx
            .send_blocking(RenderMessage::Render)
            .unwrap();

        match self.render_response_rx.try_recv() {
            Ok(response) => match response {
                RenderResponse::NewTextureView(texture_view) => {
                    let id = self
                        .egui_renderer
                        .register_texture(&self.device, &texture_view);
                    let _ = self.texture_id.insert(id);
                    // println!("New texture view is attempted!")
                }
                RenderResponse::RenderComplete => {
                    // println!("Rendered");
                }
            },
            Err(err) => match err {
                TryRecvError::Empty => {}
                TryRecvError::Closed => panic!("Disconnected"),
            },
        }

        match self.operation_response_rx.try_recv() {
            Ok(response) => match response {
                OperationResponse::Ongoing(_) => {
                    //update ui?
                }
                OperationResponse::Succeeded(text) => {
                    println!("Operation Success: {text}");
                    self.need_viewport_update = true;
                }
                OperationResponse::Failed(text, error) => {
                    println!("Operation Failed: {text} with error {error:?}");
                }
                OperationResponse::Aborted(text) => {
                    println!("Operation Cancelled: {text}");
                }
            },
            Err(err) => match err {
                TryRecvError::Empty => {}
                TryRecvError::Closed => panic!("Operation Manager is killed!!"),
            },
        }
    }
}

pub struct App {
    instance: wgpu::Instance,
    state: Option<AppState>,
    window: Option<Arc<Window>>,
    toolsheets_dock_tree: DockState<String>,
}

impl App {
    pub fn new() -> Self {
        let instance = egui_wgpu::wgpu::Instance::new(&wgpu::InstanceDescriptor::default());

        let mut toolsheets_dock_tree = DockState::new(vec![
            "Objects List".to_owned(),
            "Build Items List".to_owned(),
        ]);

        toolsheets_dock_tree.main_surface_mut().split_below(
            NodeIndex::root(),
            0.5,
            vec!["Object Tree".to_owned()],
        );

        Self {
            instance,
            state: None,
            window: None,
            toolsheets_dock_tree,
        }
    }

    async fn set_window(&mut self, window: Window) {
        let window = Arc::new(window);
        let initial_width = 1920;
        let initial_height = 1080;

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

            // take snapshot of previous frame data
            let prev_app_mode = state.current_app_mode;
            let prev_camera_data = state.camera_data.clone();

            let default_bbox = BoundingBox::default();
            let bbox = state.scene_bbox.as_ref().unwrap_or(&default_bbox);
            egui::TopBottomPanel::top("top panel")
                .resizable(false)
                .show(state.egui_renderer.context(), |ui| {
                    egui::MenuBar::new().ui(ui, |ui| {
                        if ui.button("Import Part").clicked() {
                            state.load_file_dlg.pick_file();
                        }

                        #[cfg(debug_assertions)]
                        if ui.button("Add Test Mesh").clicked() {
                            create_test_object(state.db.clone());
                            state.need_viewport_update = true;
                        }

                        ui.add_enabled_ui(!state.db.read_blocking().is_scene_empty(), |ui| {
                            if ui.button("Unzoom Scene").clicked() {
                                unzoom_bbox(&mut state.camera_data, bbox);
                            }

                            if ui.button("Clear All").clicked() {
                                let clear_ops = ClearDbOps;
                                if let Err(err) = state.operation_queue_tx.send_blocking(
                                    OperationMessage::AddSyncOperation(Box::new(clear_ops)),
                                ) {
                                    println!("{err:?}");
                                }
                            }

                            if ui.button("Save to 3mf").clicked() {
                                state.save_file_dlg.save_file();
                            }
                        });

                        //onyl show modes if there is a scene
                        if state.db.read_blocking().get_scene().is_ok() {
                            ui.with_layout(Layout::right_to_left(egui::Align::RIGHT), |ui| {
                                // ToDo: Add a tooltip here to explain the difference in modes
                                ui.radio_value(
                                    &mut state.current_app_mode,
                                    AppMode::Objects,
                                    "Objects Mode",
                                );

                                ui.radio_value(
                                    &mut state.current_app_mode,
                                    AppMode::Build,
                                    "Build Mode",
                                );
                            });
                        }
                    });
                });

            if prev_app_mode != state.current_app_mode {
                let tab_name = match state.current_app_mode {
                    AppMode::Objects => "Objects List",
                    AppMode::Build => "Build Items List",
                };

                if let Some((surface, node, _)) =
                    self.toolsheets_dock_tree.find_tab(&tab_name.to_owned())
                {
                    self.toolsheets_dock_tree
                        .set_focused_node_and_surface((surface, node))
                };
            }

            if let Some(tree) = &mut state.toolsheets {
                egui::SidePanel::left(Id::new("object list"))
                    .min_width(400.0)
                    .show(state.egui_renderer.context(), |ui| {
                        DockArea::new(&mut self.toolsheets_dock_tree)
                            .show_leaf_close_all_buttons(false)
                            .show_close_buttons(false)
                            .show_inside(ui, tree);
                    });
            }

            egui::CentralPanel::default().show(state.egui_renderer.context(), |ui| {
                match state.texture_id {
                    Some(id) => {
                        state.viewport_3d.ui(ui, id, &mut state.camera_data, bbox);
                    }
                    None => {
                        ui.label("Rendering Texture ID is missing!!");
                    }
                }
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
                    let ops = load_3mf::Load3MFOps { path };
                    if let Err(err) = state
                        .operation_queue_tx
                        .send_blocking(OperationMessage::AddAsyncOperation(Box::new(ops)))
                    {
                        println!("{err:?}");
                    }
                }
            }

            state.save_file_dlg.update(state.egui_renderer.context());
            if let Some(path) = state.save_file_dlg.take_picked() {
                println!("File path to save to is {path:?}");

                let ops = save_3mf::Save3mfOps { path };
                if let Err(err) = state
                    .operation_queue_tx
                    .send_blocking(OperationMessage::AddAsyncOperation(Box::new(ops)))
                {
                    println!("{err:?}");
                }
            }

            if state.current_app_mode != state.current_render_mode || state.need_viewport_update {
                // ToDo: Figure out a better way to do clear
                //state.renderer_3d.clear_all();
                let mut write_render_db = state.render_db.write_blocking();
                write_render_db.clear_all();
                drop(write_render_db);
                state.toolsheets = None;

                let read_db = state.db.read_blocking();
                if !read_db.is_empty() {
                    let (bbox, part_list, object_list, build_list) = match &state.current_app_mode {
                        AppMode::Objects => {
                            let bbox = add_render_items_from_unique_parts(
                                &state.device,
                                state.render_db.clone(),
                                state.db.clone(),
                            );
                            let part_list =
                                create_scene_tree_items_by_unique_parts(state.db.clone());
                            let object_list = create_objects_list(state.db.clone());
                            let build_list = create_build_items_list(state.db.clone());
                            (bbox, part_list, object_list, build_list)
                        }
                        AppMode::Build => {
                            let bbox = add_render_items_from_scene(
                                &state.device,
                                state.render_db.clone(),
                                state.db.clone(),
                            );
                            let part_list = create_build_items_list(state.db.clone());
                            let object_list = create_objects_list(state.db.clone());
                            let build_list = create_build_items_list(state.db.clone());
                            (bbox, part_list, object_list, build_list)
                        }
                    };

                    match (part_list, object_list, build_list) {
                        (Ok(part_list_items), Ok(object_items), Ok(build_items)) => match bbox {
                            Ok(bbox) => {
                                let part_list = PartList::new(part_list_items);
                                let _ = state.toolsheets.insert(Toolsheets::new(
                                    state.current_app_mode,
                                    part_list,
                                    TreeItemViewer::new(object_items, false),
                                    TreeItemViewer::new(build_items, false),
                                ));

                                //unzoom_bbox(&mut state.camera_data, &bbox);
                                let _ = state.scene_bbox.insert(bbox);

                                state.egui_renderer.context().request_repaint();
                            }
                            Err(err) => println!("{err:?}"),
                        },
                        _ => panic!("Something wrong here!!"),
                    }
                }

                state.need_viewport_update = false;
                state.current_render_mode = state.current_app_mode;
            }

            // updates from toolsheets
            if let Some(ref mut toolsheets) = state.toolsheets
                && toolsheets.has_selection_changed()
            {
                let selected_identifiables: Vec<_> = match state.current_app_mode {
                    AppMode::Objects => toolsheets.selected_objects().copied().collect(),
                    AppMode::Build => toolsheets.selected_build_items().copied().collect(),
                };

                let mut tree_items = vec![];
                for id in selected_identifiables {
                    let item = create_object_tree_from_identifiable(state.db.clone(), id).unwrap();
                    tree_items.push(item);
                }

                toolsheets.set_selected_identifiable_properties(TreeItemViewer {
                    childs: tree_items,
                    skip_inert_node: false,
                });

                toolsheets.clear_selection_changed();
            }

            //update the camera if the camera data is changed
            if prev_camera_data != state.camera_data
                && let Err(err) = state
                    .render_message_tx
                    .send_blocking(RenderMessage::UpdateCamera(state.camera_data.clone()))
            {
                println!("{err:?}");
            }

            //run the operation manager
            {
                state.operation_manager.run(
                    state.db.clone(),
                    state.executor.clone(),
                    state.render_message_tx.clone(),
                );
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

fn create_test_object(db: Arc<RwLock<Db>>) {
    let mesh = Mesh {
        vertices: ORDERED_POSITIONS
            .iter()
            .map(|p| Vec3::new(p.0[0], p.0[1], p.0[2]))
            .collect(),
        triangles: ORDERED_POSITIONS_TRI_EDGE_INDICES
            .iter()
            .map(|i| *i as u32)
            .collect(),
    };

    let mut db = db.write_blocking();
    let part_id = db
        .add_part_rep(crate::amrust_db::PartRep::Mesh(Box::new(mesh)))
        .unwrap();

    let instance_1 = db
        .make_new_part_instance_from_part(
            &part_id,
            Some(Transformation(Mat4::from_translation(
                (0.0, 5.0, 0.0).into(),
            ))),
        )
        .unwrap();

    db.add_part_instance_to_scene(&instance_1).unwrap();

    let instance_2 = db
        .make_new_part_instance_from_part(
            &part_id,
            Some(Transformation(Mat4::from_axis_angle(
                Vec3 {
                    x: 0.0,
                    y: 1.0,
                    z: 0.0,
                },
                45.0_f32.to_radians(),
            ))),
        )
        .unwrap();

    db.add_part_instance_to_scene(&instance_2).unwrap();
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
