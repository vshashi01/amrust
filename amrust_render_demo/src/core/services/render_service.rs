use std::sync::Arc;

use amrust_render::{
    RenderDatabase,
    camera::{CameraData, CameraTransform, OrthographicCameraData},
    renderer::{RenderTextureData, Renderer},
};
use glam::Mat4;
use smol::{
    channel::{Receiver, Sender, TryRecvError},
    lock::RwLock,
};

use crate::core::render_db::RenderDb;

pub enum RenderServiceRequest {
    UpdateCamera(OrthographicCameraData),
    TransformCamera(Vec<CameraTransform>),
    ResizeViewport(u32, u32),
    Render,
}

pub enum RenderServiceResponse {
    NewTextureView(wgpu::TextureView),
    RenderComplete,
    NewView(Mat4),
}

pub struct RenderService {
    renderer: Renderer,
    camera: OrthographicCameraData,

    receiver: Receiver<RenderServiceRequest>,
    sender: Sender<RenderServiceResponse>,

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

impl RenderService {
    pub async fn new(
        renderer_settings: RendererSettings,
        render_db: Arc<RwLock<RenderDb>>,
        receiver: Receiver<RenderServiceRequest>,
        sender: Sender<RenderServiceResponse>,
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
                log::error!("{err:?}");
                panic!("{err}")
            }
        }
    }

    pub async fn run(&mut self) {
        //send this message at least once.
        if let Err(err) = self
            .sender
            .send(RenderServiceResponse::NewTextureView(
                self.render_texture_data.texture_view.clone(),
            ))
            .await
        {
            log::error!("{err:?}");
        }

        loop {
            match self.receiver.try_recv() {
                Ok(msg) => match msg {
                    RenderServiceRequest::UpdateCamera(orthographic_camera_data) => {
                        self.camera = orthographic_camera_data;
                        self.update_camera().await;
                    }
                    RenderServiceRequest::TransformCamera(transforms) => {
                        for transform in transforms {
                            self.camera.transform(transform);
                        }

                        self.update_camera().await;
                    }
                    RenderServiceRequest::ResizeViewport(width, height) => {
                        self.renderer.set_size(width, height);
                        self.render_texture_data = self.renderer.create_render_texture_data();

                        if let Err(err) = self
                            .sender
                            .send(RenderServiceResponse::NewTextureView(
                                self.render_texture_data.texture_view.clone(),
                            ))
                            .await
                        {
                            log::error!("{err:?}");
                        }
                    }
                    RenderServiceRequest::Render => {
                        let render_db = self.render_db.read().await;
                        let render_data = render_db.get_renderables().collect::<Vec<_>>();
                        log::debug!("Render data count: {:?}", render_data.len());
                        match self
                            .renderer
                            .render_to_texture(&render_data, &self.render_texture_data)
                            .await
                        {
                            Ok(_) => {
                                if let Err(err) = self
                                    .sender
                                    .send(RenderServiceResponse::RenderComplete)
                                    .await
                                {
                                    log::error!("{err:?}");
                                }
                            }

                            Err(err) => {
                                log::error!("{err:?}");
                                panic!("{err:?}")
                            }
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

    async fn update_camera(&mut self) {
        self.renderer.update_camera(&self.camera);

        let view_proj = self.camera.get_view_matrix() * self.camera.get_projection_matrix();
        if let Err(err) = self
            .sender
            .send(RenderServiceResponse::NewView(view_proj))
            .await
        {
            log::error!("Sending view projection failed: {err:?}");
        }
    }
}
