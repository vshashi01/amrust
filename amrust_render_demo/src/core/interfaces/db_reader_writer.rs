use crate::core::interfaces::{db_reader::DbReader, db_writer::DbWriter};

pub trait DbReaderWriter: DbReader + DbWriter {}
