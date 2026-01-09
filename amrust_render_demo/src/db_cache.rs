use amrust_render::bounding_box::BoundingBox;

use crate::{
    amrust_db::{PartId, PartInstanceId, Transformation},
    render_db::{RenderMeshId, RenderObjectId},
};

use std::collections::HashMap;

pub struct DbCache {
    parts_data: HashMap<PartId, PartCache>,
    instance_data: HashMap<PartInstanceId, PartInstanceCache>,
    scene_data: Vec<PartInstanceId>,
}

pub struct PartCache {
    rep: PartRepCache,
}

pub enum PartRepCache {
    Mesh(MeshCache),
    ComposedPart(ComposedPartCache),
}

pub struct MeshCache {
    gpu_mesh_id: RenderMeshId,
    bbox: BoundingBox,
    vertices_count: usize,
    triangles_count: usize,
}

pub struct ComposedPartCache {
    gpu_object_id: RenderObjectId,
    bbox: BoundingBox,
    components: Vec<PartInstanceId>,
}

pub struct PartInstanceCache {
    part_id: PartId,
    gpu_object_id: u32,
    transform: Transformation,
}
