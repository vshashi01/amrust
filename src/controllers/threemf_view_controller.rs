use anyhow::{anyhow, Result};
use egui::{ColorImage, TextureHandle};
use roxmltree::{Document, NodeId};
use threemf::io::threemf_unpacked::ThreemfUnpacked;

use crate::widgets::{file_tree::FileTree, start_page::start_page};

use super::{xml_content_view_controller::XmlContentViewController, StandardFileViewController};

use std::{collections::HashMap, fs::File, path::PathBuf};

pub struct ThreemfViewController {
    unpacked: ThreemfUnpacked,
    current_content_state: ContentState,
    cached_xml: HashMap<String, XmlContentViewController>,
    cached_image_texture: HashMap<String, TextureHandle>,
    current_selected: String,
    file_tree: FileTree,
}

pub enum ContentState {
    Empty,
    XmlContent(String),
    ThreemfModelContent(String),
    Image(String),
    UnknownData(String),
}

impl ThreemfViewController {
    pub fn from_file(path: &PathBuf) -> Result<Self> {
        let unpacked = get_threemf_unpacked(path)?;
        let result = process_threemf_unpacked(&unpacked);
        if let Some(file_tree) = result {
            Ok(ThreemfViewController {
                unpacked,
                current_content_state: ContentState::Empty,
                cached_xml: HashMap::new(),
                cached_image_texture: HashMap::new(),
                current_selected: String::new(),
                file_tree,
            })
        } else {
            Err(anyhow!("No tree was generated for threemf"))
        }
    }

    fn check_for_duplicate_uuids(&mut self, _ui: &mut egui::Ui, _ctx: &egui::Context) {
        if let ContentState::ThreemfModelContent(path) = &self.current_content_state {
            let model_string = match path.as_str() {
                "Root Model" => Some(&self.unpacked.root),
                sub_model_path => self.unpacked.sub_models.get(sub_model_path),
            };

            if let Some(model_string) = model_string {
                let xml_document = Document::parse(model_string);
                match xml_document {
                    Ok(doc) => {
                        let mut all_uuid_items: Vec<(NodeId, String, String)> = vec![];
                        for node in doc.descendants().filter(|n| {
                            n.tag_name().name().to_lowercase() == "item"
                                || n.tag_name().name().to_lowercase() == "object"
                                || n.tag_name().name().to_lowercase() == "build"
                        }) {
                            let uuid =
                                node.attribute((threemf::threemf_namespaces::PROD_NS, "UUID"));
                            if let Some(uuid) = uuid {
                                all_uuid_items.push((
                                    node.id(),
                                    node.tag_name().name().to_owned(),
                                    uuid.to_owned(),
                                ));
                            }
                        }

                        if !all_uuid_items.is_empty() {
                            let ids = all_uuid_items
                                .iter()
                                .map(|(node_id, _, _)| node_id.get_usize())
                                .collect::<Vec<usize>>();

                            if let Some(controller) = self.cached_xml.get_mut(path) {
                                controller.add_highlight_ids(ids);
                                _ctx.request_repaint();
                            }
                        }
                        log::info!("{:?}", all_uuid_items);
                        log::info!("Number of UUIDs {:?}", all_uuid_items.len());
                    }
                    Err(_) => todo!(),
                }
            }
        }
    }
}

impl StandardFileViewController for ThreemfViewController {
    fn has_file_tree(&self) -> bool {
        true
    }

    fn file_tree_ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        self.file_tree.ui(
            ui,
            5,
            "Threemf",
            &mut self.current_selected,
            &mut |(path, parent_name)| {
                match (path.as_str(), parent_name.as_str()) {
                    ("[Content_Types].xml", _)
                    | (_, "Relationships") => {
                        if !self.cached_xml.contains_key(path) {
                            let xml_string = match (path.as_str(), parent_name.as_str()) {
                                ("[Content_Types].xml", _) => Some(&self.unpacked.content_types),
                                (path, "Relationships") => self.unpacked.relationships.get(path),
                                _ => None,
                            };

                            if let Some(xml_string) = xml_string {
                                let xml_controller = XmlContentViewController::from_xml(
                                    xml_string,
                                    &vec![],
                                );
                                match xml_controller {
                                    Ok(xml_controller) => {
                                        self.cached_xml.insert(path.to_owned(), xml_controller);
                                        self.current_content_state =
                                        ContentState::XmlContent(path.to_owned());
                                    }
                                    Err(err) => {
                                        log::error!("Failed to extract XML content with path {} on parent {}, with error {:?}", path, parent_name, err);
                                        self.current_content_state = ContentState::Empty;
                                    }
                                }
                                ctx.request_repaint();
                            }

                        }
                    }
                    ("Root Model", _) | (_, "Sub Models")=> {
                        if !self.cached_xml.contains_key(path) {
                            let xml_string = match (path.as_str(), parent_name.as_str()) {
                                ("Root Model", _) => Some(&self.unpacked.root),
                                (path, "Sub Models") => self.unpacked.sub_models.get(path),
                                _ => None,
                            };

                            if let Some(xml_string) = xml_string {
                                let xml_controller = XmlContentViewController::from_xml(
                                    xml_string,
                                    &vec!["vertices", "triangles", "beams"],
                                );
                                match xml_controller {
                                    Ok(xml_controller) => {
                                        self.cached_xml.insert(path.to_owned(), xml_controller);
                                        self.current_content_state =
                                        ContentState::ThreemfModelContent(path.to_owned());
                                    }
                                    Err(err) => {
                                        log::error!("Failed to extract XML content with path {} on parent {}, with error {:?}", path, parent_name, err);
                                        self.current_content_state = ContentState::Empty;
                                    }
                                }
                                ctx.request_repaint();
                            }
                        }
                    }
                    (path, "Thumbnails") => {
                    let bytes = self.unpacked.thumbnails.get(path);
                    match bytes {
                        Some(bytes) => {
                            let thumbnail = image::load_from_memory(bytes);
                            match thumbnail {
                                Ok(thumbnail) => {
                                    let handle = create_texturehandle_from_image(ctx, path, thumbnail);
                                    self.cached_image_texture.insert(path.to_owned(), handle);
                                    self.current_content_state = ContentState::Image(path.to_owned());

                                    ctx.request_repaint();
                                }
                                Err(err) => {
                                    log::error!("Failed to load thumbnail from bytes in path {} with error {:?}", path, err);
                                    self.current_content_state = ContentState::Empty;
                                }
                            }
                        }
                        None => log::error!("Failed to extract image"),
                    }

                    }
                    (path, "Unknown Parts") => {
                        self.current_content_state = ContentState::UnknownData(path.to_owned());
                    }
                    _ => {
                        self.current_content_state = ContentState::Empty;
                    }
                };
            },
        );
    }

    fn content_ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        match &self.current_content_state {
            ContentState::Empty => {
                start_page(ui);
            }
            ContentState::XmlContent(path) | ContentState::ThreemfModelContent(path) => {
                let xml_content = self.cached_xml.get_mut(path);
                match xml_content {
                    Some(content) => {
                        content.content_ui(ui, ctx);
                    }
                    None => {
                        log::error!("No cached xml");
                    }
                }
            }
            ContentState::Image(path) => {
                let texture_handle = self.cached_image_texture.get(path);
                match texture_handle {
                    Some(texture_handle) => {
                        let size = texture_handle.size_vec2();
                        ui.image((texture_handle.id(), size));
                    }
                    None => {
                        log::error!("No cached image");
                    }
                }
            }
            ContentState::UnknownData(path) => {
                let bytes = self.unpacked.unknown_parts.get(path);
                match bytes {
                    Some(bytes) => {
                        ui.label(format!("Unknown data is {} bytes", bytes.len()));
                    }
                    None => {
                        log::error!("No unknown data bytes found");
                    }
                }
            }
        };
    }

    fn add_menu_button(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        match &self.current_content_state {
            ContentState::Empty => {}
            ContentState::XmlContent(_) => {}
            ContentState::ThreemfModelContent(_) => {
                ui.menu_button("Process 3mf", |ui| {
                    if ui.button("Check for Duplicate UUIDs").clicked() {
                        self.check_for_duplicate_uuids(ui, ctx);
                    }
                });
            }
            ContentState::Image(_) => {}
            ContentState::UnknownData(_) => {}
        }
    }
}

fn create_texturehandle_from_image(
    ctx: &egui::Context,
    path: &str,
    thumbnail: image::DynamicImage,
) -> TextureHandle {
    let size = [thumbnail.width() as usize, thumbnail.height() as usize];
    let image_buffer = thumbnail.to_rgba8();
    let pixels = image_buffer.as_flat_samples();
    let color_image = ColorImage::from_rgba_unmultiplied(size, pixels.as_slice());

    ctx.load_texture(path, color_image, egui::TextureOptions::default())
}

pub fn get_threemf_unpacked(path: &PathBuf) -> Result<ThreemfUnpacked> {
    let file = File::open(path).unwrap();
    let package = ThreemfUnpacked::from_reader(file, true);

    Ok(package.unwrap())
}

fn process_threemf_unpacked(unpacked: &ThreemfUnpacked) -> Option<FileTree> {
    let mut sub_trees = Vec::new();

    let content_types = "[Content_Types].xml".to_owned();

    let root_model = "Root Model".to_owned();

    let mut relationships = Vec::new();
    for name in unpacked.relationships.keys() {
        relationships.push(name.clone());
    }
    sub_trees.push(FileTree {
        name: "Relationships".to_owned(),
        files: relationships,
        child_folders: vec![],
    });

    let mut sub_models = Vec::new();
    for name in unpacked.sub_models.keys() {
        sub_models.push(name.clone());
    }
    sub_trees.push(FileTree {
        name: "Sub Models".to_owned(),
        files: sub_models,
        child_folders: vec![],
    });

    let mut thumbnails = Vec::new();
    for name in unpacked.thumbnails.keys() {
        thumbnails.push(name.clone());
    }
    sub_trees.push(FileTree {
        name: "Thumbnails".to_owned(),
        files: thumbnails,
        child_folders: vec![],
    });

    let mut unknown_datas = Vec::new();
    for name in unpacked.unknown_parts.keys() {
        unknown_datas.push(name.clone());
    }
    sub_trees.push(FileTree {
        name: "Unknown Parts".to_owned(),
        files: unknown_datas,
        child_folders: vec![],
    });

    Some(FileTree {
        name: "Threemf".to_owned(),
        files: vec![content_types, root_model],
        child_folders: sub_trees,
    })
}
