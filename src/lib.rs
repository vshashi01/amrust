// mod threemf_reader;
mod threemf;
use threemf::threemf_reader;
use xml_dom::level2::{CharacterData, Node, NodeType, RefNode};

use std::{ffi::OsStr, fs, path::PathBuf};

use anyhow::{anyhow, Result};
use egui::{CollapsingHeader, DroppedFile, FontId, RichText};

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
                                    println!("All went well");
                                    //only the first successful file is processed
                                    break;
                                } else {
                                    let ext = path.extension();
                                    println!("File format of type {:?} not supported", ext);
                                }
                            }
                            Err(e) => println!("{:?}", e),
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
                let file_to_render = Some(
                    threemf_reader::load_threemf_get_root_model_file_as_string(file)?,
                );
                let result =
                    Tree::new_trees_from_xml_string(&self.file_to_render.as_ref().unwrap());
                match result {
                    Ok(trees) => {
                        let trees = Some(trees);
                        Ok((file_to_render, trees))
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
            _ => Err(anyhow!("File format not supported")),
        };

        let status = match processed_file_and_tree {
            Ok((file_to_render, trees)) => {
                self.clear_state();
                self.file_to_render = file_to_render;
                self.trees = trees;
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
            _ => false,
        };

        result
    }

    fn clear_state(&mut self) {
        self.file_to_render = None;
        self.trees = None;
    }
}

#[derive(Debug)]
struct Tree {
    name: String,
    content: Option<String>,
    attributes: Option<Vec<(String, String)>>,
    childs: Option<Vec<Tree>>,
}

impl Tree {
    fn new_trees_from_xml_string(xml_string: &String) -> Result<Vec<Self>> {
        let dom = threemf_reader::get_xml_dom_from_3mf_model_file_string(xml_string)?;
        println!("{:?}", dom);
        let result = process_dom(dom);
        if let Some(tree) = result.0 {
            Ok(tree)
        } else {
            Err(anyhow!("No tree was generated"))
        }
    }
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
            // if not an Element then there is highly likely it
            // contains some data that belongs to the Element item itself
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
