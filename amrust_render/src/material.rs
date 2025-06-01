#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct MaterialUniform {
    pub color: [f32; 3], // RGB color
}

pub struct Material {
    pub red: f32,
    pub green: f32,
    pub blue: f32,
}

impl Material {
    pub fn new(red: f32, green: f32, blue: f32) -> Self {
        Self { red, green, blue }
    }

    pub fn create_uniform(&self) -> MaterialUniform {
        MaterialUniform {
            color: [self.red, self.green, self.blue],
        }
    }
}
