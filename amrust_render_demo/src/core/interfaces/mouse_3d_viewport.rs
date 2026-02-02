use amrust_render::bounding_box::BoundingBox;
use smol::{
    channel::{Sender, TrySendError},
    lock::RwLock,
};

use crate::core::{
    amrust_db::Db,
    app_mode::AppMode,
    services::render_service::RenderServiceRequest,
    types::{part::PartId, part_instance::PartInstanceId, transformation::Transformation},
    utils::picker::{PickedEntity, Picker, PickerConfig},
};

use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct Mouse3dFrameContext {
    pub viewport_rect: egui::Rect,
    pub view_proj: glam::Mat4,
    pub scene_bbox: BoundingBox,
    pub parts_can_be_picked: Vec<PartId>,
    pub instances_can_be_picked: Vec<PartInstanceId>,
    pub app_mode: AppMode,
}

pub struct Mouse3dContext {
    viewport_rect: egui::Rect,
    render_request_tx: Sender<RenderServiceRequest>,
    view_proj: glam::Mat4,
    scene_bbox: BoundingBox,
    db: Arc<RwLock<Db>>,
    parts_can_be_picked: Vec<PartId>,
    instances_can_be_picked: Vec<PartInstanceId>,
    app_mode: AppMode,
}

impl Mouse3dContext {
    pub fn new(
        frame_context: Mouse3dFrameContext,
        db: &Arc<RwLock<Db>>,
        render_request_tx: &Sender<RenderServiceRequest>,
    ) -> Self {
        Self {
            viewport_rect: frame_context.viewport_rect,
            view_proj: frame_context.view_proj,
            scene_bbox: frame_context.scene_bbox,
            parts_can_be_picked: frame_context.parts_can_be_picked,
            instances_can_be_picked: frame_context.instances_can_be_picked,
            app_mode: frame_context.app_mode,
            render_request_tx: render_request_tx.clone(),
            db: db.clone(),
        }
    }

    #[inline]
    pub fn get_viewport_size(&self) -> glam::Vec2 {
        let rect_size = self.viewport_rect.max - self.viewport_rect.min;
        glam::Vec2 {
            x: rect_size.x,
            y: rect_size.y,
        }
    }

    #[inline]
    pub fn pick_entities(&self, screen_pt: &glam::Vec2) -> Vec<PickedEntity> {
        pick_entities(
            self.app_mode,
            screen_pt,
            self.get_viewport_size(),
            self.view_proj,
            &self.db,
            &self.parts_can_be_picked,
            &self.instances_can_be_picked,
        )
    }

    #[inline]
    pub fn to_screen_pt(&self, pos: egui::Pos2) -> glam::Vec2 {
        let relative_pos = pos - self.viewport_rect.min;
        glam::Vec2::new(relative_pos.x, relative_pos.y)
    }

    #[inline]
    pub fn get_camera_vectors(
        &self,
        pivot: glam::Vec3,
    ) -> (glam::Vec3, glam::Vec3, glam::Vec3, f32) {
        let view = self.view_proj.inverse();
        let right = glam::Vec3::new(view.col(0).x, view.col(0).y, view.col(0).z);
        let up = glam::Vec3::new(view.col(1).x, view.col(1).y, view.col(1).z);
        let camera_pos = glam::Vec3::new(view.col(3).x, view.col(3).y, view.col(3).z);
        let distance = camera_pos.distance(pivot);

        (right, up, camera_pos, distance)
    }

    #[inline]
    pub fn get_pivot_and_screen_pt(&self, pos: egui::Pos2) -> (glam::Vec3, glam::Vec2) {
        let screen_pt = self.to_screen_pt(pos);
        let entities = self.pick_entities(&screen_pt);
        let pivot = entities
            .first()
            .map_or(self.scene_bbox.center(), |p| p.intersection);
        (pivot, screen_pt)
    }

    #[inline]
    pub fn try_send_render_request(
        &self,
        request: RenderServiceRequest,
    ) -> Result<(), TrySendError<RenderServiceRequest>> {
        self.render_request_tx.try_send(request)
    }
}

// ToDo currently all Pos2, are in the global egui coordinate and not the local coordinate
// within the response region.
#[allow(dead_code)]
pub enum MouseEvt {
    PrimaryBtnDown(egui::Pos2, egui::InputState),
    PrimaryBtnUp(egui::Pos2, egui::InputState),
    PrimaryBtnDrag(egui::Vec2, egui::Pos2, egui::InputState),
    SecondaryBtnDown(egui::Pos2, egui::InputState),
    SecondaryBtnUp(egui::Pos2, egui::InputState),
    SecondaryBtnDrag(egui::Vec2, egui::Pos2, egui::InputState),
    MiddleBtnDown(egui::Pos2, egui::InputState),
    MiddleBtnUp(egui::Pos2, egui::InputState),
    MiddleBtnDrag(egui::Vec2, egui::Pos2, egui::InputState),
    MiddleBtnScroll(egui::Vec2, egui::Pos2, egui::InputState),
    Hovered(egui::Pos2, egui::InputState),
    NotHovered(),
}

pub trait Mouse3DViewport {
    fn can_handle(&self, event: &MouseEvt, context: &mut Mouse3dContext) -> bool;

    fn is_clean(&self) -> bool;

    fn can_allow_passthrough(&self, _event: &MouseEvt) -> bool {
        false
    }

    fn handle_event(&mut self, event: &MouseEvt, context: &mut Mouse3dContext);
}

fn pick_entities(
    app_mode: AppMode,
    screen_pt: &glam::Vec2,
    viewport_size: glam::Vec2,
    view_proj: glam::Mat4,
    db: &Arc<RwLock<Db>>,
    parts_can_be_picked: &[PartId],
    instance_can_be_picked: &[PartInstanceId],
) -> Vec<PickedEntity> {
    let radius = 0.1; // example radius

    if let Some(read_db) = db.try_read() {
        match app_mode {
            AppMode::Objects => Picker.pick_from_parts(
                &PickerConfig {
                    screen_pt: *screen_pt,
                    viewport_size,
                    view_proj,
                    snap_radius: radius,
                    return_all_intersections: false,
                },
                &read_db,
                parts_can_be_picked,
                true,
            ),
            AppMode::Build => Picker.pick_from_instances(
                &PickerConfig {
                    screen_pt: *screen_pt,
                    viewport_size,
                    view_proj,
                    snap_radius: radius,
                    return_all_intersections: false,
                },
                &read_db,
                instance_can_be_picked,
                true,
                Transformation(glam::Mat4::IDENTITY),
            ),
        }
    } else {
        vec![]
    }
}
