mod threemf;
mod widgets;
use threemf::threemf_reader::get_threemf_package;
use widgets::tree::{self, Tree};

use std::{ffi::OsStr, fs, path::PathBuf};

use anyhow::{anyhow, Result};
use egui::{
    ahash::{HashMap, HashMapExt},
    DroppedFile, Layout,
};

pub struct MyApp {
    name: String,
    dropped_files: Vec<DroppedFile>,
    current_selected_file: String,
    cached_tree: HashMap<String, Vec<Tree>>,
    rendered_file_name: Option<String>,
    current_trees_path: Option<String>,
    file_tree: Option<tree::Tree>,
    show_log: bool,
}

impl Default for MyApp {
    fn default() -> Self {
        Self {
            name: "AMRUST".to_owned(),
            dropped_files: Vec::new(),
            current_selected_file: "".to_owned(),
            cached_tree: HashMap::new(),
            rendered_file_name: None,
            current_trees_path: None,
            file_tree: None,
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
        if let Some(tree) = &self.file_tree {
            egui::SidePanel::left("left_panel")
                .resizable(true)
                .default_width(100.0)
                .show(ctx, |ui| {
                    egui::ScrollArea::both().auto_shrink(false).show(ui, |ui| {
                        tree.ui_with_selectable_contents(
                            ui,
                            0,
                            "ThreemfTest",
                            &mut self.current_selected_file,
                            &mut |path, content| {
                                if self.cached_tree.contains_key(path) {
                                    self.current_trees_path = Some(path.clone());
                                } else {
                                    let trees = tree::Tree::new_trees_from_xml_string(content);
                                    match trees {
                                        Ok(trees) => {
                                            self.cached_tree.insert(path.clone(), trees);
                                            self.current_trees_path = Some(path.clone());
                                        }
                                        Err(err) => println!("{:?}", err),
                                    }
                                }
                            },
                        );
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
            if self.current_trees_path.is_some() {
                ui.vertical(|ui| {
                    ui.horizontal_top(|ui| {
                        if let Some(file_name) = &self.rendered_file_name {
                            ui.label(file_name);
                        }

                        ui.with_layout(Layout::right_to_left(egui::Align::Min), |ui| {
                            if ui.button("Clear content").clicked() {
                                self.clear_state();
                            }
                        });
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
                            if let Some(path) = &self.current_trees_path {
                                let tree = self.cached_tree.get(path);
                                if let Some(trees) = tree {
                                    for (count, tree) in trees.iter().enumerate() {
                                        tree.ui(ui, 0, &format!("{} - {}", tree.name, count));
                                    }
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
                let package = get_threemf_package(path)?;
                let file_tree = tree::Tree::new_trees_from_threemf(&package);
                match file_tree {
                    Ok(tree) => Ok((None, Some(tree))),
                    Err(e) => return Err(e),
                }
            }
            Some("xml") => {
                let file_to_render = fs::read_to_string(path)?;
                let result = tree::Tree::new_trees_from_xml_string(&file_to_render);
                match result {
                    Ok(trees) => {
                        let trees = Some(trees);
                        Ok((trees, None))
                    }
                    Err(e) => return Err(e),
                }
            }
            _ => Err(anyhow!("File format not supported")),
        };

        let status = match processed_file_and_tree {
            Ok((trees, file_tree)) => {
                self.clear_state();
                self.current_trees_path = None;
                self.file_tree = file_tree;
                self.rendered_file_name = path
                    .file_name()
                    .and_then(OsStr::to_str)
                    .map(|file_name| file_name.to_string());
                if let Some(trees) = trees {
                    self.cached_tree.insert("target".to_owned(), trees);
                    self.current_trees_path = Some("target".to_owned());
                }
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
        self.current_trees_path = None;
        self.rendered_file_name = None;
        self.cached_tree = HashMap::new();
        self.file_tree = None;
    }
}
