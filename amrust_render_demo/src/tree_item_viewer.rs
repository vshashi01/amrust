#[derive(Debug, Clone)]
pub enum TreeItem<T: PartialEq + Copy> {
    Leaf {
        id: T,
        name: String,
        selectable: bool,
    },
    Node {
        id: T,
        name: String,
        childs: Vec<TreeItem<T>>,
        selectable: bool,
    },
    InertNode {
        id: T,
        name: String,
        childs: Vec<TreeItem<T>>,
    },
}

impl<T> TreeItem<T>
where
    T: std::cmp::PartialEq + Copy,
{
    pub fn id(&self) -> T {
        match self {
            TreeItem::Leaf { id, .. } => *id,
            TreeItem::Node { id, .. } => *id,
            TreeItem::InertNode { id, .. } => *id,
        }
    }

    fn draw_ui(&self, ui: &mut egui::Ui, selected_items: &mut Vec<T>, skip_inert_node: bool) {
        match self {
            TreeItem::Leaf {
                id,
                name,
                selectable,
            } => {
                if *selectable {
                    if ui
                        .selectable_label(selected_items.contains(id), name)
                        .clicked()
                    {
                        selected_items.clear();
                        selected_items.push(*id);
                    };
                } else {
                    ui.label(name);
                }
            }
            TreeItem::Node {
                id,
                name,
                childs,
                selectable,
            } => {
                if egui::CollapsingHeader::new(name)
                    .show_background(*selectable && selected_items.contains(id))
                    .show(ui, |ui| {
                        for child in childs {
                            child.draw_ui(ui, selected_items, skip_inert_node);
                        }
                    })
                    .header_response
                    .clicked()
                    && *selectable
                {
                    selected_items.clear();
                    selected_items.push(*id);
                };
            }
            TreeItem::InertNode { name, childs, .. } => {
                if skip_inert_node {
                    for child in childs {
                        child.draw_ui(ui, selected_items, skip_inert_node);
                    }
                } else {
                    egui::CollapsingHeader::new(name).show(ui, |ui| {
                        for child in childs {
                            child.draw_ui(ui, selected_items, skip_inert_node);
                        }
                    });
                }
            }
        }
    }
}

pub struct TreeItemViewer<T: PartialEq + Copy> {
    pub childs: Vec<TreeItem<T>>,

    pub skip_inert_node: bool,
}

impl<T> TreeItemViewer<T>
where
    T: PartialEq + Copy,
{
    pub fn new(tree_items: Vec<TreeItem<T>>, skip_inert_node: bool) -> Self {
        TreeItemViewer {
            childs: tree_items,
            skip_inert_node,
        }
    }

    pub fn core_ui(&mut self, ui: &mut egui::Ui, selected_items: &mut Vec<T>) {
        ui.vertical(|ui| {
            egui::ScrollArea::both()
                .auto_shrink(false)
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded)
                .show(ui, |ui| {
                    self.tree_ui(ui, selected_items);
                });
        });
    }

    fn tree_ui(&mut self, ui: &mut egui::Ui, selected_items: &mut Vec<T>) {
        for item in &self.childs {
            item.draw_ui(ui, selected_items, self.skip_inert_node);
        }
    }
}
