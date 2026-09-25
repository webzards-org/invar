//! Path resolution and project-tree discovery.
//!
//! Everything the core reads or writes is addressed relative to the **project
//! root**: `specs/`, `.invar/plans/`, `.invar/decisions/`, `invar.yaml`, and
//! the feature paths recorded in test files (§4b). One module owns that
//! resolution so no command invents its own.
//!
//! Rules worth stating before there is code:
//!
//! - **The project root is where `invar.yaml` is**, found by walking upward
//!   from the current directory. A command run from a subdirectory must behave
//!   exactly as one run from the root.
//! - **Feature paths are written relative to the project root** (§4b), which is
//!   what keeps test discovery a plain string match instead of an emulation of
//!   pytest-bdd's own path resolution.
//! - **Ambiguity is a hard error.** Plan lookup (§4a) and test-file lookup
//!   (§4b) both fail loudly on zero or on several matches. `invar` never
//!   guesses which artifact was meant.
//!
//! Arrives with M1-5 (Plan lookup) and M1-13 (`init`).
