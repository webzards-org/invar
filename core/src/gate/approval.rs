//! A Plan's approval, read against the Plan as it is now — what
//! `plan approve` and `plan check` both decide from (plan-header.md §5.1,
//! §5.3).

use crate::digest;
use crate::format::plan::{Acceptance, Header, Plan, Status};

/// The approval digest of the Plan as it is now: what `Approved-checksum` must
/// equal for the approval to hold.
pub fn current_digest(plan: &Plan) -> String {
    approval_digest(plan, plan.header().acceptance)
}

/// The approval digest, and the one place it is computed (plan-header.md §5.3,
/// D-19). With an `Acceptance` value, the §4d digest of the line
/// `Acceptance: <value>`, a newline, then the canonical body; so changing,
/// removing or clearing the value stales the approval. Without one, the digest
/// of the body alone, which keeps an approval from before the field existed.
pub fn approval_digest(plan: &Plan, acceptance: Option<Acceptance>) -> String {
    let body = digest::canonicalize(plan.body());
    match acceptance {
        Some(value) => digest::digest(&format!("Acceptance: {}\n{body}", value.as_str())),
        None => digest::digest(&body),
    }
}

/// What a recorded checksum shows about the `Acceptance` value it was computed
/// over, given the body as it is now (plan-header.md D-20).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovedValue {
    /// The checksum matches the value in the header.
    Confirmed(Acceptance),
    /// The checksum matches another value: the body is what was approved, and
    /// the value was this one until the header changed.
    Changed(Acceptance),
    /// The checksum matches the body alone: the approval had no value.
    None,
    /// The checksum matches nothing: the body changed too, so it cannot say.
    Unknown,
}

/// Compare `recorded` with the digest of every value the approval could have had.
pub fn approved_value(plan: &Plan, recorded: &str) -> ApprovedValue {
    let in_header = plan.header().acceptance;
    if let Some(value) = in_header
        && approval_digest(plan, Some(value)) == recorded
    {
        return ApprovedValue::Confirmed(value);
    }
    for value in [Acceptance::Required, Acceptance::Skipped] {
        if in_header != Some(value) && approval_digest(plan, Some(value)) == recorded {
            return ApprovedValue::Changed(value);
        }
    }
    if approval_digest(plan, None) == recorded {
        return ApprovedValue::None;
    }
    ApprovedValue::Unknown
}

/// What the header says about earlier approvals, against the Plan as it is
/// now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Prior {
    /// `Status: draft`, whatever is in the approval fields.
    Draft,
    /// Approved, every value present, and the checksum matches the Plan.
    Current { by: String, at: String },
    /// Approved, but the body or `Acceptance` changed since: the checksum no
    /// longer matches.
    Changed {
        by: String,
        at: String,
        recorded: String,
    },
    /// `Status: approved` with approval values missing: no `plan approve`
    /// wrote it (§5.1, example 9.3).
    Unrecorded { missing: Vec<&'static str> },
}

/// Compare the header's approval with `current`, the approval digest now.
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
