use amrust_render::{bounding_box::BoundingBox, camera::CameraTransform};
use egui::{Image, UiBuilder, Vec2, epaint};
use glam::Mat4;
use smol::channel::Sender;
use smol::lock::RwLock;

use crate::core::amrust_db::Db;
use crate::core::app_mode::AppMode;
use crate::core::services::picker_service::{PickerConfig, PickerService};
use crate::core::services::render_service::RenderServiceRequest;
use crate::core::types::transformation::Transformation;
use crate::db_view_model::DbViewModel;

use std::sync::Arc;
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
        bbox: &BoundingBox,
        db: &Arc<RwLock<Db>>,
        db_view_model: &DbViewModel,
        app_mode: &AppMode,
        picker_service: &PickerService,
        current_view_proj: Mat4,
    ) {
        let size_we_want_to_use = ui.available_size();
        if size_we_want_to_use != self.prev_frame_size
            && (time::Instant::now() - self.last_request_for_resize) > Duration::from_millis(100)
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

        let mut transforms: Vec<CameraTransform> = vec![];
        // Track scroll for zoom
        // use only the inner response to ensure mouse only responses to the viewport region
        if ui_response.inner.inner.hovered() {
            let scroll_delta = ui.ctx().input(|i| i.raw_scroll_delta.y);
            if scroll_delta.abs() > 0.0 {
                transforms.push(CameraTransform::Zoom(scroll_delta * 0.0001));
            }
        }

        if ui_response
            .inner
            .inner
            .dragged_by(egui::PointerButton::Secondary)
        {
            let delta = ui_response.inner.inner.drag_delta();
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
        } else if ui_response
            .inner
            .inner
            .dragged_by(egui::PointerButton::Middle)
        {
            let delta = ui_response.inner.inner.drag_delta();
            let pan_sensitivity = 0.01;
            transforms.push(CameraTransform::Pan(
                glam::Vec3::new(-delta.x, -delta.y, 0.0) * pan_sensitivity,
            ));
        } else if ui_response.inner.inner.clicked()
            && let Some(pos) = ui_response.inner.inner.interact_pointer_pos()
            && let Some(db_read) = db.try_read()
        {
            let rect = ui_response.inner.inner.rect;
            let relative_pos = pos - rect.min;
            log::debug!("Clicked at position: {relative_pos:?} ");

            let screen_pt = glam::Vec2::new(relative_pos.x, relative_pos.y);
            let viewport_size = glam::Vec2::new(size_we_want_to_use.x, size_we_want_to_use.y);

            let radius = 0.1; // example radius
            match app_mode {
                AppMode::Objects => {
                    let picked = picker_service.pick_only_parts(
                        &PickerConfig {
                            screen_pt,
                            viewport_size,
                            view_proj: current_view_proj,
                            snap_radius: radius,
                            return_all_intersections: false,
                        },
                        &db_read,
                        &db_view_model.get_all_parts_id(),
                    );

                    log::info!("Picked entities: {:?}", picked);
                }
                AppMode::Build => {
                    let picked = picker_service.pick_only_instances(
                        &PickerConfig {
                            screen_pt,
                            viewport_size,
                            view_proj: current_view_proj,
                            snap_radius: radius,
                            return_all_intersections: false,
                        },
                        &db_read,
                        db_view_model.get_instance_on_scene(),
                        Transformation(Mat4::IDENTITY),
                    );

                    log::info!("Picked entities: {:?}", picked);
                }
            }
            // let picks = picker_service.pick(
            //     &db_read,
            //     app_mode,
            //     screen_point,
            //     viewport_size,
            //     current_view_proj,
            //     radius,
            // );
        }

        if !transforms.is_empty()
            && let Err(err) = render_service_request_sender
                .send_blocking(RenderServiceRequest::TransformCamera(transforms))
        {
            log::error!("Error sending camera transform from Viewport: {err:?}");
        }
    }
}
