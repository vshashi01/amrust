mod camera;
mod challenge_vertex;
mod instance;
mod texture;
use camera::{Camera, CameraController, CameraStaging, CameraUniform};
use cgmath::{InnerSpace, Rotation3, Vector3, Zero};
use challenge_vertex::Vertex;
use texture::Texture;
use thiserror::Error;
use wgpu::util::DeviceExt;

pub struct State<'a> {
    texture_data: TextureData<'a>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    size: (f32, f32),
    render_pipeline: wgpu::RenderPipeline,

    //managing camera
    camera_staging: CameraStaging,
    camera_controller: CameraController,
    camera_uniform: CameraUniform,
    camera_buffer: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,

    //mesh buffers
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    index_buffer_len: u32,
}

pub struct TextureData<'a> {
    texture_desc: wgpu::TextureDescriptor<'a>,
    texture: wgpu::Texture,
    texture_size: u32,
}

#[derive(Debug, Error)]
pub enum WgpuError {
    #[error("Something went wrong with the Device request")]
    DeviceError(#[from] wgpu::RequestDeviceError),

    #[error("When something goes wrong with the adapter")]
    AdapterError(#[from] wgpu::RequestAdapterError),
}

impl<'a> State<'a> {
    pub async fn new() -> Result<State<'a>, WgpuError> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::DX12,
            ..Default::default()
        });

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::default(),
                compatible_surface: None,
                force_fallback_adapter: false,
            })
            .await?;
        let (device, queue) = adapter.request_device(&Default::default()).await?;

        let texture_size = 256u32;

        let texture_desc = wgpu::TextureDescriptor {
            size: wgpu::Extent3d {
                width: texture_size,
                height: texture_size,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::COPY_SRC | wgpu::TextureUsages::RENDER_ATTACHMENT,
            label: None,
            view_formats: &[],
        };
        let texture = device.create_texture(&texture_desc);
        // let texture_view = texture.create_view(&Default::default());
        let texture_data = TextureData {
            texture_desc,
            texture,
            texture_size,
        };

        let size = (600.0, 400.0);

        //mesh buffer
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Vertex Buffer"),
            contents: bytemuck::cast_slice(challenge_vertex::VERTICES),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Index Buffer"),
            contents: bytemuck::cast_slice(challenge_vertex::INDICES),
            usage: wgpu::BufferUsages::INDEX,
        });

        let index_buffer_len = challenge_vertex::INDICES.len() as u32;

        // instance tutorials
        let instances = generate_instances();

        let instance_data = instances
            .iter()
            .map(instance::Instance::to_raw)
            .collect::<Vec<_>>();
        let instance_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Instance buffer"),
            contents: bytemuck::cast_slice(&instance_data),
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        });

        //camera tutorials
        let camera = Camera {
            eye: (0.0, 1.0, 2.0).into(),
            target: (0.0, 0.0, 0.0).into(),
            up: cgmath::Vector3::unit_y(),
            fovy: 45.0,
            aspect: (size.0 / size.1),
            z_near: 0.1,
            z_far: 100.0,
        };

        let camera_controller = CameraController::new(0.2);

        let mut camera_uniform = CameraUniform::new();

        let camera_staging = CameraStaging::new(camera);
        camera_staging.update_camera(&mut camera_uniform);

        let camera_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Camera buffer"),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            contents: bytemuck::cast_slice(&[camera_uniform]),
        });

        let camera_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Camera bind group layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Camera bind group"),
            layout: &camera_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buffer.as_entire_binding(),
            }],
        });

        let render_pipeline = generate_basic_render_pipeline(
            &device,
            "shader",
            wgpu::ShaderSource::Wgsl((include_str!("challenge.wgsl")).into()),
            &texture_data.texture_desc.format,
            &[challenge_vertex::ChallengeVertex::desc()],
            &[&camera_bind_group_layout],
            false,
        );

        Ok(Self {
            device,
            queue,
            size,
            render_pipeline,
            texture_data,

            camera_staging,
            camera_controller,
            camera_uniform,
            camera_buffer,
            camera_bind_group,

            vertex_buffer,
            index_buffer,
            index_buffer_len,
        })
    }

    pub async fn render(&self) {
        let texture_view = self.texture_data.texture.create_view(&Default::default());

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

        {
            let render_pass_desc = wgpu::RenderPassDescriptor {
                label: Some("Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &texture_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.1,
                            g: 0.2,
                            b: 0.3,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            };
            let mut render_pass = encoder.begin_render_pass(&render_pass_desc);

            render_pass.set_bind_group(0, &self.camera_bind_group, &[]);
            // render_pass.set_bind_group(1, &data.2, &[]);
            render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            // render_pass.set_vertex_buffer(1, self.instance_buffer.slice(..));
            render_pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint16);

            render_pass.set_pipeline(&self.render_pipeline);
            render_pass.draw(0..5, 0..1);
        }

        let u32_size = std::mem::size_of::<u32>() as u32;
        let texture_size = self.texture_data.texture_size;

        let output_buffer_size = (u32_size * texture_size * texture_size) as wgpu::BufferAddress;
        let output_buffer_desc = wgpu::BufferDescriptor {
            size: output_buffer_size,
            usage: wgpu::BufferUsages::COPY_DST
        // this tells wpgu that we want to read this buffer from the cpu
        | wgpu::BufferUsages::MAP_READ,
            label: None,
            mapped_at_creation: false,
        };
        let output_buffer = self.device.create_buffer(&output_buffer_desc);

        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                aspect: wgpu::TextureAspect::All,
                texture: &self.texture_data.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &output_buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(u32_size * texture_size),
                    rows_per_image: Some(texture_size),
                },
            },
            self.texture_data.texture_desc.size,
        );

        self.queue.submit(Some(encoder.finish()));

        // We need to scope the mapping variables so that we can
        // unmap the buffer
        {
            let buffer_slice = output_buffer.slice(..);

            // NOTE: We have to create the mapping THEN device.poll() before await
            // the future. Otherwise the application will freeze.
            let (tx, rx) = futures_intrusive::channel::shared::oneshot_channel();
            buffer_slice.map_async(wgpu::MapMode::Read, move |result| {
                tx.send(result).unwrap();
            });
            self.device.poll(wgpu::PollType::Wait).unwrap();
            rx.receive().await.unwrap().unwrap();

            let data = buffer_slice.get_mapped_range();

            use image::{ImageBuffer, Rgba};
            let buffer =
                ImageBuffer::<Rgba<u8>, _>::from_raw(texture_size, texture_size, data).unwrap();
            buffer.save("image.png").unwrap();
        }
    }
}

pub async fn run() {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
        backends: wgpu::Backends::VULKAN,
        ..Default::default()
    });
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::default(),
            compatible_surface: None,
            force_fallback_adapter: false,
        })
        .await
        .unwrap();
    let (device, queue) = adapter.request_device(&Default::default()).await.unwrap();

    let texture_size = 256u32;
    let texture_desc = wgpu::TextureDescriptor {
        size: wgpu::Extent3d {
            width: texture_size,
            height: texture_size,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::COPY_SRC | wgpu::TextureUsages::RENDER_ATTACHMENT,
        label: None,
        view_formats: &[],
    };
    let texture = device.create_texture(&texture_desc);
    let texture_view = texture.create_view(&Default::default());

    // we need to store this for later
    let u32_size = std::mem::size_of::<u32>() as u32;

    let output_buffer_size = (u32_size * texture_size * texture_size) as wgpu::BufferAddress;
    let output_buffer_desc = wgpu::BufferDescriptor {
        size: output_buffer_size,
        usage: wgpu::BufferUsages::COPY_DST
                // this tells wpgu that we want to read this buffer from the cpu
                | wgpu::BufferUsages::MAP_READ,
        label: None,
        mapped_at_creation: false,
    };
    let output_buffer = device.create_buffer(&output_buffer_desc);

    //camera tutorials
    let camera = Camera {
        eye: (0.0, 1.0, 2.0).into(),
        target: (0.0, 0.0, 0.0).into(),
        up: cgmath::Vector3::unit_y(),
        fovy: 45.0,
        aspect: (texture_desc.size.height / texture_desc.size.width) as f32,
        z_near: 0.1,
        z_far: 100.0,
    };

    let camera_controller = CameraController::new(0.2);

    let mut camera_uniform = CameraUniform::new();

    let camera_staging = CameraStaging::new(camera);
    camera_staging.update_camera(&mut camera_uniform);

    let camera_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Camera buffer"),
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        contents: bytemuck::cast_slice(&[camera_uniform]),
    });

    let camera_bind_group_layout =
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Camera bind group layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

    let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("Camera bind group"),
        layout: &camera_bind_group_layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: camera_buffer.as_entire_binding(),
        }],
    });

    let basic_texture_bind_group_layout =
        generate_texture_bind_group_layout(&device, "Texture Bind Group Layout", true);

    let happy_tree_bytes = include_bytes!("happy-tree.png");
    let basic_diffuse_texture =
        Texture::from_bytes(&device, &queue, happy_tree_bytes, "Happy-tree.png").unwrap();

    let basic_diffuse_bind_group = generate_basic_texture_bind_group(
        &device,
        basic_diffuse_texture,
        &basic_texture_bind_group_layout,
    );

    //mesh buffer
    let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Vertex Buffer"),
        contents: bytemuck::cast_slice(challenge_vertex::VERTICES),
        usage: wgpu::BufferUsages::VERTEX,
    });

    let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Index Buffer"),
        contents: bytemuck::cast_slice(challenge_vertex::INDICES),
        usage: wgpu::BufferUsages::INDEX,
    });

    // instance tutorials
    let instances = generate_instances();

    let instance_data = instances
        .iter()
        .map(instance::Instance::to_raw)
        .collect::<Vec<_>>();
    let instance_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Instance buffer"),
        contents: bytemuck::cast_slice(&instance_data),
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
    });

    let instance_buffer_len = instance_data.len() as u32;

    let source = wgpu::ShaderSource::Wgsl((include_str!("challenge.wgsl")).into());
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Test"),
        source,
    });

    let render_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("Render Pipeline Layout"),
        bind_group_layouts: &[&camera_bind_group_layout, &basic_texture_bind_group_layout],
        push_constant_ranges: &[],
    });

    let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Render Pipeline"),
        layout: Some(&render_pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            buffers: &[
                challenge_vertex::ChallengeVertex::desc(),
                instance::InstanceRaw::desc(),
            ],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            targets: &[Some(wgpu::ColorTargetState {
                format: texture_desc.format,
                blend: Some(wgpu::BlendState {
                    alpha: wgpu::BlendComponent::REPLACE,
                    color: wgpu::BlendComponent::REPLACE,
                }),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: Default::default(),
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: Some(wgpu::Face::Back),
            // Setting this to anything other than Fill requires Features::NON_FILL_POLYGON_MODE
            polygon_mode: wgpu::PolygonMode::Fill,
            // Requires Features::DEPTH_CLIP_CONTROL
            unclipped_depth: false,
            // Requires Features::CONSERVATIVE_RASTERIZATION
            conservative: false,
        },
        depth_stencil: None,
        multisample: wgpu::MultisampleState {
            count: 1,
            mask: !0,
            alpha_to_coverage_enabled: false,
        },
        // If the pipeline will be used with a multiview render pass, this
        // indicates how many array layers the attachments will have.
        multiview: None,
        cache: None,
    });

    let mut encoder =
        device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

    {
        let render_pass_desc = wgpu::RenderPassDescriptor {
            label: Some("Render Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &texture_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.1,
                        g: 0.2,
                        b: 0.3,
                        a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            occlusion_query_set: None,
            timestamp_writes: None,
        };
        let mut render_pass = encoder.begin_render_pass(&render_pass_desc);

        render_pass.set_bind_group(0, &camera_bind_group, &[]);
        render_pass.set_bind_group(1, &basic_diffuse_bind_group, &[]);
        render_pass.set_vertex_buffer(0, vertex_buffer.slice(..));
        render_pass.set_vertex_buffer(1, instance_buffer.slice(..));
        render_pass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint16);
        render_pass.set_pipeline(&render_pipeline);
        render_pass.draw_indexed(0..6, 0, 0..instance_buffer_len);
    }

    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            aspect: wgpu::TextureAspect::All,
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &output_buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(u32_size * texture_size),
                rows_per_image: Some(texture_size),
            },
        },
        texture_desc.size,
    );

    queue.submit(Some(encoder.finish()));

    // We need to scope the mapping variables so that we can
    // unmap the buffer
    {
        let buffer_slice = output_buffer.slice(..);

        // NOTE: We have to create the mapping THEN device.poll() before await
        // the future. Otherwise the application will freeze.
        let (tx, rx) = futures_intrusive::channel::shared::oneshot_channel();
        buffer_slice.map_async(wgpu::MapMode::Read, move |result| {
            tx.send(result).unwrap();
        });
        device.poll(wgpu::PollType::Wait).unwrap();
        rx.receive().await.unwrap().unwrap();

        let data = buffer_slice.get_mapped_range();

        use image::{ImageBuffer, Rgba};
        let buffer =
            ImageBuffer::<Rgba<u8>, _>::from_raw(texture_size, texture_size, data).unwrap();
        buffer.save("image.png").unwrap();
    }
    output_buffer.unmap();
}

fn generate_basic_render_pipeline(
    device: &wgpu::Device,
    shader_name: &str,
    source: wgpu::ShaderSource,
    texture_format: &wgpu::TextureFormat,
    buffers: &[wgpu::VertexBufferLayout<'static>],
    bind_group_layouts: &[&wgpu::BindGroupLayout],
    need_depth_stencil: bool,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(shader_name),
        source,
    });

    let pipeline_layout_name = format!("Pipeline Layout {shader_name}");
    let render_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some(&pipeline_layout_name),
        bind_group_layouts,
        push_constant_ranges: &[],
    });

    let render_pipeline_name = format!("Render Pipeline {shader_name}");

    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(&render_pipeline_name),
        layout: Some(&render_pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            buffers,
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            targets: &[Some(wgpu::ColorTargetState {
                format: texture_format.clone(),
                blend: Some(wgpu::BlendState {
                    color: wgpu::BlendComponent::REPLACE,
                    alpha: wgpu::BlendComponent::REPLACE,
                }),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: Default::default(),
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: Some(wgpu::Face::Back),
            unclipped_depth: false,
            polygon_mode: wgpu::PolygonMode::Fill,
            conservative: false,
        },
        depth_stencil: if need_depth_stencil {
            Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth16Unorm,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::LessEqual,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            })
        } else {
            None
        },
        multisample: wgpu::MultisampleState {
            count: 1,
            mask: !0,
            alpha_to_coverage_enabled: false,
        },
        multiview: None,
        cache: None,
    })
}

fn generate_basic_texture_bind_group(
    device: &wgpu::Device,
    texture: Texture,
    layout: &wgpu::BindGroupLayout,
) -> wgpu::BindGroup {
    let bind_group_label = texture.label.clone() + "Bind Group";
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some(&bind_group_label.as_str()),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&texture.view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(&texture.sampler),
            },
        ],
    })
}

fn generate_texture_bind_group_layout(
    device: &wgpu::Device,
    label: &str,
    filterable: bool,
) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(if filterable {
                    wgpu::SamplerBindingType::Filtering
                } else {
                    wgpu::SamplerBindingType::NonFiltering
                }),
                count: None,
            },
        ],
        label: Some(label),
    })
}

fn generate_instances() -> Vec<instance::Instance> {
    // let instances = (0..NUM_INSTANCES_PER_ROW)
    //     .flat_map(|z| {
    //         (0..NUM_INSTANCES_PER_ROW).map(move |x| {
    //             let (position, rotation) = initialize_instance_position_and_rotation(x, z);
    //             instance::Instance { position, rotation }
    //         })
    //     })
    //     .collect::<Vec<_>>();
    // instances
    vec![instance::Instance {
        position: Vector3 {
            x: 0.0,
            y: 0.0,
            z: 0.0,
        },
        rotation: cgmath::Quaternion {
            v: Vector3 {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
            s: 0.0,
        },
    }]
}

const NUM_INSTANCES_PER_ROW: u32 = 1;
const INSTANCE_DISPLACEMENT: cgmath::Vector3<f32> = cgmath::Vector3::new(
    NUM_INSTANCES_PER_ROW as f32 * 0.5,
    0.0,
    NUM_INSTANCES_PER_ROW as f32 * 0.5,
);

const SPACE_BETWEEN: f32 = 3.0;

fn initialize_instance_position_and_rotation(
    x: u32,
    z: u32,
) -> (cgmath::Vector3<f32>, cgmath::Quaternion<f32>) {
    let position = cgmath::Vector3 {
        x: SPACE_BETWEEN * (x as f32),
        y: 0.0,
        z: SPACE_BETWEEN * (z as f32),
    } - INSTANCE_DISPLACEMENT;

    let rotation = if position.is_zero() {
        cgmath::Quaternion::from_axis_angle(cgmath::Vector3::unit_z(), cgmath::Deg(0.0))
    } else {
        cgmath::Quaternion::from_axis_angle(position.normalize(), cgmath::Deg(0.0))
    };
    (position, rotation)
}

#[cfg(test)]
pub mod tests {
    use super::*;

    #[test]
    fn test_run() {
        env_logger::init();
        pollster::block_on(run());
    }
}
