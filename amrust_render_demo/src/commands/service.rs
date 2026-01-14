use std::collections::HashMap;

use crate::commands::{Command, CommandCategory, CommandContext};

/// Central service for managing and executing commands
pub struct CommandsService {
    commands: HashMap<String, Box<dyn Command>>,
    shortcuts: HashMap<String, String>, // shortcut -> command_id
    categories: HashMap<CommandCategory, Vec<String>>, // category -> command_ids
}

impl CommandsService {
    pub fn new() -> Self {
        Self {
            commands: HashMap::new(),
            shortcuts: HashMap::new(),
            categories: HashMap::new(),
        }
    }

    /// Register a new command with the service
    pub fn register_command(&mut self, command: Box<dyn Command>) {
        let id = command.id().to_string();
        let category = command.category();

        // Register shortcut if provided
        if let Some(shortcut) = command.shortcut() {
            self.shortcuts.insert(shortcut.to_string(), id.clone());
        }

        // Add to category
        self.categories
            .entry(category)
            .or_insert_with(Vec::new)
            .push(id.clone());

        // Store command
        self.commands.insert(id, command);
    }

    /// Execute a command by its ID
    pub fn execute_command(&self, id: &str, context: &mut CommandContext) {
        if let Some(cmd) = self.commands.get(id) {
            if cmd.is_enabled(context) {
                cmd.execute(context);
            } else {
                println!("Command '{}' is disabled", id);
            }
        } else {
            println!("Unknown command: {}", id);
        }
    }

    /// Get all commands for a specific category that are visible
    pub fn get_commands_for_category(
        &self,
        category: CommandCategory,
        context: &CommandContext,
    ) -> Vec<&dyn Command> {
        if let Some(command_ids) = self.categories.get(&category) {
            command_ids
                .iter()
                .filter_map(|id| self.commands.get(id))
                .filter(|cmd| cmd.is_visible(context))
                .map(|cmd| cmd.as_ref())
                .collect()
        } else {
            Vec::new()
        }
    }

    /// Handle keyboard shortcut and return true if handled
    pub fn handle_keyboard_shortcut(&self, shortcut: &str, context: &mut CommandContext) -> bool {
        if let Some(command_id) = self.shortcuts.get(shortcut) {
            self.execute_command(command_id, context);
            true
        } else {
            false
        }
    }

    /// Populate a menu bar with commands
    pub fn populate_menu_bar(&self, ui: &mut egui::Ui, context: &mut CommandContext) {
        egui::MenuBar::new().ui(ui, |ui| {
            // File commands
            let file_commands = self.get_commands_for_category(CommandCategory::File, context);
            for cmd in file_commands {
                ui.add_enabled_ui(cmd.is_enabled(context), |ui| {
                    if ui.button(cmd.label()).clicked() {
                        cmd.execute(context);
                    }
                });
            }

            // Add separator if there are file commands and other commands
            if !self
                .get_commands_for_category(CommandCategory::File, context)
                .is_empty()
                && (!self
                    .get_commands_for_category(CommandCategory::View, context)
                    .is_empty()
                    || !self
                        .get_commands_for_category(CommandCategory::Debug, context)
                        .is_empty())
            {
                ui.separator();
            }

            // View commands
            let view_commands = self.get_commands_for_category(CommandCategory::View, context);
            for cmd in view_commands {
                ui.add_enabled_ui(cmd.is_enabled(context), |ui| {
                    if ui.button(cmd.label()).clicked() {
                        cmd.execute(context);
                    }
                });
            }

            // Debug commands (only in debug builds)
            #[cfg(debug_assertions)]
            {
                let debug_commands =
                    self.get_commands_for_category(CommandCategory::Debug, context);
                if !debug_commands.is_empty() {
                    ui.separator();
                    for cmd in debug_commands {
                        ui.add_enabled_ui(cmd.is_enabled(context), |ui| {
                            if ui.button(cmd.label()).clicked() {
                                cmd.execute(context);
                            }
                        });
                    }
                }
            }
        });
    }
}
