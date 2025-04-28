/// A tree structure widget with expandable dropdowns
#[derive(Debug)]
pub struct XmlContentTree {
    pub id: usize,
    pub name: String,
    pub attributes: Option<Vec<(String, String)>>,
    pub childs: Option<Vec<XmlContentTree>>,
}

impl XmlContentTree {
    pub fn ui(&self, ui: &mut egui::Ui, depth: usize, unique_id: &str, highlight_ids: &Vec<usize>) {
        ui.vertical(|ui| {
            egui::ScrollArea::both()
                .auto_shrink(false)
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded)
                .show(ui, |ui| {
                    self.tree_ui(ui, depth, unique_id, highlight_ids);
                });
        });
    }

    fn tree_ui(
        &self,
        ui: &mut egui::Ui,
        depth: usize,
        unique_id: &str,
        highlight_ids: &Vec<usize>,
    ) {
        if self.childs.is_some() || self.attributes.is_some() {
            egui::CollapsingHeader::new(&self.name)
                .default_open(depth < 1)
                .id_salt(unique_id)
                .show_background(highlight_ids.contains(&self.id))
                .show(ui, |ui| {
                    if let Some(attributes) = &self.attributes {
                        for attribute in attributes {
                            ui.label(format!("{} = {}", attribute.0, attribute.1));
                        }
                    }

                    self.children_ui(ui, depth, highlight_ids)
                });
        } else {
            ui.label(&self.name);
        }
    }

    fn children_ui(&self, ui: &mut egui::Ui, depth: usize, highlight_ids: &Vec<usize>) {
        if let Some(trees) = &self.childs {
            for (count, tree) in trees.iter().enumerate() {
                tree.tree_ui(
                    ui,
                    depth + 1,
                    &format!("{} - {}", tree.name, count),
                    highlight_ids,
                );
            }
        }
    }
}
