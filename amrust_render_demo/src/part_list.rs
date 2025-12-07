use crate::tree_item_viewer::{TreeItem, TreeItemViewer};

pub struct PartList {
    pub object_tree: TreeItemViewer,

    current_view_mode: ViewMode,
}

#[derive(Debug, PartialEq, Eq)]
enum ViewMode {
    PartInstancesOnly,
    ByUniqueParts,
}

impl PartList {
    pub fn new(tree_items: Vec<TreeItem>) -> Self {
        Self {
            object_tree: TreeItemViewer {
                name: "Part List".to_owned(),
                childs: tree_items,
                selected_items: vec![],
                skip_inert_node: false,
            },
            current_view_mode: ViewMode::ByUniqueParts,
        }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) {
        ui.vertical(|ui| {
            ui.horizontal_top(|ui| {
                ui.radio_value(
                    &mut self.current_view_mode,
                    ViewMode::ByUniqueParts,
                    "Unique Parts",
                );
                ui.radio_value(
                    &mut self.current_view_mode,
                    ViewMode::PartInstancesOnly,
                    "Physical Parts",
                );
            });

            ui.separator();

            match self.current_view_mode {
                ViewMode::PartInstancesOnly => self.object_tree.skip_inert_node = true,
                ViewMode::ByUniqueParts => self.object_tree.skip_inert_node = false,
            };

            self.object_tree.core_ui(ui);
        });
    }
}
