use bytecheck::CheckBytes;
use rkyv::{Archive, Deserialize, Serialize, rancor::Fallible};
use slotmap::{KeyData, new_key_type};

use crate::core::types::{archived::ArchivedSlotMapId, mesh::Mesh, part_instance::PartInstanceId};

use std::fmt;

#[derive(Debug, Clone, Archive, Serialize, Deserialize, CheckBytes)]
pub struct Part {
    rep: PartRep,
}

impl Part {
    pub fn new(part_rep: PartRep) -> Self {
        Self { rep: part_rep }
    }

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
