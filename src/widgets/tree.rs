use anyhow::{anyhow, Result};
use instant_xml::to_string;
use threemf::io::threemf_package::ThreemfPackage;
use xml_dom::level2::{CharacterData, Node, NodeType, RefNode};
use xml_dom::parser::read_xml;

/// A tree structure widget with expandable dropdowns
#[derive(Debug)]
pub struct Tree {
    pub name: String,
    pub attributes: Option<Vec<(String, String)>>,
    pub childs: Option<Vec<Tree>>,
}

impl Tree {
    /// Creates a new Tree struct from a XML string structure.
    /// Returns error if the string is an malformed XML string
    pub fn new_trees_from_xml_string(xml_string: &String) -> Result<Vec<Self>> {
        let dom = read_xml(xml_string)?;
        // println!("{:?}", dom);
        let result = process_dom(dom);
        if let Some(tree) = result.0 {
            Ok(tree)
        } else {
            Err(anyhow!("No tree was generated"))
        }
    }

    pub fn new_trees_from_threemf(package: &ThreemfPackage) -> Result<Self> {
        let result = process_threemf_package(package);
        if let Some(trees) = result {
            Ok(trees)
        } else {
            Err(anyhow!("No tree was generated for threemf"))
        }
    }

    /// Draws the ui
    pub fn ui(&self, ui: &mut egui::Ui, depth: usize, unique_id: &str) {
        egui::CollapsingHeader::new(&self.name)
            .default_open(depth < 1)
            .id_salt(unique_id)
            .show(ui, |ui| {
                if let Some(attributes) = &self.attributes {
                    for attribute in attributes {
                        ui.label(format!("{} - {}", attribute.0, attribute.1));
                    }
                }

                self.children_ui_with_unselectable_label(ui, depth)
            });
    }

    pub fn ui_with_selectable_contents(
        &self,
        ui: &mut egui::Ui,
        depth: usize,
        unique_id: &str,
        current_selected_value: &mut String,
        on_clicked: &mut impl FnMut(&String, &String),
    ) {
        egui::CollapsingHeader::new(&self.name)
            .default_open(depth < 1)
            .id_salt(unique_id)
            .show(ui, |ui| {
                if let Some(attributes) = &self.attributes {
                    attributes.iter().for_each(|(path, content)| {
                        if ui
                            .selectable_value(current_selected_value, path.clone(), path)
                            .clicked()
                        {
                            on_clicked(path, content);
                        }
                    });
                }

                self.children_ui_with_selectable_content(
                    ui,
                    depth,
                    current_selected_value,
                    on_clicked,
                )
            });
    }

    fn children_ui_with_unselectable_label(&self, ui: &mut egui::Ui, depth: usize) {
        if let Some(trees) = &self.childs {
            for (count, tree) in trees.iter().enumerate() {
                tree.ui(ui, depth + 1, &format!("{} - {}", tree.name, count));
            }
        }
    }

    fn children_ui_with_selectable_content(
        &self,
        ui: &mut egui::Ui,
        depth: usize,
        current_selected_value: &mut String,
        on_clicked: &mut impl FnMut(&String, &String),
    ) {
        if let Some(trees) = &self.childs {
            for (count, tree) in trees.iter().enumerate() {
                tree.ui_with_selectable_contents(
                    ui,
                    depth + 1,
                    &format!("{} - {}", tree.name, count),
                    current_selected_value,
                    on_clicked,
                );
            }
        }
    }
}

fn process_dom(ref_node: RefNode) -> (Option<Vec<Tree>>, Option<String>) {
    let mut sub_trees = Vec::new();
    let mut sub_content = String::new();
    for node in ref_node.child_nodes() {
        let mut name = node.local_name();
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

            if let Some(content) = entry {
                name.push_str(" - ");
                name.push_str(&content);
            }

            sub_trees.push(Tree {
                name,
                attributes: Some(attributes),
                childs,
            });
        } else {
            // if not an Element then its highly likely it
            // contains some data that belongs to the Element item itself
            if let Some(data) = node.data() {
                sub_content.push_str(&data);
            }
        }
    }

    let trees = if !sub_trees.is_empty() {
        Some(sub_trees)
    } else {
        None
    };

    let content = if !sub_content.is_empty() {
        Some(sub_content)
    } else {
        None
    };

    (trees, content)
}

fn process_threemf_package(package: &ThreemfPackage) -> Option<Tree> {
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
    sub_trees.push(Tree {
        name: "Relationships".to_owned(),
        // content: None,
        attributes: Some(relationships),
        childs: None,
    });

    let mut sub_models = Vec::new();
    for (name, model) in &package.sub_models {
        let xml_string = to_string(model).unwrap();
        sub_models.push((name.clone(), xml_string));
    }
    sub_trees.push(Tree {
        name: "Sub Models".to_owned(),
        attributes: Some(sub_models),
        childs: None,
    });

    let mut thumbnails = Vec::new();
    for name in package.thumbnails.keys() {
        thumbnails.push((name.clone(), "".to_owned()));
    }
    sub_trees.push(Tree {
        name: "Thumbnails".to_owned(),
        attributes: Some(thumbnails),
        childs: None,
    });

    let mut unknown_datas = Vec::new();
    for name in package.unknown_parts.keys() {
        unknown_datas.push((name.clone(), "".to_owned()));
    }
    sub_trees.push(Tree {
        name: "Unknown Parts".to_owned(),
        attributes: Some(unknown_datas),
        childs: None,
    });

    Some(Tree {
        name: "Threemf".to_owned(),
        attributes: Some(vec![content_types, root_model]),
        childs: Some(sub_trees),
    })
}

pub mod tests {

    use super::Tree;
    use std::{
        env::{self},
        fs::{self},
        path::PathBuf,
    };

    #[test]
    fn test_a_valid_tree_generated_from_valid_xml() {
        let file = get_file_as_string_from_test_resource("test-xml.xml");
        let result = Tree::new_trees_from_xml_string(&file);

        assert!(
            result.is_ok(),
            "A valid tree is not generated from a valid xml"
        );
    }

    #[test]
    fn test_error_returned_when_invalid_xml() {
        let file = get_file_as_string_from_test_resource("fake-xml.xml");
        let result = Tree::new_trees_from_xml_string(&file);

        assert!(
            result.is_err(),
            "Operation did not return en error when given invalid xml"
        );
    }

    fn get_file_as_string_from_test_resource(file_name: &str) -> String {
        let root_dir = &env::var("CARGO_MANIFEST_DIR").expect("$CARGO_MANIFEST_DIR");
        let mut test_file_path = PathBuf::from(root_dir);
        test_file_path.push("test_resources\\");
        test_file_path.push(file_name);
        // println!("{:?}", test_file_path);

        fs::read_to_string(test_file_path).unwrap()
    }
}
