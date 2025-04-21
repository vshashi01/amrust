use anyhow::{anyhow, Result};
use roxmltree::{Document, Node};

use crate::widgets::xml_content_tree::XmlContentTree;

use super::StandardFileViewController;

pub struct XmlContentViewController {
    tree: XmlContentTree,
}

impl XmlContentViewController {
    pub fn from_xml(xml_string: &str, collections_to_skip: &Vec<&str>) -> Result<Self> {
        let doc_tree = Document::parse(xml_string);
        let processed = process_doc_tree(doc_tree, collections_to_skip);
        match processed {
            Some(tree) => Ok(XmlContentViewController { tree }),
            None => Err(anyhow!("No tree was generated")),
        }
    }
}

impl StandardFileViewController for XmlContentViewController {
    fn has_file_tree(&self) -> bool {
        false
    }

    fn content_ui(&self, ui: &mut egui::Ui) {
        self.tree.ui(ui, 5, "XmlContentTree");
    }

    fn file_tree_ui(&mut self, ui: &mut egui::Ui) {
        ui.label("Nothing to show");
    }
}

fn process_doc_tree(
    doc_tree: std::result::Result<Document<'_>, roxmltree::Error>,
    collections_to_skip: &Vec<&str>,
) -> Option<XmlContentTree> {
    match doc_tree {
        Ok(doc_tree) => process_node(doc_tree.root_element(), 8, collections_to_skip),
        Err(_) => None,
    }
}

// fn process_doc(doc: roxmltree::Document<'_>, max_depth: usize) -> Option<XmlContentTree> {
//     process_node(doc.root_element(), max_depth)
// }

fn process_node(
    node: roxmltree::Node<'_, '_>,
    max_depth: usize,
    collections_to_skip: &Vec<&str>,
) -> Option<XmlContentTree> {
    if max_depth < 1 {
        return None;
    }

    let mut name = String::new();
    let mut attributes: Vec<(String, String)> = vec![];
    let mut childs: Vec<XmlContentTree> = vec![];

    if node.is_element() {
        let prefix = get_prefix(&node, node.tag_name().namespace().unwrap_or(""));

        log::error!(
            "Namespace is {:?} for node: {}",
            prefix,
            node.tag_name().name()
        );
        name.push_str(&format!("{}{}", prefix, node.tag_name().name()));

        for attribute in node.attributes() {
            //make attributes the same line as the element name.
            let prefix = get_prefix(&node, attribute.namespace().unwrap_or(""));
            let attr_string = format!(
                " < {}{} = {} > ",
                prefix,
                attribute.name(),
                attribute.value()
            );
            name.push_str(&attr_string);
        }

        if node.has_children() {
            if !collections_to_skip.contains(&node.tag_name().name()) {
                for child in node.children() {
                    let processed = process_node(child, max_depth - 1, collections_to_skip);
                    if let Some(child_tree) = processed {
                        childs.push(child_tree);
                    }
                }
            } else {
                let count = node.children().count();
                childs.push(XmlContentTree {
                    name: node.tag_name().name().to_owned(),
                    attributes: Some(vec![("Child Count".to_string(), count.to_string())]),
                    childs: None,
                });
            }
        }

        if let Some(content) = node.text() {
            attributes.push((content.to_owned(), "".to_owned()));
        }
    }

    let attributes = if attributes.is_empty() {
        None
    } else {
        Some(attributes)
    };

    let childs = if childs.is_empty() {
        None
    } else {
        Some(childs)
    };

    if attributes.is_some() || childs.is_some() || !name.is_empty() {
        return Some(XmlContentTree {
            name,
            attributes,
            childs,
        });
    }

    None
}

fn get_prefix(node: &Node<'_, '_>, namespace: &str) -> String {
    if let Some(prefix) = node.lookup_prefix(namespace) {
        format!("{}:", prefix)
    } else {
        "".to_owned()
    }
}
