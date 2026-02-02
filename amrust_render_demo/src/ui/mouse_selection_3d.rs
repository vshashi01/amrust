use smol::{channel::Sender, lock::RwLock};
use statig::prelude::*;
use statig::{
    Outcome::{self, Handled, Transition},
    prelude::InitializedStateMachine,
    state_machine,
};

use crate::core::interfaces::mouse_3d_viewport::Mouse3dFrameContext;
use crate::core::{
    amrust_db::Db,
    interfaces::mouse_3d_viewport::{Mouse3DViewport, Mouse3dContext, MouseEvt},
    services::render_service::RenderServiceRequest,
};

use std::sync::Arc;

#[derive(Debug, Default, Clone)]
pub struct MouseSelection3d;

impl MouseSelection3d {
    pub fn get_initialized_sm(
        frame_context: Mouse3dFrameContext,
        db: &Arc<RwLock<Db>>,
        render_request_tx: &Sender<RenderServiceRequest>,
    ) -> InitializedStateMachine<Self> {
        Self.uninitialized_state_machine()
            .init_with_context(&mut Mouse3dContext::new(
                frame_context,
                db,
                render_request_tx,
            ))
    }
}

#[state_machine(initial = "State::idle()", state(derive(Debug, Clone)))]
impl MouseSelection3d {
    #[state]
    fn idle(event: &MouseEvt) -> Outcome<State> {
        match event {
            MouseEvt::Hovered(..) => Transition(State::Hovered {}),
            MouseEvt::PrimaryBtnDown(..) => Transition(State::Pick {}),
            _ => Handled,
        }
    }

    #[state]
    fn hovered(event: &MouseEvt) -> Outcome<State> {
        match event {
            MouseEvt::Hovered(..) => {
                //Todo Show Hover Preview
                Handled
            }
            MouseEvt::PrimaryBtnDown(..) => Transition(State::Pick {}),
            MouseEvt::NotHovered() => Transition(State::Idle {}),
            _ => Handled,
        }
    }

    #[state]
    fn pick(event: &MouseEvt, context: &mut Mouse3dContext) -> Outcome<State> {
        match event {
            MouseEvt::PrimaryBtnDrag(..) => todo!("Implement drag selection"),
            MouseEvt::PrimaryBtnUp(pos, _) => {
                let screen_pt = context.to_screen_pt(*pos);
                let entities = context.pick_entities(&screen_pt);
                log::info!("Picked entities: {entities:?}");

                Transition(State::Idle {})
            }
            _ => Handled,
        }
    }
}

impl Mouse3DViewport for InitializedStateMachine<MouseSelection3d> {
    fn can_handle(&self, event: &MouseEvt, _context: &mut Mouse3dContext) -> bool {
        matches!(
            event,
            MouseEvt::Hovered(..)
                | MouseEvt::PrimaryBtnDown(..)
                | MouseEvt::PrimaryBtnUp(..)
                | MouseEvt::NotHovered()
        )
    }

    fn is_clean(&self) -> bool {
        matches!(self.state(), State::Idle {} | State::Hovered {})
    }

    fn handle_event(&mut self, event: &MouseEvt, context: &mut Mouse3dContext) {
        self.handle_with_context(event, context);
    }

    fn can_allow_passthrough(&self, event: &MouseEvt) -> bool {
        matches!(event, MouseEvt::Hovered(..) | MouseEvt::NotHovered())
    }
}
