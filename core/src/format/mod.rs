//! Reading and writing the on-disk formats (§14.3).
//!
//! Six artifacts are the system's public contract, and this module is where
//! the core reads them:
//!
//! - `invar.yaml` (§6)                           — M1-3, [`config`]
//! - the Plan header and required sections (§4a) — M1-6, [`plan`]
//! - decision entries (§7)                       — M1-11, [`decision`]
//! - the `.feature` metadata block (§4)          — the feature-tests milestone
//! - the `invar-lock:` line (§4d)                — AT-5, [`lock`]
//! - JUnit XML test results (§14.2)              — AT-6, [`junit`]
//!
//! The specifications live in `contracts/formats/`; this module implements
//! them and must not drift from them. A change here that changes what a file
//! may contain is a public-contract change, not an implementation detail.
//!
//! **The header/body boundary is the delicate part.** Checksums are computed
//! over the body alone, so a parser that splits one byte differently from
//! another implementation produces a different digest for the same file and
//! every integrity check fails. Splitting happens here; hashing happens in
//! `digest`.
//!
//! Gherkin is parsed only as far as the core needs: the metadata block and the
//! `Scenario:` lines. Steps are the adapter's concern, never the core's
//! (§14.2).

pub mod config;
pub mod decision;
pub mod junit;
pub mod lock;
pub mod plan;
pub mod values;
