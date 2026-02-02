use amrust_render::camera::CameraTransform;
use egui::Key;
use smol::{channel::Sender, lock::RwLock};
use statig::{
    Outcome::{self, Handled, Super, Transition},
    prelude::{InitializedStateMachine, IntoStateMachineExt},
    state_machine,
};

use crate::core::{
    amrust_db::Db,
    interfaces::mouse_3d_viewport::{
        Mouse3DViewport, Mouse3dContext, Mouse3dFrameContext, MouseEvt,
    },
    services::render_service::RenderServiceRequest,
};

use std::sync::Arc;

#[derive(Debug, Default, Clone)]
pub struct MouseCamera3d {}

const BASE_ROTATE_SENSITIVITY: f32 = 0.0003;
const ROTATE_SENSITIVITY_FACTOR: f32 = 0.2; //used to scale the sensitivity based on zoom levels
const BASE_PAN_SENSITIVITY: f32 = 0.001;
const PAN_SENSITIVITY_FACTOR: f32 = 0.2; // used to scale the sensitivity based on zoom levels

fn zoom_aware_sensitivity(base_sensitivity: f32, distance: f32, scale_factor: f32) -> f32 {
    base_sensitivity * ((distance * scale_factor).clamp(0.5, 3.0))
}

impl MouseCamera3d {
    pub fn get_initialized_sm(
        frame_context: Mouse3dFrameContext,
        db: &Arc<RwLock<Db>>,
        render_request_tx: &Sender<RenderServiceRequest>,
    ) -> InitializedStateMachine<Self> {
        Self::default()
            .uninitialized_state_machine()
            .init_with_context(&mut Mouse3dContext::new(
                frame_context,
                db,
                render_request_tx,
            ))
    }
}

#[state_machine(initial = "State::idle()", state(derive(Debug, Clone)))]
impl MouseCamera3d {
    #[state]
    fn idle(event: &MouseEvt) -> Outcome<State> {
        match event {
            MouseEvt::Hovered(_, _) => Transition(State::Hovered {}),
            MouseEvt::NotHovered() => Handled,
            _ => Handled,
        }
    }

    #[state]
    fn hovered(context: &mut Mouse3dContext, event: &MouseEvt) -> Outcome<State> {
        match event {
            MouseEvt::SecondaryBtnDown(pos, _) => {
                //rotate
                let (pivot, screen_pt) = context.get_pivot_and_screen_pt(*pos);
                let (right, up, _, distance) = context.get_camera_vectors(pivot);
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
                let (pivot, _) = context.get_pivot_and_screen_pt(*pos);
                let (right, up, _, distance) = context.get_camera_vectors(pivot);
                let pan_sensitivity =
                    zoom_aware_sensitivity(BASE_PAN_SENSITIVITY, distance, PAN_SENSITIVITY_FACTOR);

                Transition(State::PanCamera {
                    pan_sensitivity,
                    up,
                    right,
                })
            }
            MouseEvt::MiddleBtnScroll(scroll_delta, pos, _) => {
                let (target, _) = context.get_pivot_and_screen_pt(*pos);

                if let Err(err) =
                    context.try_send_render_request(RenderServiceRequest::TransformCamera(vec![
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
        context: &mut Mouse3dContext,
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
        context: &mut Mouse3dContext,
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

                    if let Err(err) = context.try_send_render_request(
                        RenderServiceRequest::TransformCamera(vec![CameraTransform::Rotate {
                            pivot: *pivot,
                            rotation_axis: *locked_rotation_axis,
                            angle,
                        }]),
                    ) {
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
        context: &mut Mouse3dContext,
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
                    .try_send_render_request(RenderServiceRequest::TransformCamera(transforms))
                {
                    log::error!("Unable to send transform: {err:?}")
                }

                Handled
            }
            MouseEvt::SecondaryBtnUp(_, _) => Transition(State::Idle {}),
            _ => Super,
        }
    }

    #[state]
    fn pan_camera(
        pan_sensitivity: &f32,
        up: &glam::Vec3,
        right: &glam::Vec3,
        context: &mut Mouse3dContext,
        event: &MouseEvt,
    ) -> Outcome<State> {
        match event {
            MouseEvt::MiddleBtnDrag(delta, _, _) => {
                let movement = (right * -delta.x + up * delta.y) * pan_sensitivity;

                if let Err(err) =
                    context.try_send_render_request(RenderServiceRequest::TransformCamera(vec![
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

impl Mouse3DViewport for InitializedStateMachine<MouseCamera3d> {
    fn can_handle(&self, _event: &MouseEvt, _context: &mut Mouse3dContext) -> bool {
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

    fn handle_event(&mut self, event: &MouseEvt, context: &mut Mouse3dContext) {
        self.handle_with_context(event, context);
    }
}
