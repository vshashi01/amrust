use bytecheck::CheckBytes;
use rkyv::{Archive, Deserialize, Serialize};

use std::fmt::Debug;

#[derive(Clone, Archive, Serialize, Deserialize, CheckBytes)]
pub struct Mesh {
    pub vertices: Vec<glam::Vec3>,
    pub triangles: Vec<u32>,
}

impl Debug for Mesh {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Mesh")
            .field("vertices:", &self.vertices.len())
            .field("triangles:", &self.triangles.len())
            // .field("bbox:", &self.bbox)
            .finish()
    }
}
