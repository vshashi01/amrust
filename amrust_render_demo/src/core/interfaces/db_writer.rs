use crate::core::types::{part::PartId, part_instance::PartInstanceId};

pub trait DbWriter {
    fn clear_all(&mut self);

    fn remove_part(&mut self, id: PartId) -> bool;

    fn remove_part_instance(&mut self, id: PartInstanceId) -> bool;
}
