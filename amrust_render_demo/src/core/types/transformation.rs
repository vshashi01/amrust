use amrust_render::transformation::TransformationData;
use bytecheck::CheckBytes;
use glam::Mat4;
use rkyv::{Archive, Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Archive, Serialize, Deserialize, CheckBytes)]
pub struct Transformation(pub Mat4);

impl From<&Transformation> for TransformationData {
    fn from(val: &Transformation) -> Self {
        TransformationData(val.0.to_cols_array_2d())
    }
}

impl From<amrust_render::transformation::Transformation> for Transformation {
    fn from(value: amrust_render::transformation::Transformation) -> Self {
        Transformation(value.0)
    }
}
