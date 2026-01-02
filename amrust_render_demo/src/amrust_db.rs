#![allow(clippy::needless_lifetimes)]
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

use crate::operation::DbReader;
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

#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash)]
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

    /// List of all detached unique parts
    detached_unique_parts: Vec<PartId>,

    /// Part Instances point to a Part in unique_parts
    /// What essentially users will interact with are Part Instances in practical sense
    part_instances: SlotMap<PartInstanceId, PartInstance>,

    /// List of all detached part_instances
    detached_part_instances: Vec<PartInstanceId>,

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
            detached_unique_parts: vec![],
            part_instances: SlotMap::with_key(),
            detached_part_instances: vec![],
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

    pub fn is_empty(&self) -> bool {
        self.unique_parts.is_empty() && self.part_instances.is_empty() && self.is_scene_empty()
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

    // detaching a part will also detach instances that are used by the Part (e.g Component instances within Composed Parts)
    pub fn detach_parts_and_add_to_detached_db(
        &mut self,
        detached_db: &mut DetachedDb,
        parts_to_detach: &[PartId],
    ) -> Result<(), DbError> {
        for part_id in parts_to_detach {
            let mut map_of_parts = HashMap::new();
            if self.detach_part(&part_id, &mut map_of_parts).is_err() {
                panic!("Detaching parts failed partially! Db is dirty!")
            }

            for (part_id, part) in map_of_parts {
                if detached_db.map_part_id_to_detached.contains_key(&part_id) {
                    // in this case the part was already previously detached and registered
                    continue;
                }

                let instances_pointing_to_part = self
                    .get_part_instances()
                    .filter_map(|(instance_id, instance, _)| {
                        if instance.part_id == part_id {
                            Some(instance_id)
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>();

                if detached_db.add_detached_part(&part_id, part).is_ok() {
                    //detaching Parts will detach all the instances the Part is pointed to as well.
                    for id in instances_pointing_to_part {
                        if let Ok(instance) = self.detach_part_instance(&id) {
                            if detached_db
                                .add_detached_part_instance(&id, instance)
                                .is_err()
                            {
                                panic!("Registering Instance to Detached Db failed! Db is dirty!")
                            }
                        } else {
                            panic!("Detaching Instance failed! Db is dirty!")
                        }
                    }
                } else {
                    panic!("Registering Part to DetachedDB failed! Db is dirty!")
                }
            }
        }

        Ok(())
    }

    pub fn detach_part_instances_and_add_to_detached_db(
        &mut self,
        detached_db: &mut DetachedDb,
        part_instances_to_detach: &[PartInstanceId],
        is_detach_parts_pointed_to: bool,
    ) -> Result<(), DbError> {
        let mut parts_to_detach = vec![];

        for instance_id in part_instances_to_detach {
            if detached_db
                .map_instance_id_to_detached
                .contains_key(instance_id)
            {
                // in this case the instance was already previously detached and registered
                continue;
            }

            let instance = self.detach_part_instance(instance_id)?;
            let part_pointed_to = instance.part_id.clone();

            match detached_db.add_detached_part_instance(instance_id, instance) {
                Ok(_) => {
                    if is_detach_parts_pointed_to {
                        parts_to_detach.push(part_pointed_to);
                    }
                }
                Err(err) => {
                    panic!("Registering Instance to Detached Db failed: {err:?}");
                }
            }
        }

        if is_detach_parts_pointed_to {
            self.detach_parts_and_add_to_detached_db(detached_db, &parts_to_detach)?;
        }

        Ok(())
    }

    pub fn reattach(&mut self, mut detached: DetachedDb) -> Result<(), DbError> {
        for (part_id, internal_id) in &detached.map_part_id_to_detached {
            if let Some(part) = detached.detached_unique_parts.detach(*internal_id)
                && let Err(_) = self.reattach_part(part_id, part)
            {
                panic!("Reattaching parts that were not detached");
            }
        }

        for (instance_id, internal_id) in &detached.map_instance_id_to_detached {
            if let Some(instance) = detached.detached_part_instances.detach(*internal_id)
                && let Err(_) = self.reattach_part_instance(instance_id, instance)
            {
                panic!("Reattaching instance that were not detached!");
            }
        }

        Ok(())
    }

    fn detach_part(
        &mut self,
        part_id: &PartId,
        o_parts: &mut HashMap<PartId, Part>,
    ) -> Result<(), DbError> {
        if let Some(part) = self.unique_parts.detach(*part_id) {
            self.detached_unique_parts.push(*part_id);
            match part.get_rep() {
                PartRep::Mesh(_) => {
                    o_parts.insert(*part_id, part);
                }
                PartRep::ComposedPart(part_instance_ids) => {
                    let mut undetached_parts = vec![];
                    for id in part_instance_ids {
                        let instance = self.get_part_instance_data(id)?;
                        if !o_parts.contains_key(&instance.part_id) {
                            undetached_parts.push(instance.part_id);
                        }
                    }

                    for comp_id in undetached_parts {
                        self.detach_part(&comp_id, o_parts)?;
                    }

                    o_parts.insert(*part_id, part);
                }
            }
        } else {
            return Err(DbError::PartIdNotFound(*part_id));
        }

        Ok(())
    }

    fn reattach_part(&mut self, part_id: &PartId, part: Part) -> Result<(), DbError> {
        if let Some(pos) = self
            .detached_unique_parts
            .iter()
            .position(|id| id == part_id)
        {
            self.unique_parts.reattach(*part_id, part);
            self.detached_unique_parts.swap_remove(pos);
        } else {
            return Err(DbError::PartIdNotFound(*part_id));
        }

        Ok(())
    }

    fn detach_part_instance(
        &mut self,
        part_instance_id: &PartInstanceId,
    ) -> Result<PartInstance, DbError> {
        if let Some(part) = self.part_instances.detach(*part_instance_id) {
            self.detached_part_instances.push(*part_instance_id);
            Ok(part)
        } else {
            Err(DbError::PartInstanceIdNotFound(*part_instance_id))
        }
    }

    fn reattach_part_instance(
        &mut self,
        part_instance_id: &PartInstanceId,
        part_instance: PartInstance,
    ) -> Result<(), DbError> {
        if let Some(pos) = self
            .detached_part_instances
            .iter()
            .position(|id| id == part_instance_id)
        {
            self.part_instances
                .reattach(*part_instance_id, part_instance);
            self.detached_part_instances.swap_remove(pos);
        } else {
            return Err(DbError::PartInstanceIdNotFound(*part_instance_id));
        }

        Ok(())
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

impl DbReader for Db {
    fn get_parts_count<'a>(&'a self) -> usize {
        self.unique_parts.len()
    }

    fn get_parts<'a>(&'a self) -> Box<dyn Iterator<Item = (PartId, &'a Part)> + 'a> {
        Box::new(self.get_parts())
    }

    fn get_part_instance_count<'a>(&'a self) -> usize {
        self.part_instances.len()
    }

    fn get_part_instance<'a>(
        &'a self,
        part_instance_id: &PartInstanceId,
    ) -> Option<&'a PartInstance> {
        self.part_instances.get(*part_instance_id)
    }

    fn get_part_instances<'a>(
        &'a self,
    ) -> Box<dyn Iterator<Item = (PartInstanceId, &'a PartInstance, &'a Part)> + 'a> {
        Box::new(self.get_part_instances())
    }

    fn get_scene<'a>(&'a self) -> Option<&'a Scene> {
        self.get_scene().ok()
    }

    fn get_part<'a>(&'a self, part_id: &PartId) -> Option<&'a Part> {
        self.unique_parts.get(*part_id)
    }
}

#[derive(Debug, Error)]
pub enum DetachedDbError {
    #[error("Part Exists")]
    DetachedPartAlreadyExists,

    #[error("Part Instance Exists")]
    DetachedPartInstanceAlreadyExists,
}

new_key_type! {
pub struct DetachedPartID;
}

new_key_type! {
pub struct DetachedPartInstanceId;
}

pub struct DetachedDb {
    detached_unique_parts: SlotMap<DetachedPartID, Part>,
    detached_part_instances: SlotMap<DetachedPartInstanceId, PartInstance>,

    map_part_id_to_detached: HashMap<PartId, DetachedPartID>,
    map_instance_id_to_detached: HashMap<PartInstanceId, DetachedPartInstanceId>,
}

impl DetachedDb {
    pub fn new() -> Self {
        Self {
            detached_unique_parts: SlotMap::with_key(),
            detached_part_instances: SlotMap::with_key(),
            map_part_id_to_detached: HashMap::new(),
            map_instance_id_to_detached: HashMap::new(),
        }
    }

    pub fn append_db(&mut self, other: Db) -> Result<(), DetachedDbError> {
        todo!("Implement appending to Detach Db")
    }

    fn add_detached_part(
        &mut self,
        part_id: &PartId,
        part: Part,
    ) -> Result<DetachedPartID, DetachedDbError> {
        if !self.map_part_id_to_detached.contains_key(part_id) {
            let detached_id = self.detached_unique_parts.insert(part);
            self.map_part_id_to_detached.insert(*part_id, detached_id);
            Ok(detached_id)
        } else {
            Err(DetachedDbError::DetachedPartAlreadyExists)
        }
    }

    fn add_detached_part_instance(
        &mut self,
        part_instance_id: &PartInstanceId,
        part_instance: PartInstance,
    ) -> Result<DetachedPartInstanceId, DetachedDbError> {
        if !self
            .map_instance_id_to_detached
            .contains_key(part_instance_id)
        {
            let detached_id = self.detached_part_instances.insert(part_instance);
            self.map_instance_id_to_detached
                .insert(*part_instance_id, detached_id);
            Ok(detached_id)
        } else {
            Err(DetachedDbError::DetachedPartInstanceAlreadyExists)
        }
    }
}

pub fn create_build_items_list(
    db: Arc<RwLock<Db>>,
) -> Result<Vec<TreeItem<Identifiable>>, DbError> {
    let read_db = db.read_blocking();
    let scene = read_db.get_scene()?;

    let mut tree_items = vec![];
    for i in &scene.instances {
        let part = read_db.get_part_data_from_part_instance(i)?;
        let instance_data = read_db.get_part_instance_data(i)?;
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

impl DbReader for DetachedDb {
    fn get_parts_count<'a>(&'a self) -> usize {
        self.detached_unique_parts.len()
    }

    fn get_parts<'a>(&'a self) -> Box<dyn Iterator<Item = (PartId, &'a Part)> + 'a> {
        let iterator = self
            .map_part_id_to_detached
            .iter()
            .map(|(part_id, internal_id)| {
                (
                    *part_id,
                    self.detached_unique_parts.get(*internal_id).unwrap(),
                )
            });

        Box::new(iterator)
    }

    fn get_part_instance_count<'a>(&'a self) -> usize {
        self.detached_part_instances.len()
    }

    fn get_part_instance<'a>(
        &'a self,
        part_instance_id: &PartInstanceId,
    ) -> Option<&'a PartInstance> {
        if let Some(detached_id) = self.map_instance_id_to_detached.get(part_instance_id) {
            self.detached_part_instances.get(*detached_id)
        } else {
            None
        }
    }

    fn get_part_instances<'a>(
        &'a self,
    ) -> Box<dyn Iterator<Item = (PartInstanceId, &'a PartInstance, &'a Part)> + 'a> {
        let iterator = self
            .map_instance_id_to_detached
            .iter()
            .map(|(instance_id, internal_id)| {
                let instance = self.detached_part_instances.get(*internal_id).unwrap();
                let get_internal_part_id =
                    self.map_part_id_to_detached.get(&instance.part_id).unwrap();
                let part = self
                    .detached_unique_parts
                    .get(*get_internal_part_id)
                    .unwrap();

                (*instance_id, instance, part)
            });

        Box::new(iterator)
    }

    fn get_scene(&self) -> Option<&Scene> {
        None
    }

    fn get_part<'a>(&'a self, part_id: &PartId) -> Option<&'a Part> {
        if let Some(detached_id) = self.map_part_id_to_detached.get(part_id) {
            self.detached_unique_parts.get(*detached_id)
        } else {
            None
        }
    }
}

pub fn create_objects_list(db: Arc<RwLock<Db>>) -> Result<Vec<TreeItem<Identifiable>>, DbError> {
    let mut tree_items = vec![];

    let read_db = db.read_blocking();

    for (id, part) in read_db.get_parts() {
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
    db: Arc<RwLock<Db>>,
    identifiable: Identifiable,
) -> Result<TreeItem<usize>, DbError> {
    match identifiable {
        Identifiable::Part(part_id) => create_object_tree_from_part(db, &part_id),
        Identifiable::PartInstance(part_instance_id) => {
            create_object_tree_from_instance(db, &part_instance_id)
        }
    }
}

pub fn create_object_tree_from_part(
    db: Arc<RwLock<Db>>,
    id: &PartId,
) -> Result<TreeItem<usize>, DbError> {
    let read_db = db.read_blocking();
    let part = read_db.get_part_data(id)?;

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
                    let item = create_object_tree_from_instance(db.clone(), id)?;
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
    db: Arc<RwLock<Db>>,
    instance_id: &PartInstanceId,
) -> Result<TreeItem<usize>, DbError> {
    let read_db = db.read_blocking();
    let instance_data = read_db.get_part_instance_data(instance_id)?;
    let object_tree = create_object_tree_from_part(db.clone(), &instance_data.part_id)?;

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
    db: Arc<RwLock<Db>>,
) -> Result<Vec<TreeItem<Identifiable>>, DbError> {
    let read_db = db.read_blocking();
    let scene = read_db.get_scene()?;

    let mut instance_id_to_tree_item_map: HashMap<PartInstanceId, TreeItem<Identifiable>> =
        HashMap::new();

    let mut unprocessed_instances = vec![];

    // process all the items one round first
    for (id, _, part) in read_db.get_part_instances() {
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
        let instance_data = read_db.get_part_instance_data(i)?;

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
    db: Arc<RwLock<Db>>,
) -> Result<BoundingBox, DbError> {
    let mut part_id_to_instance_data = HashMap::<PartId, InstanceData>::new();
    let mut bboxes = vec![];

    //setup all the instance data
    for (part_id, part) in db.read_blocking().get_parts() {
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
                let read_db = db.read_blocking();
                for instance in part_instance_ids {
                    let render_db = render_db.clone();
                    let instance_data = read_db.get_part_instance_data(instance)?;
                    match process_part_instance(
                        device,
                        render_db,
                        db.clone(),
                        &mut part_id_to_instance_data,
                        instance_data,
                        &Transformation(Mat4::IDENTITY),
                    ) {
                        Ok(_) => {}
                        Err(err) => return Err(err),
                    }

                    match compute_instance_bbox(
                        db.clone(),
                        instance_data,
                        &Transformation(Mat4::IDENTITY),
                    ) {
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
    db: Arc<RwLock<Db>>,
) -> Result<BoundingBox, DbError> {
    let read_db = db.read_blocking();
    let scene = read_db.get_scene()?;
    let mut total_bbox = BoundingBox::default();

    let mut part_id_to_instance_data = HashMap::<PartId, InstanceData>::new();
    let mut bboxes = vec![];

    //setup all the instance data
    for instance in &scene.instances {
        let render_db = render_db.clone();
        let db = db.clone();
        let instance_data = read_db.get_part_instance_data(instance)?;
        match process_part_instance(
            device,
            render_db,
            db.clone(),
            &mut part_id_to_instance_data,
            instance_data,
            &Transformation(Mat4::IDENTITY),
        ) {
            Ok(_) => {}
            Err(err) => return Err(err),
        }

        match compute_instance_bbox(db.clone(), instance_data, &Transformation(Mat4::IDENTITY)) {
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
    db: Arc<RwLock<Db>>,
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

    let temp_db = db.read_blocking();
    let part = temp_db.get_part_data(&instance.part_id);
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
                    let child_instance_data = temp_db.get_part_instance_data(i)?;
                    let render_db = render_db.clone();
                    match process_part_instance(
                        device,
                        render_db,
                        db.clone(),
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
    db: Arc<RwLock<Db>>,
    instance: &PartInstance,
    parent_transform: &Transformation,
) -> Result<BoundingBox, DbError> {
    let combined_transform = Transformation(parent_transform.0 * instance.transform.0);
    let read_db = db.read_blocking();
    let part = read_db.get_part_data(&instance.part_id)?;

    match &part.rep {
        PartRep::Mesh(mesh) => {
            let bbox = compute_transformed_bounding_box_from_mesh(mesh, &combined_transform);
            Ok(bbox)
        }
        PartRep::ComposedPart(children) => {
            let mut bbox = BoundingBox::default();
            for child in children {
                let child_instance_data = read_db.get_part_instance_data(child)?;
                let child_bbox =
                    compute_instance_bbox(db.clone(), child_instance_data, &combined_transform)?;
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
