use amrust_render::bounding_box::BoundingBox;

use crate::core::{app_mode::AppMode, types::identifiable::Identifiable};

pub trait DbView {
    fn get_total_visible_bbox(&self, app_mode: AppMode) -> BoundingBox;

    fn get_all_operable_selected_identifiables(&self) -> Vec<Identifiable>;

    fn is_database_empty(&self) -> bool;
}
