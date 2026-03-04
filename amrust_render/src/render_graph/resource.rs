//! Resource types for render graph

use std::cell::OnceCell;

use super::ResourceId;

/// Description for creating a texture resource
#[derive(Debug, Clone)]
pub struct TextureDesc {
    pub format: wgpu::TextureFormat,
    pub size: wgpu::Extent3d,
    pub usage: wgpu::TextureUsages,
    pub sample_count: u32,
    pub transient: bool,
}

impl Default for TextureDesc {
    fn default() -> Self {
        Self {
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            size: wgpu::Extent3d {
                width: 1920,
                height: 1080,
                depth_or_array_layers: 1,
            },
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            sample_count: 1,
            transient: true,
        }
    }
}

/// Description for creating a buffer resource
#[derive(Debug, Clone)]
pub struct BufferDesc {
    pub size: u64,
    pub usage: wgpu::BufferUsages,
    pub transient: bool,
}

impl Default for BufferDesc {
    fn default() -> Self {
        Self {
            size: 1024,
            usage: wgpu::BufferUsages::UNIFORM,
            transient: true,
        }
    }
}

/// Handle to a resource in the graph (either texture or buffer)
#[derive(Debug)]
pub enum ResourceHandle {
    Texture(TextureResource),
    Buffer(BufferResource),
}

/// Texture resource with lazy initialization
#[derive(Debug)]
pub struct TextureResource {
    pub id: ResourceId,
    pub name: &'static str,
    pub desc: wgpu::TextureDescriptor<'static>,
    pub usage: wgpu::TextureUsages,
    pub transient: bool,
    pub texture: OnceCell<wgpu::Texture>,
    pub view: OnceCell<wgpu::TextureView>,
    pub sampler: OnceCell<wgpu::Sampler>,
}

impl TextureResource {
    /// Get or create the texture
    pub fn get_texture(&self, device: &wgpu::Device) -> &wgpu::Texture {
        self.texture
            .get_or_init(|| device.create_texture(&self.desc))
    }

    /// Get or create the texture view
    pub fn get_view(&self, device: &wgpu::Device) -> &wgpu::TextureView {
        self.view.get_or_init(|| {
            let texture = self.get_texture(device);
            texture.create_view(&wgpu::TextureViewDescriptor::default())
        })
    }

    /// Get or create a default sampler
    pub fn get_sampler(&self, device: &wgpu::Device) -> &wgpu::Sampler {
        self.sampler
            .get_or_init(|| device.create_sampler(&wgpu::SamplerDescriptor::default()))
    }

    /// Check if this is a depth/stencil format
    pub fn is_depth_stencil(&self) -> bool {
        self.desc.format.is_depth_stencil_format()
    }
}

/// Buffer resource with lazy initialization
#[derive(Debug)]
pub struct BufferResource {
    pub id: ResourceId,
    pub name: &'static str,
    pub desc: wgpu::BufferDescriptor<'static>,
    pub transient: bool,
    pub buffer: OnceCell<wgpu::Buffer>,
}

impl BufferResource {
    /// Get or create the buffer
    pub fn get_buffer(&self, device: &wgpu::Device) -> &wgpu::Buffer {
        self.buffer.get_or_init(|| device.create_buffer(&self.desc))
    }
}

impl ResourceHandle {
    /// Get as texture reference if this is a texture
    pub fn as_texture(&self) -> Option<&TextureResource> {
        match self {
            ResourceHandle::Texture(t) => Some(t),
            _ => None,
        }
    }

    /// Get as buffer reference if this is a buffer
    pub fn as_buffer(&self) -> Option<&BufferResource> {
        match self {
            ResourceHandle::Buffer(b) => Some(b),
            _ => None,
        }
    }

    /// Check if this is a transient resource
    pub fn is_transient(&self) -> bool {
        match self {
            ResourceHandle::Texture(t) => t.transient,
            ResourceHandle::Buffer(b) => b.transient,
        }
    }

    /// Get the resource name
    pub fn name(&self) -> &'static str {
        match self {
            ResourceHandle::Texture(t) => t.name,
            ResourceHandle::Buffer(b) => b.name,
        }
    }

    /// Get the resource ID
    pub fn id(&self) -> ResourceId {
        match self {
            ResourceHandle::Texture(t) => t.id,
            ResourceHandle::Buffer(b) => b.id,
        }
    }
}
