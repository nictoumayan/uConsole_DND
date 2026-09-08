//! vellum — offline D&D Beyond character sheet.
//!
//! Split lib/bin so the derive layer is testable headlessly. Phase 1 can be
//! built and verified entirely on a laptop; the uConsole is not involved until
//! there is a TUI to look at.

pub mod app;
pub mod content;
pub mod ddb;
pub mod device;
pub mod dice;
pub mod derive;
pub mod paths;
pub mod portrait;
pub mod render;
pub mod rules;
pub mod session;
pub mod tabs;
