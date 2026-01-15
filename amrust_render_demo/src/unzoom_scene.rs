use amrust_render::{bounding_box::BoundingBox, camera::CameraData};
use glam::Vec3;

use crate::{
    commands::{Command, CommandCategory, CommandContext, CommandsService},
    db_view_model::get_total_bbox_from_cache,
};

pub struct UnzoomSceneCommand;

impl Command for UnzoomSceneCommand {
    fn id(&self) -> &str {
        "unzoom_scene"
    }

    fn label(&self) -> &str {
        "Unzoom Scene"
    }

    fn is_visible(&self, _context: &CommandContext) -> bool {
        true
    }

    fn is_enabled(&self, context: &CommandContext) -> bool {
        !context.db_view_model.is_empty()
    }

    fn category(&self) -> CommandCategory {
        CommandCategory::View
    }

    fn execute(&self, context: &mut CommandContext) {
        let total_bbox = get_total_bbox_from_cache(context.db_view_model, context.current_app_mode);
        context.update_camera_data(|camera_data| {
            unzoom_bbox(camera_data, &total_bbox);
        });
    }
}

/// Register all commands provided by the clear_db module
pub fn register_commands(commands_service: &mut CommandsService) {
    commands_service.register_command(Box::new(UnzoomSceneCommand));
}

pub fn unzoom_bbox(camera: &mut impl CameraData, total_bbox: &BoundingBox) {
    let top_left_corner = Vec3::new(total_bbox.min.x, total_bbox.min.y, total_bbox.max.z);
    // println!("The top left corner is: {}", top_left_corner);
    camera
        .transform(amrust_render::camera::CameraTransform::SetView {
            eye_position: top_left_corner,
            target_position: total_bbox.center(),
            up_vector: Vec3::Z,
        })
        .transform(amrust_render::camera::CameraTransform::FitToExtent {
            min: total_bbox.min,
            max: total_bbox.max,
        });
}
