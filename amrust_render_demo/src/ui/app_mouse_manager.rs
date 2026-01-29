use crate::ui::standard_mouse::{InputContext, Mouse3DStateMachine, MouseEvt};

pub struct AppMouseManager {
    pub stack: Vec<Box<dyn Mouse3DStateMachine>>,
    captured_index: Option<usize>, // SM currently owning the gesture
}

impl AppMouseManager {
    pub fn new() -> Self {
        Self {
            stack: Vec::new(),
            captured_index: None,
        }
    }

    pub fn push(&mut self, sm: Box<dyn Mouse3DStateMachine>) {
        self.stack.push(sm);
    }

    pub fn pop(&mut self) {
        self.stack.pop();
    }

    pub fn handle_event(&mut self, evt: &MouseEvt, ctx: &mut InputContext) {
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
        if is_gesture_starter(evt) {
            if let Some(capture_idx) = last_handler_index {
                if !self.stack[capture_idx].is_clean() {
                    self.captured_index = Some(capture_idx);
                }
            }
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
