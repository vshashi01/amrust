use egui::{Image, UiBuilder, Vec2, epaint};
use smol::{
    channel::{Receiver, Sender, TryRecvError},
    lock::RwLock,
};
use statig::{
    Outcome::{Handled, Super, Transition},
    prelude::{InitializedStateMachine, IntoStateMachineExt, UninitializedStateMachine},
    state_machine,
};

use crate::{
    core::{
        amrust_db::Db,
        app_mode::AppMode,
        interfaces::db_view::DbView,
        services::render_service::{RenderServiceRequest, RenderServiceResponse},
    },
    db_view_model::DbViewModel,
    egui_tools::EguiRenderer,
    ui::{
        mouse_3d_manager::{Mouse3d, Mouse3dFrameContext, Mouse3dManager},
        mouse_camera_3d::MouseCamera3d,
        mouse_selection_3d::MouseSelection3d,
    },
};

use std::{
    sync::Arc,
    time::{self, Duration, Instant},
};

enum ViewportSm {
    Uninit(UninitializedStateMachine<ViewportStateMachine>),
    Init(InitializedStateMachine<ViewportStateMachine>),
}

pub struct Viewport3D {
    sm: Option<ViewportSm>,
}

impl Viewport3D {
    pub fn new(
        initial_width: u32,
        initial_height: u32,
        db: Arc<RwLock<Db>>,
        db_view_model: &DbViewModel,
        app_mode: AppMode,
        initial_view_proj: glam::Mat4,
        render_service_response_rx: Receiver<RenderServiceResponse>,
        render_service_request_tx: Sender<RenderServiceRequest>,
    ) -> Self {
        let initial_rect = egui::Rect::from_center_size(
            egui::Pos2::ZERO,
            egui::Vec2::new(initial_width as f32, initial_height as f32),
        );

        let mut mouse_manager = Mouse3dManager::new();
        let initial_mouse_frame_context = Mouse3dFrameContext {
            viewport_rect: initial_rect,
            view_proj: initial_view_proj,
            scene_bbox: db_view_model.get_total_visible_bbox(app_mode),
            parts_can_be_picked: db_view_model.get_all_parts_id(),
            instances_can_be_picked: db_view_model.get_instance_on_scene(),
            app_mode,
        };

        let camera_mouse = MouseCamera3d::get_initialized_sm(
            &initial_mouse_frame_context,
            &db,
            &render_service_request_tx,
        );
        mouse_manager.push(Mouse3d::Camera(camera_mouse));

        let selection_mouse = MouseSelection3d::get_initialized_sm(
            &initial_mouse_frame_context,
            &db,
            &render_service_request_tx,
        );
        mouse_manager.push(Mouse3d::Selection(selection_mouse));

        let viewport_sm = ViewportStateMachine::new(
            egui::Vec2::new(initial_width as f32, initial_height as f32),
            render_service_request_tx,
            render_service_response_rx,
            initial_view_proj,
            db,
            mouse_manager,
        )
        .uninitialized_state_machine();

        Self {
            sm: Some(ViewportSm::Uninit(viewport_sm)),
        }
    }

    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        db_view_model: &DbViewModel,
        app_mode: AppMode,
        egui_renderer: &mut EguiRenderer,
        device: &wgpu::Device,
    ) {
        let mut context = ViewportFrameContext {
            ui,
            db_view_model,
            app_mode,
            egui_renderer,
            device,
        };

        if let Some(sm) = self.sm.take() {
            match sm {
                ViewportSm::Uninit(uninit_sm) => {
                    let mut init_sm = uninit_sm.init_with_context(&mut context);
                    init_sm.handle_with_context(&ViewportEvt::Frame, &mut context);
                    self.sm = Some(ViewportSm::Init(init_sm));
                }
                ViewportSm::Init(mut init_sm) => {
                    init_sm.handle_with_context(&ViewportEvt::Frame, &mut context);
                    self.sm = Some(ViewportSm::Init(init_sm));
                }
            }
        }
    }
}

#[allow(unused)]
enum ViewportEvt {
    Frame,
    PushMouse(Mouse3d),
    PopTopMouse,
}

struct ViewportData {
    prev_frame_size: Vec2,
    render_service_request_tx: Sender<RenderServiceRequest>,
    render_service_response_rx: Receiver<RenderServiceResponse>,
    view_proj: glam::Mat4,
    db: Arc<RwLock<Db>>,
    mouse_manager: Mouse3dManager,
}

impl ViewportData {
    fn drain_render_responses_and_return_new_texture(&mut self) -> Option<wgpu::TextureView> {
        let mut new_render_texture: Option<wgpu::TextureView> = None;

        loop {
            match self.render_service_response_rx.try_recv() {
                Ok(response) => match response {
                    RenderServiceResponse::NewTextureView(texture_view) => {
                        let _ = new_render_texture.insert(texture_view);
                    }
                    RenderServiceResponse::NewView(view_proj) => {
                        self.view_proj = view_proj;
                    }
                    RenderServiceResponse::RenderComplete => {}
                },
                Err(err) => match err {
                    TryRecvError::Empty => break,
                    TryRecvError::Closed => panic!("Disconnected from render service"),
                },
            }
        }

        new_render_texture
    }

    fn request_resize(&self, size: Vec2) {
        if let Err(err) =
            self.render_service_request_tx
                .try_send(RenderServiceRequest::ResizeViewport(
                    size.x as u32,
                    size.y as u32,
                ))
        {
            log::error!("Unable to send a resize viewport request: {err:?}");
        }
    }

    fn request_render(&self) {
        if let Err(err) = self
            .render_service_request_tx
            .try_send(RenderServiceRequest::Render)
        {
            log::error!("Unabled to send a render request: {err:?}");
        }
    }
}

struct ViewportFrameContext<'a> {
    ui: &'a mut egui::Ui,
    db_view_model: &'a DbViewModel,
    app_mode: AppMode,
    egui_renderer: &'a mut EguiRenderer,
    device: &'a wgpu::Device,
}

struct ViewportStateMachine {
    data: ViewportData,
}

impl ViewportStateMachine {
    fn new(
        initial_frame_size: egui::Vec2,
        render_service_request_tx: Sender<RenderServiceRequest>,
        render_service_response_rx: Receiver<RenderServiceResponse>,
        view_proj: glam::Mat4,
        db: Arc<RwLock<Db>>,
        mouse_manager: Mouse3dManager,
    ) -> Self {
        Self {
            data: ViewportData {
                prev_frame_size: initial_frame_size,
                render_service_request_tx,
                render_service_response_rx,
                view_proj,
                db,
                mouse_manager,
            },
        }
    }
}

#[state_machine(initial = "State::no_texture()")]
impl ViewportStateMachine {
    #[superstate]
    fn core(&mut self, event: &ViewportEvt) -> statig::Outcome<State> {
        let next_state = match event {
            ViewportEvt::PushMouse(mouse) => {
                self.data.mouse_manager.push_clone(mouse);
                Handled
            }
            ViewportEvt::PopTopMouse => {
                self.data.mouse_manager.pop();
                Handled
            }
            _ => Handled,
        };

        self.data.request_render();
        next_state
    }

    #[state(superstate = "core")]
    fn no_texture(
        &mut self,
        context: &mut ViewportFrameContext<'_>,
        event: &ViewportEvt,
    ) -> statig::Outcome<State> {
        match event {
            ViewportEvt::Frame => {
                let next_state = if let Some(texture_view) =
                    self.data.drain_render_responses_and_return_new_texture()
                {
                    let texture_id = context
                        .egui_renderer
                        .register_new_texture(context.device, &texture_view);

                    Transition(State::Render { texture_id })
                } else {
                    context.ui.label("Renderer uninitialized...");
                    Handled
                };

                self.data.request_render();
                next_state
            }
            _ => Super,
        }
    }

    #[state(superstate = "core")]
    fn render(
        &mut self,
        texture_id: &epaint::TextureId,
        context: &mut ViewportFrameContext<'_>,
        event: &ViewportEvt,
    ) -> statig::Outcome<State> {
        match event {
            ViewportEvt::Frame => {
                if let Some(texture_view) =
                    self.data.drain_render_responses_and_return_new_texture()
                {
                    context.egui_renderer.update_existing_texture(
                        texture_id,
                        context.device,
                        &texture_view,
                    );
                }

                let size_we_want_to_use = context.ui.available_size();
                let next_state = if size_we_want_to_use != self.data.prev_frame_size {
                    self.data.request_resize(size_we_want_to_use);
                    Transition(State::NotSized {
                        texture_id: *texture_id,
                        last_resize_requested_time: Instant::now(),
                        last_requested_frame_size: size_we_want_to_use,
                    })
                } else {
                    let image =
                        Image::new((*texture_id, size_we_want_to_use)).sense(egui::Sense::all());
                    let ui_response = context.ui.centered_and_justified(|ui| {
                        ui.scope_builder(UiBuilder::new().sense(egui::Sense::all()), |ui| {
                            ui.add(image)
                        })
                    });
                    let image_response = ui_response.inner.inner;

                    let frame_context = Mouse3dFrameContext {
                        viewport_rect: image_response.rect,
                        view_proj: self.data.view_proj,
                        scene_bbox: context
                            .db_view_model
                            .get_total_visible_bbox(context.app_mode),
                        parts_can_be_picked: context.db_view_model.get_all_parts_id(),
                        instances_can_be_picked: context.db_view_model.get_instance_on_scene(),
                        app_mode: context.app_mode,
                    };

                    self.data.mouse_manager.run(
                        &image_response,
                        context.egui_renderer.context(),
                        frame_context,
                        &self.data.db,
                        &self.data.render_service_request_tx,
                    );

                    Handled
                };

                self.data.request_render();
                next_state
            }
            _ => Super,
        }
    }

    #[state(superstate = "core")]
    fn not_sized(
        &mut self,
        texture_id: &epaint::TextureId,
        last_requested_frame_size: &egui::Vec2,
        last_resize_requested_time: &time::Instant,
        context: &mut ViewportFrameContext<'_>,
        event: &ViewportEvt,
    ) -> statig::Outcome<State> {
        match event {
            ViewportEvt::Frame => {
                let next_state = if let Some(texture_view) =
                    self.data.drain_render_responses_and_return_new_texture()
                {
                    let current_target_size = context.ui.available_size();
                    if *last_requested_frame_size == current_target_size {
                        context.egui_renderer.update_existing_texture(
                            texture_id,
                            context.device,
                            &texture_view,
                        );
                        self.data.prev_frame_size = current_target_size;
                        Transition(State::Render {
                            texture_id: *texture_id,
                        })
                    } else if *last_requested_frame_size != current_target_size
                        && (Instant::now() - *last_resize_requested_time)
                            >= Duration::from_millis(500)
                    {
                        self.data.request_resize(current_target_size);
                        Transition(State::NotSized {
                            texture_id: *texture_id,
                            last_requested_frame_size: current_target_size,
                            last_resize_requested_time: Instant::now(),
                        })
                    } else {
                        Handled
                    }
                } else {
                    Handled
                };

                self.data.request_render();
                next_state
            }
            _ => Super,
        }
    }
}
