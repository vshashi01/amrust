use amrust_render::{
    bounding_box::BoundingBox,
    camera::{self, CameraData},
};
use egui::{Image, UiBuilder, Vec2, epaint};

pub struct Viewport3D {}

impl Viewport3D {
    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        texture_id: epaint::TextureId,
        camera_data: &mut impl CameraData,
        bbox: &BoundingBox,
    ) {
        let size = ui.available_size() - Vec2::splat(10.0);
        let image_texture = Image::new((texture_id, size)).sense(egui::Sense::all());

        let ui_response = ui.scope_builder(UiBuilder::new().sense(egui::Sense::all()), |ui| {
            ui.add(image_texture)
        });

        // Track scroll for zoom
        // use only the inner response to ensure mouse only responses to the viewport region
        if ui_response.inner.ctx.input(|i| i.raw_scroll_delta.y != 0.0) {
            let scroll = ui.ctx().input(|i| i.raw_scroll_delta.y);
            camera_data.transform(camera::CameraTransform::Zoom(scroll * 0.0001));
            ui.ctx().request_repaint();
        }

        if ui_response.inner.dragged() {
            let delta = ui_response.inner.drag_delta();
            println!("Drag delta is: {:?}", delta);
            let drag_sensitivity = 0.01;
            camera_data.transform(camera::CameraTransform::Rotate {
                pivot: bbox.center(),
                rotation_axis: glam::Vec3::Y,
                angle: delta.x * drag_sensitivity,
            });
            camera_data.transform(camera::CameraTransform::Rotate {
                pivot: bbox.center(),
                rotation_axis: glam::Vec3::X,
                angle: delta.y * drag_sensitivity,
            });
            ui.ctx().request_repaint();
        } else if ui_response.inner.clicked() {
            println!("Clicked in the region");
        }
    }
}
