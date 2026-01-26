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

use std::sync::Arc;

pub struct StandardMouse {
    rotation_center: Option<glam::Vec3>,
    rotation_lock_axis: Option<glam::Vec3>,
    rotation_start_pt: Option<glam::Vec2>,
}

struct InputContext<'a> {
    ctx: &'a egui::Context,
    response: &'a egui::Response,
    pivot: glam::Vec3,
    right: glam::Vec3,
    up: glam::Vec3,
    camera_pos: glam::Vec3,
    distance: f32,
    view_proj: Mat4,
    viewport_rect: egui::Rect,
    scene_bbox: &'a BoundingBox,
    db: &'a Arc<RwLock<Db>>,
    db_view_model: &'a DbViewModel,
    app_mode: AppMode,
}

impl<'a> InputContext<'a> {
    fn get_viewport_size(&self) -> glam::Vec2 {
        let rect_size = self.viewport_rect.max - self.viewport_rect.min;
        glam::Vec2 {
            x: rect_size.x,
            y: rect_size.y,
        }
    }

    fn pick_entities(&self, screen_pt: glam::Vec2) -> Vec<PickedEntity> {
        pick_entities(
            self.app_mode,
            screen_pt,
            self.get_viewport_size(),
            self.view_proj,
            self.db,
            self.db_view_model,
        )
    }
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
    ) {
        let mut transforms: Vec<CameraTransform> = vec![];

        let pivot = self.rotation_center.unwrap_or(bbox.center());
        let (right, up, camera_pos, distance) = get_camera_vectors(view_proj, pivot);
        let input_context = InputContext {
            ctx,
            response,
            pivot,
            right,
            up,
            camera_pos,
            distance,
            view_proj,
            viewport_rect: response.rect,
            scene_bbox: bbox,
            db,
            db_view_model,
            app_mode,
        };

        //zooming
        if response.hovered()
            && ctx.input(|i| i.raw_scroll_delta.y.abs() > 0.0)
            && let Some(pos) = response.hover_pos()
        {
            let zoom_transform = self.zoom_towards_pointer(&input_context, pos);

            transforms.push(zoom_transform);
        }

        //rotation
        if response.dragged_by(egui::PointerButton::Secondary)
            && let Some(pos) = response.interact_pointer_pos()
        {
            let rotate_transforms = self.handle_rotate(&input_context, response.drag_delta(), pos);
            transforms.extend(rotate_transforms);
        }

        //panning
        if response.dragged_by(egui::PointerButton::Middle) {
            let pan_transform = self.handle_panning(&input_context, response.drag_delta());
            transforms.push(pan_transform);
        }

        //capturing the necessary points for movements
        if (ctx.input(|i| i.pointer.button_pressed(egui::PointerButton::Secondary))
            || ctx.input(|i| i.pointer.button_pressed(egui::PointerButton::Middle)))
            && response.hovered()
            && let Some(pos) = response.interact_pointer_pos()
        {
            self.handle_capture_pivot(&input_context, pos);
        }

        // clear internal states
        if response.drag_stopped_by(egui::PointerButton::Secondary)
            || response.drag_stopped_by(egui::PointerButton::Middle)
        {
            self.handle_drag_end();
        }

        // send transform data
        if !transforms.is_empty()
            && let Err(err) = render_service_request_sender
                .send_blocking(RenderServiceRequest::TransformCamera(transforms))
        {
            log::error!("Error sending camera transform from Viewport: {err:?}");
        }

        // entity picking
        if response.clicked()
            && let Some(pos) = response.interact_pointer_pos()
        {
            self.handle_pick_entity(&input_context, pos);
        }
    }

    fn zoom_towards_pointer(&self, ctx: &InputContext, pos: egui::Pos2) -> CameraTransform {
        let screen_pt = to_screen_pt(ctx.viewport_rect, pos);
        let entities = ctx.pick_entities(screen_pt);

        let zoom_target = if let Some(first) = entities.first() {
            first.intersection
        } else {
            self.rotation_center.unwrap_or(ctx.scene_bbox.center())
        };

        let scroll_delta = ctx.ctx.input(|i| i.raw_scroll_delta.y);
        CameraTransform::ZoomTowards {
            target: zoom_target,
            amount: scroll_delta * 0.0001,
        }
    }

    fn handle_rotate(
        &mut self,
        ctx: &InputContext,
        delta: egui::Vec2,
        pos: egui::Pos2,
    ) -> Vec<CameraTransform> {
        let base_sensitivity = 0.0003;
        let scale_factor = 0.2;
        let drag_sensitivity = zoom_aware_sensitivity(base_sensitivity, ctx.distance, scale_factor);

        let alt_held = ctx.ctx.input(|i| i.key_down(Key::Num5));

        if alt_held {
            if self.rotation_lock_axis.is_none()
                && let Some(start_pt) = self.rotation_start_pt
            {
                let current_screen_pt = to_screen_pt(ctx.viewport_rect, pos);

                let displacement = current_screen_pt - start_pt;
                let lock_threshold = 2.0; //px
                if displacement.length() >= lock_threshold {
                    if displacement.x.abs() > displacement.y.abs() {
                        self.rotation_lock_axis = Some(ctx.up);
                    } else {
                        self.rotation_lock_axis = Some(ctx.right);
                    }
                }
            }

            if let Some(axis) = self.rotation_lock_axis {
                let angle = if axis == ctx.up {
                    -delta.x * drag_sensitivity
                } else {
                    delta.y * drag_sensitivity
                };

                vec![CameraTransform::Rotate {
                    pivot: ctx.pivot,
                    rotation_axis: axis,
                    angle,
                }]
            } else {
                vec![]
            }
        } else {
            vec![
                CameraTransform::Rotate {
                    pivot: ctx.pivot,
                    rotation_axis: ctx.up,
                    angle: -delta.x * drag_sensitivity,
                },
                CameraTransform::Rotate {
                    pivot: ctx.pivot,
                    rotation_axis: ctx.right,
                    angle: -delta.y * drag_sensitivity,
                },
            ]
        }
    }

    fn handle_panning(&self, ctx: &InputContext, delta: egui::Vec2) -> CameraTransform {
        let base_sensitivity = 0.001;
        let scale_factor = 0.2;
        let pan_sensitivity = zoom_aware_sensitivity(base_sensitivity, ctx.distance, scale_factor);

        let movement = (ctx.right * -delta.x + ctx.up * delta.y) * pan_sensitivity;
        CameraTransform::Pan(movement)
    }

    fn handle_drag_end(&mut self) {
        self.rotation_center = None;
        self.rotation_lock_axis = None;
        self.rotation_start_pt = None;
    }

    fn handle_capture_pivot(&mut self, ctx: &InputContext, pos: egui::Pos2) {
        let screen_pt = to_screen_pt(ctx.viewport_rect, pos);
        let entities = ctx.pick_entities(screen_pt);
        log::info!("Entities found on drag: {entities:?}");

        self.rotation_start_pt = Some(screen_pt);
        self.rotation_center = entities
            .first()
            .map(|e| e.intersection)
            .or_else(|| Some(ctx.scene_bbox.center()));
    }

    fn handle_pick_entity(&self, ctx: &InputContext, pos: egui::Pos2) {
        let screen_pt = to_screen_pt(ctx.viewport_rect, pos);
        let entities = ctx.pick_entities(screen_pt);

        log::info!("Picked entities: {entities:?}");
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
            AppMode::Objects => Picker.pick_from_parts(
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
            ),
            AppMode::Build => Picker.pick_from_instances(
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
            ),
        }
    } else {
        vec![]
    }
}

fn zoom_aware_sensitivity(base_sensitivity: f32, distance: f32, scale_factor: f32) -> f32 {
    base_sensitivity * ((distance * scale_factor).clamp(0.5, 3.0))
}

#[inline]
fn get_camera_vectors(
    view_proj: Mat4,
    pivot: glam::Vec3,
) -> (glam::Vec3, glam::Vec3, glam::Vec3, f32) {
    let view = view_proj.inverse();
    let right = glam::Vec3::new(view.col(0).x, view.col(0).y, view.col(0).z);
    let up = glam::Vec3::new(view.col(1).x, view.col(1).y, view.col(1).z);
    let camera_pos = glam::Vec3::new(view.col(3).x, view.col(3).y, view.col(3).z);
    let distance = camera_pos.distance(pivot);

    (right, up, camera_pos, distance)
}

#[inline]
fn to_screen_pt(viewport_rect: egui::Rect, pos: egui::Pos2) -> glam::Vec2 {
    let relative_pos = pos - viewport_rect.min;
    glam::Vec2::new(relative_pos.x, relative_pos.y)
}
