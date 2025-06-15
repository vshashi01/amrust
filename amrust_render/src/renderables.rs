#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Renderable {
    ColoredMesh(u32),
    TexturedMesh(u32, Vec<(u32, u32)>), // (local binding resource index, slot index)
    ArrayTexturedMesh(u32, Vec<(u32, u32)>),
    Mesh(u32),
    WireframeMesh(u32),
}
