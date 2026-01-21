use std::collections::VecDeque;

use smol::channel::{Receiver, TryRecvError};

use crate::ui::popup_dialog::{DialogResponse, error_modal_dialog, info_modal_dialog};

pub enum DialogServiceRequest {
    Error(&'static str, &'static str),
    Info(&'static str, &'static str),
    // ShowModalDialog,
    SemiModalDialog(&'static str, Box<dyn FnMut() -> DialogStateInNextFrame>),
    // ShowNonModalPopupDialog,
}

pub enum DialogStateInNextFrame {
    Show,
    Closed,
}

enum DialogKind {
    Error,
    Info,
    StandardSemiModal,
}

pub struct DialogData {
    kind: DialogKind,
    title: &'static str,
    message: &'static str,
    content: Option<Box<dyn FnMut() -> DialogStateInNextFrame + 'static>>,
    id: u64,
}

pub struct DialogService {
    dialog_request_queue_rx: Receiver<DialogServiceRequest>,
    dialog_queue: VecDeque<DialogData>,

    next_dialog_id: u64,
}

impl DialogService {
    pub fn new(dialog_request_queue_rx: Receiver<DialogServiceRequest>) -> Self {
        Self {
            dialog_request_queue_rx,
            dialog_queue: VecDeque::new(),
            next_dialog_id: 0,
        }
    }

    pub fn add_error_dialog(&mut self, title: &'static str, message: &'static str) {
        self.dialog_queue.push_front(DialogData {
            kind: DialogKind::Error,
            title,
            message,
            id: self.next_dialog_id,
            content: None,
        });

        self.next_dialog_id += 1;
    }

    pub fn update_and_show_dialogs(&mut self, ctx: &egui::Context) {
        let mut dialogs_to_keep = vec![];
        for dialog in &mut self.dialog_queue {
            match dialog.kind {
                DialogKind::Error => {
                    match error_modal_dialog(
                        ctx,
                        egui::Id::new(dialog.id),
                        dialog.title,
                        dialog.message,
                    ) {
                        DialogResponse::Ok | DialogResponse::Cancel | DialogResponse::Continue => {}
                        DialogResponse::IsShowing => {
                            dialogs_to_keep.push(dialog.id);
                        }
                    }
                }
                DialogKind::Info => match info_modal_dialog(
                    ctx,
                    egui::Id::new(dialog.id),
                    dialog.title,
                    dialog.message,
                ) {
                    DialogResponse::Ok | DialogResponse::Cancel | DialogResponse::Continue => {}
                    DialogResponse::IsShowing => {
                        dialogs_to_keep.push(dialog.id);
                    }
                },
                DialogKind::StandardSemiModal => {
                    // draw the egui window here.
                    if let Some(content) = &mut dialog.content {
                        match content() {
                            DialogStateInNextFrame::Show => {
                                dialogs_to_keep.push(dialog.id);
                            }
                            DialogStateInNextFrame::Closed => {}
                        }
                    }
                }
            }
        }

        self.dialog_queue
            .retain(|data| dialogs_to_keep.contains(&data.id));

        match self.dialog_request_queue_rx.try_recv() {
            Ok(request) => {
                let id = self.next_dialog_id;
                self.next_dialog_id += 1;
                match request {
                    DialogServiceRequest::Error(title, message) => {
                        self.dialog_queue.push_front(DialogData {
                            kind: DialogKind::Error,
                            title,
                            message,
                            id,
                            content: None,
                        });
                    }
                    DialogServiceRequest::Info(title, message) => {
                        self.dialog_queue.push_front(DialogData {
                            kind: DialogKind::Info,
                            title,
                            message,
                            id,
                            content: None,
                        });
                    }
                    DialogServiceRequest::SemiModalDialog(title, content) => {
                        self.dialog_queue.push_back(DialogData {
                            kind: DialogKind::StandardSemiModal,
                            title,
                            message: "StandardDialog",
                            id,
                            content: Some(content),
                        });
                    }
                }
            }
            Err(err) => match err {
                TryRecvError::Empty => {}
                TryRecvError::Closed => panic!("Dialog request queue is closed"),
            },
        }
    }
}
