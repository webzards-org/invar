//! Acceptance tests: what the core tells the test-writer, and later what it checks
//! of the result (A-1 to A-8).
//!
//! The test-writer's prompt stays one fixed text for every project and style (A-7,
//! §13b). What changes with configuration is the output of these commands.

pub mod lock;
pub mod spec;
// `plan check`, `done` and `invar acceptance verify` arrive later in AT-9; remove this
// with them.
#[allow(dead_code)]
pub mod verify;
