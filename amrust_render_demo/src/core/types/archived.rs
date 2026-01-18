use bytecheck::CheckBytes;
use rkyv::Archive;

#[derive(Debug, Archive, CheckBytes, Hash, PartialEq, Eq)]
#[repr(transparent)]
pub struct ArchivedSlotMapId(pub u64);

unsafe impl rkyv::Portable for ArchivedSlotMapId {}

unsafe impl rkyv::traits::NoUndef for ArchivedSlotMapId {}
