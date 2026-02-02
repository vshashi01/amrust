use smol::{channel::Sender, lock::RwLock};
use statig::prelude::InitializedStateMachine;

use crate::{
    core::{
        amrust_db::Db,
        interfaces::mouse_3d_viewport::{
            Mouse3DViewport, Mouse3dContext, Mouse3dFrameContext, MouseEvt,
        },
        services::render_service::RenderServiceRequest,
    },
    ui::{mouse_camera_3d::MouseCamera3d, mouse_selection_3d::MouseSelection3d},
};

use std::sync::Arc;

#[derive(Debug, Clone)]
pub enum Mouse3d {
    Selection(InitializedStateMachine<MouseSelection3d>),
    Camera(InitializedStateMachine<MouseCamera3d>),
}

pub struct Mouse3dManager {
    pub stack: Vec<Mouse3d>,
    captured_index: Option<usize>, // SM currently owning the gesture
}

impl Mouse3dManager {
    pub fn new() -> Self {
        Self {
            stack: Vec::new(),
            captured_index: None,
        }
    }

    pub fn push(&mut self, sm: Mouse3d) {
        self.stack.push(sm.clone());
    }

    pub fn push_clone(&mut self, sm: &Mouse3d) {
        self.stack.push(sm.clone());
    }

    pub fn pop(&mut self) {
        self.stack.pop();
    }

    pub fn run(
        &mut self,
        response: &egui::Response,
        egui_context: &egui::Context,
        frame_context: Mouse3dFrameContext,
        db: &Arc<RwLock<Db>>,
        render_request_tx: &Sender<RenderServiceRequest>,
    ) {
        let mut input_context = Mouse3dContext::new(frame_context, db, render_request_tx);

        let events = gather_mouse_events(response, egui_context);
        for evt in events {
            self.handle_event(&evt, &mut input_context);
        }
    }

    fn handle_event(&mut self, evt: &MouseEvt, ctx: &mut Mouse3dContext) {
        // If an SM has capture, it gets everything until it becomes inactive
        if let Some(i) = self.captured_index {
            self.stack[i].handle_event(evt, ctx);
            if self.stack[i].is_clean() {
                self.captured_index = None;
            }
            return;
        }

        let mut last_handler_index: Option<usize> = None;

        let stack_len = self.stack.len();
        // Walk stack from top to bottom
        for (idx, sm) in self.stack.iter_mut().rev().enumerate() {
            let actual_idx = stack_len - 1 - idx;

            if sm.can_handle(evt, ctx) {
                sm.handle_event(evt, ctx);
                last_handler_index = Some(actual_idx);

                if !sm.can_allow_passthrough(evt) {
                    break; // stop bubbling down
                }
            }
        }

        // Gesture capture: if this was a button-down event or similar,
        // the last handler gets exclusive control for remainder of gesture
        // ToDo: Revisit this in the future for more complex mouse interactions
        if is_gesture_starter(evt)
            && let Some(capture_idx) = last_handler_index
            && !self.stack[capture_idx].is_clean()
        {
            self.captured_index = Some(capture_idx);
        }
    }
}

impl Mouse3DViewport for Mouse3d {
    fn can_handle(&self, event: &MouseEvt, context: &mut Mouse3dContext) -> bool {
        match self {
            Mouse3d::Selection(sm) => sm.can_handle(event, context),
            Mouse3d::Camera(sm) => sm.can_handle(event, context),
        }
    }

    fn is_clean(&self) -> bool {
        match self {
            Mouse3d::Selection(sm) => sm.is_clean(),
            Mouse3d::Camera(sm) => sm.is_clean(),
        }
    }

    fn can_allow_passthrough(&self, event: &MouseEvt) -> bool {
        match self {
            Mouse3d::Selection(sm) => sm.can_allow_passthrough(event),
            Mouse3d::Camera(sm) => sm.can_allow_passthrough(event),
        }
    }

    fn handle_event(&mut self, event: &MouseEvt, context: &mut Mouse3dContext) {
        match self {
            Mouse3d::Selection(sm) => sm.handle_event(event, context),
            Mouse3d::Camera(sm) => sm.handle_event(event, context),
        }
    }
}

fn is_gesture_starter(evt: &MouseEvt) -> bool {
    matches!(
        evt,
        MouseEvt::PrimaryBtnDown(..) | MouseEvt::SecondaryBtnDown(..) | MouseEvt::MiddleBtnDown(..)
    )
}

fn gather_mouse_events(response: &egui::Response, ctx: &egui::Context) -> Vec<MouseEvt> {
    let mut events = Vec::new();
    let input_state = ctx.input(|i| i.clone());

    if response.hovered() {
        if let Some(pos) = response.hover_pos() {
            events.push(MouseEvt::Hovered(pos, input_state.clone()));
        }
    } else {
        events.push(MouseEvt::NotHovered());
        return events;
    }

    // Button down events
    check_button_down(
        &mut events,
        &input_state,
        response,
        egui::PointerButton::Primary,
        MouseEvt::PrimaryBtnDown,
    );
    check_button_down(
        &mut events,
        &input_state,
        response,
        egui::PointerButton::Secondary,
        MouseEvt::SecondaryBtnDown,
    );
    check_button_down(
        &mut events,
        &input_state,
        response,
        egui::PointerButton::Middle,
        MouseEvt::MiddleBtnDown,
    );

    // Button up events
    check_button_up(
        &mut events,
        &input_state,
        response,
        egui::PointerButton::Primary,
        MouseEvt::PrimaryBtnUp,
    );
    check_button_up(
        &mut events,
        &input_state,
        response,
        egui::PointerButton::Secondary,
        MouseEvt::SecondaryBtnUp,
    );
    check_button_up(
        &mut events,
        &input_state,
        response,
        egui::PointerButton::Middle,
        MouseEvt::MiddleBtnUp,
    );

    // Drag events
    check_button_drag(
        &mut events,
        &input_state,
        response,
        egui::PointerButton::Primary,
        MouseEvt::PrimaryBtnDrag,
    );
    check_button_drag(
        &mut events,
        &input_state,
        response,
        egui::PointerButton::Secondary,
        MouseEvt::SecondaryBtnDrag,
    );
    check_button_drag(
        &mut events,
        &input_state,
        response,
        egui::PointerButton::Middle,
        MouseEvt::MiddleBtnDrag,
    );

    // Scroll events (MiddleBtnScroll - and only care about vertical scroll for now)
    if response.hovered()
        && input_state.raw_scroll_delta.y.abs() > 0.0
        && let Some(pos) = response.hover_pos()
    {
        events.push(MouseEvt::MiddleBtnScroll(
            input_state.raw_scroll_delta,
            pos,
            input_state.clone(),
        ));
    }

    events
}

/// Helper functions to reduce duplication
fn check_button_down<F>(
    events: &mut Vec<MouseEvt>,
    input_state: &egui::InputState,
    response: &egui::Response,
    button: egui::PointerButton,
    make_evt: F,
) where
    F: Fn(egui::Pos2, egui::InputState) -> MouseEvt,
{
    if input_state.pointer.button_pressed(button)
        && let Some(pos) = response.interact_pointer_pos()
    {
        events.push(make_evt(pos, input_state.clone()));
    }
}

fn check_button_up<F>(
    events: &mut Vec<MouseEvt>,
    input_state: &egui::InputState,
    response: &egui::Response,
    button: egui::PointerButton,
    make_evt: F,
) where
    F: Fn(egui::Pos2, egui::InputState) -> MouseEvt,
{
    if input_state.pointer.button_released(button)
        && let Some(pos) = response.interact_pointer_pos()
    {
        events.push(make_evt(pos, input_state.clone()));
    }
}

fn check_button_drag<F>(
    events: &mut Vec<MouseEvt>,
    input_state: &egui::InputState,
    response: &egui::Response,
    button: egui::PointerButton,
    make_evt: F,
) where
    F: Fn(egui::Vec2, egui::Pos2, egui::InputState) -> MouseEvt,
{
    if response.dragged_by(button)
        && let Some(pos) = response.interact_pointer_pos()
    {
        events.push(make_evt(response.drag_delta(), pos, input_state.clone()));
    }
}
