//! Main render graph implementation

use std::collections::HashMap;

use super::{
    BufferDesc, BufferResource, CompiledGraph, GraphCompiler, GraphError, PassBuilder, PassContext,
    PassId, RenderPass, ResourceHandle, ResourceId, TextureDesc, TextureResource,
};
use crate::renderer::ViewportRect;

/// The main render graph structure
#[derive(Debug, Default)]
pub struct RenderGraph {
    pub(crate) passes: HashMap<PassId, RenderPass>,
    pub(crate) resources: HashMap<ResourceId, ResourceHandle>,
    pub(crate) compiled_data: Option<CompiledGraphData>,
    pub(crate) next_pass_id: usize,
    pub(crate) next_resource_id: usize,
}

/// Internal compiled data storage
#[derive(Debug, Clone)]
pub(crate) struct CompiledGraphData {
    pub order: Vec<PassId>,
    pub barriers: Vec<super::compiler::Barrier>,
    pub lifetimes: HashMap<ResourceId, (usize, usize)>,
}

impl RenderGraph {
    /// Create a new empty render graph
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a texture resource
    pub fn create_texture(&mut self, name: &'static str, desc: TextureDesc) -> ResourceId {
        let id = ResourceId(self.next_resource_id);
        self.next_resource_id += 1;

        let texture_desc = wgpu::TextureDescriptor {
            label: Some(name),
            size: desc.size,
            mip_level_count: 1,
            sample_count: desc.sample_count,
            dimension: wgpu::TextureDimension::D2,
            format: desc.format,
            usage: desc.usage,
            view_formats: &[],
        };

        let resource = ResourceHandle::Texture(TextureResource {
            id,
            name,
            desc: texture_desc,
            usage: desc.usage,
            transient: desc.transient,
            texture: std::cell::OnceCell::new(),
            view: std::cell::OnceCell::new(),
            sampler: std::cell::OnceCell::new(),
        });

        self.resources.insert(id, resource);
        self.compiled_data = None; // Invalidate compilation
        id
    }

    /// Create a buffer resource
    pub fn create_buffer(&mut self, name: &'static str, desc: BufferDesc) -> ResourceId {
        let id = ResourceId(self.next_resource_id);
        self.next_resource_id += 1;

        let buffer_desc = wgpu::BufferDescriptor {
            label: Some(name),
            size: desc.size,
            usage: desc.usage,
            mapped_at_creation: false,
        };

        let resource = ResourceHandle::Buffer(BufferResource {
            id,
            name,
            desc: buffer_desc,
            transient: desc.transient,
            buffer: std::cell::OnceCell::new(),
        });

        self.resources.insert(id, resource);
        self.compiled_data = None; // Invalidate compilation
        id
    }

    /// Import an external texture (already created)
    pub fn import_texture(
        &mut self,
        name: &'static str,
        texture: wgpu::Texture,
        view: wgpu::TextureView,
    ) -> ResourceId {
        let id = ResourceId(self.next_resource_id);
        self.next_resource_id += 1;

        let resource = ResourceHandle::Texture(TextureResource {
            id,
            name,
            desc: wgpu::TextureDescriptor {
                label: Some(name),
                size: texture.size(),
                mip_level_count: texture.mip_level_count(),
                sample_count: texture.sample_count(),
                dimension: texture.dimension(),
                format: texture.format(),
                usage: texture.usage(),
                view_formats: &[],
            },
            usage: texture.usage(),
            transient: false,
            texture: std::cell::OnceCell::from(texture),
            view: std::cell::OnceCell::from(view),
            sampler: std::cell::OnceCell::new(),
        });

        self.resources.insert(id, resource);
        self.compiled_data = None;
        id
    }

    /// Import an external buffer (already created)
    pub fn import_buffer(&mut self, name: &'static str, buffer: wgpu::Buffer) -> ResourceId {
        let id = ResourceId(self.next_resource_id);
        self.next_resource_id += 1;

        let resource = ResourceHandle::Buffer(BufferResource {
            id,
            name,
            desc: wgpu::BufferDescriptor {
                label: Some(name),
                size: buffer.size(),
                usage: buffer.usage(),
                mapped_at_creation: false,
            },
            transient: false,
            buffer: std::cell::OnceCell::from(buffer),
        });

        self.resources.insert(id, resource);
        self.compiled_data = None;
        id
    }

    /// Add a new pass to the graph
    pub fn add_pass(&mut self, name: &'static str) -> PassBuilder<'_> {
        PassBuilder::new(self, name)
    }

    /// Get a pass by ID
    pub fn get_pass(&self, id: PassId) -> Option<&RenderPass> {
        self.passes.get(&id)
    }

    /// Get a resource by ID
    pub fn get_resource(&self, id: ResourceId) -> Option<&ResourceHandle> {
        self.resources.get(&id)
    }

    /// Compile the graph for execution
    pub fn compile(&mut self) -> Result<CompiledGraph, GraphError> {
        let compiled = GraphCompiler::compile(self)?;

        // Store compiled data
        self.compiled_data = Some(CompiledGraphData {
            order: compiled.execution_order.clone(),
            barriers: compiled.barriers.clone(),
            lifetimes: compiled.resource_lifetimes.clone(),
        });

        Ok(compiled)
    }

    /// Get the compiled execution order (if compiled)
    pub fn execution_order(&self) -> Option<&[PassId]> {
        self.compiled_data.as_ref().map(|d| d.order.as_slice())
    }

    /// Check if the graph is compiled
    pub fn is_compiled(&self) -> bool {
        self.compiled_data.is_some()
    }

    /// Execute the compiled graph for a single view
    pub fn execute_view(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        context: &PassContext<'_>,
        view_rect: Option<ViewportRect>,
    ) -> Result<(), GraphError> {
        let compiled = self
            .compiled_data
            .as_ref()
            .ok_or(GraphError::CycleDetected)?;

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Render Graph"),
        });

        for pass_id in &compiled.order {
            let pass = self
                .passes
                .get(pass_id)
                .ok_or(GraphError::InvalidPass(*pass_id))?;

            // Build color attachments
            let color_attachments: Vec<Option<wgpu::RenderPassColorAttachment>> = pass
                .color_attachments
                .iter()
                .map(|att| {
                    let resource = self.resources.get(&att.resource_id)?;
                    let texture = resource.as_texture()?;
                    let view = texture.get_view(device);

                    Some(wgpu::RenderPassColorAttachment {
                        view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: att.load_op,
                            store: att.store_op,
                        },
                    })
                })
                .collect();

            // Build depth attachment
            let depth_stencil_attachment = pass.depth_stencil_attachment.as_ref().and_then(
                |att| -> Option<wgpu::RenderPassDepthStencilAttachment<'_>> {
                    let resource = self.resources.get(&att.resource_id)?;
                    let texture = resource.as_texture()?;
                    let view = texture.get_view(device);

                    Some(wgpu::RenderPassDepthStencilAttachment {
                        view,
                        depth_ops: Some(wgpu::Operations {
                            load: att.depth_load_op,
                            store: att.depth_store_op,
                        }),
                        stencil_ops: att.stencil_load_op.map(|load_op| wgpu::Operations {
                            load: load_op,
                            store: att.stencil_store_op.unwrap_or(wgpu::StoreOp::Store),
                        }),
                    })
                },
            );

            // Begin render pass
            {
                let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some(pass.name),
                    color_attachments: &color_attachments,
                    depth_stencil_attachment,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });

                // Apply viewport for sub-views
                if let Some(rect) = view_rect {
                    render_pass.set_viewport(
                        rect.x as f32,
                        rect.y as f32,
                        rect.width as f32,
                        rect.height as f32,
                        0.0,
                        1.0,
                    );
                    render_pass.set_scissor_rect(rect.x, rect.y, rect.width, rect.height);
                }

                // Execute pass
                pass.exec.execute(&mut render_pass, context);
            }
        }

        queue.submit(Some(encoder.finish()));
        Ok(())
    }

    /// Execute the compiled graph for multiple views
    pub fn execute_views(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        views: &[ViewExecutionData<'_>],
    ) -> Result<(), GraphError> {
        if views.is_empty() {
            return Ok(());
        }

        let compiled = self
            .compiled_data
            .as_ref()
            .ok_or(GraphError::CycleDetected)?;

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Multi-View Render Graph"),
        });

        for view_data in views {
            for pass_id in &compiled.order {
                let pass = self
                    .passes
                    .get(pass_id)
                    .ok_or(GraphError::InvalidPass(*pass_id))?;

                // Build color attachments with view-specific clear colors
                let color_attachments: Vec<Option<wgpu::RenderPassColorAttachment>> = pass
                    .color_attachments
                    .iter()
                    .map(|att| {
                        let resource = self.resources.get(&att.resource_id)?;
                        let texture = resource.as_texture()?;
                        let view = texture.get_view(device);

                        // For sub-views, use Load instead of Clear after first view
                        let load_op = if view_data.is_main_view {
                            att.load_op
                        } else {
                            wgpu::LoadOp::Load
                        };

                        Some(wgpu::RenderPassColorAttachment {
                            view,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: load_op,
                                store: att.store_op,
                            },
                        })
                    })
                    .collect();

                // Build depth attachment
                let depth_stencil_attachment =
                    pass.depth_stencil_attachment.as_ref().and_then(|att| {
                        let resource = self.resources.get(&att.resource_id)?;
                        let texture = resource.as_texture()?;
                        let view = texture.get_view(device);

                        // For sub-views with Load, preserve depth
                        let depth_load = if view_data.is_main_view {
                            att.depth_load_op
                        } else if att.depth_load_op == wgpu::LoadOp::Clear(1.0) {
                            // Main view clears, sub-views load
                            wgpu::LoadOp::Load
                        } else {
                            att.depth_load_op
                        };

                        Some(wgpu::RenderPassDepthStencilAttachment {
                            view,
                            depth_ops: Some(wgpu::Operations {
                                load: depth_load,
                                store: att.depth_store_op,
                            }),
                            stencil_ops: None,
                        })
                    });

                // Begin render pass
                {
                    let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some(&format!("{} - View", pass.name)),
                        color_attachments: &color_attachments,
                        depth_stencil_attachment,
                        timestamp_writes: None,
                        occlusion_query_set: None,
                    });

                    // Apply viewport for sub-views
                    if let Some(rect) = view_data.rect {
                        render_pass.set_viewport(
                            rect.x as f32,
                            rect.y as f32,
                            rect.width as f32,
                            rect.height as f32,
                            0.0,
                            1.0,
                        );
                        render_pass.set_scissor_rect(rect.x, rect.y, rect.width, rect.height);
                    }

                    // Execute pass with this view's context
                    pass.exec.execute(&mut render_pass, &view_data.context);
                }
            }
        }

        queue.submit(Some(encoder.finish()));
        Ok(())
    }

    /// Get all resources
    pub fn resources(&self) -> &HashMap<ResourceId, ResourceHandle> {
        &self.resources
    }

    /// Get all passes
    pub fn passes(&self) -> &HashMap<PassId, RenderPass> {
        &self.passes
    }

    /// Clear all passes and resources (reset to empty state)
    pub fn clear(&mut self) {
        self.passes.clear();
        self.resources.clear();
        self.compiled_data = None;
        self.next_pass_id = 0;
        self.next_resource_id = 0;
    }
}

/// Data needed to execute a view
pub struct ViewExecutionData<'a> {
    /// Is this the main (full-screen) view
    pub is_main_view: bool,
    /// Viewport rectangle (None for full screen)
    pub rect: Option<ViewportRect>,
    /// Pass context for this view
    pub context: PassContext<'a>,
}
