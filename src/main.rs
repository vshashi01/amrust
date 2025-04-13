#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use amrust::MyApp;
use eframe::egui;

fn main() -> eframe::Result {
    // env_logger::init(); // Log to stderr (if you run with `RUST_LOG=debug`).
    egui_logger::builder().init().unwrap();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([800.0, 450.0]),
        centered: true,
        ..Default::default()
    };
    eframe::run_native(
        "AMRUST",
        options,
        Box::new(|cc| {
            // This gives us image support:
            egui_extras::install_image_loaders(&cc.egui_ctx);

            Ok(Box::<MyApp>::default())
        }),
    )
}

/* use amrust::MyApp;
use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts, EguiPlugin};

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(EguiPlugin)
        .add_systems(Update, ui_example_sytem)
        .run();
}

fn ui_example_sytem(mut contexts: EguiContexts) {
    // egui::Window::new("hello").show(contexts.ctx_mut(), |ui| ui.label("world"));
    let mut my_app = MyApp::default();

    my_app.create_ui(contexts.ctx_mut());

    /* egui::Window::new("test").show(contexts.ctx_for_entity_mut(enity), |_ui| {
        let mut my_app = MyApp::default();

        my_app.create_ui(contexts.ctx_mut());
    }); */
} */
