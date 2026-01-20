use egui::ModalResponse;
use smol::channel::{Receiver, TryRecvError};

pub enum DialogServiceRequest {
    ShowErrorDialog(&'static str, &'static str),
    ShowInfoDialog(&'static str, &'static str),
    // ShowModalDialog,
    // ShowSemiModalDialog,
    // ShowNonModalPopupDialog,
}

pub trait ShowableDialog {
    fn update(&mut self, ctx: egui::Context, ui: &mut egui::Ui);
}

pub struct DialogService {
    dialog_request_queue_rx: Receiver<DialogServiceRequest>,

    next_dialog_id: u64,
}

impl DialogService {
    pub fn new(dialog_request_queue_rx: Receiver<DialogServiceRequest>) -> Self {
        Self {
            dialog_request_queue_rx,
            next_dialog_id: 0,
        }
    }

    pub fn update_and_get_dialogs_to_service(
        &mut self,
        ctx: &mut egui::Context,
        ui: &mut egui::Ui,
    ) {
        match self.dialog_request_queue_rx.try_recv() {
            Ok(request) => match request {
                DialogServiceRequest::ShowErrorDialog(title, message) => {
                    egui::Modal::new(egui::Id::new(self.next_dialog_id)).show(ctx, |ui| {
                        ui.label(message);
                        if ui.button("OK").clicked() {}
                    });
                }
                DialogServiceRequest::ShowInfoDialog(title, message) => todo!(),
            },
            Err(err) => match err {
                TryRecvError::Empty => {}
                TryRecvError::Closed => panic!("Dialog request queue is closed"),
            },
        }
    }
}
