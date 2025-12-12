use std::sync::mpsc::{Receiver, Sender};

use amrust_render::{
    camera::OrthographicCameraData,
    gpu_mesh::GpuMesh,
    instance::GpuInstance,
    renderer::{self, RenderTextureData, Renderer},
};
use egui::ColorImage;
use image::{ImageBuffer, Rgba};

pub enum RenderMessage {
    UpdateCamera(OrthographicCameraData),
    AddGpuMesh(GpuMesh),
    AddGpuInstance(GpuInstance),
    ResizeViewport(u32, u32),
    Render(egui::Context),
}

pub enum RenderResponse {
    NewTextureView(wgpu::TextureView),
    RenderComplete,
}

pub struct RenderWorker {
    renderer: Renderer,

    receiver: Receiver<RenderMessage>,
    sender: Sender<RenderResponse>,

    camera_data: OrthographicCameraData,
    render_texture_data: RenderTextureData,
}

pub struct RendererSettings {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub format: wgpu::TextureFormat,
    pub width: u32,
    pub height: u32,
    pub initial_camera_data: OrthographicCameraData,
}

impl RenderWorker {
    pub async fn new(
        renderer_settings: RendererSettings,
        receiver: Receiver<RenderMessage>,
        sender: Sender<RenderResponse>,
    ) -> Self {
        // let renderer = renderer::Renderer::from_existing_device_and_queue(
        //     renderer_settings.device,
        //     renderer_settings.queue,
        //     renderer_settings.format,
        //     renderer_settings.width,
        //     renderer_settings.height,
        // )
        // .await
        // .unwrap();

        match Renderer::from_new_device(renderer_settings.width, renderer_settings.height).await {
            Ok(renderer) => {
                let render_texture_data = renderer.create_render_texture_data();

                Self {
                    renderer,
                    receiver,
                    sender,
                    camera_data: renderer_settings.initial_camera_data,
                    render_texture_data,
                }
            }
            Err(err) => {
                println!("{err:?}");
                panic!("{err}")
            }
        }
    }

    pub async fn run(&mut self) {
        //send this message at least once.
        if let Err(err) = self.sender.send(RenderResponse::NewTextureView(
            self.render_texture_data.texture_view.clone(),
        )) {
            println!("{err:?}");
        }

        loop {
            match self.receiver.try_recv() {
                Ok(msg) => match msg {
                    RenderMessage::UpdateCamera(orthographic_camera_data) => {
                        self.camera_data = orthographic_camera_data
                    }
                    RenderMessage::AddGpuMesh(gpu_mesh) => todo!(),
                    RenderMessage::AddGpuInstance(gpu_instance) => todo!(),
                    RenderMessage::ResizeViewport(width, height) => {
                        self.renderer.set_size(width, height);
                        self.render_texture_data = self.renderer.create_render_texture_data();

                        if let Err(err) = self.sender.send(RenderResponse::NewTextureView(
                            self.render_texture_data.texture_view.clone(),
                        )) {
                            println!("{err:?}");
                        }
                    }
                    RenderMessage::Render(ctx) => match self.renderer.render().await {
                        Ok(_) => {
                            // {
                            //     let buf = self.renderer.present().await;
                            //     let egui_color_image = image_buffer_to_color_image(&buf);

                            // }
                            if let Err(err) = self.sender.send(RenderResponse::RenderComplete) {
                                println!("{err:?}");
                            }
                        }
                        Err(err) => panic!("{err:?}"),
                    },
                },

                Err(err) => match err {
                    std::sync::mpsc::TryRecvError::Empty => {
                        //keep the tasks running
                        smol::future::yield_now().await;
                    }
                    std::sync::mpsc::TryRecvError::Disconnected => break,
                },
            }
        }
    }
}

fn image_buffer_to_color_image(imgbuf: &ImageBuffer<Rgba<u8>, Vec<u8>>) -> ColorImage {
    let size = [imgbuf.width() as usize, imgbuf.height() as usize];

    let pixels: Vec<_> = imgbuf
        .pixels()
        .map(|p| {
            let [r, g, b, a] = p.0;
            egui::Color32::from_rgba_premultiplied(r, g, b, a)
        })
        .collect();

    ColorImage {
        size,
        source_size: egui::Vec2 {
            x: imgbuf.width() as f32,
            y: imgbuf.height() as f32,
        },
        pixels,
    }
}
