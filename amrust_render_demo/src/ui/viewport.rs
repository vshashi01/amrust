use egui::{Image, UiBuilder, epaint};
use smol::{
    channel::{Receiver, Sender, TryRecvError},
    lock::RwLock,
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
        mouse_3d_manager::{Mouse3dFrameContext, Mouse3dManager},
        mouse_camera_3d::MouseCamera3d,
        mouse_selection_3d::MouseSelection3d,
    },
};

use std::{
    sync::Arc,
    time::{self, Duration},
};

pub struct Viewport3D {
    texture_id: Option<epaint::TextureId>,
    prev_frame_size: egui::Rect,
    last_request_for_resize: time::Instant,
    render_service_request_tx: Sender<RenderServiceRequest>,
    render_service_response_rx: Receiver<RenderServiceResponse>,
    prev_view_proj: glam::Mat4,
    db: Arc<RwLock<Db>>,
    mouse_manager: Mouse3dManager,
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
        let rect = egui::Rect::from_center_size(
            egui::Pos2::ZERO,
            egui::Vec2::new(initial_width as f32, initial_height as f32),
        );
        let mut mouse_manager = Mouse3dManager::new();

        let frame_context = Mouse3dFrameContext {
            viewport_rect: rect,
            view_proj: initial_view_proj,
            scene_bbox: db_view_model.get_total_visible_bbox(app_mode),
            parts_can_be_picked: db_view_model.get_all_parts_id(),
            instances_can_be_picked: db_view_model.get_instance_on_scene(),
            app_mode,
        };

        let camera_mouse =
            MouseCamera3d::get_initialized_sm(&frame_context, &db, &render_service_request_tx);
        mouse_manager.push(Box::new(camera_mouse));

        let selection_mouse =
            MouseSelection3d::get_initialized_sm(&frame_context, &db, &render_service_request_tx);
        mouse_manager.push(Box::new(selection_mouse));

        Self {
            prev_frame_size: rect,
            last_request_for_resize: time::Instant::now(),
            render_service_request_tx,
            render_service_response_rx,
            db,
            mouse_manager,
            prev_view_proj: initial_view_proj,
            texture_id: None,
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
        let mut new_render_texture: Option<wgpu::TextureView> = None;
        loop {
            match self.render_service_response_rx.try_recv() {
                Ok(response) => match response {
                    RenderServiceResponse::NewTextureView(texture_view) => {
                        let _ = new_render_texture.insert(texture_view);
                    }
                    RenderServiceResponse::NewView(view_proj) => {
                        self.prev_view_proj = view_proj;
                    }
                    RenderServiceResponse::RenderComplete => {}
                },
                Err(err) => match err {
                    TryRecvError::Empty => {
                        break;
                    }
                    TryRecvError::Closed => panic!("Disconnected from render service"),
                },
            }
        }
        // register the latest texture view
        if let Some(texture_view) = new_render_texture.take() {
            let id = egui_renderer.register_texture(device, &texture_view);
            let _ = self.texture_id.insert(id);
        }

        if self.texture_id.is_none() {
            ui.label("Rendering is missing");
            return;
        }

        self.render_service_request_tx
            .try_send(RenderServiceRequest::Render);

        let rect_we_want_to_use = ui.available_rect_before_wrap();
        let size_we_want_to_use = rect_we_want_to_use.max - rect_we_want_to_use.min;
        if rect_we_want_to_use != self.prev_frame_size
            && (time::Instant::now() - self.last_request_for_resize) > Duration::from_millis(500)
        {
            if let Err(err) =
                self.render_service_request_tx
                    .send_blocking(RenderServiceRequest::ResizeViewport(
                        size_we_want_to_use.x as u32,
                        size_we_want_to_use.y as u32,
                    ))
            {
                log::error!("Unable to send a resize viewport request: {err:?}");
            } else {
                self.prev_frame_size = rect_we_want_to_use;
                self.last_request_for_resize = time::Instant::now();
            }
        }

        let image_texture =
            Image::new((self.texture_id.unwrap(), size_we_want_to_use)).sense(egui::Sense::all());
        let ui_response = ui.centered_and_justified(|ui| {
            ui.scope_builder(UiBuilder::new().sense(egui::Sense::all()), |ui| {
                // ui.label("something went wrong");
                ui.add(image_texture)
            })
        });

        let frame_context = Mouse3dFrameContext {
            viewport_rect: self.prev_frame_size,
            view_proj: self.prev_view_proj,
            scene_bbox: db_view_model.get_total_visible_bbox(app_mode),
            parts_can_be_picked: db_view_model.get_all_parts_id(),
            instances_can_be_picked: db_view_model.get_instance_on_scene(),
            app_mode,
        };

        self.mouse_manager.run(
            &ui_response.inner.inner,
            egui_renderer.context(),
            frame_context,
            &self.db,
            &self.render_service_request_tx,
        );
    }
}
