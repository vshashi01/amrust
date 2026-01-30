use amrust_render::bounding_box::BoundingBox;
use smol::{channel::Sender, lock::RwLock};

use crate::core::{
    amrust_db::Db,
    app_mode::AppMode,
    interfaces::mouse_3d_viewport::{Mouse3DViewport, Mouse3dContext, MouseEvt},
    services::render_service::RenderServiceRequest,
    types::{part::PartId, part_instance::PartInstanceId},
};

use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct Mouse3dFrameContext {
    pub viewport_rect: egui::Rect,
    pub view_proj: glam::Mat4,
    pub scene_bbox: BoundingBox,
    pub parts_can_be_picked: Vec<PartId>,
    pub instances_can_be_picked: Vec<PartInstanceId>,
    pub app_mode: AppMode,
}

pub struct Mouse3dManager {
    pub stack: Vec<Box<dyn Mouse3DViewport>>,
    captured_index: Option<usize>, // SM currently owning the gesture
}

impl Mouse3dManager {
    pub fn new() -> Self {
        Self {
            stack: Vec::new(),
            captured_index: None,
        }
    }

    pub fn push(&mut self, sm: Box<dyn Mouse3DViewport>) {
        self.stack.push(sm);
    }

    pub fn pop(&mut self) {
        self.stack.pop();
    }

    pub fn run(
        &mut self,
        response: &egui::Response,
        ctx: &egui::Context,
        context: Mouse3dFrameContext,
        db: &Arc<RwLock<Db>>,
        render_request_tx: &Sender<RenderServiceRequest>,
    ) {
        let mut input_context = Mouse3dContext::new(
            context.viewport_rect,
            context.view_proj,
            context.scene_bbox,
            context.parts_can_be_picked,
            context.instances_can_be_picked,
            context.app_mode,
            db,
            render_request_tx,
        );

        let events = gather_mouse_events(response, ctx);
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
        if is_gesture_starter(evt)
            && let Some(capture_idx) = last_handler_index
            && !self.stack[capture_idx].is_clean()
        {
            self.captured_index = Some(capture_idx);
        }
    }
}

/// Define what counts as a "gesture starter"
fn is_gesture_starter(evt: &MouseEvt) -> bool {
    matches!(
        evt,
        MouseEvt::PrimaryBtnDown(..) | MouseEvt::SecondaryBtnDown(..) | MouseEvt::MiddleBtnDown(..)
    )
}

/// Collect all mouse-related events for this frame from egui context + response.
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

    // Scroll events (MiddleBtnScroll - we treat as from middle)
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
