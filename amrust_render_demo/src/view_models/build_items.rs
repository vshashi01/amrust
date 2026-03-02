use egui::ahash::{HashSet, HashSetExt};
use egui::Ui;
use slotmap::{SecondaryMap, SparseSecondaryMap};

use crate::core::types::part::PartId;
use crate::core::types::part_instance::PartInstanceId;
use crate::db_view_model::PartInstanceCache;
use crate::models::view_modes::{LightingMode, MaterialOpacity, ShadingMode};
use crate::ui::tree_table::model::TreeTableViewModel;
use crate::ui::tree_table::types::{CellResponse, ColumnConfig, TreeRow};

use std::collections::HashMap;
use std::fmt::Debug;
use std::hash::Hash;
use std::ops::Range;

pub struct BuildItemsModel {
    // Tree data
    tree_rows: Vec<TreeRow<PartInstanceId>>, // to be generalized in the future to use Identifiables?

    // External State
    selected_items: Vec<PartInstanceId>,
    disabled_items: Vec<PartInstanceId>,

    // Internal state
    expanded_items: Vec<PartInstanceId>,
    data_map: SecondaryMap<PartInstanceId, Data>,

    // component tracking
    changed_instances: HashSet<PartInstanceId>,
}

#[derive(Debug, Clone, Default)]
pub struct Data {
    name: String,
    pub visibility: bool,
    pub shading: ShadingMode,
    pub lighting: LightingMode,
    pub opacity: MaterialOpacity,
    parent_part: PartId,
}

impl BuildItemsModel {
    pub fn new(part_instances: &[(PartInstanceId, &PartInstanceCache)]) -> Self {
        let tree_rows = Self::tree_row_from_cache(part_instances);
        let mut data_map = SecondaryMap::with_capacity(part_instances.len());

        for (id, cache) in part_instances {
            data_map.insert(
                *id,
                Data {
                    name: format!("{id}"),
                    visibility: cache.visible,
                    shading: cache.shading,
                    lighting: cache.lighting,
                    opacity: cache.opacity,
                    parent_part: cache.part_id,
                },
            );
        }

        Self {
            tree_rows,
            selected_items: Vec::new(),
            disabled_items: Vec::new(),
            expanded_items: Vec::new(),
            data_map,
            changed_instances: HashSet::new(),
        }
    }

    pub fn get_data(&self) -> &SecondaryMap<PartInstanceId, Data> {
        &self.data_map
    }

    pub fn get_changed_data(&self) -> impl Iterator<Item = (PartInstanceId, Data)> {
        self.changed_instances.iter().map(|id| {
            if let Some(data) = self.data_map.get(*id) {
                (*id, data.clone())
            } else {
                (*id, Default::default())
            }
        })
    }

    pub fn clear_changed(&mut self) {
        self.changed_instances.clear();
    }

    pub fn selected_items(&self) -> &[PartInstanceId] {
        &self.selected_items
    }

    pub fn set_disabled_items(&mut self, disabled: &[PartInstanceId]) {
        self.disabled_items = disabled.to_vec();
    }

    // Helper methods for rendering
    fn render_expand_column(&mut self, ui: &mut Ui, row: &TreeRow<PartInstanceId>) -> CellResponse {
        if row.has_children && row.is_node {
            let is_expanded = self.expanded_items.contains(&row.id);
            let icon = if is_expanded { "▼" } else { "▶" };
            let button = egui::Button::new(icon).small().frame(false);
            let response = ui.add(button);
            if response.clicked() {
                return CellResponse::ExpansionToggled;
            }
        } else {
            ui.add_space(16.0);
        }
        CellResponse::None
    }

    fn tree_row_from_cache(
        part_instances: &[(PartInstanceId, &PartInstanceCache)],
    ) -> Vec<TreeRow<PartInstanceId>> {
        part_instances
            .iter()
            .map(|(id, _)| TreeRow {
                id: *id,
                depth: 0,
                is_node: false,
                has_children: false,
                selectable: true,
            })
            .collect::<Vec<_>>()
    }

    fn render_name_column(&mut self, ui: &mut Ui, row: &TreeRow<PartInstanceId>) -> CellResponse {
        let indent = row.depth as f32 * 20.0;
        ui.add_space(indent);

        let data = self
            .data_map
            .get(row.id)
            .expect("Crash for now because name is empty");
        let name = &data.name;

        if self.disabled_items.contains(&row.id) {
            ui.add(egui::ProgressBar::new(0.0).animate(true));
            ui.add_space(8.0);
            ui.label(name);
            CellResponse::None
        } else if row.selectable {
            let is_selected = self.selected_items.contains(&row.id);
            let response = ui.selectable_label(is_selected, name);

            if response.clicked() {
                let ctrl_pressed = ui.input(|i| i.modifiers.ctrl);
                return CellResponse::SelectionToggled { ctrl_pressed };
            }
            CellResponse::None
        } else {
            ui.label(name);
            CellResponse::None
        }
    }

    fn render_visibility_column(
        &mut self,
        ui: &mut Ui,
        row: &TreeRow<PartInstanceId>,
    ) -> CellResponse {
        if let Some(data) = self.data_map.get_mut(row.id) {
            let prev_visibility = data.visibility;
            ui.add_enabled(
                !self.disabled_items.contains(&row.id),
                |ui: &mut egui::Ui| ui.checkbox(&mut data.visibility, ""),
            );
            if prev_visibility != data.visibility {
                self.changed_instances.insert(row.id);
            }
        }

        CellResponse::None
    }

    fn render_shading_column(
        &mut self,
        ui: &mut Ui,
        row: &TreeRow<PartInstanceId>,
    ) -> CellResponse {
        if let Some(data) = self.data_map.get_mut(row.id) {
            let prev_shading_mode = data.shading;
            ui.add_enabled_ui(!self.disabled_items.contains(&row.id), |ui| {
                let id = ui.id().with(format!("shading_{:?}", row.id));
                egui::ComboBox::from_id_salt(id)
                    .width(ui.available_width())
                    .selected_text(data.shading.display_name())
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut data.shading,
                            ShadingMode::Shade,
                            ShadingMode::Shade.display_name(),
                        );
                        ui.selectable_value(
                            &mut data.shading,
                            ShadingMode::ShadeAndWire,
                            ShadingMode::ShadeAndWire.display_name(),
                        );
                        ui.selectable_value(
                            &mut data.shading,
                            ShadingMode::WireOnly,
                            ShadingMode::WireOnly.display_name(),
                        );
                    });
            });

            if prev_shading_mode != data.shading {
                self.changed_instances.insert(row.id);
            }
        }

        CellResponse::None
    }

    fn render_lighting_column(
        &mut self,
        ui: &mut Ui,
        row: &TreeRow<PartInstanceId>,
    ) -> CellResponse {
        if let Some(data) = self.data_map.get_mut(row.id) {
            let prev_lighting = data.lighting;
            ui.add_enabled_ui(!self.disabled_items.contains(&row.id), |ui| {
                let id = ui.id().with(format!("lighting_{:?}", row.id));
                egui::ComboBox::from_id_salt(id)
                    .width(ui.available_width())
                    .selected_text(data.lighting.display_name())
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut data.lighting,
                            LightingMode::Lit,
                            LightingMode::Lit.display_name(),
                        );
                        ui.selectable_value(
                            &mut data.lighting,
                            LightingMode::Unlit,
                            LightingMode::Unlit.display_name(),
                        );
                    });
            });

            if prev_lighting != data.lighting {
                self.changed_instances.insert(row.id);
            }
        }

        CellResponse::None
    }

    fn render_opacity_column(
        &mut self,
        ui: &mut Ui,
        row: &TreeRow<PartInstanceId>,
    ) -> CellResponse {
        if let Some(data) = self.data_map.get_mut(row.id) {
            let prev_opacity = data.opacity;
            ui.add_enabled_ui(!self.disabled_items.contains(&row.id), |ui| {
                let id = ui.id().with(format!("opacity_{:?}", row.id));
                egui::ComboBox::from_id_salt(id)
                    .width(ui.available_width())
                    .selected_text(data.opacity.display_name())
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut data.opacity,
                            MaterialOpacity::Opaque,
                            MaterialOpacity::Opaque.display_name(),
                        );
                        ui.selectable_value(
                            &mut data.opacity,
                            MaterialOpacity::Transparent,
                            MaterialOpacity::Transparent.display_name(),
                        );
                    });
            });
            if prev_opacity != data.opacity {
                self.changed_instances.insert(row.id);
            }
        }

        CellResponse::None
    }

    fn render_unique_name(&self, ui: &mut Ui, row: &TreeRow<PartInstanceId>) -> CellResponse {
        if let Some(data) = self.data_map.get(row.id) {
            ui.label(format!("{}", data.parent_part));
        }

        CellResponse::None
    }
}

impl TreeTableViewModel for BuildItemsModel {
    type Id = PartInstanceId;

    fn flat_rows(&self) -> &[TreeRow<Self::Id>] {
        &self.tree_rows
    }

    fn column_count(&self) -> usize {
        7
    }

    fn column_config(&self, index: usize) -> ColumnConfig {
        match index {
            0 => ColumnConfig::new("", 16.0)
                .with_min_width(16.0)
                .with_max_width(40.0)
                .resizable(false)
                .sticky(true),
            1 => ColumnConfig::new("Name", 200.0).sticky(true),
            2 => ColumnConfig::new("Visible", 60.0)
                .with_min_width(50.0)
                .with_max_width(80.0),
            3 => ColumnConfig::new("Shading", 100.0)
                .with_min_width(80.0)
                .with_max_width(150.0),
            4 => ColumnConfig::new("Lighting", 70.0)
                .with_min_width(60.0)
                .with_max_width(100.0),
            5 => ColumnConfig::new("Opacity", 80.0)
                .with_min_width(70.0)
                .with_max_width(120.0),
            6 => ColumnConfig::new("Unique Part", 200.0),
            _ => panic!("Invalid column index"),
        }
    }

    fn render_cell(&mut self, ui: &mut Ui, row_idx: usize, col_idx: usize) -> CellResponse {
        // Clone the row to avoid borrow issues with helper methods
        let row = self.tree_rows[row_idx].clone();
        match col_idx {
            0 => self.render_expand_column(ui, &row),
            1 => self.render_name_column(ui, &row),
            2 => self.render_visibility_column(ui, &row),
            3 => self.render_shading_column(ui, &row),
            4 => self.render_lighting_column(ui, &row),
            5 => self.render_opacity_column(ui, &row),
            6 => self.render_unique_name(ui, &row),
            _ => CellResponse::None,
        }
    }

    fn is_selected(&self, id: &Self::Id) -> bool {
        self.selected_items.contains(id)
    }

    fn is_disabled(&self, id: &Self::Id) -> bool {
        self.disabled_items.contains(id)
    }

    fn toggle_selection(&mut self, id: &Self::Id, ctrl_pressed: bool) {
        if !ctrl_pressed {
            self.selected_items.clear();
        }
        if let Some(pos) = self.selected_items.iter().position(|x| x == id) {
            if ctrl_pressed {
                self.selected_items.remove(pos);
            }
        } else {
            self.selected_items.push(*id);
        }
    }

    fn toggle_expansion(&mut self, id: &Self::Id) {
        let is_expanded = self
            .expanded_items
            .iter()
            .enumerate()
            .find(|(_, item)| id == *item);
        if let Some((index, _)) = is_expanded {
            self.expanded_items.remove(index);
        } else {
            self.expanded_items.push(*id);
        }
    }
}
