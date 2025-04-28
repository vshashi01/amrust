use anyhow::Result;
use egui::DroppedFile;

use crate::widgets::{dropped_files::DroppedFilesWidget, start_page};

use super::{
    StandardFileViewController, threemf_view_controller::ThreemfViewController,
    xml_content_view_controller::XmlContentViewController,
};

use std::{ffi::OsStr, fs, path::PathBuf};

pub struct DefaultViewController {
    current_controller: Option<Box<dyn StandardFileViewController>>,
    pub file_name: Option<String>,
    dropped_files: DroppedFilesWidget,
}

impl Default for DefaultViewController {
    fn default() -> Self {
        Self {
            current_controller: Default::default(),
            file_name: Default::default(),
            dropped_files: DroppedFilesWidget::new(),
        }
    }
}

impl DefaultViewController {
    // pub fn new() -> Self {
    //     Self {
    //         file_name: None,
    //         current_controller: None,
    //         dropped_files: DroppedFilesWidget::new(),
    //     }
    // }

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
                        self.current_controller = Some(Box::new(model))
                    }
                }
                Some("xml") => {
                    log::info!("This is the xml path: {:?}", path);
                    let xml_string = fs::read_to_string(path).unwrap_or_default();
                    let xml_model = XmlContentViewController::from_xml(&xml_string, &vec![]);
                    if let Ok(model) = xml_model {
                        self.current_controller = Some(Box::new(model))
                    }
                }
                _ => {}
            };

            if self.current_controller.is_some() {
                self.file_name = Some(path.to_string_lossy().to_string());
                break;
            }
        }
    }

    pub fn has_data(&self) -> bool {
        self.current_controller.is_some()
    }

    pub fn clear_state(&mut self) {
        self.current_controller = None;
        self.file_name = None;
    }

    pub fn content_ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        match &mut self.current_controller {
            Some(controller) => {
                controller.content_ui(ui, ctx);
            }
            None => start_page::start_page(ui),
        }
    }

    pub fn update_state(&mut self, ctx: &egui::Context) -> Result<()> {
        if let Some(controller) = &mut self.current_controller {
            return controller.update_state(ctx);
        }

        Ok(())
    }

    pub fn run_dropped_files(&mut self, ctx: &egui::Context) {
        self.dropped_files.run(ctx, &Self::can_process_file);
        let files = self.dropped_files.pull_dropped_file();

        self.process_dropped_files(&files);
    }

    pub fn has_side_panel(&self) -> bool {
        if let Some(controller) = &self.current_controller {
            controller.has_file_tree()
        } else {
            false
        }
    }

    pub fn file_tree_ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        if let Some(controller) = &mut self.current_controller {
            if controller.has_file_tree() {
                controller.file_tree_ui(ui, ctx);
            }
        }
    }

    fn can_process_file(path: PathBuf) -> bool {
        matches!(
            path.extension().and_then(OsStr::to_str),
            Some("3mf") | Some("xml")
        )
    }

    pub fn add_menu_button(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        if let Some(controller) = &mut self.current_controller {
            controller.add_menu_button(ui, ctx);
        }
    }
}
