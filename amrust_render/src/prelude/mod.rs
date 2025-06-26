#[cfg(feature = "egui_wgpu")]
pub use egui_wgpu::{wgpu, wgpu::util::DeviceExt};

#[cfg(feature = "wgpu")]
pub use wgpu;

#[cfg(feature = "wgpu")]
pub use wgpu::util::DeviceExt;
