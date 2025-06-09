use crate::{instance::GpuInstance, renderables::Renderable};

pub struct RenderObject {
    pub renderable: Renderable,
    pub instance: GpuInstance,
}
