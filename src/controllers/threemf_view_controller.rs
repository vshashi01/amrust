use anyhow::{anyhow, Result};
use threemf::io::threemf_unpacked::ThreemfUnpacked;

use crate::widgets::file_tree::FileTree;

use super::{xml_content_view_controller::XmlContentViewController, StandardFileViewController};

use std::{collections::HashMap, fs::File, path::PathBuf};

pub struct ThreemfViewController {
    unpacked: ThreemfUnpacked,
    cached_xml: HashMap<String, XmlContentViewController>,
    current_selected: String,
    file_tree: FileTree,
}

impl ThreemfViewController {
    pub fn from_file(path: &PathBuf) -> Result<Self> {
        let unpacked = get_threemf_unpacked(path)?;
        let result = process_threemf_unpacked(&unpacked);
        if let Some(file_tree) = result {
            Ok(ThreemfViewController {
                unpacked,
                cached_xml: HashMap::new(),
                current_selected: String::new(),
                file_tree,
            })
        } else {
            Err(anyhow!("No tree was generated for threemf"))
        }
    }
}

impl StandardFileViewController for ThreemfViewController {
    fn has_file_tree(&self) -> bool {
        true
    }

    fn file_tree_ui(&mut self, ui: &mut egui::Ui) {
        self.file_tree.ui(
            ui,
            5,
            "Threemf",
            &mut self.current_selected,
            &mut |(path, parent_name)| {
                if !self.cached_xml.contains_key(path) {
                    let xml_string = match (path.as_str(), parent_name.as_str()) {
                        ("[Content_Types].xml", _) => Some(&self.unpacked.content_types),
                        ("Root Model", _) => Some(&self.unpacked.root),
                        (path, "Relationships") => self.unpacked.relationships.get(path),
                        (path, "Sub Models") => self.unpacked.relationships.get(path),
                        _ => None,
                    };
                    if let Some(xml_string) = xml_string {
                        let trees = XmlContentViewController::from_xml(xml_string);
                        match trees {
                            Ok(trees) => {
                                self.cached_xml.insert(path.clone(), trees);
                            }
                            Err(err) => {
                                println!("{:?}", err);
                            }
                        }
                    }
                }
            },
        );
    }

    fn content_ui(&self, ui: &mut egui::Ui) {
        if self.cached_xml.contains_key(&self.current_selected) {
            let xml_content = self.cached_xml.get(&self.current_selected);
            match xml_content {
                Some(content) => content.content_ui(ui),
                None => log::error!("No cached xml"),
            }
        }
    }
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
