use egui_file_dialog::FileDialog;
use std::collections::HashMap;
use std::path::PathBuf;

use crate::commands::CommandContext;

/// Service for managing file dialogs with one-shot handler callbacks
pub struct FileDialogService {
    load_dialog: FileDialog,
    save_dialogs: HashMap<String, FileDialog>,
    pending_load_handler: Option<Box<dyn FnOnce(PathBuf, &mut CommandContext)>>,
    pending_save_handlers: HashMap<String, Box<dyn FnOnce(PathBuf, &mut CommandContext)>>,
}

impl FileDialogService {
    pub fn new() -> Self {
        let load_dialog = FileDialog::new()
            .add_file_filter_extensions("3MF", vec!["3mf"])
            .default_file_filter("3MF");

        Self {
            load_dialog,
            save_dialogs: HashMap::new(),
            pending_load_handler: None,
            pending_save_handlers: HashMap::new(),
        }
    }

    /// Show a load dialog with a one-shot handler that will be called when a file is picked
    pub fn show_load_dialog(
        &mut self,
        handler: impl FnOnce(PathBuf, &mut CommandContext) + 'static,
    ) {
        self.pending_load_handler = Some(Box::new(handler));
        self.load_dialog.pick_file();
    }

    /// Show a save dialog with a one-shot handler that will be called when a file is picked
    pub fn show_save_dialog(
        &mut self,
        dialog_id: &str,
        extension: &str,
        handler: impl FnOnce(PathBuf, &mut CommandContext) + 'static,
    ) {
        // Create or get existing save dialog
        let dialog = self
            .save_dialogs
            .entry(dialog_id.to_string())
            .or_insert_with(|| {
                FileDialog::new()
                    .add_save_extension("3MF file", extension)
                    .default_save_extension("3MF file")
            });

        self.pending_save_handlers
            .insert(dialog_id.to_string(), Box::new(handler));
        dialog.save_file();
    }

    /// Update all file dialogs and execute handlers when files are picked
    pub fn update_and_handle(&mut self, ctx: &egui::Context, command_context: &mut CommandContext) {
        // Handle load dialog
        self.load_dialog.update(ctx);
        if let Some(path) = self.load_dialog.take_picked() {
            if let Some(handler) = self.pending_load_handler.take() {
                handler(path, command_context);
            }
        }

        // Handle save dialogs
        let mut completed_save_dialogs = Vec::new();
        for (dialog_id, dialog) in &mut self.save_dialogs {
            dialog.update(ctx);
            if let Some(path) = dialog.take_picked() {
                if let Some(handler) = self.pending_save_handlers.remove(dialog_id) {
                    handler(path, command_context);
                }
                completed_save_dialogs.push(dialog_id.clone());
            }
        }

        // Clean up completed save dialogs
        for dialog_id in completed_save_dialogs {
            self.save_dialogs.remove(&dialog_id);
        }
    }
}
