#![allow(clippy::needless_lifetimes)]
use amrust_render::transformation::TransformationData;
use glam::{Mat4, Vec3};
use rkyv::api::low::from_bytes_unchecked;
use rkyv::bytecheck::CheckBytes;
use rkyv::rancor::Fallible;
use rkyv::to_bytes;
use rkyv::util::AlignedVec;
use rkyv_derive::{Archive, Deserialize, Serialize};
use slotmap::{KeyData, SlotMap, new_key_type};
use smol::lock::RwLock;
use thiserror::Error;

use amrust_render::{
    bounding_box::BoundingBox, gpu_mesh::MeshBuilder, instance::InstanceDataBuilder,
    material::Material, vertex::Position,
};

use core::fmt;
use std::collections::{HashMap, HashSet};
use std::fmt::Debug;
use std::sync::Arc;

use crate::operation::DbReader;
use crate::render_db::{RenderDb, RenderObject};
use crate::tree_item_viewer::TreeItem;

#[derive(Debug, Clone, Archive, Serialize, Deserialize, CheckBytes)]
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

unsafe impl<C: ?Sized + Fallible> CheckBytes<C> for PartId {
    unsafe fn check_bytes(
        _value: *const Self,
        _context: &mut C,
    ) -> Result<(), <C as Fallible>::Error> {
        Ok(())
    }
}

#[derive(Debug, Archive, CheckBytes, Hash, PartialEq, Eq)]
#[repr(transparent)]
pub struct ArchivedSlotMapId(u64);

unsafe impl rkyv::Portable for ArchivedSlotMapId {}

unsafe impl rkyv::traits::NoUndef for ArchivedSlotMapId {}

impl rkyv::Archive for PartId {
    type Archived = ArchivedSlotMapId;

    type Resolver = ();

    fn resolve(&self, _: Self::Resolver, out: rkyv::Place<Self::Archived>) {
        out.write(ArchivedSlotMapId(self.0.as_ffi()));
    }
}

impl<S: Fallible + ?Sized> rkyv::Serialize<S> for PartId {
    fn serialize(&self, _serializer: &mut S) -> Result<Self::Resolver, S::Error> {
        Ok(())
    }
}

impl<D: Fallible + ?Sized> rkyv::Deserialize<PartId, D> for ArchivedSlotMapId {
    fn deserialize(&self, _deserializer: &mut D) -> Result<PartId, D::Error> {
        Ok(PartId(KeyData::from_ffi(self.0)))
    }
}

#[derive(Debug, Clone, Archive, Serialize, Deserialize)]
pub enum PartRep {
    Mesh(Box<Mesh>),
    ComposedPart(Vec<PartInstanceId>),
}

unsafe impl rkyv::Portable for PartRep {}

#[derive(Debug, Clone, Copy, Default, PartialEq, Archive, Serialize, Deserialize, CheckBytes)]
pub struct Transformation(pub Mat4);

impl Transformation {
    pub fn into_data(&self) -> TransformationData {
        TransformationData(self.0.to_cols_array_2d())
    }
}

impl From<Transformation> for amrust_render::transformation::Transformation {
    fn from(value: Transformation) -> Self {
        amrust_render::transformation::Transformation(value.0)
    }
}

impl From<amrust_render::transformation::Transformation> for Transformation {
    fn from(value: amrust_render::transformation::Transformation) -> Self {
        Transformation(value.0)
    }
}

#[derive(Debug, Clone, Archive, Serialize, Deserialize, CheckBytes)]
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

impl rkyv::Archive for PartInstanceId {
    type Archived = ArchivedSlotMapId;

    type Resolver = ();

    fn resolve(&self, _: Self::Resolver, out: rkyv::Place<Self::Archived>) {
        out.write(ArchivedSlotMapId(self.0.as_ffi()));
    }
}

impl<S: Fallible + ?Sized> rkyv::Serialize<S> for PartInstanceId {
    fn serialize(&self, _serializer: &mut S) -> Result<Self::Resolver, S::Error> {
        Ok(())
    }
}

impl<D: Fallible + ?Sized> rkyv::Deserialize<PartInstanceId, D> for ArchivedSlotMapId {
    fn deserialize(&self, _deserializer: &mut D) -> Result<PartInstanceId, D::Error> {
        Ok(PartInstanceId(KeyData::from_ffi(self.0)))
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash)]
pub enum Identifiable {
    Part(PartId),
    PartInstance(PartInstanceId),
}

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

    pub fn get_archived_bytes(&self) -> Result<AlignedVec, rkyv::rancor::Error> {
        to_bytes::<rkyv::rancor::Error>(self)
    }

    pub unsafe fn from_bytes(bytes: &AlignedVec) -> Result<Db, rkyv::rancor::Error> {
        unsafe { rkyv::from_bytes_unchecked(bytes) }
    }

    pub unsafe fn restore_from_bytes(&mut self, bytes: &AlignedVec) -> Result<(), DbError> {
        unsafe {
            match Self::from_bytes(bytes) {
                Ok(_) => {
                    todo!("Restore the Db from Archived Bytes");
                }
                Err(err) => {
                    todo!("Handle the restore from bytes error correctly: {err:?}")
                }
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

// pub fn create_scene_tree_items_by_unique_parts(
//     db: Arc<RwLock<Db>>,
// ) -> Result<Vec<TreeItem<Identifiable>>, DbError> {
//     let read_db = db.read_blocking();
//     let scene = read_db.get_scene()?;

//     let mut instance_id_to_tree_item_map: HashMap<PartInstanceId, TreeItem<Identifiable>> =
//         HashMap::new();

//     let mut unprocessed_instances = vec![];

//     // process all the items one round first
//     for (id, _, part) in read_db.get_part_instances() {
//         match &part.rep {
//             PartRep::Mesh(_) => {
//                 instance_id_to_tree_item_map.insert(
//                     id,
//                     TreeItem::Leaf {
//                         id: Identifiable::PartInstance(id),
//                         name: format!("Mesh: {:?}", id),
//                         selectable: true,
//                     },
//                 );
//             }
//             PartRep::ComposedPart(part_instance_ids) => {
//                 let mut tree_items = vec![];
//                 for id in part_instance_ids {
//                     if let Some(item) = instance_id_to_tree_item_map.get(id) {
//                         tree_items.push(item.clone());
//                     }
//                 }

//                 if tree_items.len() != part_instance_ids.len() {
//                     unprocessed_instances.push(id);
//                     continue;
//                 } else {
//                     let node = TreeItem::Node {
//                         id: Identifiable::PartInstance(id),
//                         name: format!("Composed Part: {:?}", id),
//                         childs: tree_items,
//                         selectable: true,
//                     };
//                     instance_id_to_tree_item_map.insert(id, node);
//                 }
//             }
//         }
//     }

//     //ToDo: Process unprocessed items
//     // Second pass: process unprocessed until none left or can't resolve more
//     let mut changed = true;
//     while changed && !unprocessed_instances.is_empty() {
//         changed = false;
//         let mut still_unresolved = vec![];

//         for id in unprocessed_instances.drain(..) {
//             let part = read_db.get_part_data_from_part_instance(&id)?;
//             if let PartRep::ComposedPart(part_instance_ids) = &part.rep {
//                 let mut tree_items = vec![];
//                 let mut all_children_ready = true;

//                 for child_id in part_instance_ids {
//                     if let Some(item) = instance_id_to_tree_item_map.get(child_id) {
//                         tree_items.push(item.clone());
//                     } else {
//                         all_children_ready = false;
//                         break;
//                     }
//                 }

//                 if all_children_ready {
//                     let node = TreeItem::Node {
//                         id: Identifiable::PartInstance(id),
//                         name: format!("Composed Part: {:?}", id),
//                         childs: tree_items,
//                         selectable: true,
//                     };
//                     instance_id_to_tree_item_map.insert(id, node);
//                     changed = true;
//                 } else {
//                     still_unresolved.push(id);
//                 }
//             }
//         }

//         unprocessed_instances = still_unresolved;
//     }

//     let mut unique_part_id_to_instance_tree_item: HashMap<PartId, TreeItem<Identifiable>> =
//         HashMap::new();
//     for i in &scene.instances {
//         let instance_data = read_db.get_part_instance_data(i)?;

//         if let Some(item) = instance_id_to_tree_item_map.get(i) {
//             if let Some(unique_item) =
//                 unique_part_id_to_instance_tree_item.get_mut(&instance_data.part_id)
//             {
//                 //unique part entry should always be a node
//                 // if let TreeItem::Node { childs, .. } = unique_item {
//                 //     childs.push(item.clone());
//                 // }
//                 match unique_item {
//                     TreeItem::Node { childs, .. } | TreeItem::InertNode { childs, .. } => {
//                         childs.push(item.clone());
//                     }
//                     _ => {}
//                 }
//             } else {
//                 let tree_item = TreeItem::InertNode {
//                     id: Identifiable::Part(instance_data.part_id),
//                     name: format!("Unique Part: {:?}", instance_data.part_id),
//                     childs: vec![item.clone()],
//                 };

//                 unique_part_id_to_instance_tree_item.insert(instance_data.part_id, tree_item);
//             }
//         }
//     }

//     let items = unique_part_id_to_instance_tree_item
//         .into_values()
//         .collect::<Vec<_>>();

//     Ok(items)
// }

pub fn create_scene_tree_items_by_unique_parts(
    db: Arc<RwLock<Db>>,
) -> Result<Vec<TreeItem<Identifiable>>, DbError> {
    let read_db = db.read_blocking();
    let scene = read_db.get_scene()?;

    let mut instance_id_to_tree_item_map: HashMap<PartInstanceId, TreeItem<Identifiable>> =
        HashMap::new();
    let mut unprocessed_instances = Vec::<PartInstanceId>::new();

    // Pass 1: process ALL Mesh parts first
    for (id, _, part) in read_db.get_part_instances() {
        if let PartRep::Mesh(_) = &part.rep {
            instance_id_to_tree_item_map.insert(
                id,
                TreeItem::Leaf {
                    id: Identifiable::PartInstance(id),
                    name: format!("Mesh: {:?}", id),
                    selectable: true,
                },
            );
        }
    }

    // Pass 2: process ComposedPart where children might already be ready
    for (id, _, part) in read_db.get_part_instances() {
        if let PartRep::ComposedPart(part_instance_ids) = &part.rep {
            let mut tree_items = vec![];
            for child_id in part_instance_ids {
                if let Some(item) = instance_id_to_tree_item_map.get(child_id) {
                    tree_items.push(item.clone());
                }
            }

            if tree_items.len() != part_instance_ids.len() {
                // Not all children ready yet → resolve later
                unprocessed_instances.push(id);
                continue;
            }

            instance_id_to_tree_item_map.insert(
                id,
                TreeItem::Node {
                    id: Identifiable::PartInstance(id),
                    name: format!("Composed Part: {:?}", id),
                    childs: tree_items,
                    selectable: true,
                },
            );
        }
    }

    // Pass 3: iterative resolution of deeper/nested composed parts
    let mut changed = true;
    while changed && !unprocessed_instances.is_empty() {
        changed = false;
        let mut still_unresolved = vec![];

        for id in unprocessed_instances.drain(..) {
            let part = read_db.get_part_data_from_part_instance(&id)?;
            if let PartRep::ComposedPart(part_instance_ids) = &part.rep {
                let mut tree_items = vec![];
                let mut all_children_ready = true;

                for child_id in part_instance_ids {
                    if let Some(item) = instance_id_to_tree_item_map.get(child_id) {
                        tree_items.push(item.clone());
                    } else {
                        all_children_ready = false;
                        break;
                    }
                }

                if all_children_ready {
                    instance_id_to_tree_item_map.insert(
                        id,
                        TreeItem::Node {
                            id: Identifiable::PartInstance(id),
                            name: format!("Composed Part: {:?}", id),
                            childs: tree_items,
                            selectable: true,
                        },
                    );
                    changed = true;
                } else {
                    still_unresolved.push(id);
                }
            }
        }

        unprocessed_instances = still_unresolved;
    }

    // Pass 4: group scene instances by unique part ID
    let mut unique_part_id_to_instance_tree_item: HashMap<PartId, TreeItem<Identifiable>> =
        HashMap::new();

    for i in &scene.instances {
        let instance_data = read_db.get_part_instance_data(i)?;

        if let Some(item) = instance_id_to_tree_item_map.get(i) {
            match unique_part_id_to_instance_tree_item.get_mut(&instance_data.part_id) {
                Some(unique_item) => {
                    // Append to Node or InertNode
                    match unique_item {
                        TreeItem::Node { childs, .. } => childs.push(item.clone()),
                        TreeItem::InertNode { childs, .. } => childs.push(item.clone()),
                        _ => {}
                    }
                }
                None => {
                    // Create a new unique part root with first child
                    let tree_item = TreeItem::InertNode {
                        id: Identifiable::Part(instance_data.part_id),
                        name: format!("Unique Part: {:?}", instance_data.part_id),
                        childs: vec![item.clone()],
                    };
                    unique_part_id_to_instance_tree_item.insert(instance_data.part_id, tree_item);
                }
            }
        }
    }

    Ok(unique_part_id_to_instance_tree_item.into_values().collect())
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
        .map(|t| t.into_data())
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
            .add_instance_stream(&[Transformation(Mat4::IDENTITY).into_data()])
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

#[cfg(test)]
mod tests {
    use amrust_render::vertex::Color;
    use slotmap::Key;

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
        match &retrieved.rep {
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
    fn test_convert_points_vec_to_position() {
        let points = vec![Vec3::new(1.0, 2.0, 3.0), Vec3::new(-1.0, -2.0, -3.0)];

        let positions = convert_points_vec_to_position(&points);
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
            assert_eq!(color, Color([0.5, 0.5, 0.5]));
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

    #[test]
    fn test_compute_instance_bbox_with_mesh_part() {
        let db = Arc::new(RwLock::new(Db::new()));
        let mut write_db = db.write_blocking();
        let mesh_id = write_db
            .add_part_rep(PartRep::Mesh(Box::new(Mesh {
                vertices: vec![
                    Vec3::new(0.0, 0.0, 0.0),
                    Vec3::new(1.0, 1.0, 1.0),
                    Vec3::new(2.0, 0.0, 2.0),
                ],
                triangles: vec![0, 1, 2],
            })))
            .unwrap();

        let instance_id = write_db
            .make_new_part_instance_from_part(&mesh_id, None)
            .unwrap();
        let instance_data = write_db
            .get_part_instance_data(&instance_id)
            .unwrap()
            .clone();
        drop(write_db);
        let bbox =
            compute_instance_bbox(db.clone(), &instance_data, &Transformation(Mat4::IDENTITY))
                .unwrap();

        assert_eq!(bbox.min, Vec3::new(0.0, 0.0, 0.0));
        assert_eq!(bbox.max, Vec3::new(2.0, 1.0, 2.0));
    }

    #[test]
    fn test_compute_instance_bbox_with_composed_part() {
        let db = Arc::new(RwLock::new(Db::new()));
        let mut write_db = db.write_blocking();
        let mesh_id = write_db
            .add_part_rep(PartRep::Mesh(Box::new(Mesh {
                vertices: vec![
                    Vec3::new(0.0, 0.0, 0.0),
                    Vec3::new(1.0, 1.0, 1.0),
                    Vec3::new(2.0, 0.0, 2.0),
                ],
                triangles: vec![0, 1, 2],
            })))
            .unwrap();

        let translated_instance_id = write_db
            .make_new_part_instance_from_part(
                &mesh_id,
                Some(Transformation(Mat4::from_translation(Vec3::new(
                    5.0, 5.0, 5.0,
                )))),
            )
            .unwrap();

        let composed_part_id = write_db
            .add_part_rep(PartRep::ComposedPart(vec![translated_instance_id]))
            .unwrap();

        let composed_instance_id = write_db
            .make_new_part_instance_from_part(&composed_part_id, None)
            .unwrap();
        let composed_instance_data = write_db
            .get_part_instance_data(&composed_instance_id)
            .unwrap()
            .clone();
        drop(write_db);

        let bbox = compute_instance_bbox(
            db.clone(),
            &composed_instance_data,
            &Transformation(Mat4::IDENTITY),
        )
        .unwrap();

        // Mesh bbox min(0,0,0) max(2,1,2) shifted by (5,5,5)
        assert_eq!(bbox.min, Vec3::new(5.0, 5.0, 5.0));
        assert_eq!(bbox.max, Vec3::new(7.0, 6.0, 7.0));
    }

    #[test]
    fn test_create_build_items_list_with_single_mesh_instance() {
        let mut db = Db::new();

        // Add mesh part
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

        // Add instance of mesh
        let instance_id = db.make_new_part_instance_from_part(&mesh_id, None).unwrap();
        db.add_part_instance_to_scene(&instance_id).unwrap();

        let db_arc = Arc::new(RwLock::new(db));
        let items = create_build_items_list(db_arc).unwrap();

        assert_eq!(items.len(), 1);
        match &items[0] {
            TreeItem::Leaf {
                id,
                name,
                selectable,
            } => {
                assert_eq!(*id, Identifiable::PartInstance(instance_id));
                assert!(name.contains("Mesh"));
                assert_eq!(*selectable, true);
            }
            _ => panic!("Expected Leaf"),
        }
    }

    #[test]
    fn test_create_objects_list_with_mesh_and_composed_part() {
        let mut db = Db::new();

        // Mesh part
        let mesh_id = db
            .add_part_rep(PartRep::Mesh(Box::new(Mesh {
                vertices: vec![Vec3::new(0.0, 0.0, 0.0)],
                triangles: vec![0, 0, 0],
            })))
            .unwrap();

        let inst_id = db.make_new_part_instance_from_part(&mesh_id, None).unwrap();

        // Composed part
        let composed_id = db
            .add_part_rep(PartRep::ComposedPart(vec![inst_id]))
            .unwrap();

        let db_arc = Arc::new(RwLock::new(db));
        let objs = create_objects_list(db_arc).unwrap();

        assert_eq!(objs.len(), 2);
        let names: Vec<_> = objs
            .iter()
            .map(|item| match item {
                TreeItem::Leaf { name, .. } => name.clone(),
                _ => panic!("Expected Leaf"),
            })
            .collect();

        assert!(names.iter().any(|n| n.contains("Mesh Object")));
        assert!(names.iter().any(|n| n.contains("Components Object")));
    }

    #[test]
    fn test_create_object_tree_from_part_mesh() {
        let mut db = Db::new();

        let mesh_id = db
            .add_part_rep(PartRep::Mesh(Box::new(Mesh {
                vertices: vec![Vec3::new(0.0, 0.0, 0.0)],
                triangles: vec![0, 0, 0],
            })))
            .unwrap();

        let db_arc = Arc::new(RwLock::new(db));
        let tree = create_object_tree_from_part(db_arc, &mesh_id).unwrap();

        match tree {
            TreeItem::InertNode { name, childs, .. } => {
                assert_eq!(name, "Mesh");
                assert!(childs.iter().any(|c| match c {
                    TreeItem::Leaf { name, .. } => name.contains("Vertices Count"),
                    _ => false,
                }));
            }
            _ => panic!("Expected InertNode"),
        }
    }

    #[test]
    fn test_create_object_tree_from_instance() {
        let mut db = Db::new();
        let mesh_id = db
            .add_part_rep(PartRep::Mesh(Box::new(Mesh {
                vertices: vec![],
                triangles: vec![],
            })))
            .unwrap();

        let instance_id = db.make_new_part_instance_from_part(&mesh_id, None).unwrap();

        let db_arc = Arc::new(RwLock::new(db));
        let tree = create_object_tree_from_instance(db_arc.clone(), &instance_id).unwrap();

        match tree {
            TreeItem::Node { name, childs, .. } => {
                assert!(name.contains("Instance"));
                assert!(!childs.is_empty());
            }
            _ => panic!("Expected Node"),
        }
    }

    #[test]
    fn test_create_scene_tree_items_by_unique_parts_with_multiple_instances() {
        let mut db = Db::new();

        let mesh_id = db
            .add_part_rep(PartRep::Mesh(Box::new(Mesh {
                vertices: vec![Vec3::new(0.0, 0.0, 0.0)],
                triangles: vec![0, 0, 0],
            })))
            .unwrap();

        let inst1 = db.make_new_part_instance_from_part(&mesh_id, None).unwrap();
        let inst2 = db.make_new_part_instance_from_part(&mesh_id, None).unwrap();

        db.add_part_instance_to_scene(&inst1).unwrap();
        db.add_part_instance_to_scene(&inst2).unwrap();

        let db_arc = Arc::new(RwLock::new(db));
        let items = create_scene_tree_items_by_unique_parts(db_arc).unwrap();

        assert_eq!(items.len(), 1);
        match &items[0] {
            TreeItem::InertNode { childs, .. } => {
                // Both instances should be children
                assert_eq!(childs.len(), 2);
            }
            _ => panic!("Expected InertNode"),
        }
    }

    #[test]
    fn test_create_scene_tree_items_nested_with_multiple_children() {
        let mut db = Db::new();

        // Mesh part 1
        let mesh_id = db
            .add_part_rep(PartRep::Mesh(Box::new(Mesh {
                vertices: vec![Vec3::new(0.0, 0.0, 0.0)],
                triangles: vec![0, 1, 2],
            })))
            .unwrap();

        // Mesh part 2
        let mesh2_id = db
            .add_part_rep(PartRep::Mesh(Box::new(Mesh {
                vertices: vec![Vec3::new(1.0, 1.0, 0.0)],
                triangles: vec![0, 1, 2],
            })))
            .unwrap();

        // Instance of mesh1
        let mesh_inst1 = db.make_new_part_instance_from_part(&mesh_id, None).unwrap();
        // Instance of mesh2
        let mesh_inst2 = db
            .make_new_part_instance_from_part(&mesh2_id, None)
            .unwrap();

        // Level 1: Compose with two mesh children
        let composed_lvl1_id = db
            .add_part_rep(PartRep::ComposedPart(vec![mesh_inst1, mesh_inst2]))
            .unwrap();
        let composed_lvl1_inst = db
            .make_new_part_instance_from_part(&composed_lvl1_id, None)
            .unwrap();

        // Level 2: Compose with one child (lvl1 composed)
        let composed_lvl2_id = db
            .add_part_rep(PartRep::ComposedPart(vec![composed_lvl1_inst]))
            .unwrap();
        let composed_lvl2_inst = db
            .make_new_part_instance_from_part(&composed_lvl2_id, None)
            .unwrap();

        // Level 3: Compose with one child (lvl2 composed)
        let composed_lvl3_id = db
            .add_part_rep(PartRep::ComposedPart(vec![composed_lvl2_inst]))
            .unwrap();
        let composed_lvl3_inst = db
            .make_new_part_instance_from_part(&composed_lvl3_id, None)
            .unwrap();

        // Scene contains ONLY lvl3 instance
        db.add_part_instance_to_scene(&composed_lvl3_inst).unwrap();

        let db_arc = Arc::new(RwLock::new(db));
        let items = create_scene_tree_items_by_unique_parts(db_arc).unwrap();

        // Only one unique part (lvl3 composed)
        assert_eq!(items.len(), 1);

        match &items[0] {
            TreeItem::InertNode { childs, .. } => {
                assert_eq!(childs.len(), 1, "One lvl3 instance as child");

                match &childs[0] {
                    TreeItem::Node {
                        childs: lvl3_children,
                        ..
                    } => {
                        assert_eq!(lvl3_children.len(), 1, "Lvl3 should have one child (lvl2)");

                        match &lvl3_children[0] {
                            TreeItem::Node {
                                childs: lvl2_children,
                                ..
                            } => {
                                assert_eq!(
                                    lvl2_children.len(),
                                    1,
                                    "Lvl2 should have one child (lvl1)"
                                );

                                match &lvl2_children[0] {
                                    TreeItem::Node {
                                        childs: lvl1_children,
                                        ..
                                    } => {
                                        assert_eq!(
                                            lvl1_children.len(),
                                            2,
                                            "Lvl1 should have two mesh leaves"
                                        );

                                        // Check both mesh children
                                        let mut saw_mesh1 = false;
                                        let mut saw_mesh2 = false;
                                        for child in lvl1_children {
                                            match child {
                                                TreeItem::Leaf { name, .. } => {
                                                    if name.contains("Mesh") {
                                                        // The name contains Mesh, but we check position to distinguish
                                                        if name.contains(&format!("{:?}", mesh_id))
                                                        {
                                                            saw_mesh1 = true;
                                                        } else if name
                                                            .contains(&format!("{:?}", mesh2_id))
                                                        {
                                                            saw_mesh2 = true;
                                                        }
                                                    }
                                                }
                                                _ => panic!("Expected mesh leaves at lvl1"),
                                            }
                                        }
                                        assert!(
                                            saw_mesh1 && saw_mesh2,
                                            "Both meshes must be present at lvl1"
                                        );
                                    }
                                    _ => panic!("Lvl1 should be a Node for composed part"),
                                }
                            }
                            _ => panic!("Lvl2 child should be a Node for lvl1 composed part"),
                        }
                    }
                    _ => panic!("Lvl3 should be a Node"),
                }
            }
            _ => panic!("Root unique part should be InertNode"),
        }
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
