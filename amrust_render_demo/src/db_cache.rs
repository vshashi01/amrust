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

    render_objects: HashMap<PartId, RenderObjectId>,
}

impl DbCache {
    pub fn get_part_data(&self, id: &PartId) -> Option<&PartCache> {
        self.parts_data.get(id)
    }

    pub fn get_part_instance_data(&self, id: &PartInstanceId) -> Option<&PartInstanceCache> {
        self.instance_data.get(id)
    }

    pub fn get_part_data_mut(&mut self, id: &PartId) -> Option<&mut PartCache> {
        self.parts_data.get_mut(id)
    }

    pub fn get_part_instance_data_mut(
        &mut self,
        id: &PartInstanceId,
    ) -> Option<&mut PartInstanceCache> {
        self.instance_data.get_mut(id)
    }

    pub fn get_parts_data(&self) -> &HashMap<PartId, PartCache> {
        &self.parts_data
    }

    pub fn get_part_instances_data(&self) -> &HashMap<PartInstanceId, PartInstanceCache> {
        &self.instance_data
    }

    pub fn insert_part(&mut self, id: PartId, cache: PartCache) {
        self.parts_data.insert(id, cache);
    }

    pub fn insert_part_instance(&mut self, id: PartInstanceId, cache: PartInstanceCache) {
        self.instance_data.insert(id, cache);
    }

    pub fn set_scene_data(&mut self, instances: Vec<PartInstanceId>) {
        self.scene_data = instances;
    }
}

pub struct PartCache {
    pub rep: PartRepCache,
}

pub enum PartRepCache {
    Mesh(MeshCache),
    ComposedPart(ComposedPartCache),
}

pub struct MeshCache {
    pub gpu_mesh_id: RenderMeshId,
    pub bbox: BoundingBox,
    pub vertices_count: usize,
    pub triangles_count: usize,

    //all instances of this Mesh
    pub instances: Vec<PartInstanceId>,
}

pub struct ComposedPartCache {
    pub bbox: BoundingBox,
    pub components: Vec<PartInstanceId>,

    //all instances of this composed part
    pub instances: Vec<PartInstanceId>,
}

pub enum PartRepType {
    Mesh,
    ComposedPart,
}

pub struct PartInstanceCache {
    pub part_id: PartId,
    pub transform: Transformation,
    pub rep_type: PartRepType,
}
