use amrust_render::{bounding_box::BoundingBox, camera::CameraTransform};
use egui::{Image, UiBuilder, Vec2, epaint};
use log::info;
use smol::channel::Sender;

use crate::core::services::render_service::RenderServiceRequest;

pub struct Viewport3D {}

impl Viewport3D {
    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        texture_id: epaint::TextureId,
        render_service_request_sender: &Sender<RenderServiceRequest>,
        bbox: &BoundingBox,
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
            // info!("Drag delta is: {:?}", delta);
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
        } else if ui_response.inner.clicked() {
            info!("Clicked in the region");
        }

        if !transforms.is_empty()
            && let Err(err) = render_service_request_sender
                .send_blocking(RenderServiceRequest::TransformCamera(transforms))
        {
            log::error!("Error sending camera transform from Viewport: {err:?}");
        }
    }
}
