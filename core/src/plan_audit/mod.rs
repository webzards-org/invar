//! The Plan audit (#167, `contracts/formats/plan-audit.md`; section numbers below point
//! into it): an agent reads a draft Plan against the code, corrects facts in two
//! sections, and reports the rest. The core keeps the baseline, prints what the agent
//! reads, and computes the report.
//!
//! **Advisory** (D-4). Nothing here refuses an approval or a `done`; `plan approve` only
//! shows a line from the report.
//!
//! **The baseline is the developer's** (D-3). `begin` writes it, refuses a second time
//! without `--restart`, and is on `HUMAN_ONLY`: a baseline taken after a forbidden edit
//! would hide that edit from the report.

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::context::invariants::{self, Invariants};
use crate::context::{self, ContextError, Kind};
use crate::format::audit::BLOCKING_KINDS;
use crate::format::plan::{Mode, Plan, Status};
use crate::fs::ProjectRoot;
use crate::gate::validate::{self, Report as Validation};

/// The files of one ticket's audit (§2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Files {
    pub dir: PathBuf,
    pub baseline: PathBuf,
    pub findings: PathBuf,
    pub report: PathBuf,
}

impl Files {
    /// `.invar/out/<TICKET>-audit/`.
    pub fn of(root: &ProjectRoot, ticket: &str) -> Self {
        let dir = root.out_dir().join(format!("{ticket}-audit"));
        Self {
            baseline: dir.join("baseline.md"),
            findings: dir.join("findings.md"),
            report: dir.join("report.txt"),
            dir,
        }
    }
}

/// A Plan read for the audit: its text, parsed, and where it is.
#[derive(Debug, Clone)]
pub struct Target {
    pub text: String,
    pub plan: Plan,
}

impl Target {
    pub fn ticket(&self) -> &str {
        &self.plan.header().ticket
    }
}

/// Read `spec`, which must pass `plan validate` and be a draft (§5, step 1).
pub fn draft(root: &ProjectRoot, spec: &Path, mode: Mode) -> Result<Target, AuditError> {
    let report = validate::file(root, spec, mode);
    if !report.is_valid() {
        return Err(AuditError::Invalid(report));
    }
    let text = read(spec)?;
    let plan = Plan::parse(&text, mode).map_err(|problems| {
        AuditError::Invalid(Validation {
            spec: spec.to_owned(),
            status: None,
            problems,
        })
    })?;
    if plan.header().status != Status::Draft {
        return Err(AuditError::NotDraft {
            spec: spec.to_owned(),
        });
    }
    Ok(Target { text, plan })
}

/// `invar plan audit begin` (§5): write the baseline, and remove an earlier audit's
/// findings and report. Refused when a baseline exists, unless `restart`.
pub fn begin(
    root: &ProjectRoot,
    spec: &Path,
    mode: Mode,
    restart: bool,
) -> Result<(Target, Files), AuditError> {
    let target = draft(root, spec, mode)?;
    let files = Files::of(root, target.ticket());
    if files.baseline.exists() && !restart {
        return Err(AuditError::BaselineExists {
            baseline: files.baseline,
        });
    }
    let io = |path: &Path| {
        let path = path.to_owned();
        move |source| AuditError::Io { path, source }
    };
    fs::create_dir_all(&files.dir).map_err(io(&files.dir))?;
    for old in [&files.findings, &files.report] {
        match fs::remove_file(old) {
            Ok(()) => {}
            Err(err) if err.kind() == io::ErrorKind::NotFound => {}
            Err(source) => {
                return Err(AuditError::Io {
                    path: old.clone(),
                    source,
                });
            }
        }
    }
    fs::write(&files.baseline, &target.text).map_err(io(&files.baseline))?;
    Ok((target, files))
}

/// `invar plan audit input` (§5.1): what the auditor reads, the same bytes for the same
/// files. Refused without a baseline. `limit` is `context.binding_limit`.
pub fn input(
    root: &ProjectRoot,
    spec: &Path,
    mode: Mode,
    limit: usize,
) -> Result<String, AuditError> {
    let text = read(spec)?;
    let ticket = Plan::check(&text, mode)
        .ticket
        .map(|(ticket, _)| ticket)
        .ok_or_else(|| AuditError::NoTicket {
            spec: spec.to_owned(),
        })?;
    let files = Files::of(root, &ticket);
    if !files.baseline.exists() {
        return Err(AuditError::NoBaseline {
            ticket,
            baseline: files.baseline,
        });
    }
    let shown = |path: &Path| shown(root, path);
    let binding = context::load_binding(root)?.render_as_within(Kind::Binding, limit)?;
    let invariants = invariants::load(root)?;

    let mut out = String::new();
    out.push_str(&format!("=== Plan audit input: {ticket} ===\n\n"));
    out.push_str(&format!("=== The Plan, {} ===\n\n", shown(spec)));
    out.push_str(&text);
    if !text.ends_with('\n') {
        out.push('\n');
    }
    out.push_str("\n=== Binding context ===\n\n");
    if binding.is_empty() {
        out.push_str("(no context file is loaded)\n");
    } else {
        out.push_str(&binding);
        if !binding.ends_with('\n') {
            out.push('\n');
        }
    }
    if let Invariants::Ids(ids) = &invariants {
        out.push_str("\n=== Invariant ids in BUSINESS_CONTEXT.md ===\n\n");
        for invariant in ids {
            out.push_str(&format!("{}: {}\n", invariant.id(), invariant.text));
        }
    }
    out.push_str(&format!(
        "\n=== What you may change ===\n\n{}\n",
        MAY_CHANGE.trim_end()
    ));
    out.push_str(&format!(
        "\n=== {} ===\n\n{}\n",
        shown(&files.findings),
        FINDINGS_FORM.trim_end()
    ));
    out.push_str("\n=== Blocking kinds ===\n\n");
    for (kind, meaning) in BLOCKING_KINDS {
        out.push_str(&format!("{kind}: {meaning}\n"));
    }
    out.push_str("\nAny other kind is a note; `blocking` on it is an error.\n");
    Ok(out)
}

/// §5.2, as the auditor reads it.
const MAY_CHANGE: &str = "\
This Plan is a draft. You may edit two sections of it, and in them only facts with one
right answer in the code, each from a line of the repository you have read:

- `## Implementation context`: a class, method, route, column, config key or mechanism
  named differently in the code; a wrong `INV-<n>`.
- `## Scope`: a path that does not exist or is misspelled.

Never edit `## Ticket`, `## Intent`, `## Open questions / answers`, `## Rejected
alternatives`, `## Acceptance criteria`, what `## Scope` covers (adding or removing
anything from it), any other section, or the header between the `---` lines. Report
everything else as a finding. `invar plan audit report` refuses an audit that changed
anything else.
";

/// §3, as the auditor reads it.
const FINDINGS_FORM: &str = "\
One entry per line; blank lines and `#` comments are ignored:

  edit: <section> | <evidence> | <reason>
  finding: <kind> | <severity> | <section> | <evidence> | <text>

- One `edit` for each change you made, in the order of the changes in its section, top
  to bottom. <section> is `Implementation context` or `Scope`.
- <section> of a finding: Ticket, Intent, Scope, Open questions / answers, Rejected
  alternatives, Acceptance criteria, or Implementation context.
- <evidence>: `<path>:<line>` or `<path>:<line>-<line>`, a file of the repository
  relative to the project root, or `ticket` for a sentence of `## Ticket`.
- <kind>: lowercase words joined by `-`. <severity>: `blocking` or `note`; `blocking`
  only for a kind below.
- <reason> and <text> are the rest of the line, not empty.
";

/// `path` relative to the project root, with `/`: the same text in every clone.
fn shown(root: &ProjectRoot, path: &Path) -> String {
    match root.relative_path(path) {
        Ok(Some(relative)) => relative
            .components()
            .map(|part| part.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/"),
        _ => path.display().to_string(),
    }
}

fn read(path: &Path) -> Result<String, AuditError> {
    fs::read_to_string(path).map_err(|source| AuditError::Io {
        path: path.to_owned(),
        source,
    })
}

/// Why an audit command refused.
#[derive(Debug)]
pub enum AuditError {
    /// `plan validate` does not pass; the report says why.
    Invalid(Validation),
    /// Not a draft: an approved Plan is frozen.
    NotDraft {
        spec: PathBuf,
    },
    /// `begin` again without `--restart`.
    BaselineExists {
        baseline: PathBuf,
    },
    /// `input` or `report` before `begin`.
    NoBaseline {
        ticket: String,
        baseline: PathBuf,
    },
    /// The Plan's header has no readable `Ticket`.
    NoTicket {
        spec: PathBuf,
    },
    Context(ContextError),
    Io {
        path: PathBuf,
        source: io::Error,
    },
}

impl From<ContextError> for AuditError {
    fn from(err: ContextError) -> Self {
        Self::Context(err)
    }
}

impl fmt::Display for AuditError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(report) => write!(
                f,
                "{} is not a valid Plan ({} problem(s), listed above), so it is not audited: fix \
                 it first, `invar plan validate` checks it",
                report.spec.display(),
                report.problems.len()
            ),
            Self::NotDraft { spec } => write!(
                f,
                "{} is approved, and an audit is of a draft: an approved Plan is frozen. To \
                 change it, see docs/USAGE.md, \"Changing an approved Plan\"",
                spec.display()
            ),
            Self::BaselineExists { baseline } => write!(
                f,
                "an audit of this Plan has begun already ({}); begin again with `--restart` to \
                 take a new baseline, which discards the findings and report of the earlier one",
                baseline.display()
            ),
            Self::NoBaseline { ticket, baseline } => write!(
                f,
                "no audit of {ticket} has begun ({} does not exist): the developer begins it \
                 with /invar:plan-audit {ticket}",
                baseline.display()
            ),
            Self::NoTicket { spec } => {
                write!(
                    f,
                    "{} has no readable `Ticket` in its header",
                    spec.display()
                )
            }
            Self::Context(err) => err.fmt(f),
            Self::Io { path, source } => write!(f, "cannot use {}: {source}", path.display()),
        }
    }
}

impl std::error::Error for AuditError {}
