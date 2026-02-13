pub mod code_block;
pub mod connectivity_badge;
pub mod context_attachment;
pub mod diff_viewer;
pub mod message_bubble;
pub mod model_selector;
pub mod split_pane;
pub mod thinking_indicator;
pub mod toast;
pub mod wallet_card;
pub mod wizard_stepper;

// Re-export key types for convenience.
pub use code_block::render_code_block;
pub use connectivity_badge::{ConnectivityState, render_connectivity_badge};
pub use context_attachment::{AttachedContext, AttachedFile, render_context_attachment};
pub use diff_viewer::{DiffLine, render_diff};
pub use message_bubble::{render_ai_message, render_error_message, render_user_message};
pub use model_selector::{ModelSelected, ModelSelectorView, render_model_badge};
pub use split_pane::{PaneLayout, SplitDirection, TilingState, render_split_pane};
pub use thinking_indicator::{ThinkingPhase, render_thinking_indicator};
pub use toast::{ToastKind, render_toast};
pub use wallet_card::render_wallet_card;
pub use wizard_stepper::render_wizard_stepper;
