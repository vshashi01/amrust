use std::sync::Arc;

use amrust_render::{
    camera::{CameraData, CameraTransform, OrthographicCameraData},
    renderer::{self, FrameViewData, RenderTextureData, Renderer},
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
    device: wgpu::Device,
    format: wgpu::TextureFormat,
    width: u32,
    height: u32,
    frame_view_data: FrameViewData,

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
            renderer_settings.device.clone(),
            renderer_settings.queue,
            renderer_settings.format,
        )
        .await
        {
            // match Renderer::from_new_device(renderer_settings.width, renderer_settings.height).await {
            Ok(renderer) => {
                //renderer.update_camera(&renderer_settings.initial_camera_data);
                let frame_view_data = renderer
                    .create_frame_view_data(renderer_settings.width, renderer_settings.height);
                let render_texture_data = renderer::create_texture_data(
                    &renderer_settings.device,
                    wgpu::Extent3d {
                        width: renderer_settings.width,
                        height: renderer_settings.height,
                        depth_or_array_layers: 1,
                    },
                    renderer_settings.format,
                );

                Self {
                    renderer,
                    receiver,
                    device: renderer_settings.device,
                    format: renderer_settings.format,
                    width: renderer_settings.width,
                    height: renderer_settings.height,
                    frame_view_data,
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
                        self.update_frame_view_data().await;
                    }
                    RenderServiceRequest::TransformCamera(transforms) => {
                        for transform in transforms {
                            self.camera.transform(transform);
                        }

                        self.update_frame_view_data().await;
                    }
                    RenderServiceRequest::ResizeViewport(width, height) => {
                        self.width = width;
                        self.height = height;

                        self.camera.set_viewport_size(width as f32, height as f32);
                        self.frame_view_data.update_viewport_size(
                            width as f32,
                            height as f32,
                            &self.camera,
                        );
                        self.update_frame_view_data().await;
                        self.render_texture_data = renderer::create_texture_data(
                            &self.device,
                            wgpu::Extent3d {
                                width,
                                height,
                                depth_or_array_layers: 1,
                            },
                            self.format,
                        );

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
                        // log::debug!("Render data count: {:?}", render_data.len());
                        match self
                            .renderer
                            .render_to_texture(
                                &render_data,
                                &self.render_texture_data,
                                &self.frame_view_data,
                            )
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

    async fn update_frame_view_data(&mut self) {
        self.frame_view_data.camera.update(&self.camera);
        self.renderer
            .write_frame_view_data_to_gpu(&self.frame_view_data);

        let view_proj = self.camera.get_view_projection();
        if let Err(err) = self
            .sender
            .send(RenderServiceResponse::NewView(view_proj))
            .await
        {
            log::error!("Sending view projection failed: {err:?}");
        }
    }
}
