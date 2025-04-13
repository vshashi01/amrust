use anyhow::{anyhow, Result};
use egui::{DroppedFile, Layout};

mod controllers;
mod widgets;
use controllers::{
    threemf_view_controller::ThreemfViewController,
    xml_content_view_controller::XmlContentViewController, StandardFileViewModel,
};

use std::{ffi::OsStr, fs, path::PathBuf};

enum ViewModel {
    Threemf(Box<ThreemfViewController>),
    Xml(Box<XmlContentViewController>),
}

pub struct MyApp {
    name: String,
    dropped_files: Vec<DroppedFile>,
    view_model: Option<ViewModel>,
    rendered_file_name: Option<String>,
    show_log: bool,
}

impl Default for MyApp {
    fn default() -> Self {
        Self {
            name: "AMRUST".to_owned(),
            dropped_files: Vec::new(),
            view_model: None,
            rendered_file_name: None,
            show_log: false,
        }
    }
}

impl eframe::App for MyApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::TopBottomPanel::top("top panel")
            .resizable(false)
            .show(ctx, |ui| {
                ui.vertical(|ui| {
                    egui::menu::bar(ui, |ui| {
                        ui.menu_button("View", |ui| {
                            if ui.button("Show Log").clicked() {
                                self.show_log = !self.show_log;
                            }
                        })
                    });

                    if let Some(file_name) = self.rendered_file_name.clone() {
                        ui.add(
                            egui::Separator::default()
                                .horizontal()
                                .shrink(4.0)
                                .spacing(10.0),
                        );

                        ui.horizontal_top(|ui| {
                            ui.label(file_name);

                            ui.with_layout(Layout::right_to_left(egui::Align::Min), |ui| {
                                if ui.button("Clear content").clicked() {
                                    self.clear_state();
                                }
                            });
                        });
                    }
                });
            });
        if self.view_model.is_some() {
            egui::SidePanel::left("left_panel")
                .resizable(true)
                .default_width(100.0)
                .show(ctx, |ui| {
                    egui::ScrollArea::both().auto_shrink(false).show(ui, |ui| {
                        if let Some(model) = &mut self.view_model {
                            match model {
                                ViewModel::Threemf(model) => model.file_tree_ui(ui),
                                ViewModel::Xml(model) => model.file_tree_ui(ui),
                            }
                        }
                    });
                });
        }

        if self.show_log {
            egui::TopBottomPanel::bottom("bottom_panel")
                .resizable(true)
                .show_separator_line(true)
                .show(ctx, |ui| {
                    egui_logger::LoggerUi::default().enable_regex(true).show(ui);
                });
        }
        egui::CentralPanel::default().show(ctx, |ui| {
            if self.view_model.is_some() {
                ui.vertical(|ui| {
                    egui::ScrollArea::both()
                        .auto_shrink(false)
                        .scroll_bar_visibility(
                            egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded,
                        )
                        .show(ui, |ui| {
                            if let Some(threemf) = &self.view_model {
                                match threemf {
                                    ViewModel::Threemf(model) => model.content_ui(ui),
                                    ViewModel::Xml(model) => model.content_ui(ui),
                                }
                            }
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
                                    log::info!("All went well");
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
        let view_model = match path.extension().and_then(OsStr::to_str) {
            Some("3mf") => {
                log::info!("This is the 3mf path: {:?}", path);
                let threemf_model = ThreemfViewController::from_file(path);
                match threemf_model {
                    Ok(model) => ViewModel::Threemf(Box::new(model)),
                    Err(err) => return Err(err),
                }
            }
            Some("xml") => {
                log::info!("This is the xml path: {:?}", path);
                let xml_string = fs::read_to_string(path)?;
                let xml_model = XmlContentViewController::from_xml(&xml_string);
                match xml_model {
                    Ok(model) => ViewModel::Xml(Box::new(model)),
                    Err(err) => return Err(err),
                }
            }
            _ => return Err(anyhow!("File format not supported")),
        };

        self.clear_state();
        self.view_model = Some(view_model);
        self.rendered_file_name = Some("file_name".to_owned());

        Ok(true)
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
            painter.rect_stroke(screen_rect, 0.0, stroke, StrokeKind::Middle);
        }
    }

    fn can_process_file(&self, path: PathBuf) -> bool {
        matches!(
            path.extension().and_then(OsStr::to_str),
            Some("3mf") | Some("xml")
        )
    }

    fn clear_state(&mut self) {
        self.rendered_file_name = None;
        self.view_model = None;
    }
}
