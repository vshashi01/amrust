//! Render graph module for amrust_render
//!
//! Provides a declarative API for building multi-pass rendering pipelines
//! with automatic dependency resolution and barrier insertion.

pub mod compiler;
pub mod error;
pub mod executor;
pub mod graph;
pub mod pass;
pub mod resource;

pub use compiler::{CompiledGraph, GraphCompiler};
pub use error::GraphError;
pub use executor::{CompositeUniforms, PassContext, RenderPassExecutor};
pub use graph::{RenderGraph, ViewExecutionData};
pub use pass::{ColorAttachment, DepthStencilAttachment, PassBuilder, PassId, RenderPass};
pub use resource::{BufferDesc, BufferResource, ResourceHandle, TextureDesc, TextureResource};

use std::fmt;

/// Unique identifier for a resource within a graph
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ResourceId(pub(crate) usize);

impl fmt::Display for ResourceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Resource({})", self.0)
    }
}

/// Re-export common types for convenience
pub use wgpu;
