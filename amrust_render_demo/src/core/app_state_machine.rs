//! Hierarchical application state machine placeholder.
//!
//! This module is being rewritten to leverage the `statig` crate for
//! hierarchical state management. The current contents define the
//! data contracts, shared context, and state placeholders that the
//! actual state machine implementation will hook into. The intent is
//! to keep the rest of the codebase untouched while we iterate on the
//! new design.

#![allow(dead_code)]
#![allow(unused_imports)]

use std::collections::{HashMap, HashSet, VecDeque};

use crate::core::app_mode::AppMode;
use crate::core::types::identifiable::Identifiable;

/// NOTE: The `statig` dependency will be introduced in `Cargo.toml` in a
/// follow-up change. The import below stays commented to keep the crate
/// compiling until the dependency is added.
// use statig::prelude::*;

pub type DialogId = u64;
pub type ShortcutId = &'static str;
pub type ShortcutContextId = &'static str;
pub type CommandId = &'static str;

#[derive(Debug, Clone)]
pub struct IdentifiableProxy {
    pub value: Identifiable,
}

impl IdentifiableProxy {
    pub fn new(value: Identifiable) -> Self {
        Self { value }
    }
}

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

/// High level events emitted by the UI and services that feed the HSM.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InteractionEventKind {
    ViewportPicked,
    ToolsheetSelection,
    ShortcutInvoked,
    DialogAction,
    ExternalCommand,
}

#[derive(Debug, Clone)]
pub enum InteractionEvent {
    ViewportPicked {
        identifiables: Vec<IdentifiableProxy>,
        cursor_world: glam::Vec3,
    },
    ToolsheetSelection {
        selection: Vec<IdentifiableProxy>,
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

pub struct ShortcutOverride {
    pub shortcut: ShortcutId,
    pub handler: ShortcutHandler,
}

pub type ShortcutHandler = Box<dyn FnMut() -> ShortcutOutcome + Send>;

/// Records deferred state mutations requested by substates/dialogs.
#[derive(Debug)]
pub enum StateChangeRequest {
    SetAppMode(AppMode),
    SetMouseMode(MouseMode),
    SetSelectionMode(SelectionMode),
    PushDialog(DialogRegistration),
    PopDialog(DialogId, OperationDialogResult),
    TriggerOperation(CommandId),
    RefreshRender,
}

#[derive(Debug)]
pub struct DialogRegistration {
    pub id: DialogId,
    pub descriptor: OperationDialogDescriptor,
}

#[derive(Debug, Default)]
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

#[derive(Debug, Default, Clone)]
pub struct DialogState {
    pub owned_points: Vec<Vec3>,
    pub captured_identifiables: Vec<Identifiable>,
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

/// Bridge exposed by the `DialogService` to the state machine.
pub trait DialogServiceBridge {
    fn active_dialog_descriptor(&mut self) -> Option<ActiveDialogHandle<'_>>;
    fn enqueue_request(&mut self, request: StateChangeRequest);
}

pub struct ActiveDialogHandle<'a> {
    pub descriptor: &'a mut OperationDialogDescriptor,
}

impl<'a> ActiveDialogHandle<'a> {
    pub fn listens_to(&self, kind: InteractionEventKind) -> bool {
        self.descriptor.listens_to(kind)
    }

    pub fn on_event(&mut self, event: &InteractionEvent) -> ShortcutOutcome {
        (self.descriptor.on_event)(event, &mut self.descriptor.state);
        // Actual implementation will propagate whether the event was consumed.
        ShortcutOutcome::Consumed
    }

    pub fn finish(self, result: OperationDialogResult) {
        (self.descriptor.on_close)(result);
    }
}

/// Shared state owned by the root state machine.
#[derive(Debug)]
pub struct SharedState {
    pub app_mode: AppMode,
    pub mouse_mode: MouseMode,
    pub selection_mode: SelectionMode,
    pub shortcut_context: ShortcutContext,
    pub pending_requests: VecDeque<StateChangeRequest>,
}

impl SharedState {
    pub fn new(default_app_mode: AppMode) -> Self {
        Self {
            app_mode: default_app_mode,
            mouse_mode: MouseMode::Navigate,
            selection_mode: SelectionMode::None,
            shortcut_context: ShortcutContext::new("app"),
            pending_requests: VecDeque::new(),
        }
    }
}

/// Context passed into state handlers. It exposes the shared state and the
/// external services the HSM needs to interact with.
pub struct InteractionContext<'a> {
    pub shared: &'a mut SharedState,
    pub db_view_model: Option<&'a mut crate::db_view_model::DbViewModel>,
    pub render_sender: Option<
        &'a smol::channel::Sender<crate::core::services::render_service::RenderServiceRequest>,
    >,
    pub dialog_bridge: Option<&'a mut dyn DialogServiceBridge>,
}

impl<'a> InteractionContext<'a> {
    pub fn push_request(&mut self, request: StateChangeRequest) {
        self.shared.pending_requests.push_back(request);
    }
}

/// Placeholder root state machine. The concrete statig-based implementation
/// will replace the enum below once the dependency is wired in.
#[derive(Debug, Default)]
pub struct AppStateMachine {
    shared: SharedState,
    active_state: RootState,
}

#[derive(Debug, Default)]
enum RootState {
    #[default]
    Idle(IdleState),
    DialogActive(DialogActiveState),
}

#[derive(Debug, Default)]
struct IdleState {
    substate: IdleSubstate,
}

#[derive(Debug, Default)]
enum IdleSubstate {
    #[default]
    Navigate,
    Select,
    Transform,
    PointPicking,
}

#[derive(Debug, Default)]
struct DialogActiveState {
    substate: DialogSubstate,
}

#[derive(Debug, Default)]
enum DialogSubstate {
    #[default]
    Listening,
    PointPicking,
    AwaitingInput,
    Executing,
}

impl AppStateMachine {
    pub fn new(default_app_mode: AppMode) -> Self {
        Self {
            shared: SharedState::new(default_app_mode),
            active_state: RootState::Idle(IdleState::default()),
        }
    }

    pub fn shared_state(&self) -> &SharedState {
        &self.shared
    }

    pub fn shared_state_mut(&mut self) -> &mut SharedState {
        &mut self.shared
    }

    pub fn handle_event(
        &mut self,
        event: InteractionEvent,
        mut context: InteractionContext<'_>,
    ) -> EventDispatchOutcome {
        // Placeholder routing logic that mimics the structure of the future HSM.
        let outcome = match &mut self.active_state {
            RootState::Idle(idle) => idle.handle(&event, &mut context),
            RootState::DialogActive(dialog) => dialog.handle(&event, &mut context),
        };

        // Process any queued state change requests.
        while let Some(request) = self.shared.pending_requests.pop_front() {
            self.apply_state_change(request);
        }

        outcome
    }

    fn apply_state_change(&mut self, request: StateChangeRequest) {
        match request {
            StateChangeRequest::SetAppMode(mode) => self.shared.app_mode = mode,
            StateChangeRequest::SetMouseMode(mode) => self.shared.mouse_mode = mode,
            StateChangeRequest::SetSelectionMode(mode) => self.shared.selection_mode = mode,
            StateChangeRequest::PushDialog(_registration) => {
                // TODO: Route through DialogService once integrated.
            }
            StateChangeRequest::PopDialog(_id, _result) => {
                self.active_state = RootState::Idle(IdleState::default());
            }
            StateChangeRequest::TriggerOperation(_command) => {
                // TODO: Integrate with command/operation services.
            }
            StateChangeRequest::RefreshRender => {
                // TODO: Send render refresh request when the sender is available.
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventDispatchOutcome {
    Consumed,
    Ignored,
}

impl IdleState {
    fn handle(
        &mut self,
        event: &InteractionEvent,
        ctx: &mut InteractionContext<'_>,
    ) -> EventDispatchOutcome {
        match (&self.substate, event.kind()) {
            (IdleSubstate::Navigate, InteractionEventKind::ViewportPicked) => {
                ctx.push_request(StateChangeRequest::SetMouseMode(MouseMode::Navigate));
                EventDispatchOutcome::Consumed
            }
            (IdleSubstate::Select, InteractionEventKind::ToolsheetSelection) => {
                ctx.push_request(StateChangeRequest::SetSelectionMode(SelectionMode::Part));
                EventDispatchOutcome::Consumed
            }
            _ => EventDispatchOutcome::Ignored,
        }
    }
}

impl DialogActiveState {
    fn handle(
        &mut self,
        event: &InteractionEvent,
        ctx: &mut InteractionContext<'_>,
    ) -> EventDispatchOutcome {
        if let Some(bridge) = ctx.dialog_bridge.as_deref_mut() {
            if let Some(mut active) = bridge.active_dialog_descriptor() {
                if active.listens_to(event.kind()) {
                    let outcome = active.on_event(event);
                    return match outcome {
                        ShortcutOutcome::Consumed => EventDispatchOutcome::Consumed,
                        ShortcutOutcome::Ignored => EventDispatchOutcome::Ignored,
                    };
                }
            }
        }

        match self.substate {
            DialogSubstate::Listening => EventDispatchOutcome::Ignored,
            DialogSubstate::PointPicking => {
                ctx.push_request(StateChangeRequest::SetMouseMode(MouseMode::PointPicking));
                EventDispatchOutcome::Consumed
            }
            DialogSubstate::AwaitingInput | DialogSubstate::Executing => {
                EventDispatchOutcome::Consumed
            }
        }
    }
}
