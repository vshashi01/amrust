use amrust_render::{bounding_box::BoundingBox, camera::OrthographicCameraData};
use smol::channel::Sender;

use crate::{
    amrust_db::Identifiable, app_mode::AppMode, db_view_model::DbViewModel,
    operation_manager::OperationRequest, services::FileDialogService,
};

/// Trait for commands that can be executed by the command service
pub trait Command: Send + Sync {
    /// Unique identifier for the command
    fn id(&self) -> &str;

    /// Human-readable label for UI display
    fn label(&self) -> &str;

    /// Optional keyboard shortcut (e.g., "Ctrl+O")
    fn shortcut(&self) -> Option<&str> {
        None
    }

    /// Whether this command should be visible in the UI
    fn is_visible(&self, _context: &CommandContext) -> bool {
        true
    }

    /// Whether this command should be enabled in the UI
    fn is_enabled(&self, _context: &CommandContext) -> bool {
        true
    }

    /// Category for organizing commands in UI
    fn category(&self) -> CommandCategory {
        CommandCategory::General
    }

    /// Execute the command with the given context
    fn execute(&self, context: &mut CommandContext);
}

/// Context provided to commands during execution
pub struct CommandContext<'a> {
    /// Current database view model state
    pub db_view_model: &'a DbViewModel,

    /// Current application mode (Objects vs Build)
    pub current_app_mode: AppMode,

    /// Currently selected identifiables
    pub selected_identifiables: &'a [Identifiable],

    /// Identifiables that are currently detached (blocked for operations)
    pub detached_identifiables: &'a [Identifiable],

    /// Channel for queuing operations
    pub operation_queue_tx: &'a Sender<OperationRequest>,

    /// File dialog service for showing dialogs
    pub file_dialog_service: &'a mut FileDialogService,

    /// Camera data for scene manipulation
    pub camera_data: &'a mut OrthographicCameraData,

    /// Current scene bounding box
    pub scene_bbox: Option<&'a BoundingBox>,

    /// Flag to indicate viewport needs update
    pub need_viewport_update: &'a mut bool,
}

/// Categories for organizing commands
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CommandCategory {
    General,
    File,
    View,
    Selection,
    Debug,
}
