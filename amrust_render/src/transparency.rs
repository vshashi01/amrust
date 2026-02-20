use std::num::NonZero;

#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Zeroable, bytemuck::Pod)]
pub struct TransparencyUniform {
    pub opacity: f32,
    pub _pad: [f32; 7], // 32-byte alignment to match shader expectations
}

impl TransparencyUniform {
    pub fn new(opacity: f32) -> Self {
        Self {
            opacity: opacity.clamp(0.0, 1.0),
            _pad: [0.0; 7],
        }
    }

    pub fn opaque() -> Self {
        Self::new(1.0)
    }

    pub fn get_size() -> usize {
        std::mem::size_of::<Self>()
    }
}

impl Default for TransparencyUniform {
    fn default() -> Self {
        Self::opaque()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Transparency {
    pub enabled: bool,
    pub z_order: u8,  // 0-255, higher values render last (on top)
    pub opacity: f32, // 0.0 - 1.0
    buffer: wgpu::Buffer,
}

impl Transparency {
    pub fn new(device: &wgpu::Device, enabled: bool, z_order: u8, opacity: f32) -> Self {
        let opacity = opacity.clamp(0.0, 1.0);
        let buffer = create_transparency_buffer(device, opacity);
        Self {
            enabled,
            z_order,
            opacity,
            buffer,
        }
    }

    pub fn opaque(device: &wgpu::Device) -> Self {
        Self::new(device, false, 0, 1.0)
    }

    pub fn transparent(device: &wgpu::Device, z_order: u8, opacity: f32) -> Self {
        Self::new(device, true, z_order, opacity)
    }

    pub fn write_buffer(&self, queue: &wgpu::Queue) {
        queue.write_buffer(
            &self.buffer,
            0,
            bytemuck::cast_slice(&[TransparencyUniform::new(self.opacity)]),
        );
    }

    pub fn get_binding_resource(&self) -> wgpu::BindingResource<'_> {
        self.buffer.as_entire_binding()
    }
}

fn create_transparency_buffer(device: &wgpu::Device, opacity: f32) -> wgpu::Buffer {
    use wgpu::util::DeviceExt;
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Transparency Buffer"),
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        contents: bytemuck::cast_slice(&[TransparencyUniform::new(opacity)]),
    })
}

pub fn create_transparency_bind_group_layout_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: NonZero::new(TransparencyUniform::get_size() as wgpu::BufferAddress), // Allow any size
        },
        count: None,
    }
}

pub fn create_transparency_bind_group_entry<'a>(
    binding: u32,
    resource: wgpu::BindingResource<'a>,
) -> wgpu::BindGroupEntry<'a> {
    wgpu::BindGroupEntry { binding, resource }
}
