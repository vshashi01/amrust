use amrust_3mf::io::threemf_unpacked::ThreemfUnpacked;
use anyhow::{Result, anyhow};
use egui::{ColorImage, TextureHandle};
use roxmltree::{Document, NodeId};

use crate::widgets::{file_tree::FileTree, start_page::start_page};

use super::{StandardFileViewController, xml_content_view_controller::XmlContentViewController};

use std::{collections::HashMap, fs::File, path::PathBuf};

pub struct ThreemfViewController {
    unpacked: ThreemfUnpacked,
    curr_state: ContentState,
    next_state: Option<ContentState>,
    cached_xml: HashMap<String, XmlContentViewController>,
    cached_image_texture: HashMap<String, TextureHandle>,
    current_selected: String,
    file_tree: FileTree,
}

#[derive(Debug, Clone)]
pub enum ContentState {
    Empty,
    XmlContent {
        path: String,
        parent: String,
    },
    ThreemfModelContent {
        path: String,
        parent: String,
        to_highlight: bool,
    },
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
                curr_state: ContentState::Empty,
                next_state: None,
                cached_xml: HashMap::new(),
                cached_image_texture: HashMap::new(),
                current_selected: String::new(),
                file_tree,
            })
        } else {
            Err(anyhow!("No tree was generated for threemf"))
        }
    }

    fn check_for_duplicate_uuids(&self, path: &str) -> Result<Option<Vec<(usize, String)>>> {
        let model_string = match path {
            "Root Model" => Some(&self.unpacked.root),
            sub_model_path => self.unpacked.sub_models.get(sub_model_path),
        };

        match model_string {
            Some(model_string) => {
                let doc = Document::parse(model_string)?;
                let mut duplicated_node_ids: Vec<(NodeId, String)> = vec![];
                let mut uuid_map = HashMap::<String, NodeId>::new();
                for node in doc.descendants().filter(|n| {
                    n.tag_name().name().to_lowercase() == "item"
                        || n.tag_name().name().to_lowercase() == "object"
                        || n.tag_name().name().to_lowercase() == "build"
                }) {
                    let uuid = node.attribute((amrust_3mf::threemf_namespaces::PROD_NS, "UUID"));
                    if let Some(uuid) = uuid {
                        if !uuid_map.contains_key(uuid) {
                            uuid_map.insert(uuid.to_owned(), node.id());
                            //for state update testing purposes.
                            //duplicated_node_ids.push((node.id(), uuid.to_owned()));
                        } else {
                            let node_id = uuid_map.get(uuid).unwrap();
                            duplicated_node_ids.push((*node_id, uuid.to_owned()));
                            duplicated_node_ids.push((node.id(), uuid.to_owned()));
                        }
                    }
                }
                if !duplicated_node_ids.is_empty() {
                    let ids = duplicated_node_ids
                        .iter()
                        .map(|(node_id, uuid)| (node_id.get_usize(), uuid.to_owned()))
                        .collect::<Vec<(usize, String)>>();

                    return Ok(Some(ids));
                }
                Ok(None)
            }
            None => Ok(None),
        }
    }

    fn process_generic_xml_paths(
        &self,
        path: &str,
        parent_name: &str,
    ) -> Option<XmlContentViewController> {
        let xml_string = match (path, parent_name) {
            ("[Content_Types].xml", _) => Some(&self.unpacked.content_types),
            (path, "Relationships") => self.unpacked.relationships.get(path),
            _ => None,
        };

        match xml_string {
            Some(xml_string) => {
                let xml_controller = XmlContentViewController::from_xml(xml_string, &vec![]);
                match xml_controller {
                    Ok(xml_controller) => Some(xml_controller),
                    Err(err) => {
                        log::error!(
                            "Failed to extract XML content with path {} on parent {}, with error {:?}",
                            path,
                            parent_name,
                            err
                        );
                        None
                    }
                }
            }
            None => None,
        }
    }

    fn process_model_xml_paths(
        &self,
        path: &str,
        parent_name: &str,
    ) -> Option<XmlContentViewController> {
        let xml_string = match (path, parent_name) {
            ("Root Model", _) => Some(&self.unpacked.root),
            (path, "Sub Models") => self.unpacked.sub_models.get(path),
            _ => None,
        };
        match xml_string {
            Some(xml_string) => {
                let xml_controller = XmlContentViewController::from_xml(
                    xml_string,
                    &vec!["vertices", "triangles", "beams"],
                );
                match xml_controller {
                    Ok(xml_controller) => Some(xml_controller),
                    Err(err) => {
                        log::error!(
                            "Failed to extract XML content with path {} on parent {}, with error {:?}",
                            path,
                            parent_name,
                            err
                        );
                        None
                    }
                }
            }
            None => None,
        }
    }

    fn process_thumbnail_path(&self, path: &str, ctx: &egui::Context) -> Option<TextureHandle> {
        let bytes = self.unpacked.thumbnails.get(path);
        match bytes {
            Some(bytes) => {
                let thumbnail = image::load_from_memory(bytes);
                match thumbnail {
                    Ok(thumbnail) => Some(create_texturehandle_from_image(ctx, path, thumbnail)),
                    Err(err) => {
                        log::error!(
                            "Failed to load thumbnail from bytes in path {} with error {:?}",
                            path,
                            err
                        );
                        None
                    }
                }
            }
            None => {
                log::error!("Failed to extract image");
                None
            }
        }
    }

    fn process_next_state(&mut self, ctx: &egui::Context) -> Result<()> {
        let next_state = &self.next_state.take();
        if let Some(state) = next_state {
            match state {
                ContentState::Empty => {
                    self.curr_state = ContentState::Empty;
                }
                ContentState::XmlContent { path, parent } => {
                    if !self.cached_xml.contains_key(path) {
                        let processed = self.process_generic_xml_paths(path, parent);
                        match processed {
                            Some(mut controller) => {
                                controller.update_state(ctx)?;
                                self.cached_xml.insert(path.to_owned(), controller);
                                self.curr_state = state.clone();
                            }
                            None => {
                                log::error!("Unable to create XML view for xml in path {}", path);
                            }
                        }
                    } else {
                        let controller = self.cached_xml.get_mut(path).unwrap();
                        controller.update_state(ctx)?;
                        self.curr_state = state.clone();
                    }
                }
                ContentState::ThreemfModelContent {
                    path,
                    parent,
                    to_highlight: false,
                } => {
                    if !self.cached_xml.contains_key(path) {
                        let processed = self.process_model_xml_paths(path, parent);
                        match processed {
                            Some(mut controller) => {
                                controller.update_state(ctx)?;
                                self.cached_xml.insert(path.to_owned(), controller);
                                self.curr_state = state.clone();
                            }
                            None => {
                                log::error!(
                                    "Unable to generate the XML view from the Model in path {}",
                                    path
                                );
                            }
                        }
                    } else {
                        let controller = self.cached_xml.get_mut(path).unwrap();
                        controller.clear_highlights();
                        controller.update_state(ctx)?;
                        self.curr_state = state.clone();
                    }
                }
                ContentState::ThreemfModelContent {
                    path: _,
                    parent: _,
                    to_highlight: true,
                } => {
                    if let ContentState::ThreemfModelContent {
                        path,
                        parent: _,
                        to_highlight: _,
                    } = &self.curr_state
                    {
                        let ids = self.check_for_duplicate_uuids(path)?;
                        match ids {
                            Some(duplicated_ids) => {
                                let controller = self.cached_xml.get_mut(path);
                                if let Some(controller) = controller {
                                    let only_element_ids = duplicated_ids
                                        .iter()
                                        .map(|(node_id, _)| *node_id)
                                        .collect::<Vec<usize>>();
                                    controller.add_highlight_ids(only_element_ids);
                                    controller.update_state(ctx)?;
                                    self.curr_state = state.clone();
                                }

                                log::info!("Found duplicated IDs on {:?}", duplicated_ids);
                            }
                            None => {
                                log::info!("No IDs found to be highlighted");
                            }
                        }
                    }
                }
                ContentState::Image(path) => {
                    if !self.cached_image_texture.contains_key(path) {
                        let processed = self.process_thumbnail_path(path, ctx);
                        match processed {
                            Some(texture_handle) => {
                                self.cached_image_texture
                                    .insert(path.to_owned(), texture_handle);
                                self.curr_state = state.clone();
                            }
                            None => {
                                log::error!("Unable to generate the image texture");
                                //never transition if the image texture is not generated
                                //to do deal with this better
                            }
                        }
                    } else {
                        self.curr_state = state.clone();
                    }
                }
                ContentState::UnknownData(_) => {
                    self.curr_state = state.clone();
                }
            }
        }
        Ok(())
    }
}

impl StandardFileViewController for ThreemfViewController {
    fn has_file_tree(&self) -> bool {
        true
    }

    fn file_tree_ui(&mut self, ui: &mut egui::Ui, _ctx: &egui::Context) {
        self.file_tree.ui(
            ui,
            5,
            "Threemf",
            &mut self.current_selected,
            &mut |(path, parent_name)| {
                let next_state = match (path.as_str(), parent_name.as_str()) {
                    ("[Content_Types].xml", _) | (_, "Relationships") => ContentState::XmlContent {
                        path: path.to_owned(),
                        parent: parent_name.to_owned(),
                    },
                    ("Root Model", _) | (_, "Sub Models") => ContentState::ThreemfModelContent {
                        path: path.to_owned(),
                        parent: parent_name.to_owned(),
                        to_highlight: false,
                    },
                    (path, "Thumbnails") => ContentState::Image(path.to_owned()),
                    (path, "Unknown Parts") => ContentState::UnknownData(path.to_owned()),
                    _ => ContentState::Empty,
                };

                self.next_state = Some(next_state);
            },
        );
    }

    fn content_ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        match &self.curr_state {
            ContentState::Empty => {
                start_page(ui);
            }
            ContentState::XmlContent { path, parent: _ } => {
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
            ContentState::ThreemfModelContent {
                path,
                parent: _,
                to_highlight: _,
            } => {
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

    fn add_menu_button(&mut self, ui: &mut egui::Ui, _ctx: &egui::Context) {
        match &self.curr_state {
            ContentState::Empty => {}
            ContentState::XmlContent { path: _, parent: _ } => {}
            ContentState::ThreemfModelContent {
                path,
                parent,
                to_highlight,
            } => {
                ui.menu_button("Process 3mf", |ui| {
                    if ui
                        .add_enabled(
                            !to_highlight,
                            egui::Button::new("Check for Duplicate UUIDs"),
                        )
                        .clicked()
                    {
                        self.next_state = Some(ContentState::ThreemfModelContent {
                            path: path.to_owned(),
                            parent: parent.to_owned(),
                            to_highlight: true,
                        });
                    }

                    if *to_highlight && ui.button("Clear Highlights").clicked() {
                        self.next_state = Some(ContentState::ThreemfModelContent {
                            path: path.to_owned(),
                            parent: parent.to_owned(),
                            to_highlight: false,
                        });
                    }
                });
            }
            ContentState::Image(_) => {}
            ContentState::UnknownData(_) => {}
        }
    }

    fn update_state(&mut self, ctx: &egui::Context) -> Result<()> {
        self.process_next_state(ctx)
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
