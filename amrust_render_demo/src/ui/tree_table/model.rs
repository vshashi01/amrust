use egui::Ui;

use crate::ui::tree_table::types::{CellResponse, ColumnConfig, TreeRow};

use std::fmt::Debug;
use std::hash::Hash;

pub trait TreeTableViewModel {
    type Id: PartialEq + Eq + Clone + Debug + Hash;

    fn flat_rows(&self) -> &[TreeRow<Self::Id>];

    fn row_count(&self) -> usize {
        self.flat_rows().len()
    }

    fn column_count(&self) -> usize;
    fn column_config(&self, index: usize) -> ColumnConfig;

    fn render_cell(&mut self, ui: &mut Ui, row_idx: usize, col_idx: usize) -> CellResponse;

    fn is_selected(&self, id: &Self::Id) -> bool;
    fn is_disabled(&self, id: &Self::Id) -> bool;

    fn toggle_selection(&mut self, id: &Self::Id, ctrl_pressed: bool);
    fn toggle_expansion(&mut self, id: &Self::Id);
}
