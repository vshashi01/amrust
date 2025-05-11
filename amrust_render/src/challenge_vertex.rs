pub trait Vertex {
    fn desc() -> wgpu::VertexBufferLayout<'static>;
}

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ChallengeVertex {
    pub position: [f32; 3],
    pub color: [f32; 3],
    pub tex_coords: [f32; 2],
}

impl Vertex for ChallengeVertex {
    fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<ChallengeVertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x3,
                    offset: 0,
                    shader_location: 0,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x3,
                    offset: std::mem::size_of::<[f32; 3]>() as wgpu::BufferAddress,
                    shader_location: 1,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x2,
                    offset: std::mem::size_of::<[f32; 6]>() as wgpu::BufferAddress,
                    shader_location: 2,
                },
            ],
        }
    }
}

pub const VERTICES: &[ChallengeVertex] = &[
    ChallengeVertex {
        position: [1.0, 1.0, 0.0],
        color: [0.5, 0.0, 0.5],
        tex_coords: [0.0, 0.0],
    }, // top right corner
    ChallengeVertex {
        position: [-1.0, 1.0, 0.0],
        color: [0.5, 0.0, 0.5],
        tex_coords: [1.0, 0.0],
    }, // top left corner
    ChallengeVertex {
        position: [1.0, -1.0, 0.0],
        color: [0.5, 0.0, 0.5],
        tex_coords: [0.0, 1.0],
    }, // bottom right corner
    ChallengeVertex {
        position: [-1.0, -1.0, 0.0],
        color: [0.5, 0.0, 0.5],
        tex_coords: [1.0, 1.0],
    }, // bottom left corner
];

pub const INDICES: &[u16] = &[0, 1, 2, 1, 3, 2];
