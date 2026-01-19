#![allow(clippy::needless_lifetimes)]

use crate::core::{
    amrust_db::Scene,
    types::{
        part::{Part, PartId},
        part_instance::{PartInstance, PartInstanceId},
    },
};

pub trait DbReader: Send + Sync + 'static {
    fn get_parts_count<'a>(&'a self) -> usize;

    fn get_part<'a>(&'a self, part_id: &PartId) -> Option<&'a Part>;

    fn get_parts<'a>(&'a self) -> Box<dyn Iterator<Item = (PartId, &'a Part)> + 'a>;

    fn get_part_instance_count<'a>(&'a self) -> usize;

    fn get_part_instance<'a>(
        &'a self,
        part_instance_id: &PartInstanceId,
    ) -> Option<&'a PartInstance>;

    fn get_part_instances<'a>(
        &'a self,
    ) -> Box<dyn Iterator<Item = (PartInstanceId, &'a PartInstance, &'a Part)> + 'a>;

    fn get_scene<'a>(&'a self) -> Option<&'a Scene>;

    fn can_remove_part<'a>(&'a self, id: &PartId) -> bool;

    fn can_remove_part_instance<'a>(&'a self, id: &PartInstanceId) -> bool;
}
