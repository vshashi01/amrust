use crate::{
    amrust_db::Identifiable, app_mode::AppMode, part_list::PartList,
    tree_item_viewer::TreeItemViewer,
};

pub struct Toolsheets {
    pub app_mode: AppMode,
    pub part_list: PartList,

    pub objects_list: TreeItemViewer<Identifiable>,
    pub build_items_list: TreeItemViewer<Identifiable>,

    selected_items_on_part_list: Vec<Identifiable>,
    selection_changed: bool,

    selected_objects: Vec<Identifiable>,
    selected_build_items: Vec<Identifiable>,

    selected_identifiable_properties: Option<TreeItemViewer<usize>>,
}

impl Toolsheets {
    pub fn new(
        app_mode: AppMode,
        part_list: PartList,
        objects_list: TreeItemViewer<Identifiable>,
        build_items_list: TreeItemViewer<Identifiable>,
    ) -> Self {
        Toolsheets {
            app_mode,
            part_list,
            objects_list,
            build_items_list,
            selected_items_on_part_list: vec![],
            selection_changed: false,
            selected_objects: vec![],
            selected_build_items: vec![],
            selected_identifiable_properties: None,
        }
    }

    pub fn set_selected_identifiable_properties(&mut self, props: TreeItemViewer<usize>) {
        let _ = self.selected_identifiable_properties.insert(props);
    }

    pub fn selected_items_on_part_list(&self) -> impl Iterator<Item = &Identifiable> {
        self.selected_items_on_part_list.iter()
    }

    pub fn selected_objects(&self) -> impl Iterator<Item = &Identifiable> {
        self.selected_objects.iter()
    }

    pub fn selected_build_items(&self) -> impl Iterator<Item = &Identifiable> {
        self.selected_build_items.iter()
    }

    pub fn has_selection_changed(&self) -> bool {
        self.selection_changed
    }

    pub fn clear_selection_changed(&mut self) {
        self.selection_changed = false;
    }
}

impl egui_dock::TabViewer for Toolsheets {
    type Tab = String;

    fn title(&mut self, tab: &mut Self::Tab) -> egui::WidgetText {
        (&*tab).into()
    }

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Self::Tab) {
        let prev_part_list_selection = self.selected_items_on_part_list.clone();
        let prev_object_list_selection = self.selected_objects.clone();
        let prev_instances_selection = self.selected_build_items.clone();
        match tab.as_str() {
            "Part List" => self.part_list.ui(ui, &mut self.selected_items_on_part_list),
            "Objects List" => self.objects_list.core_ui(ui, &mut self.selected_objects),
            "Build Items List" => self
                .build_items_list
                .core_ui(ui, &mut self.selected_build_items),
            "Object Tree" => {
                if let Some(ref mut props) = self.selected_identifiable_properties {
                    props.core_ui(ui, &mut vec![]);
                } else {
                    ui.label("No selected item");
                }
            }
            _ => {
                ui.label(format!("Unimplemented content for: {tab:?}"));
            }
        }

        if prev_part_list_selection != self.selected_items_on_part_list
            || prev_object_list_selection != self.selected_objects
            || prev_instances_selection != self.selected_build_items
        {
            self.selection_changed = true;
        }
    }
}
