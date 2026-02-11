use std::path::PathBuf;

use amrust_render::renderer;
use glam::Mat4;
use threemf2::io::ThreemfPackage;

pub struct Threemf3DViewController {
    package: Option<ThreemfPackage>,
}

const TEXTURE_WIDTH: u32 = 1366;
const TEXTURE_HEIGHT: u32 = 720;

async fn load_and_render_3mf() {
    let filepath = PathBuf::from("value");
    let file = std::fs::File::open(&filepath).expect("Failed to open 3MF file");
    let package =
        ThreemfPackage::from_reader_with_memory_optimized_deserializer(file, true).unwrap();
    let renderer = renderer::Renderer::from_new_device().await.unwrap();

    let mut object_transform_map = Vec::new();
    package.root.build.item.iter().for_each(|item| {
        let mesh_id = item.objectid;
        let transform = Mat4::IDENTITY;

        object_transform_map.push((mesh_id, transform));
    });
}

#[cfg(test)]
pub mod tests {
    use super::*;

    #[test]
    fn test_load_and_render_3mf() {
        load_and_render_3mf();
    }
}
