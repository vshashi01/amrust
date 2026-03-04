//! Pass types and builder for render graph

use super::{PassContext, RenderGraph, RenderPassExecutor, ResourceId};

/// Unique identifier for a pass within a graph
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PassId(pub(crate) usize);

/// A render pass in the graph
pub struct RenderPass {
    pub id: PassId,
    pub name: &'static str,
    pub color_attachments: Vec<ColorAttachment>,
    pub depth_stencil_attachment: Option<DepthStencilAttachment>,
    pub inputs: Vec<ResourceId>,
    pub outputs: Vec<ResourceId>,
    pub exec: Box<dyn RenderPassExecutor>,
}

impl std::fmt::Debug for RenderPass {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RenderPass")
            .field("id", &self.id)
            .field("name", &self.name)
            .field("color_attachments", &self.color_attachments)
            .field("depth_stencil_attachment", &self.depth_stencil_attachment)
            .field("inputs", &self.inputs)
            .field("outputs", &self.outputs)
            .field("exec", &"<dyn RenderPassExecutor>")
            .finish()
    }
}

/// Color attachment configuration
#[derive(Debug, Clone)]
pub struct ColorAttachment {
    pub resource_id: ResourceId,
    pub load_op: wgpu::LoadOp<wgpu::Color>,
    pub store_op: wgpu::StoreOp,
}

/// Depth/stencil attachment configuration
#[derive(Debug, Clone)]
pub struct DepthStencilAttachment {
    pub resource_id: ResourceId,
    pub depth_load_op: wgpu::LoadOp<f32>,
    pub depth_store_op: wgpu::StoreOp,
    pub stencil_load_op: Option<wgpu::LoadOp<u32>>,
    pub stencil_store_op: Option<wgpu::StoreOp>,
    pub read_only: bool,
}

/// Builder for constructing passes
pub struct PassBuilder<'a> {
    graph: &'a mut RenderGraph,
    name: &'static str,
    color_attachments: Vec<ColorAttachment>,
    depth_stencil: Option<DepthStencilAttachment>,
    inputs: Vec<ResourceId>,
    outputs: Vec<ResourceId>,
}

impl<'a> PassBuilder<'a> {
    pub(crate) fn new(graph: &'a mut RenderGraph, name: &'static str) -> Self {
        Self {
            graph,
            name,
            color_attachments: Vec::new(),
            depth_stencil: None,
            inputs: Vec::new(),
            outputs: Vec::new(),
        }
    }

    /// Add a color attachment with clear operation
    pub fn clear_color(mut self, resource_id: ResourceId, color: wgpu::Color) -> Self {
        self.outputs.push(resource_id);
        self.color_attachments.push(ColorAttachment {
            resource_id,
            load_op: wgpu::LoadOp::Clear(color),
            store_op: wgpu::StoreOp::Store,
        });
        self
    }

    /// Add a color attachment with load operation
    pub fn load_color(mut self, resource_id: ResourceId) -> Self {
        self.inputs.push(resource_id);
        self.color_attachments.push(ColorAttachment {
            resource_id,
            load_op: wgpu::LoadOp::Load,
            store_op: wgpu::StoreOp::Store,
        });
        self
    }

    /// Add a depth attachment with clear
    pub fn clear_depth(mut self, resource_id: ResourceId, depth: f32) -> Self {
        self.outputs.push(resource_id);
        self.depth_stencil = Some(DepthStencilAttachment {
            resource_id,
            depth_load_op: wgpu::LoadOp::Clear(depth),
            depth_store_op: wgpu::StoreOp::Store,
            stencil_load_op: None,
            stencil_store_op: None,
            read_only: false,
        });
        self
    }

    /// Add a depth attachment with load (read-only or read-write)
    pub fn load_depth(mut self, resource_id: ResourceId, read_only: bool) -> Self {
        self.inputs.push(resource_id);
        if !read_only {
            self.outputs.push(resource_id);
        }
        self.depth_stencil = Some(DepthStencilAttachment {
            resource_id,
            depth_load_op: wgpu::LoadOp::Load,
            depth_store_op: wgpu::StoreOp::Store,
            stencil_load_op: None,
            stencil_store_op: None,
            read_only,
        });
        self
    }

    /// Mark a resource as read (for non-attachment resources)
    pub fn reads(mut self, resource_id: ResourceId) -> Self {
        self.inputs.push(resource_id);
        self
    }

    /// Mark a resource as written
    pub fn writes(mut self, resource_id: ResourceId) -> Self {
        self.outputs.push(resource_id);
        self
    }

    /// Build with a closure executor
    pub fn execute<F>(self, exec: F) -> PassId
    where
        F: Fn(&mut wgpu::RenderPass<'_>, &PassContext<'_>) + Send + Sync + 'static,
    {
        let id = PassId(self.graph.next_pass_id);
        self.graph.next_pass_id += 1;

        let pass = RenderPass {
            id,
            name: self.name,
            color_attachments: self.color_attachments,
            depth_stencil_attachment: self.depth_stencil,
            inputs: self.inputs,
            outputs: self.outputs,
            exec: Box::new(exec),
        };

        self.graph.passes.insert(id, pass);
        self.graph.compiled_data = None; // Invalidate compilation
        id
    }

    /// Build with a struct executor (for complex passes)
    pub fn execute_with<E: RenderPassExecutor + 'static>(self, exec: E) -> PassId {
        let id = PassId(self.graph.next_pass_id);
        self.graph.next_pass_id += 1;

        let pass = RenderPass {
            id,
            name: self.name,
            color_attachments: self.color_attachments,
            depth_stencil_attachment: self.depth_stencil,
            inputs: self.inputs,
            outputs: self.outputs,
            exec: Box::new(exec),
        };

        self.graph.passes.insert(id, pass);
        self.graph.compiled_data = None; // Invalidate compilation
        id
    }
}
