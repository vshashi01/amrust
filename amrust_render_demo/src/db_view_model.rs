use amrust_render::bounding_box::BoundingBox;

use crate::{
    amrust_db::{InstanceData, PartId, PartInstanceId, Transformation, add_render_object},
    render_db::{RenderMeshId, RenderObjectId},
};

use crate::amrust_db::Identifiable;
use crate::app_mode::AppMode;
use crate::render_db::RenderDb;
use crate::tree_item_viewer::TreeItem;
use amrust_render::transformation::Transformation as RenderTransformation;
use glam::Mat4;
use smol::lock::RwLock;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct DbViewModel {
    parts_data: HashMap<PartId, PartCache>,
    instance_data: HashMap<PartInstanceId, PartInstanceCache>,
    scene_data: Vec<PartInstanceId>,

    scene_based_render_objects: HashMap<PartId, (RenderObjectId, RenderObjectId)>,
    unique_parts_based_render_objects: HashMap<PartId, (RenderObjectId, RenderObjectId)>,

    detached_parts: HashSet<PartId>,
    detached_part_instances: HashSet<PartInstanceId>,
}

impl DbViewModel {
    pub fn new() -> Self {
        Self {
            parts_data: HashMap::new(),
            instance_data: HashMap::new(),
            scene_data: Vec::new(),
            scene_based_render_objects: HashMap::new(),
            unique_parts_based_render_objects: HashMap::new(),
            detached_parts: HashSet::new(),
            detached_part_instances: HashSet::new(),
        }
    }

    pub fn clear(&mut self) {
        self.parts_data.clear();
        self.instance_data.clear();
        self.scene_data.clear();
        self.scene_based_render_objects.clear();
        self.unique_parts_based_render_objects.clear();
        self.detached_parts.clear();
        self.detached_part_instances.clear();
    }

    pub fn is_empty(&self) -> bool {
        self.parts_data.is_empty()
    }

    pub fn get_part_data(&self, id: &PartId) -> Option<&PartCache> {
        self.parts_data.get(id)
    }

    pub fn get_part_instance_data(&self, id: &PartInstanceId) -> Option<&PartInstanceCache> {
        self.instance_data.get(id)
    }

    // pub fn get_part_data_mut(&mut self, id: &PartId) -> Option<&mut PartCache> {
    //     self.parts_data.get_mut(id)
    // }

    // pub fn get_part_instance_data_mut(
    //     &mut self,
    //     id: &PartInstanceId,
    // ) -> Option<&mut PartInstanceCache> {
    //     self.instance_data.get_mut(id)
    // }

    // pub fn get_parts_data(&self) -> &HashMap<PartId, PartCache> {
    //     &self.parts_data
    // }

    pub fn get_part_instances_data(&self) -> &HashMap<PartInstanceId, PartInstanceCache> {
        &self.instance_data
    }

    pub fn insert_part(&mut self, id: PartId, cache: PartCache) {
        self.parts_data.insert(id, cache);
    }

    pub fn add_detached_part(&mut self, id: PartId) {
        self.detached_parts.insert(id);
    }

    pub fn remove_detached_part(&mut self, id: &PartId) {
        self.detached_parts.remove(id);
    }

    pub fn insert_part_instance(&mut self, id: PartInstanceId, cache: PartInstanceCache) {
        self.instance_data.insert(id, cache);
    }

    pub fn remove_detached_part_instance(&mut self, id: &PartInstanceId) {
        self.detached_part_instances.remove(id);
    }

    pub fn add_detached_part_instance(&mut self, id: PartInstanceId) {
        self.detached_part_instances.insert(id);
    }

    pub fn set_scene_data(&mut self, instances: Vec<PartInstanceId>) {
        self.scene_data = instances;
    }

    pub fn get_detached_identifiables(&self) -> impl Iterator<Item = Identifiable> {
        self.detached_parts
            .iter()
            .map(|id| Identifiable::Part(*id))
            .chain(
                self.detached_part_instances
                    .iter()
                    .map(|id| Identifiable::PartInstance(*id)),
            )
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

    pub fn get_scene_based_render_object_ids(&self) -> impl Iterator<Item = &RenderObjectId> {
        self.scene_based_render_objects
            .values()
            .flat_map(|(mesh, wireframe)| [mesh, wireframe])
    }

    pub fn get_unique_parts_based_render_object_ids(
        &self,
    ) -> impl Iterator<Item = &RenderObjectId> {
        self.unique_parts_based_render_objects
            .values()
            .flat_map(|(mesh, wireframe)| [mesh, wireframe])
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
                && let Some(part_cache) = self.get_part_data(&mesh_part_id)
                && let PartRepCache::Mesh(mesh_cache) = &part_cache.rep
            {
                let instance_data = InstanceData {
                    gpu_mesh_id: mesh_cache.gpu_mesh_id,
                    transforms,
                };
                let render_object_id = add_render_object(device, render_db.clone(), &instance_data);
                self.unique_parts_based_render_objects
                    .insert(mesh_part_id, render_object_id);
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

pub fn get_total_bbox_from_cache(db_cache: &DbViewModel, mode: AppMode) -> BoundingBox {
    let mut total_bbox = BoundingBox::default();
    match mode {
        AppMode::Objects => {
            // All parts visible: compute bbox for each part
            for part_id in db_cache.parts_data.keys() {
                let part_bbox = compute_part_bbox(db_cache, part_id);
                total_bbox.unite(&part_bbox);
            }
        }
        AppMode::Build => {
            // Scene instances visible: compute bbox for each part in scene, transformed
            for instance_id in &db_cache.scene_data {
                if let Some(instance_cache) = db_cache.get_part_instance_data(instance_id) {
                    let mut part_bbox = compute_part_bbox(db_cache, &instance_cache.part_id);
                    part_bbox.transform(&RenderTransformation(instance_cache.transform.0));
                    total_bbox.unite(&part_bbox);
                }
            }
        }
    }
    total_bbox
}

fn compute_part_bbox(db_cache: &DbViewModel, part_id: &PartId) -> BoundingBox {
    if let Some(part_cache) = db_cache.get_part_data(part_id) {
        match &part_cache.rep {
            PartRepCache::Mesh(mesh_cache) => mesh_cache.bbox,
            PartRepCache::ComposedPart(composed_cache) => {
                let mut bbox = BoundingBox::default();
                for component_id in &composed_cache.components {
                    if let Some(instance_cache) = db_cache.get_part_instance_data(component_id) {
                        let mut component_bbox =
                            compute_part_bbox(db_cache, &instance_cache.part_id);
                        component_bbox.transform(&RenderTransformation(instance_cache.transform.0));
                        bbox.unite(&component_bbox);
                    }
                }
                bbox
            }
        }
    } else {
        BoundingBox::default()
    }
}

pub fn create_objects_list_from_cache(db_cache: &DbViewModel) -> Vec<TreeItem<Identifiable>> {
    db_cache
        .parts_data
        .iter()
        .map(|(part_id, part_cache)| build_part_tree(db_cache, *part_id, part_cache))
        .collect()
}

fn build_part_tree(
    db_cache: &DbViewModel,
    part_id: PartId,
    part_cache: &PartCache,
) -> TreeItem<Identifiable> {
    match &part_cache.rep {
        PartRepCache::Mesh(_) => TreeItem::Leaf {
            id: Identifiable::Part(part_id),
            name: format!("Mesh Object: {:?}", part_id),
            selectable: true,
        },
        PartRepCache::ComposedPart(composed_cache) => {
            let childs = composed_cache
                .components
                .iter()
                .filter_map(|component_id| {
                    db_cache
                        .get_part_instance_data(component_id)
                        .and_then(|instance_cache| {
                            db_cache
                                .get_part_data(&instance_cache.part_id)
                                .map(|comp_part_cache| {
                                    build_part_tree(
                                        db_cache,
                                        instance_cache.part_id,
                                        comp_part_cache,
                                    )
                                })
                        })
                })
                .collect();
            TreeItem::Node {
                id: Identifiable::Part(part_id),
                name: format!("Components Object: {:?}", part_id),
                childs,
                selectable: true,
            }
        }
    }
}

pub fn create_build_items_list_from_cache(db_cache: &DbViewModel) -> Vec<TreeItem<Identifiable>> {
    db_cache
        .scene_data
        .iter()
        .filter_map(|instance_id| {
            db_cache
                .get_part_instance_data(instance_id)
                .map(|instance_cache| {
                    let name = match instance_cache.rep_type {
                        PartRepType::Mesh => format!("Instance: {:?} - Mesh", instance_id),
                        PartRepType::ComposedPart => {
                            format!("Instance: {:?} - Composed Part", instance_id)
                        }
                    };
                    TreeItem::Leaf {
                        id: Identifiable::PartInstance(*instance_id),
                        name,
                        selectable: true,
                    }
                })
        })
        .collect()
}

pub fn create_scene_tree_items_by_unique_parts_from_cache(
    db_cache: &DbViewModel,
) -> Vec<TreeItem<Identifiable>> {
    let mut instance_id_to_tree_item_map: HashMap<PartInstanceId, TreeItem<Identifiable>> =
        HashMap::new();
    let mut unprocessed_instances = db_cache.scene_data.clone();

    // Pass 1: process ALL Mesh parts first
    for instance_id in &db_cache.scene_data {
        if let Some(instance_cache) = db_cache.get_part_instance_data(instance_id)
            && matches!(instance_cache.rep_type, PartRepType::Mesh)
        {
            instance_id_to_tree_item_map.insert(
                *instance_id,
                TreeItem::Leaf {
                    id: Identifiable::PartInstance(*instance_id),
                    name: format!("Mesh: {:?}", instance_id),
                    selectable: true,
                },
            );
            unprocessed_instances.retain(|id| id != instance_id);
        }
    }

    // Pass 2: process ComposedPart where children might already be ready
    while !unprocessed_instances.is_empty() {
        let mut processed = vec![];
        for instance_id in &unprocessed_instances {
            if let Some(instance_cache) = db_cache.get_part_instance_data(instance_id)
                && matches!(instance_cache.rep_type, PartRepType::ComposedPart)
                && let Some(part_cache) = db_cache.get_part_data(&instance_cache.part_id)
                && let PartRepCache::ComposedPart(composed_cache) = &part_cache.rep
            {
                let mut tree_items = vec![];
                let mut all_children_ready = true;
                for child_id in &composed_cache.components {
                    if let Some(item) = instance_id_to_tree_item_map.get(child_id) {
                        tree_items.push(item.clone());
                    } else {
                        all_children_ready = false;
                        break;
                    }
                }
                if all_children_ready {
                    instance_id_to_tree_item_map.insert(
                        *instance_id,
                        TreeItem::Node {
                            id: Identifiable::PartInstance(*instance_id),
                            name: format!("Composed: {:?}", instance_id),
                            childs: tree_items,
                            selectable: true,
                        },
                    );
                    processed.push(*instance_id);
                }
            }
        }
        unprocessed_instances.retain(|i| !processed.contains(i));
        // Prevent infinite loop if dependencies are circular (though shouldn't happen)
        if processed.is_empty() {
            break;
        }
    }

    // Collect top-level items (those not children of others)
    let child_ids: HashSet<_> = db_cache
        .parts_data
        .values()
        .filter_map(|pc| {
            if let PartRepCache::ComposedPart(cc) = &pc.rep {
                Some(&cc.components)
            } else {
                None
            }
        })
        .flatten()
        .collect();
    db_cache
        .scene_data
        .iter()
        .filter(|id| !child_ids.contains(id))
        .filter_map(|id| instance_id_to_tree_item_map.get(id).cloned())
        .collect()
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
