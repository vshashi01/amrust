//! Execution context and traits for render graph

use std::collections::HashMap;

use crate::RenderData3d;
use crate::renderer::FrameViewData;

/// Uniform data for composite/silhouette effects
#[derive(Debug, Default, Clone, Copy)]
pub struct CompositeUniforms {
    /// Pixel thickness for silhouette highlighting
    pub highlight_pixel_size: i32,
}

impl CompositeUniforms {
    pub fn new(highlight_pixel_size: i32) -> Self {
        Self {
            highlight_pixel_size,
        }
    }
}

/// Execution context provided to each pass during rendering
pub struct PassContext<'a> {
    /// The wgpu device
    pub device: &'a wgpu::Device,

    /// The wgpu queue
    pub queue: &'a wgpu::Queue,

    /// Pipeline cache for accessing pre-built pipelines
    pub pipeline_cache: &'a HashMap<&'static str, wgpu::RenderPipeline>,

    /// Per-frame/view data (camera, lights, etc.)
    pub frame_view_data: &'a FrameViewData,

    /// The renderables to draw in this pass
    pub renderables: &'a [RenderData3d<'a>],

    /// Global bind groups available to all passes
    pub global_bind_groups: &'a [wgpu::BindGroup],

    /// Composite uniform data (for effects like silhouette) //ToDo: Make it PushConstants in future
    pub composite_uniforms: CompositeUniforms,
}

/// Trait for pass execution - implemented by closures or structs
pub trait RenderPassExecutor: Send + Sync {
    /// Execute the pass
    fn execute(&self, render_pass: &mut wgpu::RenderPass<'_>, context: &PassContext<'_>);
}

/// Implementation for closures
impl<F> RenderPassExecutor for F
where
    F: Fn(&mut wgpu::RenderPass<'_>, &PassContext<'_>) + Send + Sync,
{
    fn execute(&self, render_pass: &mut wgpu::RenderPass<'_>, context: &PassContext<'_>) {
        (self)(render_pass, context);
    }
}

/// Implementation for boxed executors
impl RenderPassExecutor for Box<dyn RenderPassExecutor> {
    fn execute(&self, render_pass: &mut wgpu::RenderPass<'_>, context: &PassContext<'_>) {
        (**self).execute(render_pass, context);
    }
}
