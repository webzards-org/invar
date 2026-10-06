//! A Plan's approval, read against its body as it is now — what
//! `plan approve` and `plan check` both decide from (plan-header.md §5.1,
//! §5.3).

use crate::digest;
use crate::format::plan::{Header, Plan, Status};

/// The §4d digest of the Plan's body: what `Approved-checksum` records.
pub fn body_digest(plan: &Plan) -> String {
    digest::digest(&digest::canonicalize(plan.body()))
}

/// What the header says about earlier approvals, against the body as it is
/// now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Prior {
    /// `Status: draft`, whatever is in the approval fields.
    Draft,
    /// Approved, every value present, and the checksum matches the body.
    Current { by: String, at: String },
    /// Approved, but the body changed since: the checksum no longer matches.
    Changed {
        by: String,
        at: String,
        recorded: String,
    },
    /// `Status: approved` with approval values missing: no `plan approve`
    /// wrote it (§5.1, example 9.3).
    Unrecorded { missing: Vec<&'static str> },
}

/// Compare the header's approval with `current`, the digest of the body now.
pub fn classify(header: &Header, current: &str) -> Prior {
    if header.status == Status::Draft {
        return Prior::Draft;
    }
    let fields = [
        ("Approved-by", &header.approved_by),
        ("Approved-at", &header.approved_at),
        ("Approved-checksum", &header.approved_checksum),
    ];
    let missing: Vec<&'static str> = fields
        .iter()
        .filter(|(_, value)| value.is_none())
        .map(|(key, _)| *key)
        .collect();
    let (Some(by), Some(at), Some(recorded)) = (
        &header.approved_by,
        &header.approved_at,
        &header.approved_checksum,
    ) else {
        return Prior::Unrecorded { missing };
    };
    if recorded == current {
        Prior::Current {
            by: by.clone(),
            at: at.clone(),
        }
    } else {
        Prior::Changed {
            by: by.clone(),
            at: at.clone(),
            recorded: recorded.clone(),
        }
    }
}
