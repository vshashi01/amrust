mod amrust_db;
mod app;
mod app_mode;
mod clear_db;
mod egui_tools;
mod load_3mf;
mod operation_manager;
mod part_list;
mod render_db;
mod render_worker;
mod save_3mf;
mod toolsheets;
mod tree_item_viewer;
mod viewport;

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
