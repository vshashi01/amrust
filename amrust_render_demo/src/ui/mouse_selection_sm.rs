use statig::{
    Outcome::{self, Handled, Transition},
    prelude::InitializedStateMachine,
    state_machine,
};

use crate::ui::standard_mouse::{InputContext, Mouse3DStateMachine, MouseEvt};

#[derive(Debug, Default)]
pub struct MouseSelectionSM;

#[state_machine(initial = "State::idle()")]
impl MouseSelectionSM {
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
    fn pick(event: &MouseEvt, context: &mut InputContext) -> Outcome<State> {
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

impl Mouse3DStateMachine for InitializedStateMachine<MouseSelectionSM> {
    fn can_handle(&self, event: &MouseEvt, _context: &mut InputContext) -> bool {
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

    fn handle_event(&mut self, event: &MouseEvt, context: &mut InputContext) {
        self.handle_with_context(event, context);
    }

    fn can_allow_passthrough(&self, event: &MouseEvt) -> bool {
        matches!(event, MouseEvt::Hovered(..) | MouseEvt::NotHovered())
    }
}
