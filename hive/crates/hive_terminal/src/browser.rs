//! Compatibility browser module for `hive_terminal`.
//!
//! The workspace now standardizes on the Playwright-backed browser automation
//! in `hive_integrations`. This module remains as a thin re-export surface so
//! existing `hive_terminal` imports continue to resolve while all behavior
//! flows through the shared implementation.

pub use hive_integrations::browser::*;
