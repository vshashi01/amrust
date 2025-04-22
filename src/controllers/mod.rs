pub mod default_view_controller;
pub mod threemf_view_controller;
pub mod xml_content_view_controller;

pub trait StandardFileViewController {
    fn has_file_tree(&self) -> bool;
    fn file_tree_ui(&mut self, _ui: &mut egui::Ui, _ctx: &egui::Context) {}

    fn content_ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context);
}
