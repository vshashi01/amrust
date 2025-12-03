use std::collections::HashMap;

use egui::Ui;

use crate::amrust_db::{Db, PartInstance, PartInstanceId, PartRep};

pub struct ObjectTree {
    pub childs: Vec<TreeItem>,
}

#[derive(Debug, Clone)]
pub enum TreeItem {
    Leaf {
        id: usize,
        name: String,
    },
    Node {
        id: usize,
        name: String,
        childs: Vec<TreeItem>,
    },
}

impl TreeItem {
    fn draw_ui(&self, ui: &mut egui::Ui) {
        match self {
            TreeItem::Leaf { id, name } => {
                let _ = ui.selectable_label(true, name);
            }
            TreeItem::Node { id, name, childs } => {
                egui::CollapsingHeader::new(name)
                    .show_background(true)
                    .show(ui, |ui| {
                        for child in childs {
                            child.draw_ui(ui);
                        }
                    });
            }
        }
    }
}

impl ObjectTree {
    pub fn new(db: &Db) -> Option<Self> {
        if let Ok(scene) = db.get_scene() {
            let mut items: Vec<TreeItem> = vec![];

            let mut instance_id_to_tree_item_map: HashMap<PartInstanceId, TreeItem> =
                HashMap::new();

            let mut unprocessed_instances = vec![];

            // process all the items one round first
            for (id, _, rep) in db.get_part_instances() {
                match rep {
                    PartRep::Mesh(_) => {
                        instance_id_to_tree_item_map.insert(
                            id.clone(),
                            TreeItem::Leaf {
                                id: 2,
                                name: format!("Mesh: {:?}", id),
                            },
                        );
                    }
                    PartRep::ComposedPart(part_instance_ids) => {
                        let mut tree_items = vec![];
                        for id in part_instance_ids {
                            if let Some(item) = instance_id_to_tree_item_map.get(id) {
                                tree_items.push(item.clone());
                            }
                        }

                        if tree_items.len() != part_instance_ids.len() {
                            unprocessed_instances.push(id);
                            continue;
                        } else {
                            let node = TreeItem::Node {
                                id: 3,
                                name: format!("Composed Part: {:?}", id),
                                childs: tree_items,
                            };
                            instance_id_to_tree_item_map.insert(id, node);
                        }
                    }
                }
            }

            for i in &scene.instances {
                if let Some(item) = instance_id_to_tree_item_map.get(i) {
                    items.push(item.clone());
                }
            }

            if !items.is_empty() {
                return Some(Self { childs: items });
            }
        }

        None
    }

    pub fn ui(&self, ui: &mut egui::Ui) {
        ui.vertical(|ui| {
            egui::ScrollArea::both()
                .auto_shrink(false)
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded)
                .show(ui, |ui| {
                    self.tree_ui(ui);
                });
        });
    }

    fn tree_ui(&self, ui: &mut egui::Ui) {
        for item in &self.childs {
            item.draw_ui(ui);
        }
    }
}
