use amrust_render::{bounding_box::BoundingBox, camera::CameraTransform};
use egui::{Image, UiBuilder, Vec2, epaint};
use glam::Mat4;
use smol::channel::Sender;
use smol::lock::RwLock;
use std::sync::Arc;

use crate::core::amrust_db::Db;
use crate::core::app_mode::AppMode;
use crate::core::services::picker_service::PickerService;
use crate::core::services::render_service::RenderServiceRequest;

pub struct Viewport3D {}

impl Viewport3D {
    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        texture_id: epaint::TextureId,
        render_service_request_sender: &Sender<RenderServiceRequest>,
        bbox: &BoundingBox,
        db: &Arc<RwLock<Db>>,
        app_mode: &AppMode,
        picker_service: &PickerService,
        current_view_proj: Mat4,
    ) {
        let size = ui.available_size() - Vec2::splat(10.0);
        let image_texture = Image::new((texture_id, size)).sense(egui::Sense::all());

        let ui_response = ui.scope_builder(UiBuilder::new().sense(egui::Sense::all()), |ui| {
            ui.add(image_texture)
        });

        let mut transforms: Vec<CameraTransform> = vec![];
        // Track scroll for zoom
        // use only the inner response to ensure mouse only responses to the viewport region
        if ui_response.inner.hovered() {
            let scroll_delta = ui.ctx().input(|i| i.raw_scroll_delta.y);
            if scroll_delta.abs() > 0.0 {
                transforms.push(CameraTransform::Zoom(scroll_delta * 0.0001));
            }
        }

        if ui_response.inner.dragged_by(egui::PointerButton::Secondary) {
            let delta = ui_response.inner.drag_delta();
            let drag_sensitivity = 0.01;
            transforms.push(CameraTransform::Rotate {
                pivot: bbox.center(),
                rotation_axis: glam::Vec3::Y,
                angle: -delta.x * drag_sensitivity,
            });

            transforms.push(CameraTransform::Rotate {
                pivot: bbox.center(),
                rotation_axis: glam::Vec3::X,
                angle: -delta.y * drag_sensitivity,
            });
        } else if ui_response.inner.dragged_by(egui::PointerButton::Middle) {
            let delta = ui_response.inner.drag_delta();
            let pan_sensitivity = 0.01;
            transforms.push(CameraTransform::Pan(
                glam::Vec3::new(-delta.x, -delta.y, 0.0) * pan_sensitivity,
            ));
        } else if ui_response.inner.clicked()
            && let Some(pos) = ui_response.inner.interact_pointer_pos()
            && let Some(db_read) = db.try_read()
        {
            let screen_point = glam::Vec2::new(pos.x, pos.y);
            let viewport_size = glam::Vec2::new(size.x, size.y);
            let radius = 0.1; // example radius
            let picks = picker_service.pick(
                &db_read,
                app_mode,
                screen_point,
                viewport_size,
                current_view_proj,
                radius,
            );
            log::info!("Picked entities: {:?}", picks);
        }

        if !transforms.is_empty()
            && let Err(err) = render_service_request_sender
                .send_blocking(RenderServiceRequest::TransformCamera(transforms))
        {
            log::error!("Error sending camera transform from Viewport: {err:?}");
        }
    }
}
