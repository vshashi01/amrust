use crate::prelude::*;

use crate::texture;

pub struct PipelineBuilder<'a> {
    vertex_source: Option<(wgpu::ShaderSource<'static>, Option<String>)>,
    frag_source: Option<(wgpu::ShaderSource<'static>, Option<String>)>,
    bind_group_layouts: Vec<&'a wgpu::BindGroupLayout>,
    vertex_buffer_layouts: Vec<wgpu::VertexBufferLayout<'static>>,
    depth_stencil: Option<wgpu::DepthStencilState>,
    blend_state: Option<wgpu::BlendState>,
    primitive_topology: wgpu::PrimitiveTopology,
    texture_format: Option<wgpu::TextureFormat>,
}

impl<'a> PipelineBuilder<'a> {
    pub fn new() -> Self {
        Self {
            vertex_source: None,
            frag_source: None,
            bind_group_layouts: Vec::new(),
            vertex_buffer_layouts: Vec::new(),
            depth_stencil: None,
            blend_state: None,
            primitive_topology: wgpu::PrimitiveTopology::TriangleList,
            texture_format: None,
        }
    }

    pub fn set_blend_state(&mut self, blend_state: wgpu::BlendState) -> &mut Self {
        self.blend_state = Some(blend_state);
        self
    }

    pub fn set_vertex_source(
        &mut self,
        source: wgpu::ShaderSource<'static>,
        entry_point: Option<String>,
    ) -> &mut Self {
        self.vertex_source = Some((source, entry_point));
        self
    }

    pub fn set_frag_source(
        &mut self,
        source: wgpu::ShaderSource<'static>,
        entry_point: Option<String>,
    ) -> &mut Self {
        self.frag_source = Some((source, entry_point));
        self
    }

    pub fn set_depth_stencil(&mut self, depth_stencil: wgpu::DepthStencilState) -> &mut Self {
        self.depth_stencil = Some(depth_stencil);
        self
    }

    pub fn set_topology(&mut self, topology: wgpu::PrimitiveTopology) -> &mut Self {
        self.primitive_topology = topology;
        self
    }

    pub fn set_texture_format(&mut self, texture_format: wgpu::TextureFormat) -> &mut Self {
        self.texture_format = Some(texture_format);
        self
    }

    pub fn add_bind_group_layout(
        &mut self,
        bind_group_layout: &'a wgpu::BindGroupLayout,
    ) -> &mut Self {
        self.bind_group_layouts.push(bind_group_layout);
        self
    }

    pub fn add_vertex_buffer_layout(
        &mut self,
        vertex_buffer_layout: wgpu::VertexBufferLayout<'static>,
    ) -> &mut Self {
        self.vertex_buffer_layouts.push(vertex_buffer_layout);
        self
    }

    pub fn build(&mut self, device: &wgpu::Device, name: &str) -> wgpu::RenderPipeline {
        assert!(self.vertex_source.is_some(), "Vertex source is not set");
        assert!(self.texture_format.is_some(), "Texture format is not set");

        let (vert_shader, vert_entry_point) = match self.vertex_source.take() {
            Some((source, entry_point)) => {
                let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                    label: Some(&format!("Vertex Shader: {name}")),
                    source,
                });

                (shader, entry_point)
            }
            None => panic!("Vertex shader not found"),
        };

        let render_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some(&format!("Pipeline Layout: {name}")),
                bind_group_layouts: &self.bind_group_layouts,
                push_constant_ranges: &[],
            });

        let (frag_shader, frag_entry_point) = match self.frag_source.take() {
            Some((source, entry_point)) => {
                let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                    label: Some(&format!("Fragment Shader: {name}")),
                    source: source.clone(),
                });

                (shader, entry_point)
            }
            None => (vert_shader.clone(), None),
        };

        let render_pipeline_name = format!("Render Pipeline: {name}");

        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(&render_pipeline_name),
            layout: Some(&render_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &vert_shader,
                entry_point: vert_entry_point.as_deref(),
                buffers: &self.vertex_buffer_layouts,
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &frag_shader,
                entry_point: frag_entry_point.as_deref(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: self.texture_format.unwrap(),
                    blend: Some(self.blend_state.unwrap_or(wgpu::BlendState {
                        alpha: wgpu::BlendComponent::REPLACE,
                        color: wgpu::BlendComponent::REPLACE,
                    })),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: self.primitive_topology,
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
            depth_stencil: self.depth_stencil.clone(),
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
}

pub fn create_depth_stencil_state() -> wgpu::DepthStencilState {
    wgpu::DepthStencilState {
        format: texture::DepthTexture::DEPTH_FORMAT,
        depth_write_enabled: true,
        depth_compare: wgpu::CompareFunction::Less,
        stencil: wgpu::StencilState::default(),
        bias: wgpu::DepthBiasState {
            constant: 0,
            slope_scale: 0.0,
            clamp: 0.0,
        },
    }
}

pub fn create_alpha_blend_state() -> wgpu::BlendState {
    wgpu::BlendState {
        alpha: wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::SrcAlpha,
            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
            operation: wgpu::BlendOperation::Add,
        },
        color: wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::SrcAlpha,
            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
            operation: wgpu::BlendOperation::Add,
        },
    }
}

pub fn create_depth_stencil_state_transparent() -> wgpu::DepthStencilState {
    wgpu::DepthStencilState {
        format: crate::texture::DepthTexture::DEPTH_FORMAT,
        depth_write_enabled: false, // Don't write to depth buffer for transparent objects
        depth_compare: wgpu::CompareFunction::LessEqual, // Read depth but allow equal depth
        stencil: wgpu::StencilState::default(),
        bias: wgpu::DepthBiasState {
            constant: 0,
            slope_scale: 0.0,
            clamp: 0.0,
        },
    }
}
