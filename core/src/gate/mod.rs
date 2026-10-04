//! The gate: the checks that stand between a specification and the code
//! generated against it.
//!
//! Each check is a function over the project, independent of how it is
//! invoked — the CLI, a hook and CI call the same code and get the same answer
//! (§13b). The CLI modules only parse arguments and print.

pub mod validate;
