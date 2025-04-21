/// A tree structure widget with expandable dropdowns
#[derive(Debug)]
pub struct XmlContentTree {
    pub name: String,
    pub attributes: Option<Vec<(String, String)>>,
    pub childs: Option<Vec<XmlContentTree>>,
}

impl XmlContentTree {
    /// Draws the ui
    pub fn ui(&self, ui: &mut egui::Ui, depth: usize, unique_id: &str) {
        if self.childs.is_some() {
            egui::CollapsingHeader::new(&self.name)
                .default_open(depth < 1)
                .id_salt(unique_id)
                .show(ui, |ui| {
                    if let Some(attributes) = &self.attributes {
                        for attribute in attributes {
                            ui.label(format!("{} - {}", attribute.0, attribute.1));
                        }
                    }

                    self.children_ui(ui, depth)
                });
        } else {
            ui.label(&self.name);
        }
    }

    fn children_ui(&self, ui: &mut egui::Ui, depth: usize) {
        if let Some(trees) = &self.childs {
            for (count, tree) in trees.iter().enumerate() {
                tree.ui(ui, depth + 1, &format!("{} - {}", tree.name, count));
            }
        }
    }
}

pub mod tests {}
