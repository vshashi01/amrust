use anyhow::{Result, anyhow};
use roxmltree::{Document, Node};

use crate::widgets::xml_content_tree::XmlContentTree;

use super::StandardFileViewController;

pub struct XmlContentViewController {
    tree: XmlContentTree,
    curr_state: ContentState,
    next_state: Option<ContentState>,
    //highlight_ids: Option<Vec<usize>>,
}

#[derive(Debug, Clone)]
enum ContentState {
    Simple,
    Highlighted(Vec<usize>),
}

impl XmlContentViewController {
    pub fn from_xml(xml_string: &str, collections_to_skip: &Vec<&str>) -> Result<Self> {
        let doc_tree = Document::parse(xml_string);
        let processed = process_doc_tree(doc_tree, collections_to_skip);
        match processed {
            Some(tree) => Ok(XmlContentViewController {
                tree,
                curr_state: ContentState::Simple,
                next_state: None,
                //highlight_ids: None,
            }),
            None => Err(anyhow!("No tree was generated")),
        }
    }

    pub fn add_highlight_ids(&mut self, highlight_ids: Vec<usize>) {
        // if !highlight_ids.is_empty() {
        //     self.highlight_ids = Some(highlight_ids);
        // }
        self.next_state = Some(ContentState::Highlighted(highlight_ids));
    }

    pub fn clear_highlights(&mut self) {
        // self.highlight_ids = None;
        self.next_state = Some(ContentState::Simple);
    }
}

impl StandardFileViewController for XmlContentViewController {
    fn has_file_tree(&self) -> bool {
        false
    }

    fn content_ui(&mut self, ui: &mut egui::Ui, _ctx: &egui::Context) {
        let ids = match &self.curr_state {
            ContentState::Simple => &vec![],
            ContentState::Highlighted(items) => {
                log::warn!("Highlighted was run");
                items
            }
        };
        self.tree.ui(ui, 5, "XmlContentTree", ids);
    }

    fn file_tree_ui(&mut self, ui: &mut egui::Ui, _ctx: &egui::Context) {
        ui.label("Nothing to show");
    }

    fn add_menu_button(&mut self, ui: &mut egui::Ui, _ctx: &egui::Context) {
        match &self.curr_state {
            ContentState::Simple => {}
            ContentState::Highlighted(_) => {
                ui.menu_button("XML Viewer", |ui| {
                    if ui.button("Clear Highlights").clicked() {
                        self.next_state = Some(ContentState::Simple);
                    }
                });
            }
        }
    }
    fn update_state(&mut self, _ctx: &egui::Context) -> Result<()> {
        let next_state = self.next_state.take();
        if let Some(state) = next_state {
            self.curr_state = state;
        }

        Ok(())
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
        //if its the element below root then extract all the namespaces
        if let Some(parent_node) = node.parent()
            && parent_node.is_root()
        {
            for ns in node.namespaces() {
                let prefix = match ns.name() {
                    Some(prefix) => format!(":{}", prefix),
                    None => String::new(),
                };
                let ns_attribute = (format!("xmlns{}", prefix), ns.uri().to_owned());
                attributes.push(ns_attribute);
            }
        }

        let prefix = get_prefix(&node, node.tag_name().namespace().unwrap_or(""));
        name.push_str(&format!("{}{}", prefix, node.tag_name().name()));

        for attribute in node.attributes() {
            let prefix = get_prefix(&node, attribute.namespace().unwrap_or(""));
            attributes.push((
                format!("{}{}", prefix, attribute.name()),
                attribute.value().to_owned(),
            ));
        }

        //process the children
        if !collections_to_skip.contains(&node.tag_name().name()) {
            for child in node.children().filter(|c| c.is_element()) {
                let processed = process_node(child, max_depth - 1, collections_to_skip);
                if let Some(child_tree) = processed {
                    childs.push(child_tree);
                }
            }
        } else {
            //when we skip processing a collection just write the child count instead as additional info
            let count = node.children().count();
            attributes.push(("Child count".to_owned(), count.to_string()));
        }

        if let Some(content) = node.text()
            && !content.is_empty()
            && !content.trim().is_empty()
        {
            childs.push(XmlContentTree {
                id: node.id().get_usize(),
                name: content.trim().to_owned(),
                attributes: None,
                childs: None,
            });
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
            id: node.id().get_usize(),
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
