use crate::core::amrust_db::Db;
use crate::core::app_mode::AppMode;
use crate::core::interfaces::command::CommandContext;
use crate::core::interfaces::operation::OperationResponse;
use crate::core::render_db::RenderDb;
use crate::core::services::command_service::CommandService;
use crate::core::services::dialog_service::{DialogService, DialogServiceRequest};
use crate::core::services::file_dialog_service::FileDialogRequest;
use crate::core::services::file_dialog_service::FileDialogService;
use crate::core::services::operation_service::{
    OperationService, OperationServiceError, OperationServiceRequest,
};
use crate::core::services::render_service::{
    RenderService, RenderServiceRequest, RendererSettings,
};
use crate::core::types::identifiable::Identifiable;
use crate::db_view_model::{
    self, DbViewModel, create_build_items_list, create_build_items_list_new,
    create_object_tree_from_identifiable, create_objects_list,
    create_scene_tree_items_by_unique_parts,
};
use crate::egui_tools::EguiRenderer;
use crate::features::{clear_db, load_3mf, save_3mf, unload, unzoom_scene};
use crate::ui::part_list::PartList;
use crate::ui::toolsheets::Toolsheets;
use crate::ui::tree_item_viewer::TreeItemViewer;
use crate::ui::tree_table::TreeTableViewer;
use crate::ui::viewport::Viewport3D;
use crate::view_models::build_items::BuildItemsModel;
// use amrust_lib::widgets::dropped_files::DroppedFilesWidget;
use amrust_render::camera::{self, CameraData, OrthographicCameraData};
// use amrust_render::normalized_box::{ORDERED_POSITIONS, ORDERED_POSITIONS_TRI_EDGE_INDICES};
use egui::{Align2, Direction, Id, Layout};
use egui_dock::{DockArea, DockState, NodeIndex};
use egui_toast::{Toast, ToastOptions};
use egui_wgpu::wgpu::SurfaceError;
use egui_wgpu::{ScreenDescriptor, wgpu};
use glam::Vec3;
use smol::channel::{Receiver, Sender, TryRecvError};
use smol::lock::RwLock;
use smol::{Executor, channel};
use std::sync::Arc;
use std::time::Duration;
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::window::{Window, WindowId};

use amrust_render::renderer;

struct AppState {
    pub device: Arc<wgpu::Device>,
    pub queue: Arc<wgpu::Queue>,
    pub surface_config: wgpu::SurfaceConfiguration,
    pub surface: wgpu::Surface<'static>,
    pub dpi_factor: f32,
    pub egui_renderer: EguiRenderer,
    // pub dropped_files: DroppedFilesWidget,
    pub db: Arc<RwLock<Db>>,
    pub toolsheets: Option<Toolsheets>,
    pub viewport_3d: Viewport3D,
    pub current_app_mode: AppMode,
    pub current_render_mode: AppMode,
    pub need_viewport_update: bool,

    pub executor: Arc<Executor<'static>>,
    pub render_message_tx: Sender<RenderServiceRequest>,
    pub render_db: Arc<RwLock<RenderDb>>,
    pub db_view_model: DbViewModel,
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
                        log::debug!("Trying to run executor");
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

        let mut render_worker = RenderService::new(
            renderer_settings,
            render_db.clone(),
            render_message_rx,
            render_response_tx.clone(),
        )
        .await;

        {
            executor
                .spawn(async move {
                    log::debug!("Attempted to println in separate thread");
                    render_worker.run().await;
                })
                .detach();
        }

        let db = Arc::new(RwLock::new(Db::new()));
        let db_view_model = DbViewModel::new();

        let viewport_3d = Viewport3D::new(
            (width, height),
            db.clone(),
            &db_view_model,
            AppMode::Build,
            camera_data.get_view_projection(),
            (render_response_rx.clone(), render_message_tx.clone()),
        );

        Self {
            device: Arc::new(device),
            queue: Arc::new(queue),
            surface,
            surface_config,
            dpi_factor,
            egui_renderer,
            // dropped_files: dropped_files_widget,
            db,
            toolsheets: None,
            viewport_3d,
            current_app_mode: AppMode::Build,
            current_render_mode: AppMode::Build,
            need_viewport_update: false,
            executor,
            render_message_tx,
            render_db,
            db_view_model,
        }
    }

    fn resize_surface(&mut self, width: u32, height: u32) {
        self.surface_config.width = width;
        self.surface_config.height = height;
        self.surface.configure(&self.device, &self.surface_config);
    }

    fn handle_redraw(&mut self) {}
}

pub struct App {
    instance: wgpu::Instance,
    state: Option<AppState>,
    window: Option<Arc<Window>>,
    toolsheets_dock_tree: DockState<String>,
    file_dialog_service: FileDialogService,
    file_dialog_service_request_tx: Sender<FileDialogRequest>,
    command_service: CommandService,
    operation_manager: OperationService,
    operation_queue_tx: Sender<OperationServiceRequest>,
    operation_response_rx: Receiver<OperationResponse>,
    operation_error_rx: Receiver<OperationServiceError>,
    toasts: egui_toast::Toasts,
    dialog_service: DialogService,
    dialog_request_tx: Sender<DialogServiceRequest>,
}

impl App {
    pub fn new() -> Self {
        egui_logger::builder().init().unwrap();
        let instance = egui_wgpu::wgpu::Instance::new(&wgpu::InstanceDescriptor::default());

        let toolsheets_dock_tree = DockState::new(vec![]);

        let mut command_service = CommandService::new();
        load_3mf::register_commands(&mut command_service);
        save_3mf::register_commands(&mut command_service);
        clear_db::register_commands(&mut command_service);
        unzoom_scene::register_commands(&mut command_service);
        unload::register_commands(&mut command_service);

        let (file_dialog_service_request_tx, file_dialog_service_request_rx) = channel::unbounded();
        let file_dialog_service = FileDialogService::new(file_dialog_service_request_rx);

        let (operation_queue_tx, operation_queue_rx) = channel::unbounded();
        let (operation_response_tx, operation_response_rx) = channel::unbounded();
        let (operation_error_tx, operation_error_rx) = channel::unbounded();
        let operation_manager = OperationService::new(
            operation_queue_rx,
            operation_response_tx,
            operation_error_tx,
        );

        let (dialog_service_tx, dialog_service_rx) = channel::unbounded();
        let dialog_service = DialogService::new(dialog_service_rx);

        Self {
            instance,
            state: None,
            window: None,
            toolsheets_dock_tree,
            file_dialog_service,
            file_dialog_service_request_tx,
            command_service,
            operation_manager,
            operation_queue_tx,
            operation_response_rx,
            operation_error_rx,
            toasts: egui_toast::Toasts::new()
                .anchor(Align2::RIGHT_TOP, (-10.0, 10.0))
                .direction(Direction::TopDown),
            dialog_service,
            dialog_request_tx: dialog_service_tx,
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
            // info!("Window is minimized");
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
                log::error!("wgpu surface outdated");
                return;
            }
            Err(err) => {
                log::error!("{err:?}");
                // surface_texture.expect("Failed to acquire next swap chain texture");
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

            if let Some(handler) = self
                .file_dialog_service
                .update_and_return_first_picked(state.egui_renderer.context())
            {
                let mut command_context = CommandContext::new(
                    &state.db_view_model,
                    state.current_app_mode,
                    self.operation_queue_tx.clone(),
                    self.file_dialog_service_request_tx.clone(),
                    state.render_message_tx.clone(),
                );

                handler.handle_and_close_dialog(&mut command_context);
            }

            let operable_selected_identifiables = state
                .db_view_model
                .get_all_operable_selected_identifiables()
                .collect::<Vec<_>>();
            egui::TopBottomPanel::top("top panel")
                .resizable(false)
                .show(state.egui_renderer.context(), |ui| {
                    ui.horizontal(|ui| {
                        let mut command_context = CommandContext::new(
                            &state.db_view_model,
                            state.current_app_mode,
                            self.operation_queue_tx.clone(),
                            self.file_dialog_service_request_tx.clone(),
                            state.render_message_tx.clone(),
                        );

                        self.command_service
                            .populate_menu_bar(ui, &mut command_context);

                        //onyl show modes if there is a scene
                        if !state.db_view_model.is_empty() {
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

                                #[cfg(debug_assertions)]
                                if ui.button("Show error dialog").clicked() {
                                    let _ = self.dialog_request_tx.send_blocking(DialogServiceRequest::Error("Custom Error dialog", "You made a grave error!"));
                                }

                                #[cfg(debug_assertions)]
                                if ui.button("Show info dialog").clicked() {
                                    let _ = self.dialog_request_tx.send_blocking(DialogServiceRequest::Info("Custom Info dialog", "You wanted to see some info!"));
                                }

                                #[cfg(debug_assertions)]
                                if ui.button("Show operation dialog").clicked() {
                                    let ctx = state.egui_renderer.context().clone();
                                    let _ = self.dialog_request_tx.send_blocking(
                                        DialogServiceRequest::SemiModalDialog(
                                            "Test Operation Dialog",
                                            Box::new( move || {
                                                use crate::core::services::dialog_service::DialogStateInNextFrame;

                                                let mut result = DialogStateInNextFrame::Show;

                                                egui::Window::new("Test Operation Dialog").show(
                                                    &ctx,
                                                    |ui| {
                                                        ui.label("This is a new dialog");

                                                        if ui.button("Close this dialog").clicked()
                                                        {
                                                            result = DialogStateInNextFrame::Closed;
                                                        }
                                                    },
                                                );
                                                result
                                            }),
                                        ),
                                    );
                                }
                            });
                        }
                    });
                });

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

            egui::Window::new("Log Window")
                .resizable(true)
                .anchor(Align2::RIGHT_BOTTOM, (10.0, 10.0))
                .collapsible(true)
                .default_open(false)
                .show(state.egui_renderer.context(), |ui| {
                    egui::ScrollArea::both().max_height(250.0).show(ui, |ui| {
                        egui_logger::LoggerUi::default().show(ui);
                    });
                });

            // state
            //     .dropped_files
            //     .run(state.egui_renderer.context(), &|test| false);

            let has_changes = {
                let read_db = state.db.read_blocking();
                if read_db.has_any_changes() {
                    true
                } else {
                    // hack to refresh Ui when either one it is not matching the other on empty
                    state.db_view_model.is_empty() != read_db.is_empty()
                }
            };

            if has_changes {
                let temp_db = state.db.clone();
                let temp_render_db = state.render_db.clone();
                let moved_cache = state.db_view_model.clone();
                let device = state.device.clone();

                let new_db_cache = smol::block_on(async {
                    db_view_model::update_view_model_from_db_new(
                        temp_db,
                        temp_render_db,
                        moved_cache,
                        device,
                    )
                    .await
                });

                match new_db_cache {
                    Ok(cache) => {
                        state.db_view_model = cache;
                        state.need_viewport_update = true;
                    }
                    Err(_) => log::error!("Updating data went wrong!"),
                }
            }

            if state.current_app_mode != state.current_render_mode || state.need_viewport_update {
                if state.db_view_model.is_empty() {
                    let mut write_render_db = state.render_db.write_blocking();
                    write_render_db.clear_all();
                }

                state.toolsheets = None;

                self.toolsheets_dock_tree = {
                    let primary_tab = match state.current_app_mode {
                        AppMode::Objects => "Objects List".to_owned(),
                        AppMode::Build => "Build Items List".to_owned(),
                    };
                    let mut dock_tree = DockState::new(vec![primary_tab]);
                    dock_tree.main_surface_mut().split_below(
                        NodeIndex::root(),
                        0.5,
                        vec!["Object Tree".to_owned()],
                    );
                    dock_tree
                };

                let (render_objects, part_list_items, object_items, build_items) =
                    match &state.current_app_mode {
                        AppMode::Objects => {
                            let render_objects = state
                                .db_view_model
                                .get_unique_parts_based_render_object_ids()
                                .cloned()
                                .collect::<Vec<_>>();

                            let part_list =
                                create_scene_tree_items_by_unique_parts(&state.db_view_model);
                            let object_list = create_objects_list(&state.db_view_model);
                            let build_list = create_build_items_list_new(&state.db_view_model);
                            (render_objects, part_list, object_list, build_list)
                        }
                        AppMode::Build => {
                            let render_objects = state
                                .db_view_model
                                .get_scene_based_render_object_ids()
                                .cloned()
                                .collect::<Vec<_>>();
                            let part_list = create_build_items_list(&state.db_view_model);
                            let object_list = create_objects_list(&state.db_view_model);
                            let build_list = create_build_items_list_new(&state.db_view_model);
                            (render_objects, part_list, object_list, build_list)
                        }
                    };

                //update render objects
                {
                    let mut write_render_db = state.render_db.write_blocking();
                    write_render_db.set_render_objects(&render_objects);
                }

                let part_list = PartList::new(part_list_items);

                let build_model = BuildItemsModel::new(&build_items);
                let mut toolsheets = Toolsheets::new(
                    part_list,
                    TreeItemViewer::new(object_items, false, true),
                    TreeTableViewer::new(build_model),
                );

                // Restore selections in toolsheets based on mode
                match state.current_app_mode {
                    AppMode::Objects => {
                        toolsheets.override_selected_object(
                            &operable_selected_identifiables
                                .iter()
                                .filter(|id| matches!(id, Identifiable::Part(_)))
                                .cloned()
                                .collect::<Vec<_>>(),
                        );
                    }
                    AppMode::Build => {
                        toolsheets.override_selected_build_items(
                            &operable_selected_identifiables
                                .iter()
                                .filter(|id| matches!(id, Identifiable::PartInstance(_)))
                                .cloned()
                                .collect::<Vec<_>>(),
                        );
                    }
                }
                // Update properties panel
                let mut tree_items = vec![];
                for id in &operable_selected_identifiables {
                    if let Some(item) =
                        create_object_tree_from_identifiable(&state.db_view_model, id)
                    {
                        tree_items.push(item);
                    }
                }
                toolsheets.set_selected_identifiable_properties(TreeItemViewer {
                    childs: tree_items,
                    skip_inert_node: false,
                    clear_selections_on_empty_area_click: false,
                });

                let _ = state.toolsheets.insert(toolsheets);

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

                log::debug!("Selected identifiables are: {selected_identifiables:?}");

                let mut tree_items = vec![];
                for id in &selected_identifiables {
                    if let Some(item) =
                        create_object_tree_from_identifiable(&state.db_view_model, id)
                    {
                        tree_items.push(item);
                    }
                }

                state.db_view_model.clear_selected_identifiables();
                for identifiable in selected_identifiables {
                    state.db_view_model.add_selected_identifiable(identifiable);
                }

                toolsheets.set_selected_identifiable_properties(TreeItemViewer {
                    childs: tree_items,
                    skip_inert_node: false,
                    clear_selections_on_empty_area_click: false,
                });

                toolsheets.clear_selection_changed();
            }

            if let Some(ref mut toolsheets) = state.toolsheets {
                toolsheets.set_blocked_entities(
                    &state
                        .db_view_model
                        .get_detached_identifiables()
                        .collect::<Vec<_>>(),
                );
            }

            //run the operation manager
            {
                self.operation_manager.run(&state.db, &state.executor);

                if let Some(op) = self.operation_manager.get_modal_operation() {
                    let _ = egui::Modal::new(egui::Id::new("app modal")).show(
                        state.egui_renderer.context(),
                        |ui| {
                            ui.label(format!("{} operation is running!", op.name));
                            ui.add(egui::ProgressBar::new(0.0).animate(true));
                        },
                    );
                }

                let background_ops = self
                    .operation_manager
                    .get_all_background_operation()
                    .collect::<Vec<_>>();

                let queued_ops = self
                    .operation_manager
                    .get_queued_operations()
                    .collect::<Vec<_>>();

                egui::TopBottomPanel::bottom(Id::new("bottom panel"))
                    .resizable(true)
                    .max_height(300.0)
                    .show(state.egui_renderer.context(), |ui| {
                        ui.vertical(|ui| {
                            if background_ops.is_empty() {
                                ui.label("No background operations running");
                            } else {
                                ui.label(format!("BG Operations Count: {} ", background_ops.len()));
                                for ops in background_ops {
                                    ui.horizontal(|ui| {
                                        ui.label(format!("Operation: {}", ops.name));
                                        ui.add(egui::ProgressBar::new(0.0).animate(true));
                                    });
                                }
                            }
                        });

                        ui.separator();

                        ui.horizontal(|ui| {
                            ui.label("Queued Operations: ");

                            if queued_ops.is_empty() {
                                ui.label("No operations are currently queued!");
                            } else {
                                for ops in queued_ops {
                                    ui.label(ops.name());
                                }
                            }
                        })
                    });
            }

            egui::CentralPanel::default().show(&state.egui_renderer.context().clone(), |ui| {
                // match state.texture_id {
                //     Some(id) => {
                state.viewport_3d.ui(
                    ui,
                    &state.db_view_model,
                    state.current_app_mode,
                    &mut state.egui_renderer,
                    &state.device,
                );
                // }
                //     None => {
                //         ui.label("Rendering Texture ID is missing!!");
                //     }
                // }

                self.toasts.show(state.egui_renderer.context());
            });

            match self.operation_response_rx.try_recv() {
                Ok(response) => {
                    match response {
                        OperationResponse::Succeeded { name } => {
                            log::info!("Operation Success: {name}");
                            self.toasts.add(
                                Toast::new()
                                    .kind(egui_toast::ToastKind::Success)
                                    .text(format!("Operation Succeeded: {name}"))
                                    .options(
                                        ToastOptions::default()
                                            .duration(Duration::from_secs(1))
                                            .show_icon(true),
                                    ),
                            );
                        }
                        OperationResponse::Failed { name, error, .. } => {
                            log::info!("Operation Failed: {name} with error {error:?}");
                            self.toasts.add(
                                Toast::new()
                                    .kind(egui_toast::ToastKind::Error)
                                    .text(format!("Operation Failed: {name}"))
                                    .options(
                                        ToastOptions::default()
                                            .duration(Duration::from_secs(3))
                                            .show_icon(true),
                                    ),
                            );
                        }
                        OperationResponse::Aborted { name, .. } => {
                            log::info!("Operation Cancelled: {name}");
                            self.toasts.add(
                                Toast::new()
                                    .kind(egui_toast::ToastKind::Warning)
                                    .text(format!("Operation Cancelled: {name}"))
                                    .options(
                                        ToastOptions::default()
                                            .duration(Duration::from_secs(1))
                                            .show_icon(true),
                                    ),
                            );
                        }
                    }
                    state.need_viewport_update = true;
                }
                Err(err) => match err {
                    TryRecvError::Empty => {}
                    TryRecvError::Closed => panic!("Operation Manager is killed!!"),
                },
            }

            match self.operation_error_rx.try_recv() {
                Ok(error) => match error {
                    OperationServiceError::ModalOpImmediateFailed(message) => {
                        self.dialog_service
                            .add_error_dialog("Operation Service Error", message);
                    }
                },
                Err(err) => match err {
                    TryRecvError::Empty => {}
                    TryRecvError::Closed => panic!("Operation error channel closed!!"),
                },
            }

            self.dialog_service
                .update_and_show_dialogs(state.egui_renderer.context());

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
                log::info!("The close button was pressed; stopping");
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
            //     info!("Hovered file cancelled")
            // }
            // WindowEvent::HoveredFile(filepath) => {
            //     info!("File hovered");
            // }
            // WindowEvent::DroppedFile(filepath) => {
            //     info!("File dropped");
            // }
            _ => (),
        }
    }
}

// fn create_test_object(db: Arc<RwLock<Db>>) {
//     let mesh = Mesh {
//         vertices: ORDERED_POSITIONS
//             .iter()
//             .map(|p| Vec3::new(p.0[0], p.0[1], p.0[2]))
//             .collect(),
//         triangles: ORDERED_POSITIONS_TRI_EDGE_INDICES
//             .iter()
//             .map(|i| *i as u32)
//             .collect(),
//     };

//     let mut db = db.write_blocking();
//     let part_id = db
//         .add_part_rep(crate::amrust_db::PartRep::Mesh(Box::new(mesh)))
//         .unwrap();

//     let instance_1 = db
//         .make_new_part_instance_from_part(
//             &part_id,
//             Some(Transformation(Mat4::from_translation(
//                 (0.0, 5.0, 0.0).into(),
//             ))),
//         )
//         .unwrap();

//     db.add_part_instance_to_scene(&instance_1).unwrap();

//     let instance_2 = db
//         .make_new_part_instance_from_part(
//             &part_id,
//             Some(Transformation(Mat4::from_axis_angle(
//                 Vec3 {
//                     x: 0.0,
//                     y: 1.0,
//                     z: 0.0,
//                 },
//                 45.0_f32.to_radians(),
//             ))),
//         )
//         .unwrap();

//     db.add_part_instance_to_scene(&instance_2).unwrap();
// }

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
