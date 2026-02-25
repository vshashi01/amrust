use std::fmt::Debug;
use std::hash::Hash;

#[derive(Debug, Clone)]
pub struct TreeRow<T: PartialEq + Eq + Clone + Debug + Hash> {
    pub id: T,
    pub depth: usize,
    pub is_node: bool,
    pub has_children: bool,
    pub selectable: bool,
}

pub struct ColumnConfig {
    pub header: String,
    pub initial_width: f32,
    pub min_width: f32,
    pub max_width: f32,
    pub resizable: bool,
    pub sticky: bool,
}

impl ColumnConfig {
    pub fn new(header: impl Into<String>, initial_width: f32) -> Self {
        Self {
            header: header.into(),
            initial_width,
            min_width: 20.0,
            max_width: 1000.0,
            resizable: true,
            sticky: false,
        }
    }

    pub fn with_min_width(mut self, width: f32) -> Self {
        self.min_width = width;
        self
    }

    pub fn with_max_width(mut self, width: f32) -> Self {
        self.max_width = width;
        self
    }

    pub fn resizable(mut self, resizable: bool) -> Self {
        self.resizable = resizable;
        self
    }

    pub fn sticky(mut self, sticky: bool) -> Self {
        self.sticky = sticky;
        self
    }

    pub fn to_egui_column(&self) -> egui_table::Column {
        egui_table::Column::new(self.initial_width)
            .range(self.min_width..=self.max_width)
            .resizable(self.resizable)
    }
}

pub enum CellResponse {
    ExpansionToggled,
    SelectionToggled { ctrl_pressed: bool },
    None,
}
