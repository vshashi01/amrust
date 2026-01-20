#![allow(clippy::needless_lifetimes)]
use rkyv::api::low::from_bytes_unchecked;
use rkyv::bytecheck::CheckBytes;
use rkyv::rancor::Fallible;
use rkyv::to_bytes;
use rkyv::util::AlignedVec;
use rkyv_derive::{Archive, Deserialize, Serialize};
use slotmap::{KeyData, SlotMap, new_key_type};
use thiserror::Error;

use std::collections::{HashMap, HashSet};
use std::fmt::Debug;

use crate::core::interfaces::db_reader::DbReader;
use crate::core::interfaces::db_reader_writer::DbReaderWriter;
use crate::core::interfaces::db_writer::DbWriter;
use crate::core::types::archived::ArchivedSlotMapId;
use crate::core::types::part::{Part, PartId, PartRep};
use crate::core::types::part_instance::{PartInstance, PartInstanceId};
use crate::core::types::transformation::Transformation;

#[derive(Debug, Clone, Archive, Serialize, Deserialize, CheckBytes)]
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

#[derive(Debug, Clone, Copy, Archive, Serialize, Deserialize)]
pub enum EntityChanges {
    Added,
    Removed,
    Detached,
    Reattached,
}

#[derive(Debug, CheckBytes)]
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

    /// Set of parts that may have changed
    changed_parts: HashMap<PartId, EntityChanges>,

    /// Set of part instances that may have changed
    changed_part_instances: HashMap<PartInstanceId, EntityChanges>,
}

#[derive(Debug, Clone, Archive, Serialize, Deserialize, CheckBytes)]
pub struct ArchivedSlotMap<K, V> {
    entries: Vec<(K, V)>,
}

impl<K: slotmap::Key + Copy, V: Clone> From<&SlotMap<K, V>> for ArchivedSlotMap<K, V> {
    fn from(map: &SlotMap<K, V>) -> Self {
        Self {
            entries: map.iter().map(|(k, v)| (k, v.clone())).collect(),
        }
    }
}

impl<K: slotmap::Key, V: Clone> ArchivedSlotMap<K, V> {
    pub fn into_slotmap(self) -> SlotMap<K, V> {
        let mut map = SlotMap::with_key();
        for (_, v) in self.entries {
            map.insert(v);
        }
        map
    }
}

#[derive(Debug, Clone, Archive, Serialize, Deserialize, CheckBytes)]
pub struct ArchivedDb {
    unique_parts: ArchivedSlotMap<PartId, Part>,
    detached_unique_parts: Vec<PartId>,

    part_instances: ArchivedSlotMap<PartInstanceId, PartInstance>,
    detached_part_instances: Vec<PartInstanceId>,

    scene: Option<Scene>,

    changed_parts: HashMap<PartId, EntityChanges>,
    changed_part_instances: HashMap<PartInstanceId, EntityChanges>,
}

unsafe impl rkyv::Portable for ArchivedDb {}

unsafe impl rkyv::traits::NoUndef for ArchivedDb {}

impl rkyv::Archive for Db {
    type Archived = ArchivedDb;
    type Resolver = ();

    fn resolve(&self, _: (), out: rkyv::Place<Self::Archived>) {
        out.write(ArchivedDb {
            unique_parts: ArchivedSlotMap::from(&self.unique_parts),
            detached_unique_parts: self.detached_unique_parts.clone(),

            part_instances: ArchivedSlotMap::from(&self.part_instances),
            detached_part_instances: self.detached_part_instances.clone(),

            scene: self.scene.clone(),
            changed_parts: self.changed_parts.clone(),
            changed_part_instances: self.changed_part_instances.clone(),
        });
    }
}

impl<S: Fallible + ?Sized> rkyv::Serialize<S> for Db {
    fn serialize(&self, _: &mut S) -> Result<Self::Resolver, S::Error> {
        Ok(())
    }
}

impl<D: Fallible + ?Sized> rkyv::Deserialize<Db, D> for ArchivedDb {
    fn deserialize(&self, _: &mut D) -> Result<Db, D::Error> {
        Ok(Db {
            unique_parts: self.unique_parts.clone().into_slotmap(),
            detached_unique_parts: self.detached_unique_parts.clone(),

            part_instances: self.part_instances.clone().into_slotmap(),
            detached_part_instances: self.detached_part_instances.clone(),

            scene: self.scene.clone(),
            changed_parts: HashMap::new(), // Reset on deserialization
            changed_part_instances: HashMap::new(),
        })
    }
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

    #[error("Failed to restore DB from bytes: {0}")]
    RestoreError(String),
}

impl Db {
    pub fn new() -> Self {
        Db {
            unique_parts: SlotMap::with_key(),
            detached_unique_parts: vec![],
            part_instances: SlotMap::with_key(),
            detached_part_instances: vec![],
            scene: None,
            changed_parts: HashMap::new(),
            changed_part_instances: HashMap::new(),
        }
    }

    // returns the PartID of the newly created Part
    // always creates a new part.
    pub fn add_part_rep(&mut self, part_rep: PartRep) -> Result<PartId, DbError> {
        let valid = self.validate_part_rep(&part_rep)?;
        if !valid {
            return Err(DbError::InvalidPartRep);
        }

        let part_id = self.unique_parts.insert(Part::new(part_rep));
        self.mark_part_changed(part_id, EntityChanges::Added);
        Ok(part_id)
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
                self.mark_part_instance_changed(id, EntityChanges::Added);
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
        // Note: Scene addition doesn't change the instance itself, so no marking
    }

    pub fn get_scene(&self) -> Result<&Scene, DbError> {
        match &self.scene {
            Some(scene) => Ok(scene),
            None => Err(DbError::SceneNotSet),
        }
    }

    pub fn append(&mut self, other: Db) -> Result<(), DbError> {
        // if other.is_scene_empty() {
        //     return Err(DbError::SceneNotSet);
        // }

        //key is from other, and the value is from current
        let mut unique_part_other_to_unique_part_self: HashMap<PartId, PartId> = HashMap::new();

        //process all the mesh unique parts first
        let unique_mesh_parts = other
            .get_parts()
            .filter(|(_, part)| matches!(part.get_rep(), PartRep::Mesh(_)))
            .collect::<Vec<_>>();

        for (unique_part_id, mesh_part) in unique_mesh_parts {
            if let PartRep::Mesh(mesh) = &mesh_part.get_rep() {
                let new_unique_part_id = self.add_part_rep(PartRep::Mesh(mesh.clone()))?;

                // self.mark_part_changed(new_unique_part_id, EntityChanges::Added);
                unique_part_other_to_unique_part_self.insert(unique_part_id, new_unique_part_id);
            }
        }

        //process all unique composed parts
        let unique_composed_parts = other
            .get_parts()
            .filter(|(_, part)| matches!(part.get_rep(), PartRep::ComposedPart(_)))
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

                if let PartRep::ComposedPart(instances) = &composed_parts.get_rep() {
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

                            // self.mark_part_instance_changed(new_instance);
                            new_instances.push(new_instance);
                        }
                    }

                    if new_instances.len() == instances.len() {
                        let new_unique_part_id =
                            self.add_part_rep(PartRep::ComposedPart(new_instances))?;
                        // self.mark_part_changed(new_unique_part_id);

                        unique_part_other_to_unique_part_self
                            .insert(*unique_part_id, new_unique_part_id);

                        let item_index = unprocessed_unique_composed_parts
                            .iter()
                            .enumerate()
                            .find_map(|(index, u)| {
                                if *u == unique_part_id {
                                    Some(index)
                                } else {
                                    None
                                }
                            });
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

                    // self.mark_part_instance_changed(new_instance);
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
        self.changed_parts.clear();
        self.changed_part_instances.clear();
    }

    fn mark_part_changed(&mut self, part_id: PartId, change: EntityChanges) {
        self.changed_parts.insert(part_id, change);
    }

    fn mark_part_instance_changed(&mut self, instance_id: PartInstanceId, change: EntityChanges) {
        self.changed_part_instances.insert(instance_id, change);
    }

    pub fn get_changed_parts(&self) -> &HashMap<PartId, EntityChanges> {
        &self.changed_parts
    }

    pub fn get_changed_part_instances(&self) -> &HashMap<PartInstanceId, EntityChanges> {
        &self.changed_part_instances
    }

    pub fn clear_changed_parts(&mut self) {
        self.changed_parts.clear();
    }

    pub fn clear_changed_part_instances(&mut self) {
        self.changed_part_instances.clear();
    }

    pub fn has_any_changes(&self) -> bool {
        !self.changed_parts.is_empty() || !self.changed_part_instances.is_empty()
    }

    pub fn get_archived_bytes(&self) -> Result<AlignedVec, rkyv::rancor::Error> {
        to_bytes::<rkyv::rancor::Error>(self)
    }

    pub unsafe fn from_bytes(bytes: &AlignedVec) -> Result<Db, rkyv::rancor::Error> {
        unsafe { rkyv::from_bytes_unchecked(bytes) }
    }

    pub unsafe fn restore_from_bytes(&mut self, bytes: &AlignedVec) -> Result<(), DbError> {
        unsafe {
            match Self::from_bytes(bytes) {
                Ok(db) => {
                    *self = db;
                    Ok(())
                }
                Err(err) => Err(DbError::RestoreError(err.to_string())),
            }
        }
    }

    pub fn create_detached_db(
        &mut self,
        parts_to_detach: &[PartId],
        part_instances_to_detach: &[PartInstanceId],
    ) -> Result<DetachedDb, DbError> {
        let mut detached_db = DetachedDb::new();

        for id in parts_to_detach {
            match self.detach_part(id) {
                Ok(part) => {
                    if let Err(err) = detached_db.add_detached_part(id, part) {
                        panic!("Registering part to detached db failed: {err:?}")
                    }
                }
                Err(err) => panic!("Detaching parts failed: {err:?}"),
            }
        }

        for id in part_instances_to_detach {
            match self.detach_part_instance(id) {
                Ok(part_instance) => {
                    if let Err(err) = detached_db.add_detached_part_instance(id, part_instance) {
                        panic!("Registering part instances to detached db failed: {err:?}");
                    }
                }
                Err(err) => panic!("Detaching instances to detached db failed: {err:?}"),
            }
        }

        Ok(detached_db)
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

    pub fn get_all_parts_and_part_instances_in_part(
        &self,
        part_id: &PartId,
    ) -> Result<(Vec<PartId>, Vec<PartInstanceId>), DbError> {
        let part = self.get_part_data(part_id)?;

        match part.get_rep() {
            PartRep::Mesh(_) => Ok((vec![*part_id], vec![])),
            PartRep::ComposedPart(part_instance_ids) => {
                let mut all_parts = vec![];
                let mut all_instances = vec![];

                for id in part_instance_ids {
                    all_instances.push(*id);
                    let part_instance = self.get_part_instance_data(id)?;
                    let (mut parts, mut instances) =
                        self.get_all_parts_and_part_instances_in_part(&part_instance.part_id)?;
                    all_parts.append(&mut parts);
                    all_instances.append(&mut instances);
                }

                // ensure no repeating parts/part instances are in the list
                let mut part_ids = HashSet::new();
                for id in all_parts {
                    part_ids.insert(id);
                }

                let mut part_instances = HashSet::new();
                for id in all_instances {
                    part_instances.insert(id);
                }

                //include the original part id as well
                part_ids.insert(*part_id);

                Ok((
                    part_ids.into_iter().collect(),
                    part_instances.into_iter().collect(),
                ))
            }
        }
    }

    /// Returns true if all parts can be detached
    /// Returns false on the first part that cannot be detached.
    /// Returns true if the part_ids is empty
    pub fn can_detach_parts(&self, part_ids: &[PartId]) -> bool {
        if part_ids.is_empty() {
            true
        } else {
            for id in part_ids {
                if self.detached_unique_parts.contains(id) {
                    return false;
                }
            }

            true
        }
    }

    /// Returns true if all part instances can eb detached
    /// Returns false on the first part instance that cannot be detached.
    /// Returns true if part_instance_ids is empty
    pub fn can_detach_part_instances(&self, part_instance_ids: &[PartInstanceId]) -> bool {
        if part_instance_ids.is_empty() {
            true
        } else {
            for id in part_instance_ids {
                if self.detached_part_instances.contains(id) {
                    return false;
                }
            }

            true
        }
    }

    fn detach_part(&mut self, part_id: &PartId) -> Result<Part, DbError> {
        if let Some(part) = self.unique_parts.detach(*part_id) {
            self.detached_unique_parts.push(*part_id);
            self.mark_part_changed(*part_id, EntityChanges::Detached);
            Ok(part)
        } else {
            Err(DbError::PartIdNotFound(*part_id))
        }
    }

    fn reattach_part(&mut self, part_id: &PartId, part: Part) -> Result<(), DbError> {
        if let Some(pos) = self
            .detached_unique_parts
            .iter()
            .position(|id| id == part_id)
        {
            self.unique_parts.reattach(*part_id, part);
            self.detached_unique_parts.swap_remove(pos);
            self.mark_part_changed(*part_id, EntityChanges::Reattached);
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
            self.mark_part_instance_changed(*part_instance_id, EntityChanges::Detached);
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
            self.mark_part_instance_changed(*part_instance_id, EntityChanges::Reattached);
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

    fn can_remove_part<'a>(&'a self, id: &PartId) -> bool {
        !self
            .get_part_instances()
            .any(|(_, instance_data, _)| instance_data.part_id == *id)
    }

    fn can_remove_part_instance<'a>(&'a self, id: &PartInstanceId) -> bool {
        !self.get_parts().any(|(_, part)| match part.get_rep() {
            PartRep::Mesh(_) => false,
            PartRep::ComposedPart(part_instance_ids) => part_instance_ids.contains(&id),
        })
    }
}

impl DbWriter for Db {
    fn clear_all(&mut self) {
        self.clear_all();
    }

    fn remove_part(&mut self, id: PartId) -> bool {
        if let Some(part) = self.unique_parts.remove(id) {
            self.mark_part_changed(id, EntityChanges::Removed);

            match part.get_rep() {
                PartRep::Mesh(_) => {}
                PartRep::ComposedPart(components) => {
                    for id in components {
                        if self.can_remove_part_instance(id) {
                            let _ = self.remove_part_instance(*id);
                        }
                    }
                }
            }
            return true;
        }

        false
    }

    fn remove_part_instance(&mut self, id: PartInstanceId) -> bool {
        if self.part_instances.remove(id).is_some() {
            self.mark_part_instance_changed(id, EntityChanges::Removed);
            if let Some(scene) = &mut self.scene {
                scene.instances.retain(|instance_id| *instance_id != id);
            }
            return true;
        }

        false
    }
}

impl DbReaderWriter for Db {}

#[derive(Debug, Error)]
pub enum DetachedDbError {
    #[error("Part Exists")]
    DetachedPartAlreadyExists,

    #[error("Part Instance Exists")]
    DetachedPartInstanceAlreadyExists,
}

new_key_type! {
pub struct DetachedPartId;
}

impl rkyv::Archive for DetachedPartId {
    type Archived = ArchivedSlotMapId;

    type Resolver = ();

    fn resolve(&self, _: Self::Resolver, out: rkyv::Place<Self::Archived>) {
        out.write(ArchivedSlotMapId(self.0.as_ffi()));
    }
}

impl<S: Fallible + ?Sized> rkyv::Serialize<S> for DetachedPartId {
    fn serialize(&self, _serializer: &mut S) -> Result<Self::Resolver, S::Error> {
        Ok(())
    }
}

impl<D: Fallible + ?Sized> rkyv::Deserialize<DetachedPartId, D> for ArchivedSlotMapId {
    fn deserialize(&self, _deserializer: &mut D) -> Result<DetachedPartId, D::Error> {
        Ok(DetachedPartId(KeyData::from_ffi(self.0)))
    }
}

new_key_type! {
pub struct DetachedPartInstanceId;
}

impl rkyv::Archive for DetachedPartInstanceId {
    type Archived = ArchivedSlotMapId;

    type Resolver = ();

    fn resolve(&self, _: Self::Resolver, out: rkyv::Place<Self::Archived>) {
        out.write(ArchivedSlotMapId(self.0.as_ffi()));
    }
}

impl<S: Fallible + ?Sized> rkyv::Serialize<S> for DetachedPartInstanceId {
    fn serialize(&self, _serializer: &mut S) -> Result<Self::Resolver, S::Error> {
        Ok(())
    }
}

impl<D: Fallible + ?Sized> rkyv::Deserialize<DetachedPartInstanceId, D> for ArchivedSlotMapId {
    fn deserialize(&self, _deserializer: &mut D) -> Result<DetachedPartInstanceId, D::Error> {
        Ok(DetachedPartInstanceId(KeyData::from_ffi(self.0)))
    }
}

pub struct DetachedDb {
    detached_unique_parts: SlotMap<DetachedPartId, Part>,
    detached_part_instances: SlotMap<DetachedPartInstanceId, PartInstance>,

    map_part_id_to_detached: HashMap<PartId, DetachedPartId>,
    map_instance_id_to_detached: HashMap<PartInstanceId, DetachedPartInstanceId>,
}

#[derive(Debug, Clone, Archive, Serialize, Deserialize)]
pub struct ArchivedDetachedDb {
    detached_unique_parts: ArchivedSlotMap<DetachedPartId, Part>,
    detached_part_instances: ArchivedSlotMap<DetachedPartInstanceId, PartInstance>,

    map_part_id_to_detached: HashMap<PartId, DetachedPartId>,
    map_instance_id_to_detached: HashMap<PartInstanceId, DetachedPartInstanceId>,
}

unsafe impl rkyv::Portable for ArchivedDetachedDb {}

unsafe impl rkyv::traits::NoUndef for ArchivedDetachedDb {}

impl rkyv::Archive for DetachedDb {
    type Archived = ArchivedDetachedDb;
    type Resolver = ();

    fn resolve(&self, _: (), out: rkyv::Place<Self::Archived>) {
        out.write(ArchivedDetachedDb {
            detached_unique_parts: ArchivedSlotMap::from(&self.detached_unique_parts),
            detached_part_instances: ArchivedSlotMap::from(&self.detached_part_instances),
            map_part_id_to_detached: self.map_part_id_to_detached.clone(),
            map_instance_id_to_detached: self.map_instance_id_to_detached.clone(),
        });
    }
}

impl<S: Fallible + ?Sized> rkyv::Serialize<S> for DetachedDb {
    fn serialize(&self, _: &mut S) -> Result<Self::Resolver, S::Error> {
        Ok(())
    }
}

impl<D: Fallible + ?Sized> rkyv::Deserialize<DetachedDb, D> for ArchivedDetachedDb {
    fn deserialize(&self, _: &mut D) -> Result<DetachedDb, D::Error> {
        Ok(DetachedDb {
            detached_unique_parts: self.detached_unique_parts.clone().into_slotmap(),
            detached_part_instances: self.detached_part_instances.clone().into_slotmap(),
            map_part_id_to_detached: self.map_part_id_to_detached.clone(),
            map_instance_id_to_detached: self.map_instance_id_to_detached.clone(),
        })
    }
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

    pub fn append_db(&mut self, _other: Db) -> Result<(), DetachedDbError> {
        todo!("Implement appending to Detach Db")
    }

    pub fn get_archived_bytes(&self) -> Result<AlignedVec, rkyv::rancor::Error> {
        to_bytes::<rkyv::rancor::Error>(self)
    }

    pub unsafe fn from_archived_bytes(
        bytes: &AlignedVec,
    ) -> Result<DetachedDb, rkyv::rancor::Error> {
        unsafe { from_bytes_unchecked::<DetachedDb, rkyv::rancor::Error>(bytes) }
    }

    fn add_detached_part(
        &mut self,
        part_id: &PartId,
        part: Part,
    ) -> Result<DetachedPartId, DetachedDbError> {
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

    fn can_remove_part<'a>(&'a self, id: &PartId) -> bool {
        //for now we wont allow removing Parts from DetachedDb
        false
    }

    fn can_remove_part_instance<'a>(&'a self, id: &PartInstanceId) -> bool {
        // for now we wont allow removing Part Instances for DetachedDb
        false
    }
}

#[cfg(test)]
mod tests {

    use glam::Vec3;
    use slotmap::Key;

    use crate::core::types::mesh::Mesh;

    use super::*;

    #[test]
    fn add_and_retrieve_mesh_part() {
        let mut db = Db::new();
        let mesh = Mesh {
            vertices: vec![Vec3::new(0.0, 0.0, 0.0), Vec3::new(1.0, 0.0, 0.0)],
            triangles: vec![0, 1, 0], // minimal triangle
        };
        let part_id = db
            .add_part_rep(PartRep::Mesh(Box::new(mesh.clone())))
            .unwrap();

        let retrieved = db.get_part_data(&part_id).unwrap();
        match &retrieved.get_rep() {
            PartRep::Mesh(m) => assert_eq!(m.vertices.len(), mesh.vertices.len()),
            _ => panic!("Expected Mesh part"),
        }
    }

    #[test]
    fn composed_part_with_invalid_instance_fails() {
        let mut db = Db::new();
        let invalid_instance_id = PartInstanceId::null();
        let result = db.add_part_rep(PartRep::ComposedPart(vec![invalid_instance_id]));
        assert!(matches!(result, Err(DbError::PartInstancesNotFound(_))));
    }

    #[test]
    fn create_part_instance_and_get_data() {
        let mut db = Db::new();
        let mesh_id = db
            .add_part_rep(PartRep::Mesh(Box::new(Mesh {
                vertices: vec![],
                triangles: vec![],
            })))
            .unwrap();

        let instance_id = db.make_new_part_instance_from_part(&mesh_id, None).unwrap();
        let instance_data = db.get_part_instance_data(&instance_id).unwrap();

        assert_eq!(instance_data.part_id, mesh_id);
        assert_eq!(instance_data.transform, Transformation::default());
    }

    #[test]
    fn add_part_instance_to_scene_and_retrieve() {
        let mut db = Db::new();
        let mesh_id = db
            .add_part_rep(PartRep::Mesh(Box::new(Mesh {
                vertices: vec![],
                triangles: vec![],
            })))
            .unwrap();

        let instance_id = db.make_new_part_instance_from_part(&mesh_id, None).unwrap();
        db.add_part_instance_to_scene(&instance_id).unwrap();

        let scene = db.get_scene().unwrap();
        assert_eq!(scene.instances.len(), 1);
        assert_eq!(scene.instances[0], instance_id);
    }

    #[test]
    fn append_two_dbs_merges_parts_and_instances() {
        let mut db1 = Db::new();
        let mut db2 = Db::new();

        let p1 = db1
            .add_part_rep(PartRep::Mesh(Box::new(Mesh {
                vertices: vec![],
                triangles: vec![],
            })))
            .unwrap();
        let i1 = db1.make_new_part_instance_from_part(&p1, None).unwrap();
        db1.add_part_instance_to_scene(&i1).unwrap();

        let p2 = db2
            .add_part_rep(PartRep::Mesh(Box::new(Mesh {
                vertices: vec![],
                triangles: vec![],
            })))
            .unwrap();
        let i2 = db2.make_new_part_instance_from_part(&p2, None).unwrap();
        db2.add_part_instance_to_scene(&i2).unwrap();

        assert!(db1.append(db2).is_ok());
        assert!(db1.get_parts_count() >= 2);
        assert!(db1.get_scene().unwrap().instances.len() >= 2);
    }

    #[test]
    fn detach_and_reattach_part() {
        let mut db = Db::new();
        let part_id = db
            .add_part_rep(PartRep::Mesh(Box::new(Mesh {
                vertices: vec![],
                triangles: vec![],
            })))
            .unwrap();

        // Detach
        let detached_db = db.create_detached_db(&[part_id], &[]).unwrap();
        assert!(db.detached_unique_parts.contains(&part_id));

        // Reattach
        db.reattach(detached_db).unwrap();
        assert!(!db.detached_unique_parts.contains(&part_id));
    }

    #[test]
    fn detach_and_reattach_instance() {
        let mut db = Db::new();
        let part_id = db
            .add_part_rep(PartRep::Mesh(Box::new(Mesh {
                vertices: vec![],
                triangles: vec![],
            })))
            .unwrap();
        let instance_id = db.make_new_part_instance_from_part(&part_id, None).unwrap();

        let detached_db = db.create_detached_db(&[], &[instance_id]).unwrap();
        assert!(db.detached_part_instances.contains(&instance_id));

        db.reattach(detached_db).unwrap();
        assert!(!db.detached_part_instances.contains(&instance_id));
    }

    #[test]
    fn can_detach_checks() {
        let mut db = Db::new();
        let p = db
            .add_part_rep(PartRep::Mesh(Box::new(Mesh {
                vertices: vec![],
                triangles: vec![],
            })))
            .unwrap();

        assert!(db.can_detach_parts(&[p]));
        let _ = db.detach_part(&p).unwrap();
        assert!(!db.can_detach_parts(&[p]));
    }

    #[test]
    fn test_db_archiving_round_trip() {
        let mut db = Db::new();
        // Add some test data: a mesh part and instance
        let mesh = Mesh {
            vertices: vec![Vec3::new(0.0, 0.0, 0.0), Vec3::new(1.0, 0.0, 0.0)],
            triangles: vec![0, 1, 0],
        };
        let part_id = db.add_part_rep(PartRep::Mesh(Box::new(mesh))).unwrap();
        let instance_id = db.make_new_part_instance_from_part(&part_id, None).unwrap();
        db.add_part_instance_to_scene(&instance_id).unwrap();

        // Archive
        let archived = db.get_archived_bytes().unwrap();

        // Unarchive
        let restored_db = unsafe { Db::from_bytes(&archived).unwrap() };

        // Verify: Compare key fields (parts, instances, scene)
        assert_eq!(db.get_parts().count(), restored_db.get_parts().count());
        assert_eq!(
            db.get_part_instances().count(),
            restored_db.get_part_instances().count()
        );
        assert_eq!(
            db.get_scene().unwrap().instances.len(),
            restored_db.get_scene().unwrap().instances.len()
        );
    }

    #[test]
    fn test_restore_from_bytes() {
        let mut db = Db::new();
        // Initial state: Add a part
        let mesh = Mesh {
            vertices: vec![Vec3::new(1.0, 1.0, 1.0)],
            triangles: vec![0],
        };
        let _part_id = db.add_part_rep(PartRep::Mesh(Box::new(mesh))).unwrap();

        // Archive initial state
        let archived = db.get_archived_bytes().unwrap();

        // Modify DB: Add another part
        let mesh2 = Mesh {
            vertices: vec![Vec3::new(2.0, 2.0, 2.0)],
            triangles: vec![0],
        };
        db.add_part_rep(PartRep::Mesh(Box::new(mesh2))).unwrap();
        assert_eq!(db.get_parts().count(), 2); // Verify modification

        // Restore from archive
        unsafe {
            db.restore_from_bytes(&archived).unwrap();
        }

        // Verify restoration: Should have only 1 part again
        assert_eq!(db.get_parts().count(), 1);
    }

    #[test]
    fn test_archiving_error_on_invalid_bytes() {
        let invalid_bytes = AlignedVec::new(); // Empty bytes

        // Test from_bytes
        let result = unsafe { Db::from_bytes(&invalid_bytes) };
        assert!(result.is_err());

        // Test restore_from_bytes on a valid DB with invalid bytes
        let mut db = Db::new();
        let result = unsafe { db.restore_from_bytes(&invalid_bytes) };
        assert!(result.is_err());
        assert!(matches!(result, Err(DbError::RestoreError(_))));
    }

    #[test]
    fn test_archiving_with_composed_parts() {
        let mut db = Db::new();
        // Create mesh parts
        let mesh1 = Mesh {
            vertices: vec![Vec3::ZERO],
            triangles: vec![0],
        };
        let part1_id = db.add_part_rep(PartRep::Mesh(Box::new(mesh1))).unwrap();
        let mesh2 = Mesh {
            vertices: vec![Vec3::ONE],
            triangles: vec![0],
        };
        let part2_id = db.add_part_rep(PartRep::Mesh(Box::new(mesh2))).unwrap();

        // Create instances and composed part
        let inst1 = db
            .make_new_part_instance_from_part(&part1_id, None)
            .unwrap();
        let inst2 = db
            .make_new_part_instance_from_part(&part2_id, None)
            .unwrap();
        let _composed_id = db
            .add_part_rep(PartRep::ComposedPart(vec![inst1, inst2]))
            .unwrap();

        // Archive and restore
        let archived = db.get_archived_bytes().unwrap();
        let restored_db = unsafe { Db::from_bytes(&archived).unwrap() };

        // Verify composed part and instances
        assert_eq!(restored_db.get_parts().count(), 3); // 2 meshes + 1 composed
        assert_eq!(restored_db.get_part_instances().count(), 2); // 2 instances
    }

    #[test]
    fn get_all_parts_and_instances_in_composed_part() {
        let mut db = Db::new();
        let mesh_id = db
            .add_part_rep(PartRep::Mesh(Box::new(Mesh {
                vertices: vec![],
                triangles: vec![],
            })))
            .unwrap();
        let inst_id = db.make_new_part_instance_from_part(&mesh_id, None).unwrap();

        let composed_id = db
            .add_part_rep(PartRep::ComposedPart(vec![inst_id]))
            .unwrap();
        let (parts, instances) = db
            .get_all_parts_and_part_instances_in_part(&composed_id)
            .unwrap();

        assert!(parts.contains(&mesh_id));
        assert!(instances.contains(&inst_id));
    }

    #[test]
    fn test_db_serialization_roundtrip() {
        let mut db = Db::new();

        // Add a Mesh part
        let mesh_id = db
            .add_part_rep(PartRep::Mesh(Box::new(Mesh {
                vertices: vec![
                    Vec3::new(0.0, 0.0, 0.0),
                    Vec3::new(1.0, 0.0, 0.0),
                    Vec3::new(0.0, 1.0, 0.0),
                ],
                triangles: vec![0, 1, 2],
            })))
            .unwrap();

        // Create part instance
        let instance_id = db
            .make_new_part_instance_from_part(&mesh_id, Some(Transformation::default()))
            .unwrap();

        // Add to scene
        db.add_part_instance_to_scene(&instance_id).unwrap();

        let parts_before = db.get_parts_count();
        let instances_before = db.get_part_instance_count();
        let scene_before = db.get_scene().unwrap().instances.clone();

        // Serialize
        let bytes = db.get_archived_bytes().unwrap();

        // Deserialize with unsafe from_bytes (as per your API)
        let db_restored = unsafe { Db::from_bytes(&bytes) }.unwrap();

        // Compare counts
        assert_eq!(db_restored.get_parts_count(), parts_before);
        assert_eq!(db_restored.get_part_instance_count(), instances_before);

        // Compare scene
        let scene_after = db_restored.get_scene().unwrap().instances.clone();
        assert_eq!(scene_after, scene_before);
    }

    #[test]
    fn test_detached_db_serialization_roundtrip() {
        let mut db = Db::new();

        // Add mesh part + instance
        let mesh_id = db
            .add_part_rep(PartRep::Mesh(Box::new(Mesh {
                vertices: vec![Vec3::new(0.0, 0.0, 0.0)],
                triangles: vec![0, 1, 2],
            })))
            .unwrap();

        let instance_id = db.make_new_part_instance_from_part(&mesh_id, None).unwrap();

        // Detach both
        let detached_db = db.create_detached_db(&[mesh_id], &[instance_id]).unwrap();

        let parts_before = detached_db.get_parts_count();
        let instances_before = detached_db.get_part_instance_count();

        // Serialize DetachedDb
        let bytes = detached_db.get_archived_bytes().unwrap();

        // Deserialize
        let detached_restored = unsafe { DetachedDb::from_archived_bytes(&bytes) }.unwrap();

        // Compare counts
        assert_eq!(detached_restored.get_parts_count(), parts_before);
        assert_eq!(
            detached_restored.get_part_instance_count(),
            instances_before
        );

        // Compare mappings to ensure IDs are preserved
        for (original_id, detached_id) in &detached_db.map_part_id_to_detached {
            let detached_id_restored = detached_restored
                .map_part_id_to_detached
                .get(original_id)
                .unwrap();
            assert_eq!(detached_id, detached_id_restored);
        }

        for (original_id, detached_id) in &detached_db.map_instance_id_to_detached {
            let detached_id_restored = detached_restored
                .map_instance_id_to_detached
                .get(original_id)
                .unwrap();
            assert_eq!(detached_id, detached_id_restored);
        }
    }

    #[test]
    fn test_db_and_detached_db_roundtrip_combined() {
        let mut db = Db::new();

        // Add two parts
        let mesh_id1 = db
            .add_part_rep(PartRep::Mesh(Box::new(Mesh {
                vertices: vec![Vec3::new(1.0, 0.0, 0.0)],
                triangles: vec![0, 1, 2],
            })))
            .unwrap();

        let mesh_id2 = db
            .add_part_rep(PartRep::Mesh(Box::new(Mesh {
                vertices: vec![Vec3::new(0.0, 0.0, 1.0)],
                triangles: vec![0, 1, 2],
            })))
            .unwrap();

        // Create instances
        let inst1 = db
            .make_new_part_instance_from_part(&mesh_id1, None)
            .unwrap();
        let inst2 = db
            .make_new_part_instance_from_part(&mesh_id2, None)
            .unwrap();

        db.add_part_instance_to_scene(&inst1).unwrap();
        db.add_part_instance_to_scene(&inst2).unwrap();

        // Detach one part + one instance
        let detached_db = db.create_detached_db(&[mesh_id1], &[inst2]).unwrap();

        // Serialize DB
        let db_bytes = db.get_archived_bytes().unwrap();
        let db_restored = unsafe { Db::from_bytes(&db_bytes) }.unwrap();

        assert_eq!(db.get_parts_count(), db_restored.get_parts_count());
        assert_eq!(
            db.get_part_instance_count(),
            db_restored.get_part_instance_count()
        );

        // Serialize DetachedDb
        let detached_bytes = detached_db.get_archived_bytes().unwrap();
        let detached_restored =
            unsafe { DetachedDb::from_archived_bytes(&detached_bytes) }.unwrap();

        assert_eq!(
            detached_db.get_parts_count(),
            detached_restored.get_parts_count()
        );
        assert_eq!(
            detached_db.get_part_instance_count(),
            detached_restored.get_part_instance_count()
        );
    }

    #[test]
    fn test_dberror_part_id_not_found() {
        let db = Db::new();
        let fake_id = PartId::null();
        let err = db.get_part_data(&fake_id).unwrap_err();
        assert!(matches!(err, DbError::PartIdNotFound(id) if id == fake_id));
    }

    #[test]
    fn test_dberror_part_instance_id_not_found() {
        let db = Db::new();
        let fake_id = PartInstanceId::null();
        let err = db.get_part_instance_data(&fake_id).unwrap_err();
        assert!(matches!(err, DbError::PartInstanceIdNotFound(id) if id == fake_id));
    }

    #[test]
    fn test_dberror_invalid_part_rep_mesh_and_composed() {
        let mut db = Db::new();

        // Composed part with nonexistent instance
        let fake_instance_id = PartInstanceId::null();
        let err = db
            .add_part_rep(PartRep::ComposedPart(vec![fake_instance_id]))
            .unwrap_err();
        assert!(
            matches!(err, DbError::PartInstancesNotFound(ids) if ids.contains(&fake_instance_id))
        );
    }

    #[test]
    fn test_dberror_scenarios_scene_not_set() {
        let db = Db::new();
        let err = db.get_scene().unwrap_err();
        assert!(matches!(err, DbError::SceneNotSet));
    }

    #[test]
    fn test_dberror_append_scene_not_set() {
        let mut db1 = Db::new();
        let db2 = Db::new();
        let err = db1.append(db2).unwrap_err();
        assert!(matches!(err, DbError::SceneNotSet));
    }

    #[test]
    fn test_detachederror_part_already_exists() {
        let mut db = Db::new();

        let mesh_id = db
            .add_part_rep(PartRep::Mesh(Box::new(Mesh {
                vertices: vec![Vec3::new(0.0, 0.0, 0.0)],
                triangles: vec![0, 1, 2],
            })))
            .unwrap();

        let mut detached_db = DetachedDb::new();
        let part = db.detach_part(&mesh_id).unwrap();

        // First addition should succeed
        detached_db
            .add_detached_part(&mesh_id, part.clone())
            .unwrap();

        // Second addition should fail
        let err = detached_db.add_detached_part(&mesh_id, part).unwrap_err();
        assert!(matches!(err, DetachedDbError::DetachedPartAlreadyExists));
    }

    #[test]
    fn test_detachederror_part_instance_already_exists() {
        let mut db = Db::new();

        let mesh_id = db
            .add_part_rep(PartRep::Mesh(Box::new(Mesh {
                vertices: vec![Vec3::new(0.0, 0.0, 0.0)],
                triangles: vec![0, 1, 2],
            })))
            .unwrap();

        let instance_id = db.make_new_part_instance_from_part(&mesh_id, None).unwrap();

        let mut detached_db = DetachedDb::new();
        let instance = db.detach_part_instance(&instance_id).unwrap();

        // First addition should succeed
        detached_db
            .add_detached_part_instance(&instance_id, instance.clone())
            .unwrap();

        // Second addition should fail
        let err = detached_db
            .add_detached_part_instance(&instance_id, instance)
            .unwrap_err();
        assert!(matches!(
            err,
            DetachedDbError::DetachedPartInstanceAlreadyExists
        ));
    }
}
