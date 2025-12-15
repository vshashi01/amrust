use glam::{Mat4, Vec3};
use slotmap::{SlotMap, new_key_type};
use smol::lock::RwLock;
use thiserror::Error;

use amrust_render::{
    bounding_box::BoundingBox, gpu_mesh::MeshBuilder, instance::InstanceDataBuilder,
    material::Material, transformation::Transformation, vertex::Position,
};

use core::fmt;
use std::collections::HashMap;
use std::fmt::Debug;
use std::sync::Arc;

use crate::render_db::{RenderDb, RenderObject};
use crate::tree_item_viewer::TreeItem;

#[derive(Debug)]
pub struct Part {
    rep: PartRep,
}

impl Part {
    pub fn get_rep(&self) -> &PartRep {
        &self.rep
    }
}

new_key_type! {
    pub struct PartId;
}

impl fmt::Display for PartId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&format!("{self:?}"))
    }
}

#[derive(Debug)]
pub enum PartRep {
    Mesh(Box<Mesh>),
    ComposedPart(Vec<PartInstanceId>),
}

#[derive(Debug, Clone)]
pub struct PartInstance {
    pub part_id: PartId,
    pub transform: Transformation,
}

impl PartInstance {
    pub fn new(part_id: PartId, transform: Option<Transformation>) -> PartInstance {
        PartInstance {
            part_id,
            transform: transform.unwrap_or_default(),
        }
    }
}

new_key_type! {
    pub struct PartInstanceId;
}

impl fmt::Display for PartInstanceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&format!("{self:?}"))
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Identifiable {
    Part(PartId),
    PartInstance(PartInstanceId),
}

#[derive(Debug)]
pub struct Scene {
    pub instances: Vec<PartInstanceId>,
}

impl Scene {
    pub fn new() -> Self {
        Scene { instances: vec![] }
    }

    pub fn new_from_instances(instances: Vec<PartInstanceId>) -> Self {
        let mut scene = Scene::new();
        scene.instances = instances;

        scene
    }

    pub fn add_part_instance(&mut self, instance: PartInstanceId) {
        self.instances.push(instance);
    }

    pub fn is_empty(&self) -> bool {
        self.instances.is_empty()
    }
}

#[derive(Clone)]
pub struct Mesh {
    pub vertices: Vec<Vec3>,
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

#[derive(Debug)]
pub struct Db {
    /// List of all unique Part configurations
    unique_parts: SlotMap<PartId, Part>,

    /// Part Instances point to a Part in unique_parts
    /// What essentially users will interact with are Part Instances in practical sense
    part_instances: SlotMap<PartInstanceId, PartInstance>,

    /// The main scene
    scene: Option<Scene>,
}

#[derive(Debug, Error)]
pub enum DbError {
    #[error("No valid part found with the id: {0}")]
    PartIdNotFound(PartId),

    #[error("No valid part instance found with the id: {0}")]
    PartInstanceIdNotFound(PartInstanceId),

    #[error("An invalid part rep is passed")]
    InvalidPartRep,

    #[error("Invalid Part Instances found")]
    PartInstancesNotFound(Vec<PartInstanceId>),

    #[error("There is no scene set currently")]
    SceneNotSet,
}

impl Db {
    pub fn new() -> Self {
        Db {
            unique_parts: SlotMap::with_key(),
            part_instances: SlotMap::with_key(),
            scene: None,
        }
    }

    // returns the PartID of the newly created Part
    // always creates a new part.
    pub fn add_part_rep(&mut self, part_rep: PartRep) -> Result<PartId, DbError> {
        let valid = self.validate_part_rep(&part_rep)?;
        if !valid {
            return Err(DbError::InvalidPartRep);
        }

        Ok(self.unique_parts.insert(Part { rep: part_rep }))
    }

    pub fn get_parts(&self) -> impl Iterator<Item = (PartId, &Part)> {
        self.unique_parts.iter()
    }

    pub fn get_part_instances(
        &self,
    ) -> impl Iterator<Item = (PartInstanceId, &PartInstance, &Part)> {
        self.part_instances.iter().map(|(id, instance_data)| {
            let part = self.get_part_data(&instance_data.part_id);

            match part {
                Ok(rep) => (id, instance_data, rep),
                Err(_) => panic!("Part Rep not found"),
            }
        })
    }

    pub fn get_part_data<'a>(&'a self, id: &PartId) -> Result<&'a Part, DbError> {
        let instance = self.unique_parts.get(*id);
        match instance {
            Some(i) => Ok(i),
            None => Err(DbError::PartIdNotFound(*id)),
        }
    }

    pub fn get_part_instance_data<'a>(
        &'a self,
        part_instance: &PartInstanceId,
    ) -> Result<&'a PartInstance, DbError> {
        let instance = self.part_instances.get(*part_instance);
        match instance {
            Some(i) => Ok(i),
            None => Err(DbError::PartInstanceIdNotFound(*part_instance)),
        }
    }

    pub fn get_part_from_part_instance(
        &self,
        part_instance: &PartInstanceId,
    ) -> Result<PartId, DbError> {
        let instance = self.part_instances.get(*part_instance);
        match instance {
            Some(i) => Ok(i.part_id),
            None => Err(DbError::PartInstanceIdNotFound(*part_instance)),
        }
    }

    pub fn get_part_data_from_part_instance<'a>(
        &'a self,
        part_instance: &PartInstanceId,
    ) -> Result<&'a Part, DbError> {
        let id = self.get_part_from_part_instance(part_instance)?;
        self.get_part_data(&id)
    }

    pub fn make_new_part_instance_from_part(
        &mut self,
        part: &PartId,
        transform: Option<Transformation>,
    ) -> Result<PartInstanceId, DbError> {
        match self.unique_parts.get(*part) {
            Some(_) => {
                let instance = PartInstance::new(*part, transform);
                let id = self.part_instances.insert(instance);
                Ok(id)
            }
            None => Err(DbError::PartIdNotFound(*part)),
        }
    }

    pub fn add_scene(&mut self, scene: Scene) -> Result<(), DbError> {
        let first_not_valid = scene
            .instances
            .iter()
            .find(|i| !self.validate_part_instance(i));

        match first_not_valid {
            Some(i) => Err(DbError::PartInstanceIdNotFound(*i)),
            None => {
                let _ = self.scene.insert(scene);
                Ok(())
            }
        }
    }

    pub fn add_part_instance_to_scene(
        &mut self,
        part_instance: &PartInstanceId,
    ) -> Result<(), DbError> {
        let valid = self.validate_part_instance(part_instance);
        if valid {
            match &mut self.scene {
                Some(scene) => {
                    let _: () = scene.add_part_instance(*part_instance);
                    Ok(())
                }
                None => {
                    let mut scene = Scene::new();
                    scene.add_part_instance(*part_instance);
                    self.scene.get_or_insert(scene);

                    Ok(())
                }
            }
        } else {
            Err(DbError::PartInstanceIdNotFound(*part_instance))
        }
    }

    pub fn get_scene(&self) -> Result<&Scene, DbError> {
        match &self.scene {
            Some(scene) => Ok(scene),
            None => Err(DbError::SceneNotSet),
        }
    }

    pub fn append(&mut self, other: Db) -> Result<(), DbError> {
        if other.is_scene_empty() {
            return Err(DbError::SceneNotSet);
        }

        //key is from other, and the value is from current
        let mut unique_part_other_to_unique_part_self: HashMap<PartId, PartId> = HashMap::new();

        //process all the mesh unique parts first
        let unique_mesh_parts = other
            .get_parts()
            .filter(|(_, part)| matches!(part.rep, PartRep::Mesh(_)))
            .collect::<Vec<_>>();

        for (unique_part_id, mesh_part) in unique_mesh_parts {
            if let PartRep::Mesh(mesh) = &mesh_part.rep {
                let new_unique_part_id = self.add_part_rep(PartRep::Mesh(mesh.clone()))?;

                unique_part_other_to_unique_part_self.insert(unique_part_id, new_unique_part_id);
            }
        }

        //process all unique composed parts
        let unique_composed_parts = other
            .get_parts()
            .filter(|(_, part)| matches!(part.rep, PartRep::ComposedPart(_)))
            .collect::<Vec<_>>();

        let mut unprocessed_unique_composed_parts = unique_composed_parts
            .iter()
            .map(|(u, _)| u)
            .collect::<Vec<_>>();

        loop {
            //if all is processed then just exit
            if unprocessed_unique_composed_parts.is_empty() {
                break;
            }

            for (unique_part_id, composed_parts) in &unique_composed_parts {
                //if its not in the list then skip because its already processed.
                if !unprocessed_unique_composed_parts.contains(&unique_part_id) {
                    continue;
                }

                if let PartRep::ComposedPart(instances) = &composed_parts.rep {
                    let can_process = instances.iter().all(|i| {
                        let part_id = &other.get_part_from_part_instance(i);
                        match part_id {
                            Ok(id) => unique_part_other_to_unique_part_self.contains_key(id),
                            Err(_) => false,
                        }
                    });

                    if !can_process {
                        continue;
                    }

                    let mut new_instances = vec![];
                    for i in instances {
                        let part_instance = &other.get_part_instance_data(i)?;
                        if let Some(new_unique_part_id) =
                            unique_part_other_to_unique_part_self.get(&part_instance.part_id)
                        {
                            let new_instance = self.make_new_part_instance_from_part(
                                new_unique_part_id,
                                Some(part_instance.transform),
                            )?;

                            new_instances.push(new_instance);
                        }
                    }

                    if new_instances.len() == instances.len() {
                        let new_unique_part_id =
                            self.add_part_rep(PartRep::ComposedPart(new_instances))?;

                        unique_part_other_to_unique_part_self
                            .insert(*unique_part_id, new_unique_part_id);

                        let item_index = unprocessed_unique_composed_parts
                            .iter()
                            .enumerate()
                            .find(|(_, u)| u.0 == unique_part_id.0)
                            .map(|(index, _)| index);

                        if let Some(index) = item_index {
                            unprocessed_unique_composed_parts.remove(index);
                        }
                    }
                }
            }
        }

        //now actually add the part instances
        if let Some(scene) = &other.scene {
            for i in &scene.instances {
                let part_instance = &other.get_part_instance_data(i)?;
                if let Some(new_unique_part_id) =
                    unique_part_other_to_unique_part_self.get(&part_instance.part_id)
                {
                    let new_instance = self.make_new_part_instance_from_part(
                        new_unique_part_id,
                        Some(part_instance.transform),
                    )?;

                    self.add_part_instance_to_scene(&new_instance)?;
                }
            }
        }

        drop(other);
        Ok(())
    }

    pub fn is_scene_empty(&self) -> bool {
        match &self.scene {
            Some(scene) => scene.is_empty(),
            None => true,
        }
    }

    pub fn clear_all(&mut self) {
        self.scene = None;
        self.part_instances.clear();
        self.unique_parts.clear();
    }

    fn validate_part_rep(&self, part_rep: &PartRep) -> Result<bool, DbError> {
        match part_rep {
            PartRep::Mesh(_) => {}
            PartRep::ComposedPart(part_instances) => {
                let mut invalid_part_instances = vec![];
                for instance in part_instances {
                    let validated = self.validate_part_instance(instance);
                    if !validated {
                        invalid_part_instances.push(*instance);
                    }
                }

                if !invalid_part_instances.is_empty() {
                    return Err(DbError::PartInstancesNotFound(invalid_part_instances));
                }
            }
        }
        Ok(true)
    }

    fn validate_part_instance(&self, part_instance: &PartInstanceId) -> bool {
        if let Some(instance) = self.part_instances.get(*part_instance)
            && self.unique_parts.get(instance.part_id).is_some()
        {
            return true;
        }

        false
    }
}

pub fn create_build_items_list(db: &Db) -> Result<Vec<TreeItem<Identifiable>>, DbError> {
    let scene = db.get_scene()?;

    let mut tree_items = vec![];
    for i in &scene.instances {
        let part = db.get_part_data_from_part_instance(i)?;
        let instance_data = db.get_part_instance_data(i)?;
        let name = match &part.rep {
            PartRep::Mesh(_) => format!("Instance: {:?} - Mesh: {:?}", i, instance_data.part_id),
            PartRep::ComposedPart(_) => format!(
                "Instance: {:?} - Composed Part: {:?}",
                i, instance_data.part_id
            ),
        };

        let item = TreeItem::Leaf {
            id: Identifiable::PartInstance(*i),
            name,
            selectable: true,
        };

        tree_items.push(item);
    }

    Ok(tree_items)
}

pub fn create_objects_list(db: &Db) -> Result<Vec<TreeItem<Identifiable>>, DbError> {
    let mut tree_items = vec![];

    for (id, part) in db.get_parts() {
        let item = match part.get_rep() {
            PartRep::Mesh(_) => TreeItem::Leaf {
                id: Identifiable::Part(id),
                name: format!("Mesh Object: {id:?}"),
                selectable: true,
            },
            PartRep::ComposedPart(_) => TreeItem::Leaf {
                id: Identifiable::Part(id),
                name: format!("Components Object: {id:?}"),
                selectable: true,
            },
        };

        tree_items.push(item);
    }

    Ok(tree_items)
}

pub fn create_object_tree_from_identifiable(
    db: &Db,
    identifiable: Identifiable,
) -> Result<TreeItem<usize>, DbError> {
    match identifiable {
        Identifiable::Part(part_id) => create_object_tree_from_part(db, &part_id),
        Identifiable::PartInstance(part_instance_id) => {
            create_object_tree_from_instance(db, &part_instance_id)
        }
    }
}

pub fn create_object_tree_from_part(db: &Db, id: &PartId) -> Result<TreeItem<usize>, DbError> {
    let part = db.get_part_data(id)?;

    let item = match &part.rep {
        PartRep::Mesh(mesh) => {
            let vertices_item = TreeItem::Leaf {
                id: 0_usize,
                name: format!("Vertices Count: {:?}", mesh.vertices.len()),
                selectable: false,
            };
            let triangles_item = TreeItem::Leaf {
                id: 1_usize,
                name: format!("Triangles Count: {:?}", mesh.triangles.len()),
                selectable: false,
            };

            TreeItem::InertNode {
                id: 2_usize,
                name: "Mesh".to_owned(),
                childs: vec![vertices_item, triangles_item],
            }
        }
        PartRep::ComposedPart(part_instance_ids) => {
            let mut map: HashMap<&PartInstanceId, TreeItem<usize>> = HashMap::new();
            let mut childs = vec![];
            for id in part_instance_ids {
                if let Some(item) = map.get(id) {
                    childs.push(item.clone());
                } else {
                    let item = create_object_tree_from_instance(db, id)?;
                    map.insert(id, item.clone());
                    childs.push(item);
                }
            }

            TreeItem::Node {
                id: 4_usize,
                name: format!("Composed Part - {:?}", id),
                childs,
                selectable: false,
            }
        }
    };

    Ok(item)
}

pub fn create_object_tree_from_instance(
    db: &Db,
    instance_id: &PartInstanceId,
) -> Result<TreeItem<usize>, DbError> {
    let instance_data = db.get_part_instance_data(instance_id)?;
    let object_tree = create_object_tree_from_part(db, &instance_data.part_id)?;

    let transform_item = TreeItem::Leaf {
        id: 105_usize,
        name: format!("Transform - {:?}", instance_data.transform),
        selectable: false,
    };

    Ok(TreeItem::Node {
        id: 5_usize,
        name: format!("Instance - {:?}", instance_id),
        childs: vec![object_tree, transform_item],
        selectable: false,
    })
}

pub fn create_scene_tree_items_by_unique_parts(
    db: &Db,
) -> Result<Vec<TreeItem<Identifiable>>, DbError> {
    let scene = db.get_scene()?;

    let mut instance_id_to_tree_item_map: HashMap<PartInstanceId, TreeItem<Identifiable>> =
        HashMap::new();

    let mut unprocessed_instances = vec![];

    // process all the items one round first
    for (id, _, part) in db.get_part_instances() {
        match &part.rep {
            PartRep::Mesh(_) => {
                instance_id_to_tree_item_map.insert(
                    id,
                    TreeItem::Leaf {
                        id: Identifiable::PartInstance(id),
                        name: format!("Mesh: {:?}", id),
                        selectable: true,
                    },
                );
            }
            PartRep::ComposedPart(part_instance_ids) => {
                let mut tree_items = vec![];
                for id in part_instance_ids {
                    if let Some(item) = instance_id_to_tree_item_map.get(id) {
                        tree_items.push(item.clone());
                    }
                }

                if tree_items.len() != part_instance_ids.len() {
                    unprocessed_instances.push(id);
                    continue;
                } else {
                    let node = TreeItem::Node {
                        id: Identifiable::PartInstance(id),
                        name: format!("Composed Part: {:?}", id),
                        childs: tree_items,
                        selectable: true,
                    };
                    instance_id_to_tree_item_map.insert(id, node);
                }
            }
        }
    }

    //ToDo: Process unprocessed items

    let mut unique_part_id_to_instance_tree_item: HashMap<PartId, TreeItem<Identifiable>> =
        HashMap::new();
    for i in &scene.instances {
        let instance_data = db.get_part_instance_data(i)?;

        if let Some(item) = instance_id_to_tree_item_map.get(i) {
            if let Some(unique_item) =
                unique_part_id_to_instance_tree_item.get_mut(&instance_data.part_id)
            {
                //unique part entry should always be a node
                if let TreeItem::Node { childs, .. } = unique_item {
                    childs.push(item.clone());
                }
            } else {
                let tree_item = TreeItem::InertNode {
                    id: Identifiable::Part(instance_data.part_id),
                    name: format!("Unique Part: {:?}", instance_data.part_id),
                    childs: vec![item.clone()],
                };

                unique_part_id_to_instance_tree_item.insert(instance_data.part_id, tree_item);
            }
        }
    }

    let items = unique_part_id_to_instance_tree_item
        .into_values()
        .collect::<Vec<_>>();

    Ok(items)
}

struct InstanceData {
    pub gpu_mesh_id: u32,
    pub transforms: Vec<Transformation>,
}

/// This creates a 3D scene based on the unique parts
pub fn add_render_items_from_unique_parts(
    device: &wgpu::Device,
    render_db: Arc<RwLock<RenderDb>>,
    db: &Db,
) -> Result<BoundingBox, DbError> {
    let mut part_id_to_instance_data = HashMap::<PartId, InstanceData>::new();
    let mut bboxes = vec![];

    //setup all the instance data
    for (part_id, part) in db.get_parts() {
        match &part.rep {
            PartRep::Mesh(mesh) => {
                let gpu_mesh_id = create_mesh_gpu_data(device, render_db.clone(), mesh);
                let transform = Transformation(Mat4::IDENTITY);
                let bbox = compute_transformed_bounding_box_from_mesh(mesh, &transform);
                bboxes.push(bbox);
                part_id_to_instance_data.insert(
                    part_id,
                    InstanceData {
                        gpu_mesh_id,
                        transforms: vec![transform],
                    },
                );
            }
            PartRep::ComposedPart(part_instance_ids) => {
                for instance in part_instance_ids {
                    let render_db = render_db.clone();
                    let instance_data = db.get_part_instance_data(instance)?;
                    match process_part_instance(
                        device,
                        render_db,
                        db,
                        &mut part_id_to_instance_data,
                        instance_data,
                        &Transformation(Mat4::IDENTITY),
                    ) {
                        Ok(_) => {}
                        Err(err) => return Err(err),
                    }

                    match compute_instance_bbox(db, instance_data, &Transformation(Mat4::IDENTITY))
                    {
                        Ok(bbox) => bboxes.push(bbox),
                        Err(err) => return Err(err),
                    }
                }
            }
        }

        // match compute_instance_bbox(db, instance_data, &Transformation(Mat4::IDENTITY)) {
        //     Ok(bbox) => bboxes.push(bbox),
        //     Err(err) => return Err(err),
        // }
    }

    part_id_to_instance_data
        .into_iter()
        .for_each(|(_, instance_data)| {
            let render_db = render_db.clone();
            add_render_object(device, render_db, &instance_data);
        });

    let mut total_bbox = BoundingBox::default();
    bboxes.iter().for_each(|bbox| {
        let render_db = render_db.clone();
        add_bounding_box_wireframe(device, render_db, bbox);
        total_bbox.unite(bbox)
    });

    add_bounding_box_wireframe(device, render_db, &total_bbox);
    println!("Total BBOX is {:?}", total_bbox);

    Ok(total_bbox)
}

/// This creates a 3D scene based on the Scene object
pub fn add_render_items_from_scene(
    device: &wgpu::Device,
    render_db: Arc<RwLock<RenderDb>>,
    db: &Db,
) -> Result<BoundingBox, DbError> {
    let scene = db.get_scene()?;
    let mut total_bbox = BoundingBox::default();

    let mut part_id_to_instance_data = HashMap::<PartId, InstanceData>::new();
    let mut bboxes = vec![];

    //setup all the instance data
    for instance in &scene.instances {
        let render_db = render_db.clone();
        let instance_data = db.get_part_instance_data(instance)?;
        match process_part_instance(
            device,
            render_db,
            db,
            &mut part_id_to_instance_data,
            instance_data,
            &Transformation(Mat4::IDENTITY),
        ) {
            Ok(_) => {}
            Err(err) => return Err(err),
        }

        match compute_instance_bbox(db, instance_data, &Transformation(Mat4::IDENTITY)) {
            Ok(bbox) => bboxes.push(bbox),
            Err(err) => return Err(err),
        }
    }

    part_id_to_instance_data
        .into_iter()
        .for_each(|(_, instance_data)| {
            let render_db = render_db.clone();
            add_render_object(device, render_db, &instance_data);
        });

    bboxes.iter().for_each(|bbox| {
        let render_db = render_db.clone();
        add_bounding_box_wireframe(device, render_db, bbox);
        total_bbox.unite(bbox)
    });

    add_bounding_box_wireframe(device, render_db, &total_bbox);
    println!("Total BBOX is {:?}", total_bbox);

    Ok(total_bbox)
}

fn process_part_instance(
    device: &wgpu::Device,
    render_db: Arc<RwLock<RenderDb>>,
    db: &Db,
    part_id_to_instance_data: &mut HashMap<PartId, InstanceData>,
    instance: &PartInstance,
    parent_transform: &Transformation,
) -> Result<(), DbError> {
    let combined_transform = Transformation(parent_transform.0 * instance.transform.0);
    //if the necessary part is already created then just push new transform data to add an additional render object
    if let Some(instance_data) = part_id_to_instance_data.get_mut(&instance.part_id) {
        instance_data.transforms.push(combined_transform);
        return Ok(());
    }

    let part = db.get_part_data(&instance.part_id);
    match part {
        Ok(p) => match &p.rep {
            PartRep::Mesh(mesh) => {
                let gpu_mesh_id = create_mesh_gpu_data(device, render_db, mesh);
                part_id_to_instance_data.insert(
                    instance.part_id,
                    InstanceData {
                        gpu_mesh_id,
                        transforms: vec![combined_transform],
                    },
                );
            }
            PartRep::ComposedPart(part_instances) => {
                for i in part_instances {
                    let child_instance_data = db.get_part_instance_data(i)?;
                    let render_db = render_db.clone();
                    match process_part_instance(
                        device,
                        render_db,
                        db,
                        part_id_to_instance_data,
                        child_instance_data,
                        &combined_transform,
                    ) {
                        Ok(_) => {}
                        Err(err) => return Err(err),
                    }
                }
            }
        },
        Err(err) => return Err(err),
    }
    Ok(())
}

fn create_mesh_gpu_data(
    device: &wgpu::Device,
    render_db: Arc<RwLock<RenderDb>>,
    mesh: &Mesh,
) -> u32 {
    let positions = convert_vertices_to_position(&mesh.vertices);
    // println!("Number of vertices: {}", positions.len());
    let indices = mesh.triangles.clone();
    // println!("Number of triangles: {}", indices.len() / 3);
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

fn add_render_object(device: &wgpu::Device, render_db: Arc<RwLock<RenderDb>>, data: &InstanceData) {
    let transformation_data = data
        .transforms
        .iter()
        .map(|t| t.to_data())
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
    {
        let mut render_db = render_db.write_blocking();
        let _ = render_db.add_object(object);
    }

    // println!("Colored Object id {}", object_id);

    let wireframe_object = RenderObject {
        renderable: amrust_render::Renderable::WireframeMesh,
        gpu_mesh_id: data.gpu_mesh_id,
        instance: InstanceDataBuilder::new()
            .add_instance_stream(transformation_data.as_slice())
            .add_instance_stream(material_data.as_slice())
            .build(device),
        local_resources: vec![],
    };

    {
        let mut render_db = render_db.write_blocking();
        let _ = render_db.add_object(wireframe_object);
    }

    // println!("Wireframe Object id {}", wireframe_object_id);
}

fn add_bounding_box_wireframe(
    device: &wgpu::Device,
    render_db: Arc<RwLock<RenderDb>>,
    bbox: &BoundingBox,
) -> u32 {
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
            .add_instance_stream(&[Transformation(Mat4::IDENTITY).to_data()])
            .add_instance_stream(&[Material::new(1.0, 1.0, 1.0).to_data()])
            .build(device),
        local_resources: vec![],
    };

    render_db.add_object(wireframe_object)
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

fn compute_instance_bbox(
    db: &Db,
    instance: &PartInstance,
    parent_transform: &Transformation,
) -> Result<BoundingBox, DbError> {
    let combined_transform = Transformation(parent_transform.0 * instance.transform.0);
    let part = db.get_part_data(&instance.part_id)?;

    match &part.rep {
        PartRep::Mesh(mesh) => {
            let bbox = compute_transformed_bounding_box_from_mesh(mesh, &combined_transform);
            Ok(bbox)
        }
        PartRep::ComposedPart(children) => {
            let mut bbox = BoundingBox::default();
            for child in children {
                let child_instance_data = db.get_part_instance_data(child)?;
                let child_bbox =
                    compute_instance_bbox(db, child_instance_data, &combined_transform)?;
                bbox.unite(&child_bbox);
            }
            Ok(bbox)
        }
    }
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
