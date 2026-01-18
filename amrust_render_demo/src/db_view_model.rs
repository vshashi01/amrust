use anyhow::Result;
use glam::{Mat4, Vec3};
use smol::lock::RwLock;

use amrust_render::{
    bounding_box::BoundingBox, gpu_mesh::MeshBuilder, instance::InstanceDataBuilder,
    material::Material, transformation::TransformationData, vertex::Position,
};

use crate::{
    core::{
        amrust_db::{Db, EntityChanges},
        app_mode::AppMode,
        interfaces::db_view::DbView,
        render_db::{RenderMeshId, RenderObject, RenderObjectId},
        types::{
            identifiable::Identifiable,
            mesh::Mesh,
            part::{Part, PartId, PartRep},
            part_instance::PartInstanceId,
            part_rep_type::PartRepType,
            transformation::Transformation,
        },
    },
    ui::tree_item_viewer::TreeItem,
};

use crate::core::render_db::RenderDb;
use amrust_render::transformation::Transformation as RenderTransformation;

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

    selected_identifiables: HashSet<Identifiable>,
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
            selected_identifiables: HashSet::new(),
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

    // pub fn get_part_instances_data(&self) -> &HashMap<PartInstanceId, PartInstanceCache> {
    //     &self.instance_data
    // }

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

    pub fn add_selected_identifiable(&mut self, identifiable: Identifiable) {
        self.selected_identifiables.insert(identifiable);
    }

    // pub fn remove_selected_identifiable(&mut self, identifiable: &Identifiable) {
    //     self.selected_identifiables.remove(identifiable);
    // }

    pub fn clear_selected_identifiables(&mut self) {
        self.selected_identifiables.clear();
    }

    pub fn get_all_identifiables(&self) -> impl Iterator<Item = Identifiable> {
        self.parts_data
            .keys()
            .map(|id| Identifiable::Part(*id))
            .chain(
                self.instance_data
                    .keys()
                    .map(|id| Identifiable::PartInstance(*id)),
            )
    }

    pub fn get_all_operable_selected_identifiables(&self) -> impl Iterator<Item = Identifiable> {
        self.get_all_operable_identifiables()
            .filter(|identifiable| self.selected_identifiables.contains(identifiable))
    }

    //returns identifiables that are not operable currently
    pub fn get_all_operable_identifiables(&self) -> impl Iterator<Item = Identifiable> {
        self.get_all_identifiables()
            .filter(|identifiable| match identifiable {
                Identifiable::Part(id) => !self.detached_parts.contains(id),
                Identifiable::PartInstance(id) => !self.detached_part_instances.contains(id),
            })
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

pub fn create_objects_list(db_cache: &DbViewModel) -> Vec<TreeItem<Identifiable>> {
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

pub fn create_build_items_list(db_cache: &DbViewModel) -> Vec<TreeItem<Identifiable>> {
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

pub fn create_scene_tree_items_by_unique_parts(
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

pub fn create_object_tree_from_identifiable(
    db: &DbViewModel,
    identifiable: &Identifiable,
) -> Option<TreeItem<String>> {
    match identifiable {
        Identifiable::Part(part_id) => create_object_tree_from_part(db, part_id),
        Identifiable::PartInstance(part_instance_id) => {
            create_object_tree_from_instance(db, part_instance_id)
        }
    }
}

pub fn create_object_tree_from_part(db: &DbViewModel, id: &PartId) -> Option<TreeItem<String>> {
    if let Some(data) = db.get_part_data(id) {
        let item = match &data.rep {
            PartRepCache::Mesh(mesh) => {
                let vertices_item = TreeItem::Leaf {
                    id: format!("{}_VerticesCount", id),
                    name: format!("Vertices Count: {:?}", mesh.vertices_count),
                    selectable: false,
                };
                let triangles_item = TreeItem::Leaf {
                    id: format!("{}_TrianglesCount", id),
                    name: format!("Triangles Count: {:?}", mesh.triangles_count),
                    selectable: false,
                };

                TreeItem::InertNode {
                    id: format!("{}", id),
                    name: "Mesh".to_owned(),
                    childs: vec![vertices_item, triangles_item],
                }
            }
            PartRepCache::ComposedPart(cache) => {
                let mut map: HashMap<&PartInstanceId, TreeItem<String>> = HashMap::new();
                let mut childs = vec![];
                for id in &cache.components {
                    if let Some(item) = map.get(id) {
                        childs.push(item.clone());
                    } else if let Some(item) = create_object_tree_from_instance(db, id) {
                        map.insert(id, item.clone());
                        childs.push(item);
                    }
                }

                TreeItem::Node {
                    id: format!("{}", id),
                    name: format!("Composed Part - {:?}", id),
                    childs,
                    selectable: false,
                }
            }
        };

        return Some(item);
    }

    None
}

pub fn create_object_tree_from_instance(
    db: &DbViewModel,
    instance_id: &PartInstanceId,
) -> Option<TreeItem<String>> {
    if let Some(data) = db.get_part_instance_data(instance_id)
        && let Some(object_tree) = create_object_tree_from_part(db, &data.part_id)
    {
        let transform_item = TreeItem::Leaf {
            id: format!("{}_Transform", instance_id),
            name: format!("Transform - {:?}", data.transform),
            selectable: false,
        };

        return Some(TreeItem::Node {
            id: format!("{}", instance_id),
            name: format!("Instance - {:?}", instance_id),
            childs: vec![object_tree, transform_item],
            selectable: false,
        });
    }

    None
}

impl DbView for DbViewModel {
    fn get_total_visible_bbox(&self, app_mode: AppMode) -> BoundingBox {
        get_total_bbox_from_cache(self, app_mode)
    }

    fn get_all_operable_selected_identifiables(&self) -> Vec<Identifiable> {
        self.get_all_operable_selected_identifiables().collect()
    }

    fn is_database_empty(&self) -> bool {
        self.is_empty()
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
    //ToDo: Re introduce instances when it is used.
    // pub instances: Vec<PartInstanceId>,
}

#[derive(Debug, Clone)]
pub struct ComposedPartCache {
    pub components: Vec<PartInstanceId>,
    //all instances of this composed part
    //ToDo: Re introduce instances when it is used.
    // pub instances: Vec<PartInstanceId>,
}

#[derive(Debug, Clone)]
pub struct PartInstanceCache {
    pub part_id: PartId,
    pub transform: Transformation,
    pub rep_type: PartRepType,
}

pub struct InstanceData {
    pub gpu_mesh_id: RenderMeshId,
    pub transforms: Vec<Transformation>,
}

pub async fn update_view_model_from_db(
    db: Arc<RwLock<Db>>,
    render_db: Arc<RwLock<RenderDb>>,
    mut cache: DbViewModel,
    device: Arc<wgpu::Device>,
) -> Result<DbViewModel> {
    let mut new_part_caches = vec![];
    let mut new_part_instance_caches = vec![];

    {
        let read_db = db.read().await;

        if read_db.is_empty() {
            cache.clear();
        } else {
            let mut parts_that_require_new_render_objects = HashSet::new();
            for (id, change) in read_db.get_changed_part_instances() {
                //check if part instance is detached if yes push in detached and dont update cache
                if matches!(change, EntityChanges::Detached) {
                    cache.add_detached_part_instance(*id);

                    if let Some(instance_data) = cache.get_part_instance_data(id)
                        && let Some(lala) = read_db.get_changed_parts().get(&instance_data.part_id)
                        && matches!(lala, EntityChanges::Detached)
                    {
                        cache.add_detached_part(instance_data.part_id);
                    }
                } else {
                    //create new part instance cache
                    if let Ok(instance) = read_db.get_part_instance_data(id) {
                        // remove from detached (if existed) since it now back in the main db
                        cache.remove_detached_part_instance(id);

                        if let Some(change) = read_db.get_changed_parts().get(&instance.part_id)
                            && matches!(change, EntityChanges::Detached)
                        {
                            cache.add_detached_part(instance.part_id);
                        } else if let Ok(part) = read_db.get_part_data(&instance.part_id) {
                            // remove from detached (if existed) since it now back in the main db
                            cache.remove_detached_part(&instance.part_id);

                            let rep_type = match &part.get_rep() {
                                PartRep::Mesh(_) => PartRepType::Mesh,
                                PartRep::ComposedPart(_) => PartRepType::ComposedPart,
                            };
                            parts_that_require_new_render_objects.insert(instance.part_id);
                            new_part_instance_caches.push((
                                *id,
                                PartInstanceCache {
                                    part_id: instance.part_id,
                                    transform: instance.transform,
                                    rep_type,
                                },
                            ));
                        }
                    }
                }
            }

            for (id, instance_cache) in new_part_instance_caches {
                cache.insert_part_instance(id, instance_cache);
            }

            let mut parts_to_be_processed = read_db
                .get_changed_parts()
                .keys()
                .copied()
                .collect::<Vec<_>>();
            loop {
                if parts_to_be_processed.is_empty() {
                    break;
                }

                for (id, change) in read_db.get_changed_parts() {
                    match change {
                        EntityChanges::Added | EntityChanges::Reattached => {
                            parts_that_require_new_render_objects.insert(*id);

                            //create new part cache
                            if let Ok(part) = read_db.get_part_data(id)
                                && let Some(part_cache) =
                                    create_part_cache(part, &device, render_db.clone())
                            {
                                new_part_caches.push((*id, part_cache));
                            }
                        }
                        EntityChanges::Removed => todo!(),
                        EntityChanges::Detached => {
                            cache.add_detached_part(*id);
                        }
                    }

                    if let Some(pos) = parts_to_be_processed
                        .iter()
                        .position(|to_be_processed_id| to_be_processed_id == id)
                    {
                        parts_to_be_processed.swap_remove(pos);
                    }
                }
            }

            for (id, part_cache) in new_part_caches {
                cache.insert_part(id, part_cache);
            }

            // Update scene data if instances changed
            if !read_db.get_changed_part_instances().is_empty()
                && let Ok(scene) = read_db.get_scene()
            {
                cache.set_scene_data(scene.instances.clone());
            }
        }
    }

    // this is dangerous because the moment we release the read lock before changes could have happened?
    {
        let mut write_db = db.write().await;
        write_db.clear_changed_part_instances();
        write_db.clear_changed_parts();
    }

    cache.update_scene_based_render_objects(&device, render_db.clone());
    cache.update_unique_parts_based_render_objects(&device, render_db.clone());

    Ok(cache)
}

fn create_part_cache(
    part: &Part,
    device: &wgpu::Device,
    render_db: Arc<RwLock<RenderDb>>,
) -> Option<PartCache> {
    match &part.get_rep() {
        PartRep::Mesh(mesh) => {
            let gpu_mesh_id = create_mesh_gpu_data(device, render_db.clone(), mesh);
            let bbox =
                compute_transformed_bounding_box_from_mesh(mesh, &Transformation(Mat4::IDENTITY));
            let vertices_count = mesh.vertices.len();
            let triangles_count = mesh.triangles.len() / 3;
            Some(PartCache {
                rep: PartRepCache::Mesh(MeshCache {
                    gpu_mesh_id,
                    bbox,
                    vertices_count,
                    triangles_count,
                }),
            })
        }
        PartRep::ComposedPart(components) => Some(PartCache {
            rep: PartRepCache::ComposedPart(ComposedPartCache {
                components: components.clone(),
            }),
        }),
    }
}

// pub fn create_build_items_list(
//     read_db: &DbViewModel,
// ) -> Result<Vec<TreeItem<Identifiable>>, DbError> {
//     let scene = read_db.get_scene()?;

//     let mut tree_items = vec![];
//     for i in &scene.instances {
//         let part = read_db.get_part_data_from_part_instance(i)?;
//         let instance_data = read_db.get_part_instance_data(i)?;
//         let name = match &part.rep {
//             PartRep::Mesh(_) => format!("Instance: {:?} - Mesh: {:?}", i, instance_data.part_id),
//             PartRep::ComposedPart(_) => format!(
//                 "Instance: {:?} - Composed Part: {:?}",
//                 i, instance_data.part_id
//             ),
//         };

//         let item = TreeItem::Leaf {
//             id: Identifiable::PartInstance(*i),
//             name,
//             selectable: true,
//         };

//         tree_items.push(item);
//     }

//     Ok(tree_items)
// }

fn create_mesh_gpu_data(
    device: &wgpu::Device,
    render_db: Arc<RwLock<RenderDb>>,
    mesh: &Mesh,
) -> RenderMeshId {
    let positions = convert_vertices_to_position(&mesh.vertices);
    let indices = mesh.triangles.clone();
    let color = convert_vertices_to_color(&mesh.vertices);
    let wireframe_indices = convert_triangle_indices_to_wireframe_indices(&mesh.triangles);

    let gpu_mesh = MeshBuilder::new()
        .add_vertex_stream(positions.as_slice())
        .add_vertex_stream(color.as_slice())
        .add_mesh_index_stream(indices.as_slice())
        .add_wireframe_index_stream(wireframe_indices.as_slice())
        .build(device);

    let mut render_db = render_db.write_blocking();
    render_db.add_mesh(gpu_mesh)
}

fn compute_transformed_bounding_box_from_mesh(
    mesh: &Mesh,
    transform: &Transformation,
) -> BoundingBox {
    let mut bbox = BoundingBox::default();
    for v in &mesh.vertices {
        let v4 = transform.0 * v.extend(1.0);
        let transformed = Vec3::new(v4.x, v4.y, v4.z);
        bbox.expand_to_include(&transformed);
    }
    bbox
}

fn convert_points_vec_to_position(points: &[Vec3]) -> Vec<Position> {
    points.iter().map(|p| Position([p.x, p.y, p.z])).collect()
}

fn convert_vertices_to_position(vertices: &[Vec3]) -> Vec<Position> {
    vertices.iter().map(|v| Position([v.x, v.y, v.z])).collect()
}

fn convert_vertices_to_color(vertices: &[Vec3]) -> Vec<amrust_render::vertex::Color> {
    vertices
        .iter()
        .map(|_| amrust_render::vertex::Color([0.5, 0.5, 0.5]))
        .collect()
}

fn convert_triangle_indices_to_wireframe_indices(triangles: &[u32]) -> Vec<u32> {
    let mut indices = Vec::new();

    for triangle in triangles.chunks(3) {
        if triangle.len() == 3 {
            let v1 = triangle[0];
            let v2 = triangle[1];
            let v3 = triangle[2];
            indices.push(v1);
            indices.push(v2);
            indices.push(v2);
            indices.push(v3);
            indices.push(v3);
            indices.push(v1);
        }
    }
    indices
}

pub fn add_render_object(
    device: &wgpu::Device,
    render_db: Arc<RwLock<RenderDb>>,
    data: &InstanceData,
    //mesh and wireframe
) -> (RenderObjectId, RenderObjectId) {
    let transformation_data: Vec<TransformationData> = data
        .transforms
        .iter()
        .map(get_transformation_data)
        .collect::<Vec<_>>();
    let material_data = vec![Material::new(1.0, 1.0, 1.0).to_data(); transformation_data.len()];
    let object = RenderObject {
        renderable: amrust_render::Renderable::ColoredMesh,
        gpu_mesh_id: data.gpu_mesh_id,
        instance: InstanceDataBuilder::new()
            .add_instance_stream(transformation_data.as_slice())
            .add_instance_stream(material_data.as_slice())
            .build(device),
        local_resources: vec![],
    };
    let colored_object_id = {
        let mut render_db = render_db.write_blocking();
        render_db.add_object(object)
    };

    let wireframe_object = RenderObject {
        renderable: amrust_render::Renderable::WireframeMesh,
        gpu_mesh_id: data.gpu_mesh_id,
        instance: InstanceDataBuilder::new()
            .add_instance_stream(transformation_data.as_slice())
            .add_instance_stream(material_data.as_slice())
            .build(device),
        local_resources: vec![],
    };

    let wireframe_object = {
        let mut render_db = render_db.write_blocking();
        render_db.add_object(wireframe_object)
    };

    (colored_object_id, wireframe_object)
}

fn add_bounding_box_wireframe(
    device: &wgpu::Device,
    render_db: Arc<RwLock<RenderDb>>,
    bbox: &BoundingBox,
) -> RenderObjectId {
    let mesh = MeshBuilder::new()
        .add_vertex_stream(convert_points_vec_to_position(&bbox.corners()).as_slice())
        .add_wireframe_index_stream(&BoundingBox::wireframe_indices())
        .build(device);

    let mut render_db = render_db.write_blocking();
    let mesh_id = render_db.add_mesh(mesh);

    let wireframe_object = RenderObject {
        renderable: amrust_render::Renderable::WireframeMesh,
        gpu_mesh_id: mesh_id,
        instance: InstanceDataBuilder::new()
            .add_instance_stream(&[TransformationData(Mat4::IDENTITY.to_cols_array_2d())])
            .add_instance_stream(&[Material::new(1.0, 1.0, 1.0).to_data()])
            .build(device),
        local_resources: vec![],
    };

    render_db.add_object(wireframe_object)
}

fn get_transformation_data(transformation: &Transformation) -> TransformationData {
    TransformationData(transformation.0.to_cols_array_2d())
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_convert_points_vec_to_position() {
        let points = vec![Vec3::new(1.0, 2.0, 3.0), Vec3::new(-1.0, -2.0, -3.0)];

        let positions = super::convert_points_vec_to_position(&points);
        assert_eq!(positions.len(), points.len());

        assert_eq!(positions[0], Position([1.0, 2.0, 3.0]));
        assert_eq!(positions[1], Position([-1.0, -2.0, -3.0]));
    }

    #[test]
    fn test_convert_vertices_to_position() {
        let vertices = vec![Vec3::new(0.0, 0.0, 0.0), Vec3::new(5.5, 6.6, 7.7)];
        let positions = convert_vertices_to_position(&vertices);

        assert_eq!(positions.len(), vertices.len());
        assert_eq!(positions[0], Position([0.0, 0.0, 0.0]));
        assert_eq!(positions[1], Position([5.5, 6.6, 7.7]));
    }

    #[test]
    fn test_convert_vertices_to_color() {
        let vertices = vec![Vec3::new(1.0, 2.0, 3.0); 3]; // three vertices
        let colors = convert_vertices_to_color(&vertices);

        assert_eq!(colors.len(), vertices.len());
        // All colors should be [0.5, 0.5, 0.5]
        for color in colors {
            assert_eq!(color, amrust_render::vertex::Color([0.5, 0.5, 0.5]));
        }
    }

    #[test]
    fn test_convert_triangle_indices_to_wireframe_indices_single_triangle() {
        // Triangles: one triangle with vertices (0, 1, 2)
        let triangles = vec![0, 1, 2];
        let indices = convert_triangle_indices_to_wireframe_indices(&triangles);

        // Expected wireframe: (0,1), (1,2), (2,0)
        assert_eq!(indices, vec![0, 1, 1, 2, 2, 0]);
    }

    #[test]
    fn test_convert_triangle_indices_to_wireframe_indices_multiple_triangles() {
        // Two triangles: (0,1,2) and (2,3,0)
        let triangles = vec![0, 1, 2, 2, 3, 0];
        let indices = convert_triangle_indices_to_wireframe_indices(&triangles);

        let expected = vec![
            0, 1, 1, 2, 2, 0, // first triangle
            2, 3, 3, 0, 0, 2, // second triangle
        ];
        assert_eq!(indices, expected);
    }

    #[test]
    fn test_convert_triangle_indices_to_wireframe_indices_incomplete_triangle() {
        // A "triangle" with only 2 vertices shouldn't crash — it will be ignored
        let triangles = vec![0, 1];
        let indices = convert_triangle_indices_to_wireframe_indices(&triangles);

        assert!(
            indices.is_empty(),
            "No indices should be generated for incomplete triangle"
        );
    }

    #[test]
    fn test_compute_transformed_bounding_box_from_mesh_identity() {
        let mesh = Mesh {
            vertices: vec![
                Vec3::new(0.0, 0.0, 0.0),
                Vec3::new(1.0, 2.0, 3.0),
                Vec3::new(-1.0, -2.0, -3.0),
            ],
            triangles: vec![0, 1, 2],
        };

        let transform = Transformation(Mat4::IDENTITY);
        let bbox = compute_transformed_bounding_box_from_mesh(&mesh, &transform);

        assert_eq!(bbox.min, Vec3::new(-1.0, -2.0, -3.0));
        assert_eq!(bbox.max, Vec3::new(1.0, 2.0, 3.0));
    }

    #[test]
    fn test_compute_transformed_bounding_box_from_mesh_with_translation() {
        let mesh = Mesh {
            vertices: vec![
                Vec3::new(0.0, 0.0, 0.0),
                Vec3::new(1.0, 1.0, 1.0),
                Vec3::new(2.0, 0.0, 0.0),
            ],
            triangles: vec![0, 1, 2],
        };

        let transform = Transformation(Mat4::from_translation(Vec3::new(2.0, 3.0, 4.0)));
        let bbox = compute_transformed_bounding_box_from_mesh(&mesh, &transform);

        assert_eq!(bbox.min, Vec3::new(2.0, 3.0, 4.0));
        assert_eq!(bbox.max, Vec3::new(4.0, 4.0, 5.0));
    }

    #[test]
    fn test_compute_transformed_bounding_box_from_mesh_with_scaling() {
        let mesh = Mesh {
            vertices: vec![
                Vec3::new(-1.0, -1.0, -1.0),
                Vec3::new(1.0, 1.0, 1.0),
                Vec3::new(0.0, 2.0, -2.0),
            ],
            triangles: vec![0, 1, 2],
        };

        let transform = Transformation(Mat4::from_scale(Vec3::new(2.0, 2.0, 2.0)));
        let bbox = compute_transformed_bounding_box_from_mesh(&mesh, &transform);

        assert_eq!(bbox.min, Vec3::new(-2.0, -2.0, -4.0));
        assert_eq!(bbox.max, Vec3::new(2.0, 4.0, 2.0));
    }

    #[test]
    fn test_compute_transformed_bounding_box_from_mesh_with_rotation() {
        use std::f32::consts::FRAC_PI_2; // 90 degrees

        let mesh = Mesh {
            vertices: vec![
                Vec3::new(1.0, 0.0, 0.0),  // +X
                Vec3::new(0.0, 1.0, 0.0),  // +Y
                Vec3::new(-1.0, 0.0, 0.0), // -X
            ],
            triangles: vec![0, 1, 2],
        };

        let rotation = Mat4::from_rotation_z(FRAC_PI_2);
        let transform = Transformation(rotation);
        let bbox = compute_transformed_bounding_box_from_mesh(&mesh, &transform);

        // Expected transformed:
        // (1,0) -> (0,1)
        // (0,1) -> (-1,0)
        // (-1,0) -> (0,-1)
        let expected_min = Vec3::new(-1.0, -1.0, 0.0);
        let expected_max = Vec3::new(0.0, 1.0, 0.0);
        assert!(bbox.min.abs_diff_eq(expected_min, 1e-6));
        assert!(bbox.max.abs_diff_eq(expected_max, 1e-6));
    }

    #[test]
    fn test_compute_transformed_bounding_box_from_mesh_with_rotation_and_translation() {
        use std::f32::consts::FRAC_PI_2;

        let mesh = Mesh {
            vertices: vec![
                Vec3::new(0.0, 0.0, 0.0),
                Vec3::new(1.0, 0.0, 0.0),
                Vec3::new(0.0, 1.0, 0.0),
            ],
            triangles: vec![0, 1, 2],
        };

        let rotation = Mat4::from_rotation_z(FRAC_PI_2); // 90 degrees
        let translation = Mat4::from_translation(Vec3::new(2.0, 0.0, 0.0));
        // Translation * Rotation means: rotate first, then translate in world space
        let transform = Transformation(translation * rotation);
        let bbox = compute_transformed_bounding_box_from_mesh(&mesh, &transform);

        // Step-by-step:
        // (0,0)   -> (0,0) + (2,0)   = (2,0)
        // (1,0)   -> (0,1) + (2,0)   = (2,1)
        // (0,1)   -> (-1,0) + (2,0)  = (1,0)
        let expected_min = Vec3::new(1.0, 0.0, 0.0);
        let expected_max = Vec3::new(2.0, 1.0, 0.0);
        assert!(bbox.min.abs_diff_eq(expected_min, 1e-6));
        assert!(bbox.max.abs_diff_eq(expected_max, 1e-6));
    }
}
