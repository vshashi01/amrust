// mod threemf_reader;
mod threemf;
mod widgets;
use threemf::threemf_reader;
use widgets::tree;

use std::{ffi::OsStr, fs, path::PathBuf};

use anyhow::{anyhow, Result};
use egui::{DroppedFile, FontId, RichText};

pub struct MyApp {
    name: String,
    dropped_files: Vec<DroppedFile>,
    file_to_render: Option<String>,
    rendered_file_name: Option<String>,
    font_size: f32,
    trees: Option<Vec<tree::Tree>>,
    show_log: bool,
}

impl Default for MyApp {
    fn default() -> Self {
        Self {
            name: "AMRUST".to_owned(),
            dropped_files: Vec::new(),
            file_to_render: None,
            rendered_file_name: None,
            font_size: 14.0,
            trees: None,
            show_log: false,
        }
    }
}

impl eframe::App for MyApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::TopBottomPanel::top("top panel")
            .resizable(false)
            .show(ctx, |ui| {
                egui::menu::bar(ui, |ui| {
                    ui.menu_button("View", |ui| {
                        if ui.button("Show Log").clicked() {
                            self.show_log = !self.show_log;
                        }
                    })
                });
            });
        if let Some(trees) = &self.trees {
            egui::SidePanel::left("left_panel")
                .resizable(true)
                .default_width(100.0)
                .show(ctx, |ui| {
                    egui::ScrollArea::both().auto_shrink(false).show(ui, |ui| {
                        let mut count = 1;
                        for tree in trees {
                            tree.ui(ui, 0, &format!("{} - {}", tree.name, count));
                            count += 1;
                        }
                    });
                });
        }

        if self.show_log {
            egui::TopBottomPanel::bottom("bottom_panel")
                // .default_height(50.0)
                .resizable(true)
                .show_separator_line(true)
                .show(ctx, |ui| {
                    egui_logger::LoggerUi::default().enable_regex(true).show(ui);
                });
        }
        egui::CentralPanel::default().show(ctx, |ui| {
            if let Some(string) = &self.file_to_render {
                let text_to_display = string.clone();
                ui.vertical(|ui| {
                    ui.horizontal_top(|ui| {
                        if let Some(file_name) = &self.rendered_file_name {
                            ui.label(file_name);
                        }

                        if ui.button("+").clicked() {
                            self.font_size += 1.0;
                        }
                        if ui.button("-").clicked() {
                            self.font_size -= 1.0;
                        }
                        if ui.button("Clear content").clicked() {
                            self.clear_state();
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
                    let file = self.dropped_files[i].clone();
                    if let Some(path) = &file.path {
                        let processed = self.processed_file_and_update_app(path);
                        match processed {
                            Ok(success) => {
                                if success {
                                    log::debug!("All went well");
                                    //only the first successful file is processed
                                    // break;
                                } else {
                                    let ext = path.extension();
                                    log::error!("File format of type {:?} not supported", ext);
                                }
                            }
                            Err(e) => log::error!("{:?}", e),
                        }
                    }
                }
            }

            self.dropped_files.clear();
            self.preview_files_being_dropped(ctx);
            ctx.input(|i| {
                if !i.raw.dropped_files.is_empty() {
                    self.dropped_files.clone_from(&i.raw.dropped_files);
                }
            });
        });
    }
}

impl MyApp {
    fn processed_file_and_update_app(&mut self, path: &PathBuf) -> Result<bool> {
        let processed_file_and_tree = match path.extension().and_then(OsStr::to_str) {
            Some("3mf") => {
                let file = fs::File::open(path)?;
                let file_to_render =
                    threemf_reader::load_threemf_get_root_model_file_as_string(file)?;
                let result = tree::Tree::new_trees_from_xml_string(&file_to_render);
                match result {
                    Ok(trees) => {
                        let trees = Some(trees);
                        Ok((Some(file_to_render), trees))
                    }
                    Err(e) => return Err(e),
                }
            }
            Some("txt") => {
                let file_to_render = Some(fs::read_to_string(path)?);
                Ok((file_to_render, None))
            }
            Some("obj") => {
                let file_to_render = Some(fs::read_to_string(path)?);
                Ok((file_to_render, None))
            }
            Some("xml") => {
                let file_to_render = fs::read_to_string(path)?;
                let result = tree::Tree::new_trees_from_xml_string(&file_to_render);
                match result {
                    Ok(trees) => {
                        let trees = Some(trees);
                        Ok((Some(file_to_render), trees))
                    }
                    Err(e) => return Err(e),
                }
            }
            _ => Err(anyhow!("File format not supported")),
        };

        let status = match processed_file_and_tree {
            Ok((file_to_render, trees)) => {
                self.clear_state();
                self.file_to_render = file_to_render;
                self.trees = trees;
                self.rendered_file_name = match path.file_name().and_then(OsStr::to_str) {
                    Some(file_name) => Some(file_name.to_string()),
                    None => None,
                };
                Ok(true)
            }
            Err(e) => Err(e),
        };

        status
    }

    // Preview hovering files:
    fn preview_files_being_dropped(&self, ctx: &egui::Context) {
        use egui::*;
        use std::fmt::Write as _;

        let mut is_unsupported_file_exist = false;

        if !ctx.input(|i| i.raw.hovered_files.is_empty()) {
            let text = ctx.input(|i| {
                let mut text = "Dropping files:\n".to_owned();
                for file in &i.raw.hovered_files {
                    if let Some(path) = &file.path {
                        write!(text, "\n{}", path.display()).ok();
                        if !self.can_process_file(path.to_path_buf()) {
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

    fn can_process_file(&self, path: PathBuf) -> bool {
        let result = match path.extension().and_then(OsStr::to_str) {
            Some("txt") => true,
            Some("obj") => true,
            Some("3mf") => true,
            Some("xml") => true,
            _ => false,
        };

        result
    }

    fn clear_state(&mut self) {
        self.file_to_render = None;
        self.trees = None;
        self.rendered_file_name = None;
    }
}
