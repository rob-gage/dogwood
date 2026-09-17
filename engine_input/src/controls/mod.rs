//! Input control state and translation abstractions.

mod control_state;
mod input_translator;
mod simple_input_translator;

pub use control_state::ControlState;
pub use input_translator::InputTranslator;
pub use simple_input_translator::SimpleInputTranslator;
