mod camera;
mod instance;
mod material;
mod normalized_box;
mod object;
mod texture;
mod vertex;
use camera::{Camera, OrthographicCameraData};
use glam::{Mat4, Vec3};
use material::Material;
use normalized_box::{INDICES, VERTICES};
use object::{GpuMesh, Object, Renderables};
use texture::Texture;
use thiserror::Error;
use vertex::Vertex;
use wgpu::util::DeviceExt;

#[derive(Debug, Error)]
pub enum WgpuError {
    #[error("Something went wrong with the Device request")]
    DeviceError(#[from] wgpu::RequestDeviceError),

    #[error("When something goes wrong with the adapter")]
    AdapterError(#[from] wgpu::RequestAdapterError),
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

    let camera_data = OrthographicCameraData::default()
        .transform(camera::CameraTransform::Zoom(-0.80))
        .transform(camera::CameraTransform::Pan(Vec3 {
            x: 0.5,
            y: 0.5,
            z: 0.0,
        }))
        .transform(camera::CameraTransform::Rotate {
            pivot: Vec3 {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
            rotation_axis: Vec3::NEG_X,
            angle: std::f32::consts::FRAC_2_PI,
        })
        .transform(camera::CameraTransform::Rotate {
            pivot: Vec3 {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
            rotation_axis: Vec3::NEG_Y,
            angle: std::f32::consts::FRAC_2_SQRT_PI,
        })
        .transform(camera::CameraTransform::Rotate {
            pivot: Vec3 {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
            rotation_axis: Vec3::X,
            angle: std::f32::consts::FRAC_2_SQRT_PI,
        });

    let camera = Camera::new(&camera_data);
    let (camera_bind_group_layout, camera_bind_group, _camera_buffer) =
        camera.create_gpu_resources(&device);

    let wireframe_material = Material::new(0.0, 0.0, 256.0);
    let wireframe_material_uniform = wireframe_material.create_uniform();
    let wireframe_material_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Wireframe Material Buffer"),
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        contents: bytemuck::cast_slice(&[wireframe_material_uniform]),
    });
    let wireframe_material_bind_group_layout =
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Material bind group layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

    let wireframe_material_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("Wireframe Material Bind Group"),
        layout: &wireframe_material_bind_group_layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: wireframe_material_buffer.as_entire_binding(),
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

    let mut renderables: Vec<Renderables> = Vec::new();

    //indexed mesh
    let mesh = GpuMesh::new_indexed_mesh(
        bytemuck::cast_slice(VERTICES),
        bytemuck::cast_slice(INDICES),
        INDICES.len() as u32,
        &device,
    );
    renderables.push(Renderables::IndexedMesh(mesh, true)); //index 0 with wireframe

    // simple mesh
    let simple_mesh = GpuMesh::new_mesh(
        bytemuck::cast_slice(VERTICES),
        VERTICES.len() as u32,
        &device,
    );
    renderables.push(Renderables::Mesh(simple_mesh)); //index 1 without wireframe

    let mut objects: Vec<Object> = Vec::new();

    let indexed_mesh_instances = Object::new(
        0,
        vec![
            instance::Instance {
                transformation: Mat4::IDENTITY,
            },
            instance::Instance {
                transformation: Mat4::from_translation((5.0, 0.0, 0.0).into()),
            },
        ],
        &device,
    );
    objects.push(indexed_mesh_instances); //index 0

    let simple_mesh_instances = Object::new(
        1,
        vec![instance::Instance {
            transformation: Mat4::from_translation((-5.0, 0.0, 0.0).into()),
        }],
        &device,
    );
    objects.push(simple_mesh_instances); //index 0

    let source = wgpu::ShaderSource::Wgsl((include_str!("vertex.wgsl")).into());

    let surface_render_pipeline = generate_basic_render_pipeline(
        &device,
        "Surface",
        source,
        texture_desc.format,
        &[vertex::VertexPC::desc(), instance::InstanceRaw::desc()],
        &[&camera_bind_group_layout, &basic_texture_bind_group_layout],
        wgpu::PrimitiveTopology::TriangleList,
    );

    let wireframe_source =
        wgpu::ShaderSource::Wgsl((include_str!("uniform_color_vertex.wgsl")).into());

    let wireframe_render_pipeline = generate_basic_render_pipeline(
        &device,
        "Wireframe",
        wireframe_source,
        texture_desc.format,
        &[vertex::VertexP::desc(), instance::InstanceRaw::desc()],
        &[
            &camera_bind_group_layout,
            &wireframe_material_bind_group_layout,
        ],
        wgpu::PrimitiveTopology::LineList,
    );

    let mut encoder =
        device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

    {
        let render_pass_desc = wgpu::RenderPassDescriptor {
            label: Some("Surface Render Pass"),
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

        for object in objects {
            render_pass.set_vertex_buffer(1, object.instance_buffer.slice(..));

            if let Some(renderable) = renderables.get(object.renderable_id) {
                match renderable {
                    Renderables::IndexedMesh(mesh, draw_wireframe) => {
                        render_pass.set_vertex_buffer(0, mesh.vertex_buffer.slice(..));
                        if let Some(index_buffer) = &mesh.index_buffer {
                            render_pass.set_index_buffer(
                                index_buffer.slice(..),
                                wgpu::IndexFormat::Uint16,
                            );

                            //render surface
                            render_pass.set_pipeline(&surface_render_pipeline);
                            render_pass.set_bind_group(1, &basic_diffuse_bind_group, &[]);

                            render_pass.draw_indexed(
                                0..mesh.buffer_length,
                                0,
                                0..object.instances.len() as u32,
                            );

                            if *draw_wireframe {
                                //render wireframe
                                render_pass.set_pipeline(&wireframe_render_pipeline);
                                render_pass.set_bind_group(1, &wireframe_material_bind_group, &[]);

                                render_pass.draw_indexed(
                                    0..mesh.buffer_length,
                                    0,
                                    0..object.instances.len() as u32,
                                );
                            }
                        } else {
                            panic!("Mesh does not have an index buffer");
                        }
                    }
                    Renderables::Mesh(mesh) => {
                        render_pass.set_vertex_buffer(0, mesh.vertex_buffer.slice(..));
                        render_pass.set_pipeline(&surface_render_pipeline);
                        render_pass.set_bind_group(1, &basic_diffuse_bind_group, &[]);

                        render_pass.draw(0..mesh.buffer_length, 0..object.instances.len() as u32);
                    }
                }
            }
        }
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
    texture_format: wgpu::TextureFormat,
    buffers: &[wgpu::VertexBufferLayout<'static>],
    bind_group_layouts: &[&wgpu::BindGroupLayout],
    topology: wgpu::PrimitiveTopology,
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

    let render_pipeline_name = format!("Render Pipeline: {shader_name}");

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
                format: texture_format,
                blend: Some(wgpu::BlendState {
                    alpha: wgpu::BlendComponent::REPLACE,
                    color: wgpu::BlendComponent::REPLACE,
                }),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: Default::default(),
        }),
        primitive: wgpu::PrimitiveState {
            topology,
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
    })
}

fn generate_basic_texture_bind_group(
    device: &wgpu::Device,
    texture: Texture,
    layout: &wgpu::BindGroupLayout,
) -> wgpu::BindGroup {
    let bind_group_label = texture.label.clone() + "Bind Group";
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some(bind_group_label.as_str()),
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

#[cfg(test)]
pub mod tests {
    use super::*;

    #[test]
    fn test_run() {
        env_logger::init();
        pollster::block_on(run());
    }
}
