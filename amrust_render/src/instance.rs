use glam::Mat4;

pub struct Instance {
    pub transformation: Mat4,
}

impl Instance {
    pub fn to_raw(&self) -> InstanceRaw {
        InstanceRaw {
            model: self.transformation.to_cols_array_2d(),
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct InstanceRaw {
    model: [[f32; 4]; 4],
    //maybe need normal in the future for shading
    // pub normal: [[f32; 3]; 3],
}

impl InstanceRaw {
    pub fn desc() -> wgpu::VertexBufferLayout<'static> {
        use std::mem;
        wgpu::VertexBufferLayout {
            array_stride: mem::size_of::<InstanceRaw>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 5,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    offset: mem::size_of::<[f32; 4]>() as wgpu::BufferAddress,
                    shader_location: 6,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    offset: mem::size_of::<[f32; 8]>() as wgpu::BufferAddress,
                    shader_location: 7,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    offset: mem::size_of::<[f32; 12]>() as wgpu::BufferAddress,
                    shader_location: 8,
                    format: wgpu::VertexFormat::Float32x4,
                },
                //normal attributes
                // wgpu::VertexAttribute {
                //     format: wgpu::VertexFormat::Float32x3,
                //     offset: mem::size_of::<[f32; 16]>() as wgpu::BufferAddress,
                //     shader_location: 9,
                // },
                // wgpu::VertexAttribute {
                //     format: wgpu::VertexFormat::Float32x3,
                //     offset: mem::size_of::<[f32; 19]>() as wgpu::BufferAddress,
                //     shader_location: 10,
                // },
                // wgpu::VertexAttribute {
                //     format: wgpu::VertexFormat::Float32x3,
                //     offset: mem::size_of::<[f32; 22]>() as wgpu::BufferAddress,
                //     shader_location: 11,
                // },
            ],
        }
    }
}
