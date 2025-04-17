pub fn start_page(ui: &mut egui::Ui) {
    ui.centered_and_justified(|ui| {
        ui.image(egui::include_image!("../../assets/ferris.png"));
    });
}
