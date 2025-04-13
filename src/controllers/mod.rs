pub mod threemf_view_controller;
pub mod xml_content_view_controller;

pub trait StandardFileViewModel {
    const IMPLEMENTS_FILE_TREE: bool;

    fn file_tree_ui(&mut self, _ui: &mut egui::Ui) {}

    fn content_ui(&self, ui: &mut egui::Ui);
}
