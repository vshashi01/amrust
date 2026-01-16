use std::fmt::Debug;

#[derive(Debug, Clone)]
pub enum TreeItem<T: PartialEq + Clone + Debug> {
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
    T: std::cmp::PartialEq + Clone + Debug,
{
    // pub fn id(&self) -> &T {
    //     match self {
    //         TreeItem::Leaf { id, .. } => id,
    //         TreeItem::Node { id, .. } => id,
    //         TreeItem::InertNode { id, .. } => id,
    //     }
    // }

    fn draw_ui(
        &self,
        ui: &mut egui::Ui,
        selected_items: &mut Vec<T>,
        disabled_items: &[T],
        skip_inert_node: bool,
    ) {
        match self {
            TreeItem::Leaf {
                id,
                name,
                selectable,
            } => {
                if *selectable && !disabled_items.contains(id) {
                    if ui
                        .selectable_label(selected_items.contains(id), name)
                        .clicked()
                    {
                        selected_items.clear();
                        selected_items.push(id.clone());
                    };
                } else {
                    ui.horizontal(|ui| {
                        ui.label(name);
                        ui.add(egui::ProgressBar::new(0.0).animate(true));
                    });
                }
            }
            TreeItem::Node {
                id,
                name,
                childs,
                selectable,
            } => {
                if egui::CollapsingHeader::new(name)
                    .id_salt(format!("Node_{:?}", id))
                    .show_background(*selectable && selected_items.contains(id))
                    .show(ui, |ui| {
                        for child in childs {
                            child.draw_ui(ui, selected_items, disabled_items, skip_inert_node);
                        }
                    })
                    .header_response
                    .clicked()
                    && *selectable
                    && !disabled_items.contains(id)
                {
                    selected_items.clear();
                    selected_items.push(id.clone());
                };
            }
            TreeItem::InertNode { name, childs, id } => {
                if skip_inert_node {
                    for child in childs {
                        child.draw_ui(ui, selected_items, disabled_items, skip_inert_node);
                    }
                } else {
                    egui::CollapsingHeader::new(name)
                        .id_salt(format!("InertNode_{:?}", id))
                        .show(ui, |ui| {
                            for child in childs {
                                child.draw_ui(ui, selected_items, disabled_items, skip_inert_node);
                            }
                        });
                }
            }
        }
    }
}

pub struct TreeItemViewer<T: PartialEq + Clone + Debug> {
    pub childs: Vec<TreeItem<T>>,

    pub skip_inert_node: bool,
    pub clear_selections_on_empty_area_click: bool,
}

impl<T> TreeItemViewer<T>
where
    T: PartialEq + Clone + Debug,
{
    pub fn new(
        tree_items: Vec<TreeItem<T>>,
        skip_inert_node: bool,
        clear_selections_on_empty_area_click: bool,
    ) -> Self {
        TreeItemViewer {
            childs: tree_items,
            skip_inert_node,
            clear_selections_on_empty_area_click,
        }
    }

    pub fn core_ui(
        &mut self,
        ui: &mut egui::Ui,
        selected_items: &mut Vec<T>,
        disabled_items: &[T],
    ) {
        ui.vertical(|ui| {
            egui::ScrollArea::both()
                .auto_shrink(false)
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded)
                .show(ui, |ui| {
                    let bg_response = ui.interact(
                        ui.available_rect_before_wrap(),
                        ui.id().with("tree_item_viewer"),
                        egui::Sense::click(),
                    );
                    if bg_response.clicked() && self.clear_selections_on_empty_area_click {
                        selected_items.clear();
                    }

                    self.tree_ui(ui, selected_items, disabled_items);
                });
        });
    }

    fn tree_ui(&mut self, ui: &mut egui::Ui, selected_items: &mut Vec<T>, disabled_items: &[T]) {
        for item in &self.childs {
            item.draw_ui(ui, selected_items, disabled_items, self.skip_inert_node);
        }
    }
}
