use std::sync::Arc;

use amrust_render::{bounding_box::BoundingBox, camera::CameraTransform};
use glam::Mat4;
use smol::{channel::Sender, lock::RwLock};

use crate::{
    core::{
        amrust_db::Db,
        app_mode::AppMode,
        services::render_service::RenderServiceRequest,
        types::transformation::Transformation,
        utils::picker::{PickedEntity, Picker, PickerConfig},
    },
    db_view_model::DbViewModel,
};

pub struct StandardMouse {
    rotation_center: Option<glam::Vec3>,
}

impl StandardMouse {
    pub fn new() -> Self {
        Self {
            rotation_center: None,
        }
    }

    pub fn run(
        &mut self,
        response: &egui::Response,
        ctx: &egui::Context,
        bbox: &BoundingBox,
        db: &Arc<RwLock<Db>>,
        db_view_model: &DbViewModel,
        app_mode: AppMode,
        view_proj: Mat4,
        render_service_request_sender: &Sender<RenderServiceRequest>,
        viewport_size: glam::Vec2,
    ) {
        let mut transforms: Vec<CameraTransform> = vec![];
        // Track scroll for zoom
        // use only the inner response to ensure mouse only responses to the viewport region
        if response.hovered() {
            let scroll_delta = ctx.input(|i| i.raw_scroll_delta.y);
            if scroll_delta.abs() > 0.0 {
                transforms.push(CameraTransform::Zoom(scroll_delta * 0.0001));
            }
        }

        if response.dragged_by(egui::PointerButton::Secondary) {
            let center = self.rotation_center.unwrap_or(bbox.center());

            let delta = response.drag_delta();
            let drag_sensitivity = 0.01;
            transforms.push(CameraTransform::Rotate {
                pivot: center,
                rotation_axis: glam::Vec3::Y,
                angle: -delta.x * drag_sensitivity,
            });

            transforms.push(CameraTransform::Rotate {
                pivot: center,
                rotation_axis: glam::Vec3::X,
                angle: -delta.y * drag_sensitivity,
            });
        } else if response.dragged_by(egui::PointerButton::Middle) {
            let delta = response.drag_delta();
            let pan_sensitivity = 0.01;
            transforms.push(CameraTransform::Pan(
                glam::Vec3::new(delta.x, delta.y, 0.0) * pan_sensitivity,
            ));
        } else if response.clicked()
            && let Some(pos) = response.interact_pointer_pos()
        // && let Some(db_read) = db.try_read()
        {
            let rect = response.rect;
            let relative_pos = pos - rect.min;
            let screen_pt = glam::Vec2::new(relative_pos.x, relative_pos.y);

            let entities = Self::pick_entities(
                app_mode,
                screen_pt,
                viewport_size,
                view_proj,
                db,
                db_view_model,
            );

            log::info!("Picked entities: {entities:?}");
        } else if response.drag_started_by(egui::PointerButton::Secondary)
            && let Some(pos) = response.interact_pointer_pos()
        {
            let rect = response.rect;
            let relative_pos = pos - rect.min;
            let screen_pt = glam::Vec2::new(relative_pos.x, relative_pos.y);

            let entities = Self::pick_entities(
                app_mode,
                screen_pt,
                viewport_size,
                view_proj,
                db,
                db_view_model,
            );

            log::info!("Entities found on drag: {entities:?}");

            self.rotation_center = if let Some(first) = entities.first() {
                Some(first.intersection)
            } else {
                Some(bbox.center())
            }
        } else if response.drag_stopped_by(egui::PointerButton::Secondary) {
            self.rotation_center = None;
        }

        if !transforms.is_empty()
            && let Err(err) = render_service_request_sender
                .send_blocking(RenderServiceRequest::TransformCamera(transforms))
        {
            log::error!("Error sending camera transform from Viewport: {err:?}");
        }
    }

    fn on_drag(response: &egui::Response) {
        if response.drag_started() {}
    }

    fn pick_entities(
        app_mode: AppMode,
        screen_pt: glam::Vec2,
        viewport_size: glam::Vec2,
        view_proj: Mat4,
        db: &Arc<RwLock<Db>>,
        db_view_model: &DbViewModel,
    ) -> Vec<PickedEntity> {
        let radius = 0.1; // example radius

        if let Some(read_db) = db.try_read() {
            match app_mode {
                AppMode::Objects => {
                    Picker.pick_from_parts(
                        &PickerConfig {
                            screen_pt,
                            viewport_size,
                            view_proj,
                            snap_radius: radius,
                            return_all_intersections: false,
                        },
                        &read_db,
                        &db_view_model.get_all_parts_id(),
                        true,
                    )

                    //log::info!("Picked entities: {:?}", picked);
                }
                AppMode::Build => {
                    Picker.pick_from_instances(
                        &PickerConfig {
                            screen_pt,
                            viewport_size,
                            view_proj,
                            snap_radius: radius,
                            return_all_intersections: false,
                        },
                        &read_db,
                        db_view_model.get_instance_on_scene(),
                        true,
                        Transformation(Mat4::IDENTITY),
                    )

                    //log::info!("Picked entities: {:?}", picked);
                }
            }
        } else {
            vec![]
        }
    }
}
