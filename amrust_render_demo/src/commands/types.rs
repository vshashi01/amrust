use smol::channel::Sender;

use crate::{
    app_mode::AppMode, db_view_model::DbViewModel, operation_manager::OperationRequest,
    render_worker::RenderMessage, services::FileDialogService,
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
    fn category(&self) -> CommandCategory;

    /// Execute the command with the given context
    fn execute(&self, context: &mut CommandContext);
}

/// Context provided to commands during execution
pub struct CommandContext<'a> {
    /// Current database view model state
    pub db_view_model: &'a DbViewModel,

    /// Current application mode (Objects vs Build)
    pub current_app_mode: AppMode,

    /// Channel for queuing operations
    pub operation_queue_tx: Sender<OperationRequest>,

    /// File dialog service for showing dialogs
    pub file_dialog_service: &'a mut FileDialogService,

    //Channel for queuing render work
    pub render_worker_queue_tx: Sender<RenderMessage>,
    // /// Flag to indicate viewport needs update
    // pub need_viewport_update: &'a mut bool,
}

impl<'a> CommandContext<'a> {
    pub fn new(
        db_view_model: &'a DbViewModel,
        app_mode: AppMode,
        operation_queue_tx: Sender<OperationRequest>,
        file_dialog_service: &'a mut FileDialogService,
        render_worker_queue_tx: Sender<RenderMessage>,
    ) -> Self {
        Self {
            db_view_model,
            current_app_mode: app_mode,
            operation_queue_tx,
            file_dialog_service,
            render_worker_queue_tx,
        }
    }
}

/// Categories for organizing commands
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CommandCategory {
    File,
    View,
    Debug,
}
