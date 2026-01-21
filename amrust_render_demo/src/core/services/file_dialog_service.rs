use egui_file_dialog::FileDialog;
use smol::channel::{Receiver, TryRecvError};
use std::path::PathBuf;

use crate::core::interfaces::command::CommandContext;

pub type FileDialogHandlerCallback = Box<dyn FnOnce(PathBuf, &mut CommandContext)>;

pub struct FileDialogHandler {
    r#type: DialogType,
    dialog: FileDialog,
    callback: FileDialogHandlerCallback,
}

impl FileDialogHandler {
    pub fn handle_and_close_dialog(mut self, context: &mut CommandContext) {
        match &self.r#type {
            DialogType::Save | DialogType::Load => {
                if let Some(picked) = &self.dialog.take_picked() {
                    (self.callback)(picked.clone(), context);
                }
            } // DialogType::LoadMultiple => todo!("Implement multi-file-loading"),
        }
    }
}

enum DialogType {
    Save,
    Load,
    // LoadMultiple,
}

pub enum FileDialogRequest {
    Load {
        extension_name: &'static str,
        extensions: Vec<&'static str>,
        handler: Box<dyn FnOnce(PathBuf, &mut CommandContext) + 'static>,
    },
    Save {
        extension_name: &'static str,
        extension: &'static str,
        handler: Box<dyn FnOnce(PathBuf, &mut CommandContext) + 'static>,
    },
}

/// Service for managing file dialogs with one-shot handler callbacks
pub struct FileDialogService {
    file_dialog_service_request_rx: Receiver<FileDialogRequest>,
    pending_handlers: Vec<FileDialogHandler>,
}

impl FileDialogService {
    pub fn new(file_dialog_service_request_rx: Receiver<FileDialogRequest>) -> Self {
        Self {
            file_dialog_service_request_rx,
            pending_handlers: Vec::new(),
        }
    }

    // /// Show a load dialog with a one-shot handler that will be called when a file is picked
    // pub fn show_load_dialog(
    //     &mut self,
    //     extension_name: &str,
    //     extensions: Vec<&'static str>,
    //     handler: impl FnOnce(PathBuf, &mut CommandContext) + 'static,
    // ) {
    //     let mut dialog = FileDialog::new()
    //         .add_file_filter_extensions(extension_name, extensions)
    //         .default_file_filter(extension_name);

    //     dialog.pick_file();

    //     let dialog_handler = FileDialogHandler {
    //         r#type: DialogType::Load,
    //         dialog,
    //         callback: Box::new(handler),
    //     };

    //     self.pending_handlers.push(dialog_handler);
    // }

    // /// Show a save dialog with a one-shot handler that will be called when a file is picked
    // pub fn show_save_dialog(
    //     &mut self,
    //     extension_name: &'static str,
    //     extension: &'static str,
    //     handler: impl FnOnce(PathBuf, &mut CommandContext) + 'static,
    // ) {
    //     let mut dialog = FileDialog::new()
    //         .add_save_extension(extension_name, extension)
    //         .default_save_extension(extension_name);

    //     dialog.save_file();

    //     let dialog_handler = FileDialogHandler {
    //         r#type: DialogType::Save,
    //         dialog,
    //         callback: Box::new(handler),
    //     };

    //     self.pending_handlers.push(dialog_handler);
    // }

    /// Update all file dialogs and returns the first picked file dialog handler
    pub fn update_and_return_first_picked(
        &mut self,
        ctx: &egui::Context,
    ) -> Option<FileDialogHandler> {
        for handler in &mut self.pending_handlers {
            handler.dialog.update(ctx);
        }

        let first_picked = self
            .pending_handlers
            .iter()
            .enumerate()
            .find_map(|(index, handler)| {
                if handler.dialog.picked().is_some() {
                    Some(index)
                } else {
                    None
                }
            });

        match self.file_dialog_service_request_rx.try_recv() {
            Ok(request) => match request {
                FileDialogRequest::Load {
                    extension_name,
                    extensions,
                    handler,
                } => {
                    let mut dialog = FileDialog::new()
                        .add_file_filter_extensions(extension_name, extensions)
                        .default_file_filter(extension_name);

                    dialog.pick_file();

                    let dialog_handler = FileDialogHandler {
                        r#type: DialogType::Load,
                        dialog,
                        callback: Box::new(handler),
                    };

                    self.pending_handlers.push(dialog_handler);
                }
                FileDialogRequest::Save {
                    extension_name,
                    extension,
                    handler,
                } => {
                    let mut dialog = FileDialog::new()
                        .add_save_extension(extension_name, extension)
                        .default_save_extension(extension_name);

                    dialog.save_file();

                    let dialog_handler = FileDialogHandler {
                        r#type: DialogType::Save,
                        dialog,
                        callback: Box::new(handler),
                    };

                    self.pending_handlers.push(dialog_handler);
                }
            },
            Err(err) => match err {
                TryRecvError::Empty => {
                    // do nothing
                }
                TryRecvError::Closed => panic!("File dialog service is closed! : {err:?}"),
            },
        }

        if let Some(index) = first_picked {
            let handler = self.pending_handlers.remove(index);

            return Some(handler);
        }

        // let mut indices_to_process = vec![];

        // for (index, handler) in self.pending_handlers.iter().enumerate() {
        //     if let Some(_) = &handler.dialog.picked() {
        //         indices_to_process.push(index);
        //     }
        // }

        // for index in indices_to_process {
        //     let mut handler = self.pending_handlers.remove(index);

        //     if let Some(picked) = &handler.dialog.take_picked() {
        //         (handler.callback)(picked.clone(), command_context);
        //     }
        // }
        None
    }
}
