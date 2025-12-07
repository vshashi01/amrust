use crate::part_list::PartList;

pub struct Toolsheets {
    pub part_list: PartList,
}

impl egui_dock::TabViewer for Toolsheets {
    type Tab = String;

    fn title(&mut self, tab: &mut Self::Tab) -> egui::WidgetText {
        (&*tab).into()
    }

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Self::Tab) {
        match tab.as_str() {
            "Part List" => self.part_list.ui(ui),
            _ => {
                ui.label(format!("Unimplemented content for: {tab:?}"));
            }
        }
    }
}
