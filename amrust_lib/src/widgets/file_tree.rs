// A tree structure widget with expandable dropdowns
#[derive(Debug)]
pub struct FileTree {
    pub name: String,
    pub files: Vec<String>,
    pub child_folders: Vec<FileTree>,
}

impl FileTree {
    pub fn ui(
        &self,
        ui: &mut egui::Ui,
        max_depth: usize,
        unique_id: &str,
        current_selected_value: &mut String,
        on_clicked: &mut impl FnMut((&String, &String)),
    ) {
        //todo: See if can use collapsingstate::show_header approach instead to customize the behavior
        egui::CollapsingHeader::new(&self.name)
            .id_salt(unique_id)
            .show(ui, |ui| {
                self.files.iter().for_each(|path| {
                    if ui
                        .selectable_value(current_selected_value, path.clone(), path)
                        .clicked()
                    {
                        on_clicked((path, &self.name));
                    }
                });
                if !self.child_folders.is_empty() {
                    self.children_ui(ui, max_depth, current_selected_value, on_clicked)
                }
            });
    }

    fn children_ui(
        &self,
        ui: &mut egui::Ui,
        depth: usize,
        current_selected_value: &mut String,
        on_clicked: &mut impl FnMut((&String, &String)),
    ) {
        for (count, tree) in self.child_folders.iter().enumerate() {
            tree.ui(
                ui,
                depth + 1,
                &format!("{} - {}", tree.name, count),
                current_selected_value,
                on_clicked,
            );
        }
    }
}

pub mod tests {}
