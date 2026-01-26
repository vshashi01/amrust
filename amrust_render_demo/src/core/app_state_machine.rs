use std::collections::{HashMap, HashSet};
use std::mem;

use crate::core::app_mode::AppMode;
use crate::core::types::identifiable::Identifiable;
use glam::Vec3;

pub type DialogId = u64;
pub type ShortcutId = &'static str;
pub type ShortcutContextId = &'static str;
pub type CommandId = &'static str;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MouseMode {
    Navigate,
    Select,
    Transform,
    PointPicking,
    DialogCapture,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SelectionMode {
    Part,
    Instance,
    DialogTarget,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InteractionEventKind {
    ViewportPicked,
    ToolsheetSelection,
    ShortcutInvoked,
    DialogAction,
    ExternalCommand,
}

#[derive(Debug, Clone)]
pub enum Primitive {
    Point,
    Line,
    Normal,
}

#[derive(Debug, Clone)]
pub enum InteractionEvent {
    ViewportPicked {
        identifiables: Vec<Identifiable>,
        primitives: Option<Primitive>,
        cursor_world: Vec3,
    },
    ToolsheetSelection {
        selection: Vec<Identifiable>,
    },
    ShortcutInvoked {
        shortcut: ShortcutId,
        context: ShortcutContextId,
    },
    DialogAction {
        dialog_id: DialogId,
        action: DialogActionKind,
    },
    ExternalCommand {
        command: CommandId,
    },
}

impl InteractionEvent {
    pub fn kind(&self) -> InteractionEventKind {
        match self {
            InteractionEvent::ViewportPicked { .. } => InteractionEventKind::ViewportPicked,
            InteractionEvent::ToolsheetSelection { .. } => InteractionEventKind::ToolsheetSelection,
            InteractionEvent::ShortcutInvoked { .. } => InteractionEventKind::ShortcutInvoked,
            InteractionEvent::DialogAction { .. } => InteractionEventKind::DialogAction,
            InteractionEvent::ExternalCommand { .. } => InteractionEventKind::ExternalCommand,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DialogActionKind {
    Confirm,
    Cancel,
    Revert,
    Refresh,
}

#[derive(Debug)]
pub struct ShortcutOverride {
    pub shortcut: ShortcutId,
    pub handler: ShortcutHandler,
}

pub type ShortcutHandler = Box<dyn FnMut() -> ShortcutOutcome + Send>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShortcutOutcome {
    Consumed,
    Ignored,
}

#[derive(Debug)]
pub struct ShortcutContext {
    active_context: ShortcutContextId,
    overrides: HashMap<ShortcutContextId, Vec<ShortcutOverride>>,
}

impl ShortcutContext {
    pub fn new(active_context: ShortcutContextId) -> Self {
        Self {
            active_context,
            overrides: HashMap::new(),
        }
    }

    pub fn set_active_context(&mut self, context: ShortcutContextId) {
        self.active_context = context;
    }

    pub fn register_overrides(
        &mut self,
        context: ShortcutContextId,
        overrides: Vec<ShortcutOverride>,
    ) {
        self.overrides.insert(context, overrides);
    }

    pub fn clear_overrides(&mut self, context: ShortcutContextId) {
        self.overrides.remove(&context);
    }

    pub fn dispatch(&mut self, id: ShortcutId) -> ShortcutOutcome {
        if let Some(list) = self.overrides.get_mut(&self.active_context) {
            for override_item in list.iter_mut() {
                if override_item.shortcut == id {
                    return (override_item.handler)();
                }
            }
        }
        ShortcutOutcome::Ignored
    }
}

#[derive(Debug, Default)]
pub struct DialogState {
    pub owned_points: Vec<Vec3>,
    pub captured_identifiables: Vec<IdentifiableProxy>,
    pub is_dirty: bool,
}

pub enum OperationDialogResult {
    Confirmed(DialogState),
    Cancelled(DialogState),
}

pub struct OperationDialogDescriptor {
    pub id: DialogId,
    pub title: &'static str,
    pub listening_events: HashSet<InteractionEventKind>,
    pub shortcut_context: ShortcutContextId,
    pub shortcut_overrides: Vec<ShortcutOverride>,
    pub on_event: Box<dyn FnMut(&InteractionEvent, &mut DialogState) + Send>,
    pub on_close: Box<dyn FnOnce(OperationDialogResult) + Send>,
    pub state: DialogState,
}

impl OperationDialogDescriptor {
    pub fn listens_to(&self, kind: InteractionEventKind) -> bool {
        self.listening_events.contains(&kind)
    }
}

pub struct DispatchContext<'a> {
    pub db_view_model: Option<&'a mut crate::db_view_model::DbViewModel>,
    pub render_sender: Option<
        &'a smol::channel::Sender<crate::core::services::render_service::RenderServiceRequest>,
    >,
}

impl<'a> DispatchContext<'a> {
    pub fn new(
        db_view_model: Option<&'a mut crate::db_view_model::DbViewModel>,
        render_sender: Option<
            &'a smol::channel::Sender<crate::core::services::render_service::RenderServiceRequest>,
        >,
    ) -> Self {
        Self {
            db_view_model,
            render_sender,
        }
    }
}

pub struct AppStateMachine {
    pub current_mode: AppMode,
    pub mouse_mode: MouseMode,
    pub selection_mode: SelectionMode,
    pub shortcut_context: ShortcutContext,
    dialog_stack: Vec<OperationDialogDescriptor>,
    event_dispatcher: EventDispatcher,
}

impl AppStateMachine {
    pub fn new(current_mode: AppMode) -> Self {
        Self {
            current_mode,
            mouse_mode: MouseMode::Navigate,
            selection_mode: SelectionMode::None,
            shortcut_context: ShortcutContext::new("app"),
            dialog_stack: Vec::new(),
            event_dispatcher: EventDispatcher::default(),
        }
    }

    pub fn handle_event(&mut self, event: InteractionEvent, ctx: &mut DispatchContext<'_>) {
        let _ = self.event_dispatcher.dispatch(self, event, ctx);
    }

    pub fn push_dialog(&mut self, mut descriptor: OperationDialogDescriptor) {
        let overrides = mem::take(&mut descriptor.shortcut_overrides);
        self.shortcut_context
            .register_overrides(descriptor.shortcut_context, overrides);
        descriptor.state.captured_identifiables.clear();
        self.dialog_stack.push(descriptor);
    }

    pub fn pop_dialog(&mut self, dialog_id: DialogId, result: OperationDialogResult) {
        if let Some(pos) = self
            .dialog_stack
            .iter()
            .position(|descriptor| descriptor.id == dialog_id)
        {
            let mut descriptor = self.dialog_stack.remove(pos);
            self.shortcut_context
                .clear_overrides(descriptor.shortcut_context);
            (descriptor.on_close)(result);
        }
    }

    pub fn active_dialog(&self) -> Option<&OperationDialogDescriptor> {
        self.dialog_stack.last()
    }

    pub fn active_dialog_mut(&mut self) -> Option<&mut OperationDialogDescriptor> {
        self.dialog_stack.last_mut()
    }

    pub fn set_mouse_mode(&mut self, next_mode: MouseMode) {
        self.mouse_mode = next_mode;
    }

    pub fn set_selection_mode(&mut self, next_mode: SelectionMode) {
        self.selection_mode = next_mode;
    }

    pub fn set_shortcut_context(&mut self, context: ShortcutContextId) {
        self.shortcut_context.set_active_context(context);
    }

    pub fn is_dialog_active(&self) -> bool {
        !self.dialog_stack.is_empty()
    }
}

#[derive(Default)]
pub struct EventDispatcher;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventDispatchOutcome {
    Consumed,
    Ignored,
}

impl EventDispatcher {
    pub fn dispatch(
        &mut self,
        app_machine: &mut AppStateMachine,
        event: InteractionEvent,
        ctx: &mut DispatchContext<'_>,
    ) -> EventDispatchOutcome {
        let _ = (app_machine, event, ctx);
        todo!("event dispatching will be implemented during integration phase");
    }
}
