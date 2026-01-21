use egui::{self, Id};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DialogResponse {
    Ok,
    Cancel,
    Continue,
    IsShowing,
}

struct DialogStyle {
    pub banner_color: egui::Color32,
    pub icon: String, // could be emoji, or later change to TextureHandle
}

fn show_modal_message_dialog(
    ctx: &egui::Context,
    id: Id,
    style: &DialogStyle,
    title: &str,
    content_ui: impl FnOnce(&mut egui::Ui),
    show_cancel: bool,
    show_continue: bool,
) -> DialogResponse {
    let mut response = DialogResponse::IsShowing;
    egui::Modal::new(id).show(ctx, |ui| {
        ui.set_width(300.0);
        ui.set_height(100.0);

        ui.horizontal(|ui| {
            // LEFT BANNER (fixed width)
            let banner_size = egui::vec2(80.0, 100.0);
            ui.allocate_ui_with_layout(
                banner_size,
                egui::Layout::centered_and_justified(egui::Direction::TopDown),
                |banner_ui| {
                    banner_ui.painter().rect_filled(
                        banner_ui.max_rect(), // full area
                        0.0,
                        style.banner_color,
                    );

                    // Icon in center
                    //banner_ui.add_space(banner_size.y * 0.4);
                    banner_ui.label(
                        egui::RichText::new(&style.icon)
                            .size(30.0)
                            .color(egui::Color32::WHITE),
                    );
                },
            );

            //actual content
            ui.vertical(|ui| {
                ui.label(
                    egui::RichText::new(title)
                        .size(18.0)
                        .color(style.banner_color),
                );
                ui.add_space(8.0);

                // Draw custom content
                content_ui(ui);
                ui.add_space(8.0); // some spacing between content and footer

                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::RIGHT), |ui| {
                        if show_cancel && ui.button("Cancel").clicked() {
                            ui.close();
                            response = DialogResponse::Cancel;
                        }

                        if show_continue && ui.button("Continue").clicked() {
                            ui.close();
                            response = DialogResponse::Continue;
                        }

                        if ui.button("OK").clicked() {
                            ui.close();
                            response = DialogResponse::Ok;
                        }
                    })
                })
            })
        })
    });

    response
}

pub fn error_modal_dialog(
    ctx: &egui::Context,
    id: egui::Id,
    title: &str,
    message: &str,
) -> DialogResponse {
    let style = DialogStyle {
        banner_color: egui::Color32::RED,
        icon: "❌".to_string(),
    };

    show_modal_message_dialog(
        ctx,
        id,
        &style,
        title,
        |ui| {
            ui.colored_label(egui::Color32::RED, "An unexpected error occurred!");
            ui.label(message);

            ui.add_space(12.0);
        },
        false,
        false,
    )
}

pub fn info_modal_dialog(
    ctx: &egui::Context,
    id: egui::Id,
    title: &str,
    message: &str,
) -> DialogResponse {
    let style = DialogStyle {
        banner_color: egui::Color32::from_rgb(0, 120, 255),
        icon: "ℹ️".to_string(),
    };

    show_modal_message_dialog(
        ctx,
        id,
        &style,
        title,
        |ui| {
            ui.label(message);

            ui.add_space(12.0);
        },
        false,
        false,
    )
}
