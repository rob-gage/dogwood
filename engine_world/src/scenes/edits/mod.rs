// Copyright Rob Gage 2026

mod edit;
mod edit_cellular_tile_pass;
mod edit_error;
mod edit_mode;
mod edit_streaming_state;

pub use edit::Edit;
pub use edit_error::EditError;
pub use edit_mode::EditMode;
pub use edit_streaming_state::EditStreamingState;

/// Maximum cellular tiles held by the edits requested before one stream
const EDIT_CELLULAR_TILE_COUNT_MAXIMUM: usize = 4096;
