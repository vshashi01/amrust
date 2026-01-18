use crate::core::types::{part::PartId, part_instance::PartInstanceId};

#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash)]
pub enum Identifiable {
    Part(PartId),
    PartInstance(PartInstanceId),
}
