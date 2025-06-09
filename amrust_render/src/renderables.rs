#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Renderable {
    ColoredMesh(u32),
    TexturedMesh(u32, Vec<(u32, u32)>), // (local binding resource index, slot index)
    Mesh(u32),
    WireframeMesh(u32),
}
