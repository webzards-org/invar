//! `invar acceptance lock` (AT-8): turn the test-writer's pending files into locked
//! ones, and only when every test checks something.
//!
//! Implements `contracts/formats/acceptance-lock.md` §9. The contract is the
//! authority; section numbers and `D-n` below point into it.
//!
//! [`find`] says which files belong to the ticket, and [`run`] runs them.

pub mod find;
pub mod run;

#[cfg(test)]
mod tests;
