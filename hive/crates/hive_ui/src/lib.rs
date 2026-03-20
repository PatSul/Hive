#![recursion_limit = "4096"]

pub mod chat_input;
pub mod chat_service;
mod command_palette;
pub mod learning_bridge_impl;
mod shell_header;
pub mod statusbar;
pub mod titlebar;

// Re-export foundation types (backward compatibility for hive_app)
pub use hive_ui_core as core_types;
pub use hive_ui_core::actions::*;
pub use hive_ui_core::globals::*;
pub use hive_ui_core::{HiveTheme, Panel, Sidebar, WelcomeScreen};
pub use hive_ui_core::{globals, sidebar, theme, welcome};

// Re-export panels and components (backward compatibility for hive_app)
pub use hive_ui_panels::{components, panels};

// Re-export workspace types
pub use chat_service::{ChatMessage, ChatService, MessageRole};
pub use workspace::HiveWorkspace;

pub mod workspace;
