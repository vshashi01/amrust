// mod threemf_reader;
mod threemf;
use quick_xml::events::attributes;
use threemf::threemf_reader;
use xml_dom::level2::{ext::Namespaced, CharacterData, Document, Element, Node, NodeType, RefNode};

use std::{ffi::OsStr, fmt::format, fs, path::PathBuf};

use anyhow::Result;
use egui::{ahash::HashMap, CollapsingHeader, DroppedFile, FontId, RichText};

pub struct MyApp {
    name: String,
    dropped_files: Vec<DroppedFile>,
    file_to_render: Option<String>,
    font_size: f32,
    trees: Option<Vec<Tree>>,
}

impl Default for MyApp {
    fn default() -> Self {
        Self {
            name: "AMRUST".to_owned(),
            dropped_files: Vec::new(),
            file_to_render: None,
            font_size: 14.0,
            trees: None,
        }
    }
}

impl eframe::App for MyApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if let Some(trees) = &self.trees {
            egui::SidePanel::left("left_panel")
                .resizable(true)
                .default_width(100.0)
                .show(ctx, |ui| {
                    egui::ScrollArea::both().auto_shrink(false).show(ui, |ui| {
                        let mut count = 1;
                        for tree in trees {
                            tree.ui_impl(ui, 0, &format!("{} - {}", tree.name, count));
                            count += 1;
                        }
                    });
                });
        }
        egui::CentralPanel::default().show(ctx, |ui| {
            if let Some(string) = &self.file_to_render {
                let text_to_display = string.clone();
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
                            self.trees = None;
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
                            self.file_to_render = Some(process_file_and_get_text(path));
                            if process_file_if_extension_matches(path, "3mf")
                                && self.file_to_render.is_some()
                            {
                                let result = generate_tree_from_3mf_xml_dom(
                                    &self.file_to_render.as_ref().unwrap(),
                                );

                                match result {
                                    Ok(trees) => self.trees = Some(trees),
                                    Err(e) => println!("{:?}", e),
                                }
                            }

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
            if let Ok(file) = fs::File::open(path) {
                return threemf_reader::load_threemf_get_root_model_file_as_string(file).unwrap();
            }
            "3mf found".to_string()
        }
        Some("txt") => fs::read_to_string(path).unwrap(),
        Some("obj") => fs::read_to_string(path).unwrap(),
        _ => "Nothing found".to_string(),
    };

    text_string
}

fn process_file_if_extension_matches(path: &PathBuf, target_ext: &str) -> bool {
    path.extension().unwrap().to_str().unwrap() == target_ext
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

#[derive(Debug)]
struct Tree {
    name: String,
    content: Option<String>,
    attributes: Option<Vec<(String, String)>>,
    childs: Option<Vec<Tree>>,
}

impl Tree {
    fn ui_impl(&self, ui: &mut egui::Ui, depth: usize, unique_id: &str) {
        let name = if let Some(content) = &self.content {
            format!("{} - {}", self.name, content)
        } else {
            self.name.clone()
        };
        CollapsingHeader::new(name)
            .default_open(depth < 1)
            .id_source(unique_id)
            .show(ui, |ui| {
                if let Some(attributes) = &self.attributes {
                    for attribute in attributes {
                        ui.label(format!("{} - {}", attribute.0, attribute.1));
                    }
                }

                self.children_ui(ui, depth)
            });
    }

    fn children_ui(&self, ui: &mut egui::Ui, depth: usize) {
        if let Some(trees) = &self.childs {
            let mut count = 0;
            for tree in trees {
                tree.ui_impl(ui, depth + 1, &format!("{} - {}", tree.name, count));
                count += 1;
            }
        }
    }
}

fn generate_tree_from_3mf_xml_dom(xml_content: &String) -> Result<Vec<Tree>> {
    let dom = threemf_reader::get_xml_dom_from_3mf_model_file_string(xml_content)?;
    println!("{:?}", dom);
    let trees = process_dom(dom);
    // println!("{:?}", trees);

    Ok(trees.0.unwrap())
}

fn process_dom(ref_node: RefNode) -> (Option<Vec<Tree>>, Option<String>) {
    let mut sub_trees = Vec::new();
    let mut sub_content = String::new();
    for node in ref_node.child_nodes() {
        let name = node.local_name();
        let mut attributes: Vec<(String, String)> = Vec::new();

        if node.node_type() == NodeType::Element {
            let attributes_map = node.attributes();

            for entry in attributes_map {
                if entry.1.node_type() == NodeType::Attribute {
                    let attribute_name = entry.0.local_name().to_string();
                    let mut attribute_value = String::new();

                    if let Some(value) = entry.1.data() {
                        attribute_value.push_str(&value);
                    } else {
                        for attribute_child_node in entry.1.child_nodes() {
                            if let Some(value) = attribute_child_node.data() {
                                attribute_value.push_str(&value);
                                // break;
                            }
                        }
                    };
                    attributes.push((attribute_name, attribute_value));
                }
            }

            let (childs, entry) = if node.has_child_nodes() {
                process_dom(node)
            } else {
                (None, None)
            };

            sub_trees.push(Tree {
                name,
                content: entry,
                attributes: Some(attributes),
                childs,
            });
        } else {
            if let Some(data) = node.data() {
                sub_content.push_str(&data);
            }
        }
    }

    let trees = if sub_trees.len() >= 1 {
        Some(sub_trees)
    } else {
        None
    };

    let content = if sub_content.len() >= 1 {
        Some(sub_content)
    } else {
        None
    };

    (trees, content)
}
