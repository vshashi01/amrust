use glam::Vec3;

use amrust_render::{bounding_box::BoundingBox, camera::CameraTransform};
use log::info;

use crate::{
    core::services::render_service::RenderServiceRequest,
    core::{
        interfaces::command::{Command, CommandCategory, CommandContext},
        services::command_service::CommandService,
    },
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
        !context.db_view_model.is_database_empty()
    }

    fn category(&self) -> CommandCategory {
        CommandCategory::View
    }

    fn execute(&self, context: &mut CommandContext) {
        let total_bbox = context
            .db_view_model
            .get_total_visible_bbox(context.current_app_mode);
        let transforms = get_transform_to_unzoom(&total_bbox);
        if let Err(err) = context
            .render_worker_queue_tx
            .send_blocking(RenderServiceRequest::TransformCamera(transforms))
        {
            info!("Error while sending Render message for unzoom: {err:?}");
        }
    }
}

/// Register all commands provided by the clear_db module
pub fn register_commands(commands_service: &mut CommandService) {
    commands_service.register_command(Box::new(UnzoomSceneCommand));
}

// pub fn unzoom_bbox(camera: &mut impl CameraData, total_bbox: &BoundingBox) {
//     let top_left_corner = Vec3::new(total_bbox.min.x, total_bbox.min.y, total_bbox.max.z);
//     // info!("The top left corner is: {}", top_left_corner);
//     camera
//         .transform(amrust_render::camera::CameraTransform::SetView {
//             eye_position: top_left_corner,
//             target_position: total_bbox.center(),
//             up_vector: Vec3::Z,
//         })
//         .transform(amrust_render::camera::CameraTransform::FitToExtent {
//             min: total_bbox.min,
//             max: total_bbox.max,
//         });
// }

pub fn get_transform_to_unzoom(total_bbox: &BoundingBox) -> Vec<CameraTransform> {
    let top_left_corner = Vec3::new(total_bbox.min.x, total_bbox.min.y, total_bbox.max.z);

    vec![
        CameraTransform::SetView {
            eye_position: top_left_corner,
            target_position: total_bbox.center(),
            up_vector: Vec3::Z,
        },
        CameraTransform::FitToExtent {
            min: total_bbox.min,
            max: total_bbox.max,
        },
    ]
}
