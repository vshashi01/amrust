use egui::DroppedFile;

use crate::widgets::{dropped_files::DroppedFilesWidget, start_page};

use super::{
    threemf_view_controller::ThreemfViewController,
    xml_content_view_controller::XmlContentViewController, StandardFileViewController,
};

use std::{ffi::OsStr, fs, path::PathBuf};

pub struct DefaultViewController {
    current_view_model: Option<Box<dyn StandardFileViewController>>,
    pub file_name: Option<String>,
    dropped_files: DroppedFilesWidget,
}

impl DefaultViewController {
    pub fn new() -> Self {
        Self {
            file_name: None,
            current_view_model: None,
            dropped_files: DroppedFilesWidget::new(),
        }
    }

    fn process_dropped_files(&mut self, files: &Vec<DroppedFile>) {
        for file in files {
            let path = match &file.path {
                Some(path) => path,
                None => continue,
            };
            match path.extension().and_then(OsStr::to_str) {
                Some("3mf") => {
                    log::info!("This is the 3mf path: {:?}", path);
                    let threemf_model = ThreemfViewController::from_file(path);
                    if let Ok(model) = threemf_model {
                        self.current_view_model = Some(Box::new(model))
                    }
                }
                Some("xml") => {
                    log::info!("This is the xml path: {:?}", path);
                    let xml_string = fs::read_to_string(path).unwrap_or_default();
                    let xml_model = XmlContentViewController::from_xml(&xml_string, &vec![]);
                    if let Ok(model) = xml_model {
                        self.current_view_model = Some(Box::new(model))
                    }
                }
                _ => {}
            };

            if self.current_view_model.is_some() {
                self.file_name = Some(path.to_string_lossy().to_string());
                break;
            }
        }
    }

    pub fn has_data(&self) -> bool {
        self.current_view_model.is_some()
    }

    pub fn clear_state(&mut self) {
        self.current_view_model = None;
        self.file_name = None;
    }

    pub fn content_ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        match &mut self.current_view_model {
            Some(model) => {
                ui.vertical(|ui| {
                    egui::ScrollArea::both()
                        .auto_shrink(false)
                        .scroll_bar_visibility(
                            egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded,
                        )
                        .show(ui, |ui| {
                            model.content_ui(ui, ctx);
                        });
                });
                // model.content_ui(ui, ctx);
            }
            None => start_page::start_page(ui),
        }
    }

    pub fn run_dropped_files(&mut self, ctx: &egui::Context) {
        self.dropped_files.run(ctx, &Self::can_process_file);
        let files = self.dropped_files.pull_dropped_file();

        self.process_dropped_files(&files);
    }

    pub fn file_tree_ui(&mut self, ctx: &egui::Context) {
        if let Some(view_model) = &mut self.current_view_model {
            if view_model.has_file_tree() {
                egui::SidePanel::left("left_panel")
                    .resizable(true)
                    .default_width(100.0)
                    .show(ctx, |ui| {
                        egui::ScrollArea::both().auto_shrink(false).show(ui, |ui| {
                            view_model.file_tree_ui(ui, ctx);
                        });
                    });
            }
        }
    }

    fn can_process_file(path: PathBuf) -> bool {
        matches!(
            path.extension().and_then(OsStr::to_str),
            Some("3mf") | Some("xml")
        )
    }
}
