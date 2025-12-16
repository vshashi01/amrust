use std::sync::Arc;

use amrust_render::{
    RenderDatabase,
    camera::{CameraData, CameraTransform, OrthographicCameraData},
    renderer::{RenderTextureData, Renderer},
};
use smol::{
    channel::{Receiver, Sender, TryRecvError},
    lock::RwLock,
};

use crate::render_db::RenderDb;

pub enum RenderMessage {
    UpdateCamera(OrthographicCameraData),
    TransformCamera(Vec<CameraTransform>),
    ResizeViewport(u32, u32),
    Render,
}

pub enum RenderResponse {
    NewTextureView(wgpu::TextureView),
    RenderComplete,
}

pub struct RenderWorker {
    renderer: Renderer,
    camera: OrthographicCameraData,

    receiver: Receiver<RenderMessage>,
    sender: Sender<RenderResponse>,

    render_texture_data: RenderTextureData,
    render_db: Arc<RwLock<RenderDb>>,
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
        render_db: Arc<RwLock<RenderDb>>,
        receiver: Receiver<RenderMessage>,
        sender: Sender<RenderResponse>,
    ) -> Self {
        match Renderer::from_existing_device_and_queue(
            renderer_settings.device,
            renderer_settings.queue,
            renderer_settings.format,
            renderer_settings.width,
            renderer_settings.height,
        )
        .await
        {
            // match Renderer::from_new_device(renderer_settings.width, renderer_settings.height).await {
            Ok(mut renderer) => {
                renderer.update_camera(&renderer_settings.initial_camera_data);
                let render_texture_data = renderer.create_render_texture_data();

                Self {
                    renderer,
                    receiver,
                    sender,
                    render_texture_data,
                    render_db,
                    camera: renderer_settings.initial_camera_data,
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
        if let Err(err) = self
            .sender
            .send(RenderResponse::NewTextureView(
                self.render_texture_data.texture_view.clone(),
            ))
            .await
        {
            println!("{err:?}");
        }

        loop {
            match self.receiver.try_recv() {
                Ok(msg) => match msg {
                    RenderMessage::UpdateCamera(orthographic_camera_data) => {
                        self.renderer.update_camera(&orthographic_camera_data);
                    }
                    RenderMessage::TransformCamera(transforms) => {
                        for transform in transforms {
                            self.camera.transform(transform);
                        }
                        println!("Transformed camera");
                    }
                    RenderMessage::ResizeViewport(width, height) => {
                        self.renderer.set_size(width, height);
                        self.render_texture_data = self.renderer.create_render_texture_data();

                        if let Err(err) = self
                            .sender
                            .send(RenderResponse::NewTextureView(
                                self.render_texture_data.texture_view.clone(),
                            ))
                            .await
                        {
                            println!("{err:?}");
                        }
                    }
                    RenderMessage::Render => {
                        let render_db = self.render_db.read().await;
                        let render_data = render_db.get_renderables().collect::<Vec<_>>();
                        // println!("Render data count: {:?}", render_data.len());
                        match self
                            .renderer
                            .render_to_texture(&render_data, &self.render_texture_data)
                            .await
                        {
                            Ok(_) => {
                                if let Err(err) =
                                    self.sender.send(RenderResponse::RenderComplete).await
                                {
                                    println!("{err:?}");
                                }
                            }

                            Err(err) => panic!("{err:?}"),
                        }
                    }
                },

                Err(err) => match err {
                    TryRecvError::Empty => {
                        //keep the tasks running
                        smol::future::yield_now().await;
                    }
                    TryRecvError::Closed => break,
                },
            }
        }
    }
}
