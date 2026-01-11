use amrust_render::bounding_box::BoundingBox;

use crate::{
    amrust_db::{InstanceData, PartId, PartInstanceId, Transformation, add_render_object},
    render_db::{RenderMeshId, RenderObjectId},
};

use crate::render_db::RenderDb;
use glam::Mat4;
use smol::lock::RwLock;
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct DbCache {
    parts_data: HashMap<PartId, PartCache>,
    instance_data: HashMap<PartInstanceId, PartInstanceCache>,
    scene_data: Vec<PartInstanceId>,

    scene_based_render_objects: HashMap<PartId, RenderObjectId>,
    unique_parts_based_render_objects: HashMap<PartId, RenderObjectId>,
}

impl DbCache {
    pub fn new() -> Self {
        Self {
            parts_data: HashMap::new(),
            instance_data: HashMap::new(),
            scene_data: Vec::new(),
            scene_based_render_objects: HashMap::new(),
            unique_parts_based_render_objects: HashMap::new(),
        }
    }

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

    pub fn update_scene_based_render_objects(
        &mut self,
        device: &wgpu::Device,
        render_db: Arc<RwLock<RenderDb>>,
    ) {
        let mut part_id_to_instance_data = HashMap::<PartId, InstanceData>::new();

        for instance_id in &self.scene_data {
            if let Some(instance_cache) = self.get_part_instance_data(instance_id) {
                self.process_part_instance_recursive(
                    &mut part_id_to_instance_data,
                    instance_cache,
                    &Transformation(Mat4::IDENTITY),
                );
            }
        }

        for (part_id, instance_data) in part_id_to_instance_data {
            let render_object_id = add_render_object(device, render_db.clone(), &instance_data);
            self.scene_based_render_objects
                .insert(part_id, render_object_id);
        }
    }

    fn process_part_instance_recursive(
        &self,
        part_id_to_instance_data: &mut HashMap<PartId, InstanceData>,
        instance_cache: &PartInstanceCache,
        parent_transform: &Transformation,
    ) {
        let combined_transform = Transformation(parent_transform.0 * instance_cache.transform.0);
        if let Some(part_cache) = self.get_part_data(&instance_cache.part_id) {
            match &part_cache.rep {
                PartRepCache::Mesh(mesh_cache) => {
                    part_id_to_instance_data
                        .entry(instance_cache.part_id)
                        .or_insert(InstanceData {
                            gpu_mesh_id: mesh_cache.gpu_mesh_id,
                            transforms: vec![],
                        })
                        .transforms
                        .push(combined_transform);
                }
                PartRepCache::ComposedPart(composed_cache) => {
                    for component_id in &composed_cache.components {
                        if let Some(child_cache) = self.get_part_instance_data(component_id) {
                            self.process_part_instance_recursive(
                                part_id_to_instance_data,
                                child_cache,
                                &combined_transform,
                            );
                        }
                    }
                }
            }
        }
    }

    pub fn update_unique_parts_based_render_objects(
        &mut self,
        device: &wgpu::Device,
        render_db: Arc<RwLock<RenderDb>>,
    ) {
        let mut mesh_to_transforms = HashMap::<PartId, Vec<Transformation>>::new();

        for (part_id, part_cache) in &self.parts_data {
            self.process_part_for_unique_render(
                &mut mesh_to_transforms,
                part_id,
                part_cache,
                &Transformation(Mat4::IDENTITY),
            );
        }

        for (mesh_part_id, transforms) in mesh_to_transforms {
            if !self
                .unique_parts_based_render_objects
                .contains_key(&mesh_part_id)
            {
                if let Some(part_cache) = self.get_part_data(&mesh_part_id) {
                    if let PartRepCache::Mesh(mesh_cache) = &part_cache.rep {
                        let instance_data = InstanceData {
                            gpu_mesh_id: mesh_cache.gpu_mesh_id,
                            transforms,
                        };
                        let render_object_id =
                            add_render_object(device, render_db.clone(), &instance_data);
                        self.unique_parts_based_render_objects
                            .insert(mesh_part_id, render_object_id);
                    }
                }
            }
        }
    }

    fn process_part_for_unique_render(
        &self,
        mesh_to_transforms: &mut HashMap<PartId, Vec<Transformation>>,
        part_id: &PartId,
        part_cache: &PartCache,
        parent_transform: &Transformation,
    ) {
        match &part_cache.rep {
            PartRepCache::Mesh(_) => {
                // For meshes not in composed parts, always use identity; otherwise, use combined
                let transform_to_use = if parent_transform == &Transformation(Mat4::IDENTITY) {
                    Transformation(Mat4::IDENTITY)
                } else {
                    *parent_transform
                };
                mesh_to_transforms
                    .entry(*part_id)
                    .or_insert(vec![])
                    .push(transform_to_use);
            }
            PartRepCache::ComposedPart(composed_cache) => {
                for component_id in &composed_cache.components {
                    if let Some(instance_cache) = self.get_part_instance_data(component_id) {
                        let combined_transform =
                            Transformation(parent_transform.0 * instance_cache.transform.0);
                        if let Some(component_part_cache) =
                            self.get_part_data(&instance_cache.part_id)
                        {
                            self.process_part_for_unique_render(
                                mesh_to_transforms,
                                &instance_cache.part_id,
                                component_part_cache,
                                &combined_transform,
                            );
                        }
                    }
                }
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct PartCache {
    pub rep: PartRepCache,
}

#[derive(Debug, Clone)]
pub enum PartRepCache {
    Mesh(MeshCache),
    ComposedPart(ComposedPartCache),
}

#[derive(Debug, Clone)]
pub struct MeshCache {
    pub gpu_mesh_id: RenderMeshId,
    pub bbox: BoundingBox,
    pub vertices_count: usize,
    pub triangles_count: usize,

    //all instances of this Mesh
    pub instances: Vec<PartInstanceId>,
}

#[derive(Debug, Clone)]
pub struct ComposedPartCache {
    pub components: Vec<PartInstanceId>,

    //all instances of this composed part
    pub instances: Vec<PartInstanceId>,
}

#[derive(Debug, Clone)]
pub enum PartRepType {
    Mesh,
    ComposedPart,
}

#[derive(Debug, Clone)]
pub struct PartInstanceCache {
    pub part_id: PartId,
    pub transform: Transformation,
    pub rep_type: PartRepType,
}
