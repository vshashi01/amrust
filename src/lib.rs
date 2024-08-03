mod threemf_reader;

use std::{ffi::OsStr, fs, io::BufReader, path::PathBuf};

use egui::{DroppedFile, FontId, RichText};
use threemf_reader::load_threemf_get_root_model_file_as_string;

pub struct MyApp {
    name: String,
    dropped_files: Vec<DroppedFile>,
    file_to_render: Option<DroppedFile>,
    font_size: f32,
}

impl Default for MyApp {
    fn default() -> Self {
        Self {
            name: "AMRUST".to_owned(),
            dropped_files: Vec::new(),
            file_to_render: None,
            font_size: 14.0,
        }
    }
}

impl eframe::App for MyApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            if let Some(file) = &self.file_to_render {
                let path = file.path.as_ref().unwrap();
                let text_to_display = process_file_and_get_text(&path);
                // let text_string = fs::read_to_string(path).unwrap();
                // ui.label(format!(
                //     "{:?} can be rendered now",
                //     path.file_name().unwrap()
                // ));
                ui.vertical(|ui| {
                    ui.horizontal_top(|ui| {
                        if ui.button("+").clicked() {
                            self.font_size += 1.0;
                        }
                        if ui.button("-").clicked() {
                            self.font_size -= 1.0;
                        }
                        if ui.button("Clear content").clicked() {
                            self.file_to_render = None;
                        }
                    });

                    ui.add(
                        egui::Separator::default()
                            .horizontal()
                            .shrink(4.0)
                            .spacing(10.0),
                    );
                    egui::ScrollArea::both()
                        .auto_shrink(false)
                        .scroll_bar_visibility(
                            egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded,
                        )
                        .show(ui, |ui| {
                            ui.label(
                                RichText::new(text_to_display)
                                    .font(FontId::monospace(self.font_size)),
                            );
                        });
                });
            } else {
                ui.centered_and_justified(|ui| {
                    ui.image(egui::include_image!("../assets/ferris.png"));
                });
            }

            if !self.dropped_files.is_empty() {
                for i in 0..self.dropped_files.len() {
                    let file = &self.dropped_files[i];
                    if let Some(path) = &file.path {
                        if can_process_file(path.to_path_buf()) {
                            self.file_to_render = Some(file.clone());
                            break;
                        }
                    }
                }
            }

            self.dropped_files.clear();
            preview_files_being_dropped(ctx);
            ctx.input(|i| {
                if !i.raw.dropped_files.is_empty() {
                    self.dropped_files.clone_from(&i.raw.dropped_files);
                }
            });
        });
    }
}

fn process_file_and_get_text(path: &PathBuf) -> String {
    let text_string = match path.extension().and_then(OsStr::to_str) {
        Some("3mf") => {
            //do nothing
            if let Ok(file) = fs::File::open(path) {
                return load_threemf_get_root_model_file_as_string(file).unwrap();
            }
            "3mf found".to_string()
        }
        Some("txt") => fs::read_to_string(path).unwrap(),
        Some("obj") => fs::read_to_string(path).unwrap(),
        _ => "Nothing found".to_string(),
    };

    text_string
}

// Preview hovering files:
fn preview_files_being_dropped(ctx: &egui::Context) {
    use egui::*;
    use std::fmt::Write as _;

    let mut is_unsupported_file_exist = false;

    if !ctx.input(|i| i.raw.hovered_files.is_empty()) {
        let text = ctx.input(|i| {
            let mut text = "Dropping files:\n".to_owned();
            for file in &i.raw.hovered_files {
                if let Some(path) = &file.path {
                    write!(text, "\n{}", path.display()).ok();
                    if !can_process_file(path.to_path_buf()) {
                        is_unsupported_file_exist = true;
                    }
                } else {
                    text += "\n???";
                }
            }
            text
        });

        let painter =
            ctx.layer_painter(LayerId::new(Order::Foreground, Id::new("file_drop_target")));

        let screen_rect = ctx.screen_rect();
        painter.rect_filled(screen_rect, 0.0, Color32::from_black_alpha(192));
        painter.text(
            screen_rect.center(),
            Align2::CENTER_CENTER,
            text,
            TextStyle::Heading.resolve(&ctx.style()),
            Color32::WHITE,
        );
        let mut stroke = (20.0, Color32::DARK_GREEN);
        if is_unsupported_file_exist {
            stroke = (20.0, Color32::DARK_RED)
        }
        painter.rect_stroke(screen_rect, 0.0, stroke);
    }
}

fn can_process_file(path: PathBuf) -> bool {
    let result = match path.extension().and_then(OsStr::to_str) {
        Some("txt") => true,
        Some("obj") => true,
        Some("3mf") => true,
        _ => false,
    };

    result
}
