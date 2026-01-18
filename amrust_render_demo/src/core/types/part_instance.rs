use bytecheck::CheckBytes;
use rkyv::{Archive, Deserialize, Serialize, rancor::Fallible};
use slotmap::{KeyData, new_key_type};

use crate::core::types::{
    archived::ArchivedSlotMapId, part::PartId, transformation::Transformation,
};

use std::fmt;

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
