use amrust_lib::default_view_controller::DefaultViewController;
use egui::ViewportCommand;

pub struct MyApp {
    name: String,
    default_view_controller: DefaultViewController,
    show_log: bool,
}

impl Default for MyApp {
    fn default() -> Self {
        Self {
            name: "AMRUST".to_owned(),
            default_view_controller: DefaultViewController::default(),
            show_log: false,
        }
    }
}

impl eframe::App for MyApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::TopBottomPanel::top("top panel")
            .resizable(false)
            .show(ctx, |ui| {
                egui::MenuBar::new().ui(ui, |ui| {
                    ui.menu_button("File", |ui| {
                        if ui.button("Close File").clicked() {
                            self.clear_state(ctx);
                        }
                    });
                    ui.menu_button("View", |ui| {
                        if ui.button("Show Log").clicked() {
                            self.show_log = !self.show_log;
                        }
                    });
                    self.default_view_controller.add_menu_button(ui, ctx);
                });
            });

        if self.default_view_controller.has_data() {
            let file_name = self
                .default_view_controller
                .file_name
                .clone()
                .unwrap_or("AMRUST".to_owned());

            self.append_window_header_with_file_name(ctx, &file_name);
        }

        if self.default_view_controller.has_side_panel() {
            egui::SidePanel::left("left_panel")
                .resizable(true)
                .default_width(100.0)
                .show(ctx, |ui| {
                    egui::ScrollArea::both().auto_shrink(false).show(ui, |ui| {
                        self.default_view_controller.file_tree_ui(ui, ctx);
                    });
                });
        }

        if self.show_log {
            egui::TopBottomPanel::bottom("bottom_panel")
                .resizable(true)
                .show_separator_line(true)
                .show(ctx, |ui| {
                    egui_logger::LoggerUi::default().enable_regex(true).show(ui);
                });
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            self.default_view_controller.content_ui(ui, ctx);
            self.default_view_controller.run_dropped_files(ctx);
        });

        pollster::block_on(self.run_update(ctx));
    }
}

impl MyApp {
    fn append_window_header_with_file_name(&mut self, ctx: &egui::Context, file_name: &String) {
        if !self.name.contains(file_name) {
            self.name.push_str(" - ");
            self.name.push_str(file_name);

            ctx.send_viewport_cmd(ViewportCommand::Title(self.name.clone()));
        }
    }

    fn clear_state(&mut self, ctx: &egui::Context) {
        self.default_view_controller.clear_state();

        self.name = "AMRUST".to_owned();
        ctx.send_viewport_cmd(ViewportCommand::Title(self.name.clone()));
    }

    async fn run_update(&mut self, ctx: &egui::Context) {
        match self.default_view_controller.update_state(ctx).await {
            Ok(_) => {}
            Err(err) => {
                println!("{:?}", err);
                log::error!("{:?}", err)
            }
        }
    }
}
