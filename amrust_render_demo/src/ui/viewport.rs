use egui::{Image, UiBuilder, Vec2, epaint};
use smol::channel::Sender;

use crate::core::services::render_service::RenderServiceRequest;

use std::time::{self, Duration};

pub struct Viewport3D {
    prev_frame_size: egui::Vec2,
    last_request_for_resize: time::Instant,
}

impl Viewport3D {
    pub fn new(initial_width: u32, initial_height: u32) -> Self {
        Self {
            prev_frame_size: Vec2 {
                x: initial_width as f32,
                y: initial_height as f32,
            },
            last_request_for_resize: time::Instant::now(),
        }
    }
    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        texture_id: epaint::TextureId,
        render_service_request_sender: &Sender<RenderServiceRequest>,
    ) -> egui::Response {
        let size_we_want_to_use = ui.available_size();
        if size_we_want_to_use != self.prev_frame_size
            && (time::Instant::now() - self.last_request_for_resize) > Duration::from_millis(500)
        {
            if let Err(err) =
                render_service_request_sender.send_blocking(RenderServiceRequest::ResizeViewport(
                    size_we_want_to_use.x as u32,
                    size_we_want_to_use.y as u32,
                ))
            {
                log::error!("Unable to send a resize viewport request: {err:?}");
            } else {
                self.prev_frame_size = size_we_want_to_use;
                self.last_request_for_resize = time::Instant::now();
            }
        }

        let image_texture = Image::new((texture_id, size_we_want_to_use)).sense(egui::Sense::all());
        let ui_response = ui.centered_and_justified(|ui| {
            ui.scope_builder(UiBuilder::new().sense(egui::Sense::all()), |ui| {
                ui.add(image_texture)
            })
        });

        ui_response.inner.inner
    }
}
