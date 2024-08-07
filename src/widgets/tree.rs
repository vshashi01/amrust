use anyhow::{anyhow, Result};
use xml_dom::level2::{CharacterData, Node, NodeType, RefNode};
use xml_dom::parser::read_xml;

#[derive(Debug)]
pub struct Tree {
    pub name: String,
    pub content: Option<String>,
    pub attributes: Option<Vec<(String, String)>>,
    pub childs: Option<Vec<Tree>>,
}

impl Tree {
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

    pub fn ui(&self, ui: &mut egui::Ui, depth: usize, unique_id: &str) {
        self.ui_impl(ui, depth, unique_id);
    }
}

impl Tree {
    fn ui_impl(&self, ui: &mut egui::Ui, depth: usize, unique_id: &str) {
        let name = if let Some(content) = &self.content {
            format!("{} - {}", self.name, content)
        } else {
            self.name.clone()
        };
        egui::CollapsingHeader::new(name)
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

mod tests {
    use super::*;
    use std::{
        env::{self},
        fs::{self},
        path::PathBuf,
    };

    fn get_file_as_string_from_test_resource(file_name: &str) -> String {
        let root_dir = &env::var("CARGO_MANIFEST_DIR").expect("$CARGO_MANIFEST_DIR");
        let mut test_file_path = PathBuf::from(root_dir);
        test_file_path.push("test_resources\\");
        test_file_path.push(file_name);
        // println!("{:?}", test_file_path);

        fs::read_to_string(test_file_path).unwrap()
    }

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
}
