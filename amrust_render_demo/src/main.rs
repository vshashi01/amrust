#![windows_subsystem = "windows"]

mod app;
mod core;
mod db_view_model;
mod egui_tools;
mod features;
mod ui;
mod view_models;

use winit::event_loop::{ControlFlow, EventLoop};

fn main() {
    #[cfg(not(target_arch = "wasm32"))]
    {
        pollster::block_on(run());
    }
}

async fn run() {
    let event_loop = EventLoop::new().unwrap();

    event_loop.set_control_flow(ControlFlow::Poll);

    let mut app = app::App::new();

    event_loop.run_app(&mut app).expect("Failed to run app");
}
