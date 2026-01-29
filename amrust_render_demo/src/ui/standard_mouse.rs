use amrust_render::{bounding_box::BoundingBox, camera::CameraTransform};
use egui::Key;
use glam::Mat4;
use smol::{channel::Sender, lock::RwLock};
use statig::{
    Outcome::{self, Handled, Super, Transition},
    prelude::{InitializedStateMachine, IntoStateMachineExt},
    state_machine,
};

use crate::{
    core::{
        amrust_db::Db,
        app_mode::AppMode,
        services::render_service::RenderServiceRequest,
        types::{part::PartId, part_instance::PartInstanceId, transformation::Transformation},
        utils::picker::{PickedEntity, Picker, PickerConfig},
    },
    ui::{app_mouse_manager::AppMouseManager, mouse_selection_sm::MouseSelectionSM},
};

use std::sync::Arc;

pub struct StandardMouse {
    db: Arc<RwLock<Db>>,
    render_request_tx: Sender<RenderServiceRequest>,
    mouse_manager: AppMouseManager,
}

#[derive(Debug, Clone)]
pub struct FrameContext {
    pub viewport_rect: egui::Rect,
    pub view_proj: Mat4,
    pub scene_bbox: BoundingBox,
    pub parts_can_be_picked: Vec<PartId>,
    pub instances_can_be_picked: Vec<PartInstanceId>,
    pub app_mode: AppMode,
}

impl StandardMouse {
    pub fn new(
        context: FrameContext,
        db: Arc<RwLock<Db>>,
        render_request_tx: Sender<RenderServiceRequest>,
    ) -> Self {
        let mut mouse_manager = AppMouseManager::new();
        let init_machine = MouseCameraSM::default()
            .uninitialized_state_machine()
            .init_with_context(&mut InputContext::new_from_frame_context(
                context.clone(),
                &db,
                &render_request_tx,
            ));

        let selection_mouse = MouseSelectionSM::default()
            .uninitialized_state_machine()
            .init_with_context(&mut InputContext::new_from_frame_context(
                context.clone(),
                &db,
                &render_request_tx,
            ));

        mouse_manager.push(Box::new(init_machine));
        mouse_manager.push(Box::new(selection_mouse));

        Self {
            db,
            render_request_tx,
            mouse_manager,
        }
    }

    pub fn run(&mut self, response: &egui::Response, ctx: &egui::Context, context: FrameContext) {
        let mut input_context =
            InputContext::new_from_frame_context(context, &self.db, &self.render_request_tx);

        let events = gather_mouse_events(response, ctx);
        for evt in events {
            self.mouse_manager.handle_event(&evt, &mut input_context);
        }
    }
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
        &response,
        egui::PointerButton::Primary,
        MouseEvt::PrimaryBtnDown,
    );
    check_button_down(
        &mut events,
        &input_state,
        &response,
        egui::PointerButton::Secondary,
        MouseEvt::SecondaryBtnDown,
    );
    check_button_down(
        &mut events,
        &input_state,
        &response,
        egui::PointerButton::Middle,
        MouseEvt::MiddleBtnDown,
    );

    // Button up events
    check_button_up(
        &mut events,
        &input_state,
        &response,
        egui::PointerButton::Primary,
        MouseEvt::PrimaryBtnUp,
    );
    check_button_up(
        &mut events,
        &input_state,
        &response,
        egui::PointerButton::Secondary,
        MouseEvt::SecondaryBtnUp,
    );
    check_button_up(
        &mut events,
        &input_state,
        &response,
        egui::PointerButton::Middle,
        MouseEvt::MiddleBtnUp,
    );

    // Drag events
    check_button_drag(
        &mut events,
        &input_state,
        &response,
        egui::PointerButton::Primary,
        MouseEvt::PrimaryBtnDrag,
    );
    check_button_drag(
        &mut events,
        &input_state,
        &response,
        egui::PointerButton::Secondary,
        MouseEvt::SecondaryBtnDrag,
    );
    check_button_drag(
        &mut events,
        &input_state,
        &response,
        egui::PointerButton::Middle,
        MouseEvt::MiddleBtnDrag,
    );

    // Scroll events (MiddleBtnScroll - we treat as from middle)
    if response.hovered() && input_state.raw_scroll_delta.y.abs() > 0.0 {
        if let Some(pos) = response.hover_pos() {
            events.push(MouseEvt::MiddleBtnScroll(
                input_state.raw_scroll_delta,
                pos,
                input_state.clone(),
            ));
        }
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
    if input_state.pointer.button_pressed(button) {
        if let Some(pos) = response.interact_pointer_pos() {
            events.push(make_evt(pos, input_state.clone()));
        }
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
    if input_state.pointer.button_released(button) {
        if let Some(pos) = response.interact_pointer_pos() {
            events.push(make_evt(pos, input_state.clone()));
        }
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
    if response.dragged_by(button) {
        if let Some(pos) = response.interact_pointer_pos() {
            events.push(make_evt(response.drag_delta(), pos, input_state.clone()));
        }
    }
}

fn pick_entities(
    app_mode: AppMode,
    screen_pt: &glam::Vec2,
    viewport_size: glam::Vec2,
    view_proj: Mat4,
    db: &Arc<RwLock<Db>>,
    parts_can_be_picked: &[PartId],
    instance_can_be_picked: &[PartInstanceId],
) -> Vec<PickedEntity> {
    let radius = 0.1; // example radius

    if let Some(read_db) = db.try_read() {
        match app_mode {
            AppMode::Objects => Picker.pick_from_parts(
                &PickerConfig {
                    screen_pt: *screen_pt,
                    viewport_size,
                    view_proj,
                    snap_radius: radius,
                    return_all_intersections: false,
                },
                &read_db,
                parts_can_be_picked,
                true,
            ),
            AppMode::Build => Picker.pick_from_instances(
                &PickerConfig {
                    screen_pt: *screen_pt,
                    viewport_size,
                    view_proj,
                    snap_radius: radius,
                    return_all_intersections: false,
                },
                &read_db,
                instance_can_be_picked,
                true,
                Transformation(Mat4::IDENTITY),
            ),
        }
    } else {
        vec![]
    }
}

fn zoom_aware_sensitivity(base_sensitivity: f32, distance: f32, scale_factor: f32) -> f32 {
    base_sensitivity * ((distance * scale_factor).clamp(0.5, 3.0))
}

#[inline]
fn get_camera_vectors(
    view_proj: Mat4,
    pivot: glam::Vec3,
) -> (glam::Vec3, glam::Vec3, glam::Vec3, f32) {
    let view = view_proj.inverse();
    let right = glam::Vec3::new(view.col(0).x, view.col(0).y, view.col(0).z);
    let up = glam::Vec3::new(view.col(1).x, view.col(1).y, view.col(1).z);
    let camera_pos = glam::Vec3::new(view.col(3).x, view.col(3).y, view.col(3).z);
    let distance = camera_pos.distance(pivot);

    (right, up, camera_pos, distance)
}

// #[inline]
// pub(crate) fn to_screen_pt(viewport_rect: egui::Rect, pos: egui::Pos2) -> glam::Vec2 {
//     let relative_pos = pos - viewport_rect.min;
//     glam::Vec2::new(relative_pos.x, relative_pos.y)
// }

// ToDo currently all Pos2, are in the global egui coordinate and not the local coordinate
// within the response region.
#[allow(dead_code)]
pub enum MouseEvt {
    PrimaryBtnDown(egui::Pos2, egui::InputState),
    PrimaryBtnUp(egui::Pos2, egui::InputState),
    PrimaryBtnDrag(egui::Vec2, egui::Pos2, egui::InputState),
    SecondaryBtnDown(egui::Pos2, egui::InputState),
    SecondaryBtnUp(egui::Pos2, egui::InputState),
    SecondaryBtnDrag(egui::Vec2, egui::Pos2, egui::InputState),
    MiddleBtnDown(egui::Pos2, egui::InputState),
    MiddleBtnUp(egui::Pos2, egui::InputState),
    MiddleBtnDrag(egui::Vec2, egui::Pos2, egui::InputState),
    MiddleBtnScroll(egui::Vec2, egui::Pos2, egui::InputState),
    Hovered(egui::Pos2, egui::InputState),
    NotHovered(),
}

#[derive(Debug, Default)]
struct MouseCameraSM {}

const BASE_ROTATE_SENSITIVITY: f32 = 0.0003;
const ROTATE_SENSITIVITY_FACTOR: f32 = 0.2; //used to scale the sensitivity based on zoom levels
const BASE_PAN_SENSITIVITY: f32 = 0.001;
const PAN_SENSITIVITY_FACTOR: f32 = 0.2; // used to scale the sensitivity based on zoom levels

#[state_machine(initial = "State::idle()")]
impl MouseCameraSM {
    #[state]
    fn idle(event: &MouseEvt) -> Outcome<State> {
        match event {
            MouseEvt::Hovered(_, _) => Transition(State::Hovered {}),
            MouseEvt::NotHovered() => Handled,
            _ => Handled,
        }
    }

    #[state]
    fn hovered(context: &mut InputContext, event: &MouseEvt) -> Outcome<State> {
        match event {
            //MouseEvt::PrimaryBtnDown(..) => Transition(State::Selection {}),
            MouseEvt::SecondaryBtnDown(pos, _) => {
                //rotate
                let (pivot, screen_pt) = get_pivot_and_screen_pt(context, *pos);
                let (right, up, _, distance) = get_camera_vectors(context.view_proj, pivot);
                let drag_sensitivity = zoom_aware_sensitivity(
                    BASE_ROTATE_SENSITIVITY,
                    distance,
                    ROTATE_SENSITIVITY_FACTOR,
                );

                Transition(State::RotateStarted {
                    pivot,
                    up,
                    right,
                    initial_screen_pt: screen_pt,
                    drag_sensitivity,
                })
            }
            MouseEvt::MiddleBtnDown(pos, _) => {
                //pan
                let (pivot, _) = get_pivot_and_screen_pt(context, *pos);
                let (right, up, _, distance) = get_camera_vectors(context.view_proj, pivot);
                let pan_sensitivity =
                    zoom_aware_sensitivity(BASE_PAN_SENSITIVITY, distance, PAN_SENSITIVITY_FACTOR);

                Transition(State::PanCamera {
                    pan_sensitivity,
                    up,
                    right,
                })
            }
            MouseEvt::MiddleBtnScroll(scroll_delta, pos, _) => {
                let (target, _) = get_pivot_and_screen_pt(context, *pos);

                if let Err(err) =
                    context
                        .render_request_tx
                        .try_send(RenderServiceRequest::TransformCamera(vec![
                            CameraTransform::ZoomTowards {
                                target,
                                amount: scroll_delta.y * 0.0001,
                            },
                        ]))
                {
                    log::error!("Unable to send transform: {err:?}");
                }

                Handled
            }
            MouseEvt::NotHovered() => Transition(State::Idle {}),
            _ => Super,
        }
    }

    #[state]
    fn rotate_started(
        pivot: &glam::Vec3,
        up: &glam::Vec3,
        right: &glam::Vec3,
        initial_screen_pt: &glam::Vec2,
        drag_sensitivity: &f32,
        context: &mut InputContext,
        event: &MouseEvt,
    ) -> Outcome<State> {
        match event {
            MouseEvt::SecondaryBtnDrag(_, pos, input_state) => {
                if input_state.key_down(Key::Num5) {
                    let current_screen_pt = context.to_screen_pt(*pos);

                    let displacement = current_screen_pt - initial_screen_pt;
                    let lock_threshold = 2.0; //px
                    if displacement.length() >= lock_threshold {
                        let rotation_axis = if displacement.x.abs() > displacement.y.abs() {
                            up
                        } else {
                            right
                        };

                        return Transition(State::RotateLockedCamera {
                            pivot: *pivot,
                            up: *up,
                            right: *right,
                            locked_rotation_axis: *rotation_axis,
                            drag_sensitivity: *drag_sensitivity,
                        });
                    }
                }
                Transition(State::RotateCameraFree {
                    pivot: *pivot,
                    up: *up,
                    right: *right,
                    drag_sensitivity: *drag_sensitivity,
                })
            }
            MouseEvt::SecondaryBtnUp(_, _) => Transition(State::Idle {}),
            _ => Super,
        }
    }

    #[state]
    fn rotate_locked_camera(
        pivot: &glam::Vec3,
        up: &glam::Vec3,
        right: &glam::Vec3,
        locked_rotation_axis: &glam::Vec3,
        drag_sensitivity: &f32,
        context: &mut InputContext,
        event: &MouseEvt,
    ) -> Outcome<State> {
        match event {
            MouseEvt::SecondaryBtnDrag(delta, _, input_state) => {
                if input_state.key_down(Key::Num5) {
                    let angle = if locked_rotation_axis == up {
                        -delta.x * drag_sensitivity
                    } else {
                        delta.y * drag_sensitivity
                    };

                    if let Err(err) =
                        context
                            .render_request_tx
                            .try_send(RenderServiceRequest::TransformCamera(vec![
                                CameraTransform::Rotate {
                                    pivot: *pivot,
                                    rotation_axis: *locked_rotation_axis,
                                    angle,
                                },
                            ]))
                    {
                        log::error!("Unable to send transform: {err:?}")
                    }

                    Handled
                } else {
                    Transition(State::RotateCameraFree {
                        pivot: *pivot,
                        up: *up,
                        right: *right,
                        drag_sensitivity: *drag_sensitivity,
                    })
                }
            }
            MouseEvt::SecondaryBtnUp(_, _) => Transition(State::Idle {}),
            _ => Super,
        }
    }

    #[state]
    fn rotate_camera_free(
        pivot: &glam::Vec3,
        up: &glam::Vec3,
        right: &glam::Vec3,
        drag_sensitivity: &f32,
        context: &mut InputContext,
        event: &MouseEvt,
    ) -> Outcome<State> {
        match event {
            MouseEvt::SecondaryBtnDrag(delta, _, _) => {
                let transforms = vec![
                    CameraTransform::Rotate {
                        pivot: *pivot,
                        rotation_axis: *up,
                        angle: -delta.x * drag_sensitivity,
                    },
                    CameraTransform::Rotate {
                        pivot: *pivot,
                        rotation_axis: *right,
                        angle: -delta.y * drag_sensitivity,
                    },
                ];

                if let Err(err) = context
                    .render_request_tx
                    .try_send(RenderServiceRequest::TransformCamera(transforms))
                {
                    log::error!("Unable to send transform: {err:?}")
                }

                Handled
            }
            MouseEvt::SecondaryBtnUp(_, _) => Transition(State::Idle {}),
            _ => Super,
        }
    }

    // #[state]
    // fn selection(context: &mut InputContext<'_>, event: &MouseEvt) -> Outcome<State> {
    //     match event {
    //         MouseEvt::PrimaryBtnDrag(..) => todo!("Implement drag selection"),
    //         MouseEvt::PrimaryBtnUp(pos, _) => {
    //             let screen_pt = context.to_screen_pt(*pos);
    //             let entities = context.pick_entities(&screen_pt);
    //             log::info!("Picked entities: {entities:?}");

    //             Transition(State::Idle {})
    //         }
    //         _ => Handled,
    //     }
    // }

    #[state]
    fn pan_camera(
        pan_sensitivity: &f32,
        up: &glam::Vec3,
        right: &glam::Vec3,
        context: &mut InputContext,
        event: &MouseEvt,
    ) -> Outcome<State> {
        match event {
            MouseEvt::MiddleBtnDrag(delta, _, _) => {
                let movement = (right * -delta.x + up * delta.y) * pan_sensitivity;

                if let Err(err) =
                    context
                        .render_request_tx
                        .try_send(RenderServiceRequest::TransformCamera(vec![
                            CameraTransform::Pan(movement),
                        ]))
                {
                    log::error!("Unable to send transform: {err:?}");
                }

                Handled
            }
            MouseEvt::MiddleBtnUp(..) => Transition(State::Idle {}),
            _ => Super,
        }
    }
}

impl Mouse3DStateMachine for InitializedStateMachine<MouseCameraSM> {
    fn can_handle(&self, _event: &MouseEvt, _context: &mut InputContext) -> bool {
        matches!(
            _event,
            MouseEvt::Hovered(..)
                | MouseEvt::PrimaryBtnDown(..)
                | MouseEvt::PrimaryBtnUp(..)
                | MouseEvt::SecondaryBtnDown(..)
                | MouseEvt::PrimaryBtnDrag(..)
                | MouseEvt::SecondaryBtnUp(..)
                | MouseEvt::MiddleBtnDown(..)
                | MouseEvt::MiddleBtnDrag(..)
                | MouseEvt::MiddleBtnUp(..)
                | MouseEvt::MiddleBtnScroll(..)
                | MouseEvt::NotHovered()
        )
    }

    fn is_clean(&self) -> bool {
        matches!(self.state(), State::Idle {} | State::Hovered {})
    }

    fn handle_event(&mut self, event: &MouseEvt, context: &mut InputContext) {
        self.handle_with_context(event, context);
    }
}

fn get_pivot_and_screen_pt(
    context: &mut InputContext,
    egui_screen_pos: egui::Pos2,
) -> (glam::Vec3, glam::Vec2) {
    let screen_pt = context.to_screen_pt(egui_screen_pos);
    let entities = context.pick_entities(&screen_pt);
    let pivot = entities
        .first()
        .map_or(context.scene_bbox.center(), |p| p.intersection);
    (pivot, screen_pt)
}

pub struct InputContext {
    viewport_rect: egui::Rect,
    render_request_tx: Sender<RenderServiceRequest>,
    view_proj: Mat4,
    scene_bbox: BoundingBox,
    db: Arc<RwLock<Db>>,
    parts_can_be_picked: Vec<PartId>,
    instances_can_be_picked: Vec<PartInstanceId>,
    app_mode: AppMode,
}

impl InputContext {
    pub fn new_from_frame_context(
        frame_context: FrameContext,
        db: &Arc<RwLock<Db>>,
        render_request_tx: &Sender<RenderServiceRequest>,
    ) -> Self {
        Self {
            viewport_rect: frame_context.viewport_rect,
            render_request_tx: render_request_tx.clone(),
            view_proj: frame_context.view_proj,
            scene_bbox: frame_context.scene_bbox,
            db: db.clone(),
            parts_can_be_picked: frame_context.parts_can_be_picked,
            instances_can_be_picked: frame_context.instances_can_be_picked,
            app_mode: frame_context.app_mode,
        }
    }

    pub fn get_viewport_size(&self) -> glam::Vec2 {
        let rect_size = self.viewport_rect.max - self.viewport_rect.min;
        glam::Vec2 {
            x: rect_size.x,
            y: rect_size.y,
        }
    }

    #[inline]
    pub fn pick_entities(&self, screen_pt: &glam::Vec2) -> Vec<PickedEntity> {
        pick_entities(
            self.app_mode,
            screen_pt,
            self.get_viewport_size(),
            self.view_proj,
            &self.db,
            &self.parts_can_be_picked,
            &self.instances_can_be_picked,
        )
    }

    #[inline]
    pub fn to_screen_pt(&self, pos: egui::Pos2) -> glam::Vec2 {
        let relative_pos = pos - self.viewport_rect.min;
        glam::Vec2::new(relative_pos.x, relative_pos.y)
    }
}

pub trait Mouse3DStateMachine {
    fn can_handle(&self, event: &MouseEvt, context: &mut InputContext) -> bool;

    fn is_clean(&self) -> bool;

    fn can_allow_passthrough(&self, event: &MouseEvt) -> bool {
        false
    }

    fn handle_event(&mut self, event: &MouseEvt, context: &mut InputContext);
}
