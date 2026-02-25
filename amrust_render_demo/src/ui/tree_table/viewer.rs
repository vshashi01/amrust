use crate::ui::tree_table::model::TreeTableViewModel;
use crate::ui::tree_table::types::CellResponse;
use egui::Ui;
use egui_table::{CellInfo, HeaderCellInfo, TableDelegate};
use std::fmt::Debug;
use std::hash::Hash;

pub struct TreeTableViewer<M: TreeTableViewModel> {
    model: M,
    id_salt: egui::Id,
}

impl<M: TreeTableViewModel> TreeTableViewer<M> {
    pub fn new(model: M) -> Self {
        Self {
            model,
            id_salt: egui::Id::new("tree_table"),
        }
    }

    pub fn ui(&mut self, ui: &mut Ui) {
        let num_rows = self.model.row_count() as u64;
        let num_sticky_cols = (0..self.model.column_count())
            .take_while(|&i| self.model.column_config(i).sticky)
            .count()
            .min(self.model.column_count());

        let table_columns: Vec<egui_table::Column> = (0..self.model.column_count())
            .map(|i| self.model.column_config(i).to_egui_column())
            .collect();

        let header_height = 24.0;
        let headers = [egui_table::HeaderRow::new(header_height)];

        let table = egui_table::Table::new()
            .id_salt(self.id_salt)
            .num_rows(num_rows)
            .columns(table_columns)
            .num_sticky_cols(num_sticky_cols)
            .headers(headers)
            .auto_size_mode(egui_table::AutoSizeMode::OnParentResize);

        table.show(ui, self);
    }

    pub fn model(&self) -> &M {
        &self.model
    }

    pub fn model_mut(&mut self) -> &mut M {
        &mut self.model
    }
}

impl<M: TreeTableViewModel> TableDelegate for TreeTableViewer<M>
where
    M::Id: PartialEq + Eq + Clone + Debug + Hash,
{
    fn prepare(&mut self, _info: &egui_table::PrefetchInfo) {
        // Could be used to prefetch data, but model already has everything
    }

    fn header_cell_ui(&mut self, ui: &mut Ui, cell_info: &HeaderCellInfo) {
        let col_index = cell_info.col_range.start;
        let margin = 4;

        egui::Frame::NONE
            .inner_margin(egui::Margin::symmetric(margin, 0))
            .show(ui, |ui| {
                let config = self.model.column_config(col_index);
                ui.label(&config.header);
            });
    }

    fn cell_ui(&mut self, ui: &mut Ui, cell_info: &CellInfo) {
        let row_nr = cell_info.row_nr as usize;
        let col_nr = cell_info.col_nr;

        // Get row data - clone the fields we need to avoid borrow issues
        let (row_id, is_disabled, is_selected, selectable) =
            match self.model.flat_rows().get(row_nr) {
                Some(row) => (
                    row.id.clone(),
                    self.model.is_disabled(&row.id),
                    self.model.is_selected(&row.id),
                    row.selectable,
                ),
                None => return,
            };

        // Paint row background
        if is_disabled {
            ui.painter()
                .rect_filled(ui.max_rect(), 0.0, ui.visuals().faint_bg_color);
        } else if is_selected {
            ui.painter()
                .rect_filled(ui.max_rect(), 0.0, ui.visuals().selection.bg_fill);
        } else if selectable && ui.rect_contains_pointer(ui.max_rect()) {
            // Hover highlight for selectable rows
            ui.painter()
                .rect_filled(ui.max_rect(), 0.0, ui.visuals().code_bg_color);
        }

        // Render cell content
        let margin = 4;
        egui::Frame::NONE
            .inner_margin(egui::Margin::symmetric(margin, 0))
            .show(ui, |ui| {
                let response = self.model.render_cell(ui, row_nr, col_nr);

                // Handle CellResponse
                match response {
                    CellResponse::ExpansionToggled => {
                        self.model.toggle_expansion(&row_id);
                    }
                    CellResponse::SelectionToggled { ctrl_pressed } => {
                        self.model.toggle_selection(&row_id, ctrl_pressed);
                    }
                    CellResponse::None => {}
                }
            });
    }

    fn row_top_offset(&self, _ctx: &egui::Context, _table_id: egui::Id, row_nr: u64) -> f32 {
        let row_height = if row_nr < self.model.row_count() as u64 {
            // Clone the ID to avoid borrow issues
            let row_id = self.model.flat_rows()[row_nr as usize].id.clone();
            if self.model.is_disabled(&row_id) {
                28.0 // Slightly taller for disabled items with progress bar
            } else {
                24.0
            }
        } else {
            24.0
        };
        row_nr as f32 * row_height
    }
}
