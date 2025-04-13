use anyhow::{anyhow, Result};
use roxmltree::Document;

use crate::widgets::xml_content_tree::XmlContentTree;

use super::StandardFileViewModel;

pub struct XmlContentViewController {
    tree: XmlContentTree,
}

impl XmlContentViewController {
    pub fn from_xml(xml_string: &str) -> Result<Self> {
        let doc_tree = Document::parse(xml_string);
        let processed = process_doc_tree(doc_tree);
        match processed {
            Some(tree) => Ok(XmlContentViewController { tree }),
            None => Err(anyhow!("No tree was generated")),
        }
    }
}

impl StandardFileViewModel for XmlContentViewController {
    const IMPLEMENTS_FILE_TREE: bool = false;

    fn content_ui(&self, ui: &mut egui::Ui) {
        self.tree.ui(ui, 5, "XmlContentTree");
    }

    fn file_tree_ui(&mut self, ui: &mut egui::Ui) {
        ui.label("Nothing to show");
    }
}

fn process_doc_tree(
    doc_tree: std::result::Result<Document<'_>, roxmltree::Error>,
) -> Option<XmlContentTree> {
    match doc_tree {
        Ok(doc_tree) => {
            let element = doc_tree.root_element();
            self::process_node(element, 8)
        }
        Err(_) => None,
    }
}

fn process_node(node: roxmltree::Node<'_, '_>, max_depth: usize) -> Option<XmlContentTree> {
    if max_depth < 1 {
        return None;
    }

    let mut name = String::new();
    let mut attributes: Vec<(String, String)> = vec![];
    let mut childs: Vec<XmlContentTree> = vec![];

    if node.is_element() {
        name.push_str(node.tag_name().name());

        for attribute in node.attributes() {
            //make attributes the same line as the element name.
            let attr_string = format!(" < {}: {} > ", attribute.name(), attribute.value());
            name.push_str(&attr_string);
        }

        for child in node.children() {
            let processed = process_node(child, max_depth - 1);
            if let Some(child_tree) = processed {
                childs.push(child_tree);
            }
        }

        if let Some(content) = node.text() {
            // name.push_str(" - ");
            // name.push_str(content);
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
