use std::sync::Arc;

use amrust_render::{bounding_box::BoundingBox, camera::CameraTransform};
use egui::Key;
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
    rotation_lock_axis: Option<glam::Vec3>,
    rotation_start_pt: Option<glam::Vec2>,
}

impl StandardMouse {
    pub fn new() -> Self {
        Self {
            rotation_center: None,
            rotation_lock_axis: None,
            rotation_start_pt: None,
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
        if response.hovered()
            && let Some(pos) = response.hover_pos()
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

            let zoom_target = if let Some(first) = entities.first() {
                first.intersection
            } else {
                self.rotation_center.unwrap_or(bbox.center())
            };

            let scroll_delta = ctx.input(|i| i.raw_scroll_delta.y);
            if scroll_delta.abs() > 0.0 {
                transforms.push(CameraTransform::ZoomTowards {
                    target: zoom_target,
                    amount: scroll_delta * 0.0001,
                });
            }
        }

        if response.dragged_by(egui::PointerButton::Secondary) {
            let pivot = self.rotation_center.unwrap_or(bbox.center());

            let delta = response.drag_delta();

            let view = view_proj.inverse();
            let right = glam::Vec3::new(view.col(0).x, view.col(0).y, view.col(0).z);
            let up = glam::Vec3::new(view.col(1).x, view.col(1).y, view.col(1).z);
            let camera_pos = glam::Vec3::new(view.col(3).x, view.col(3).y, view.col(3).z);
            let distance = camera_pos.distance(pivot);

            let base_sensitivity = 0.0003;
            let scale_factor = 0.2;
            let drag_sensitivity = base_sensitivity * ((distance * scale_factor).clamp(0.5, 3.0));

            let alt_held = ctx.input(|i| i.key_down(Key::Num5));

            if alt_held {
                if self.rotation_lock_axis.is_none()
                    && let Some(start_pt) = self.rotation_start_pt
                    && let Some(pos) = response.interact_pointer_pos()
                {
                    let rect = response.rect;
                    let relative_pos = pos - rect.min;
                    let current_screen_pt = glam::Vec2::new(relative_pos.x, relative_pos.y);

                    let displacement = current_screen_pt - start_pt;
                    let lock_threshold = 2.0; //px
                    if displacement.length() >= lock_threshold {
                        if displacement.x.abs() > displacement.y.abs() {
                            self.rotation_lock_axis = Some(up);
                        } else {
                            self.rotation_lock_axis = Some(right);
                        }
                    }
                }

                if let Some(axis) = self.rotation_lock_axis {
                    let angle = if axis == up {
                        -delta.x * drag_sensitivity
                    } else {
                        delta.y * drag_sensitivity
                    };

                    transforms.push(CameraTransform::Rotate {
                        pivot,
                        rotation_axis: axis,
                        angle,
                    });
                }
            } else {
                transforms.push(CameraTransform::Rotate {
                    pivot,
                    rotation_axis: up,
                    angle: -delta.x * drag_sensitivity,
                });

                transforms.push(CameraTransform::Rotate {
                    pivot,
                    rotation_axis: right,
                    angle: -delta.y * drag_sensitivity,
                });
            }
        } else if response.dragged_by(egui::PointerButton::Middle) {
            let pivot = self.rotation_center.unwrap_or(bbox.center());
            let delta = response.drag_delta();

            let view = view_proj.inverse();
            let right = glam::Vec3::new(view.col(0).x, view.col(0).y, view.col(0).z);
            let up = glam::Vec3::new(view.col(1).x, view.col(1).y, view.col(1).z);
            let camera_pos = glam::Vec3::new(view.col(3).x, view.col(3).y, view.col(3).z);
            let distance = camera_pos.distance(pivot);

            let base_sensitivity = 0.001;
            let scale_factor = 0.2;
            let pan_sensitivity = base_sensitivity * ((distance * scale_factor).clamp(0.5, 3.0));

            let movement = (right * -delta.x + up * delta.y) * pan_sensitivity;
            transforms.push(CameraTransform::Pan(movement));
        } else if response.clicked()
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

            log::info!("Picked entities: {entities:?}");
        } else if (ctx.input(|i| i.pointer.button_pressed(egui::PointerButton::Secondary))
            || ctx.input(|i| i.pointer.button_pressed(egui::PointerButton::Middle)))
            && response.hovered()
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

            self.rotation_start_pt = Some(screen_pt);
            self.rotation_center = entities
                .first()
                .map(|e| e.intersection)
                .or_else(|| Some(bbox.center()));
        } else if response.drag_stopped_by(egui::PointerButton::Secondary)
            || response.drag_stopped_by(egui::PointerButton::Middle)
        {
            self.rotation_center = None;
            self.rotation_start_pt = None;
            self.rotation_lock_axis = None;
        }

        if !transforms.is_empty()
            && let Err(err) = render_service_request_sender
                .send_blocking(RenderServiceRequest::TransformCamera(transforms))
        {
            log::error!("Error sending camera transform from Viewport: {err:?}");
        }
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
