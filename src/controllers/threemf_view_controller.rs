use anyhow::{anyhow, Result};
use instant_xml::to_string;
use threemf::io::threemf_package::ThreemfPackage;

use crate::widgets::file_tree::FileTree;

use super::{xml_content_view_controller::XmlContentViewController, StandardFileViewController};

use std::{collections::HashMap, fs::File, path::PathBuf};

pub struct ThreemfViewController {
    package: ThreemfPackage,
    cached_xml: HashMap<String, XmlContentViewController>,
    current_selected: String,
    file_tree: FileTree,
}

impl ThreemfViewController {
    pub fn from_file(path: &PathBuf) -> Result<Self> {
        let package = get_threemf_package(path)?;
        let result = process_threemf_package(&package);
        if let Some(file_tree) = result {
            Ok(ThreemfViewController {
                package,
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
            &mut |path, content| {
                if !self.cached_xml.contains_key(path) {
                    let trees = XmlContentViewController::from_xml(content);
                    match trees {
                        Ok(trees) => {
                            self.cached_xml.insert(path.clone(), trees);
                        }
                        Err(err) => {
                            println!("{:?}", err);
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

pub fn get_threemf_package(path: &PathBuf) -> Result<ThreemfPackage> {
    let file = File::open(path).unwrap();
    let package = ThreemfPackage::from_reader(file, true);

    Ok(package.unwrap())
}

fn process_threemf_package(package: &ThreemfPackage) -> Option<FileTree> {
    let mut sub_trees = Vec::new();

    let content_xml_string = to_string(&package.content_types).unwrap();
    let content_types = ("[Content_Types].xml".to_owned(), content_xml_string);

    let root_xml_string = to_string(&package.root).unwrap();
    let root_model = ("Root Model".to_owned(), root_xml_string);

    let mut relationships = Vec::new();
    for (name, relationship) in &package.relationships {
        let xml_string = to_string(relationship).unwrap();
        relationships.push((name.clone(), xml_string));
    }
    sub_trees.push(FileTree {
        name: "Relationships".to_owned(),
        files: relationships,
        child_folders: vec![],
    });

    let mut sub_models = Vec::new();
    for (name, model) in &package.sub_models {
        let xml_string = to_string(model).unwrap();
        sub_models.push((name.clone(), xml_string));
    }
    sub_trees.push(FileTree {
        name: "Sub Models".to_owned(),
        files: sub_models,
        child_folders: vec![],
    });

    let mut thumbnails = Vec::new();
    for name in package.thumbnails.keys() {
        thumbnails.push((name.clone(), "".to_owned()));
    }
    sub_trees.push(FileTree {
        name: "Thumbnails".to_owned(),
        files: thumbnails,
        child_folders: vec![],
    });

    let mut unknown_datas = Vec::new();
    for name in package.unknown_parts.keys() {
        unknown_datas.push((name.clone(), "".to_owned()));
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
