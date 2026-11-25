//! `invar approval verify [--base <ref>]` (#168): whether every approval on a branch
//! was signed by an approver of the base (`contracts/formats/approval-signature.md`
//! §6; section numbers below point into it).
//!
//! `approval.signed` only says what `plan approve` writes, on a machine the agent
//! shares. This is where the answer is read from outside the branch: the switch and
//! the approvers list from the base (§6.1, D-3), every commit of the branch along
//! first parents (§6.2), and each approval's signature checked by `ssh-keygen`
//! directly (D-5). It reads git and writes nothing.
//!
//! **A Plan is followed through the branch**, commit by commit, against the commit's
//! first parent: an approval is checked when it is made (rule 1), a body changed
//! under it must be settled before `HEAD` (rule 2), and a deletion needs a signed
//! approval before it with nothing unsettled in between (rule 3). An `Approval-id`
//! seen in an earlier approval of the branch, or in an approved Plan at a commit any
//! ref reaches outside it, is a replay (rule 6, D-7). At `HEAD`, an
//! approved Plan that no commit of the branch approved is checked as it stands
//! (rule 5).

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fmt::{self, Write as _};
use std::path::{Path, PathBuf};

use crate::cycle::{self, CycleError, short};
use crate::format::config::{self, ConfigError, FILE_NAME};
use crate::format::decision::{self, Type};
use crate::format::plan::{Mode, Plan, Status};
use crate::fs::plans::{SIGNATURE_FILE, SPEC_FILE};
use crate::gate::approval;
use crate::git::{self, GitError};
use crate::signature::{self, Approvers, Verified};

/// The approvers list, read from the base only (§2).
pub const APPROVERS: &str = ".invar/approvers";

const PLANS: &str = ".invar/plans";

const DECISIONS: &str = ".invar/decisions";

/// What `invar approval verify` found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    pub base: String,
    /// What was not used, or applies later, and what does not count: never a failure.
    pub notes: Vec<String>,
    /// Why the check is off, when it is.
    pub off: Option<String>,
    /// Every approval that passed rule 1, in commit order (§6.4).
    pub approvals: Vec<Signed>,
    /// Every problem, in the order found.
    pub problems: Vec<Problem>,
    /// Whether any commit of the branch, or `HEAD`, has a Plan.
    pub any_plan: bool,
}

/// An approval that passed rule 1: who signed it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Signed {
    pub commit: String,
    pub subject: String,
    pub spec: String,
    pub ticket: String,
    pub verified: Verified,
}

/// One problem, naming the commit and the Plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    pub commit: String,
    pub subject: String,
    pub spec: String,
    pub what: String,
}

impl Report {
    pub fn passes(&self) -> bool {
        self.off.is_some() || self.problems.is_empty()
    }

    /// Everything decided on, ending with `approval verify: off: …`, `approval verify:
    /// passed: …` or `approval verify: failed`.
    pub fn render(&self) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "base: {}", self.base);
        for note in &self.notes {
            let _ = writeln!(out, "note: {note}");
        }
        if let Some(off) = &self.off {
            let _ = writeln!(out, "approval verify: off: {off}");
            return out;
        }
        let _ = writeln!(out, "signed approvals:");
        if self.approvals.is_empty() {
            let _ = writeln!(out, "  none");
        }
        for signed in &self.approvals {
            let _ = writeln!(
                out,
                "  {} ({}): {}: {} approved by {} ({})",
                short(&signed.commit),
                signed.subject,
                signed.spec,
                signed.ticket,
                signed.verified.principal,
                signed.verified.fingerprint
            );
        }
        if !self.problems.is_empty() {
            let _ = writeln!(out, "problems:");
            for problem in &self.problems {
                let _ = writeln!(
                    out,
                    "  {} ({}): {}: {}",
                    short(&problem.commit),
                    problem.subject,
                    problem.spec,
                    problem.what
                );
            }
            let _ = writeln!(out, "approval verify: failed");
        } else if !self.any_plan {
            let _ = writeln!(out, "approval verify: passed: nothing to verify");
        } else {
            let _ = writeln!(
                out,
                "approval verify: passed: {} signed approval(s), each by a key in {}:{APPROVERS}",
                self.approvals.len(),
                self.base
            );
        }
        out
    }
}

/// The refusal, after the report has listed every problem.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failed {
    pub problems: usize,
}

impl fmt::Display for Failed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} problem(s) with the approvals of this branch, listed above. An approval counts \
             only when it is signed by a key in the base's {APPROVERS}: the developer approves \
             with `invar plan approve`, with `approval.signed` on (docs/INSTALL.md, \"Signed \
             approvals\")",
            self.problems
        )
    }
}

impl std::error::Error for Failed {}

/// Run the check in `dir`, the project root, against `base`, or the remote's default
/// branch, or `main` (`invar-yaml.md` §3.3.1).
pub fn check(dir: &Path, base: Option<&str>) -> Result<Report, VerifyError> {
    let given = base;
    let (base, base_commit) = cycle::resolve_base(dir, given)?;
    let mut report = Report {
        base: base.clone(),
        notes: cycle::base_note(dir, given, &base).into_iter().collect(),
        off: None,
        approvals: Vec::new(),
        problems: Vec::new(),
        any_plan: false,
    };

    // §6.1: whether the check is on is the base's to say, never the branch's.
    let shown = format!("{base}:{FILE_NAME}");
    let base_config = match git::file_at(dir, &base_commit, FILE_NAME)? {
        Some(bytes) => Some(
            config::parse_bytes(&bytes, Path::new(&shown))
                .map_err(|source| VerifyError::BaseConfig { source })?
                .config,
        ),
        None => None,
    };
    let Some(base_config) = base_config.filter(|config| config.approval.signed) else {
        report.off = Some(format!(
            "{shown} does not set `approval.signed: true`, so approvals are not signed, and \
             none is verified"
        ));
        return Ok(report);
    };

    if git::is_shallow(dir)? {
        return Err(VerifyError::Shallow { base });
    }
    let fork = cycle::merge_base(dir, &base)?;
    let head = git::commit_of(dir, "HEAD")?;
    let parents = git::parents(dir, &head)?;
    if parents.len() > 1 && parents[0] == base_commit {
        return Err(VerifyError::MergeCommit { base });
    }
    let Some(list) = git::file_at(dir, &base_commit, APPROVERS)? else {
        return Err(VerifyError::NoApprovers { base });
    };
    let approvers =
        Approvers::write(&list).map_err(|(path, source)| VerifyError::TempFile { path, source })?;

    if !signed_at(dir, "HEAD") {
        report.notes.push(format!(
            "this branch turns `approval.signed` off in {FILE_NAME}; {base} has it on, so the \
             branch's approvals are verified all the same (invar-yaml.md §3.7)"
        ));
    }

    let outside = outside_ids(dir, &fork)?;
    let cancelled = cancelled_ids(dir, &base_commit)?;
    let mut walk = Walk {
        dir,
        base: &base,
        fork: &fork,
        outside,
        cancelled,
        mode: Mode::of(&base_config),
        approvers: &approvers,
        list_shown: format!("{base}:{APPROVERS}"),
        tracks: BTreeMap::new(),
        ids: HashMap::new(),
        checked: HashSet::new(),
        subjects: HashMap::new(),
        report: &mut report,
    };
    let mut approvers_changed = false;
    let mut previous: Option<(String, BTreeMap<String, State>)> = None;
    for commit in git::first_parent_commits(dir, &fork)? {
        let parent = git::first_parent(dir, &commit)?;
        let before = match (&parent, previous.take()) {
            (Some(parent), Some((was, states))) if *parent == was => states,
            (Some(parent), _) => walk.states(parent)?,
            (None, _) => BTreeMap::new(),
        };
        let after = walk.states(&commit)?;
        let specs: BTreeSet<&String> = before.keys().chain(after.keys()).collect();
        for spec in specs {
            let absent = State::Absent;
            let was = before.get(spec).unwrap_or(&absent);
            let now = after.get(spec).unwrap_or(&absent);
            walk.step(&commit, spec, was, now)?;
        }
        if let Some(parent) = &parent
            && git::file_at(dir, parent, APPROVERS)? != git::file_at(dir, &commit, APPROVERS)?
        {
            approvers_changed = true;
        }
        previous = Some((commit, after));
    }
    walk.finish(&head)?;
    let any_plan = walk.report.any_plan;

    if approvers_changed {
        report.notes.push(format!(
            "this branch changes {APPROVERS}; the approvals were verified against \
             {base}:{APPROVERS}, and the change applies once it is on {base}"
        ));
    }
    if !any_plan {
        report.notes.push(
            "this branch has no Plan at any commit, so there is nothing to verify".to_owned(),
        );
    }
    for signed in &report.approvals {
        let by = approved_by(dir, &signed.commit, &signed.spec);
        if let Some(by) = by
            && by != signed.verified.principal
        {
            report.notes.push(format!(
                "{} ({}): {}: `Approved-by` says `{by}`, and the signature is by {}: the \
                 signature is the record of who approved, the header is text \
                 (approval-signature.md §6.4)",
                short(&signed.commit),
                signed.subject,
                signed.spec,
                signed.verified.principal
            ));
        }
    }
    Ok(report)
}

/// Whether `invar.yaml` at `rev` sets `approval.signed: true`. A file that is not
/// there, or not valid, does not.
fn signed_at(dir: &Path, rev: &str) -> bool {
    git::file_at(dir, rev, FILE_NAME)
        .ok()
        .flatten()
        .and_then(|bytes| config::parse_bytes(&bytes, Path::new(FILE_NAME)).ok())
        .is_some_and(|loaded| loaded.config.approval.signed)
}

/// `Approved-by` of the Plan at `spec`, at `commit`.
fn approved_by(dir: &Path, commit: &str, spec: &str) -> Option<String> {
    let bytes = git::file_at(dir, commit, spec).ok()??;
    let text = String::from_utf8(bytes).ok()?;
    Plan::check(&text, Mode::Light)
        .approval_values
        .into_iter()
        .find(|(key, _)| *key == "Approved-by")
        .map(|(_, value)| value)
}

// --- a Plan at one commit -------------------------------------------------------

/// A Plan at one commit, as §6.2 compares it.
#[derive(Debug, Clone, PartialEq, Eq)]
enum State {
    Absent,
    Draft,
    Approved(Snapshot),
}

/// What an approved Plan holds that rule 1 checks. A Plan whose `Status` is not
/// `draft` and that is not a valid Plan is approved as far as the check goes, and
/// fails rule 1 for that reason: a check that cannot read the approval does not pass
/// it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Snapshot {
    ticket: String,
    checksum: Option<String>,
    /// The approval digest of the Plan as it is (`plan-header.md` §5.3).
    digest: Option<String>,
    id: Option<String>,
    signature: Option<Vec<u8>>,
    /// Why it cannot be read, when it cannot.
    invalid: Option<String>,
}

/// One Plan through the branch.
#[derive(Debug, Default)]
struct Track {
    /// Whether the last approval of the branch passed rule 1, once there is one.
    last_approval: Option<bool>,
    /// The commits that changed the body while it was approved, not yet settled by a
    /// signed approval, a reset or a deletion (rule 2).
    unsettled: Vec<String>,
}

struct Walk<'a> {
    dir: &'a Path,
    base: &'a str,
    fork: &'a str,
    /// Every `Approval-id` written at a commit a ref reaches outside the branch, with
    /// where: candidates for rule 6, each checked when an approval has its id.
    outside: HashMap<String, Vec<(String, String)>>,
    /// Every `Approval-id` a `failed` entry at the base cancelled, with the entry.
    cancelled: HashMap<String, String>,
    mode: Mode,
    approvers: &'a Approvers,
    list_shown: String,
    tracks: BTreeMap<String, Track>,
    /// Every `Approval-id` an approval of the branch had, with its commit (D-7).
    ids: HashMap<String, String>,
    /// Every approval rule 1 was applied to, so that `HEAD` does not report it twice.
    checked: HashSet<Snapshot>,
    subjects: HashMap<String, String>,
    report: &'a mut Report,
}

impl Walk<'_> {
    /// Every Plan at `rev`, by the path of its `spec.md`.
    fn states(&mut self, rev: &str) -> Result<BTreeMap<String, State>, VerifyError> {
        let mut states = BTreeMap::new();
        for path in git::files_at(self.dir, rev, PLANS)? {
            let is_spec = path
                .strip_prefix(&format!("{PLANS}/"))
                .and_then(|rest| rest.strip_suffix(&format!("/{SPEC_FILE}")))
                .is_some_and(|name| !name.is_empty() && !name.contains('/'));
            if is_spec {
                let state = self.state(rev, &path)?;
                self.report.any_plan = true;
                states.insert(path, state);
            }
        }
        Ok(states)
    }

    fn state(&self, rev: &str, spec: &str) -> Result<State, VerifyError> {
        let Some(bytes) = git::file_at(self.dir, rev, spec)? else {
            return Ok(State::Absent);
        };
        let signature = git::file_at(self.dir, rev, &signature_of(spec))?;
        let Ok(text) = String::from_utf8(bytes) else {
            return Ok(State::Approved(Snapshot {
                ticket: String::new(),
                checksum: None,
                digest: None,
                id: None,
                signature,
                invalid: Some("is not valid UTF-8, so its approval cannot be read".to_owned()),
            }));
        };
        match Plan::parse(&text, self.mode) {
            Ok(plan) if plan.header().status == Status::Draft => Ok(State::Draft),
            Ok(plan) => {
                let header = plan.header();
                Ok(State::Approved(Snapshot {
                    ticket: header.ticket.clone(),
                    checksum: header.approved_checksum.clone(),
                    digest: Some(approval::current_digest(&plan)),
                    id: header.approval_id.clone(),
                    signature,
                    invalid: None,
                }))
            }
            Err(problems) => {
                let checked = Plan::check(&text, self.mode);
                if checked.status == Some(Status::Draft) {
                    return Ok(State::Draft);
                }
                let value = |wanted: &str| {
                    checked
                        .approval_values
                        .iter()
                        .find(|(key, _)| *key == wanted)
                        .map(|(_, value)| value.clone())
                };
                Ok(State::Approved(Snapshot {
                    ticket: checked.ticket.map(|(ticket, _)| ticket).unwrap_or_default(),
                    checksum: value("Approved-checksum"),
                    digest: None,
                    id: value("Approval-id"),
                    signature,
                    invalid: Some(format!(
                        "is not a valid Plan ({} problem(s); `invar plan validate` lists them), \
                         so its approval cannot be checked",
                        problems.len()
                    )),
                }))
            }
        }
    }

    /// §6.2 and §6.3 for one Plan at one commit.
    fn step(
        &mut self,
        commit: &str,
        spec: &str,
        was: &State,
        now: &State,
    ) -> Result<(), VerifyError> {
        match (was, now) {
            (_, State::Approved(snapshot)) => {
                if let State::Approved(before) = was
                    && before.checksum == snapshot.checksum
                    && before.signature == snapshot.signature
                {
                    // Not a new approval. Another digest under the same one is a body
                    // change while approved.
                    if before.digest != snapshot.digest {
                        self.track(spec).unsettled.push(commit.to_owned());
                    }
                    return Ok(());
                }
                self.approval(commit, spec, snapshot)
            }
            (State::Approved(_), State::Draft) => {
                // A reset needs no signature (rule 4), and settles what came before.
                self.track(spec).unsettled.clear();
                Ok(())
            }
            (State::Absent, State::Absent) | (_, State::Draft) => Ok(()),
            (_, State::Absent) => self.deletion(commit, spec),
        }
    }

    /// Rule 1, and the replay rule, for an approval at `commit`.
    fn approval(
        &mut self,
        commit: &str,
        spec: &str,
        snapshot: &Snapshot,
    ) -> Result<(), VerifyError> {
        let mut result = self.rule_1(snapshot)?;
        self.checked.insert(snapshot.clone());
        if let Some(id) = &snapshot.id {
            match self.ids.get(id) {
                Some(first) => {
                    let replayed = format!(
                        "this approval is replayed: its `Approval-id` {id} is the one of the \
                         approval at {}, and every approval has a new one \
                         (approval-signature.md D-7). Approve the Plan again",
                        short(first)
                    );
                    let mut problems = result.err().unwrap_or_default();
                    problems.push(replayed);
                    result = Err(problems);
                }
                None => {
                    self.ids.insert(id.clone(), commit.to_owned());
                    let replayed = match self.replayed_from_outside(id)? {
                        Some(replayed) => Some(replayed),
                        None => self.cancelled.get(id).map(|entry| {
                            format!(
                                "this approval is replayed: its `Approval-id` {id} is the one \
                                 the `failed` entry {entry} on {} cancelled \
                                 (approval-signature.md §6.3, rule 6). Approve the Plan again",
                                self.base
                            )
                        }),
                    };
                    if let Some(replayed) = replayed {
                        let mut problems = result.err().unwrap_or_default();
                        problems.push(replayed);
                        result = Err(problems);
                    }
                }
            }
        }
        match result {
            Ok(verified) => {
                let subject = self.subject(commit)?;
                self.report.approvals.push(Signed {
                    commit: commit.to_owned(),
                    subject,
                    spec: spec.to_owned(),
                    ticket: snapshot.ticket.clone(),
                    verified,
                });
                let track = self.track(spec);
                track.last_approval = Some(true);
                track.unsettled.clear();
            }
            Err(problems) => {
                self.track(spec).last_approval = Some(false);
                for what in problems {
                    self.problem(commit, spec, what)?;
                }
            }
        }
        Ok(())
    }

    /// Rule 6 outside the branch: the first commit a ref reaches, not on the branch,
    /// whose Plan was approved with `id`, said with a ref that holds it.
    fn replayed_from_outside(&mut self, id: &str) -> Result<Option<String>, VerifyError> {
        let Some(places) = self.outside.get(id).cloned() else {
            return Ok(None);
        };
        for (commit, spec) in places {
            if !self.approved_with(&commit, &spec, id)? {
                continue;
            }
            // On the base's history, the base is the ref to name: a ref that holds it may
            // be this very branch.
            let on = if git::is_ancestor(self.dir, &commit, self.fork)? {
                Some(self.base.to_owned())
            } else {
                git::ref_containing(self.dir, &commit)?
            };
            let on = on.map_or_else(String::new, |name| format!(", on {name}"));
            let subject = self.subject(&commit)?;
            return Ok(Some(format!(
                "this approval is replayed: its `Approval-id` {id} is the one of the approved \
                 Plan {spec} at {} ({subject}){on}, outside this branch, and every approval \
                 has a new one (approval-signature.md §6.3, rule 6). Approve the Plan again",
                short(&commit)
            )));
        }
        Ok(None)
    }

    /// Whether the Plan at `spec` is approved with `id` at `commit`.
    fn approved_with(&self, commit: &str, spec: &str, id: &str) -> Result<bool, VerifyError> {
        let Some(bytes) = git::file_at(self.dir, commit, spec)? else {
            return Ok(false);
        };
        let checked = Plan::check(&String::from_utf8_lossy(&bytes), self.mode);
        let has = |key: &str, value: &str| {
            checked
                .approval_values
                .iter()
                .any(|(k, v)| *k == key && v == value)
        };
        Ok(checked.status == Some(Status::Approved) && has("Approval-id", id))
    }

    /// Rule 1: what the approval holds, and its signature against the base's list.
    /// Every reason it fails, or who signed it.
    fn rule_1(&self, snapshot: &Snapshot) -> Result<Result<Verified, Vec<String>>, VerifyError> {
        if let Some(invalid) = &snapshot.invalid {
            return Ok(Err(vec![invalid.clone()]));
        }
        let mut problems = Vec::new();
        if snapshot.signature.is_none() {
            problems.push(format!(
                "is approved, and has no {SIGNATURE_FILE}: the approval is not signed. The \
                 developer approves it with `invar plan approve`, with `approval.signed` on"
            ));
        }
        match (&snapshot.checksum, &snapshot.digest) {
            (None, _) => problems.push("is approved, and has no `Approved-checksum`".to_owned()),
            (Some(checksum), Some(digest)) if checksum != digest => problems.push(format!(
                "`Approved-checksum` does not match the Plan at this commit (approved \
                 {checksum}, the Plan is {digest})"
            )),
            _ => {}
        }
        if snapshot.id.is_none() {
            problems.push(
                "is approved, and has no `Approval-id`, which every signed approval names \
                 (approval-signature.md §3): approve it again with this version of invar"
                    .to_owned(),
            );
        }
        if let (Some(signature), Some(checksum), Some(id)) =
            (&snapshot.signature, &snapshot.checksum, &snapshot.id)
        {
            let statement = signature::statement(&snapshot.ticket, checksum, id);
            match self
                .approvers
                .verify(&statement, signature)
                .map_err(VerifyError::SshKeygen)?
            {
                Ok(verified) if problems.is_empty() => return Ok(Ok(verified)),
                Ok(_) => {}
                Err(refused) => problems.push(format!("{refused} ({})", self.list_shown)),
            }
        }
        Ok(Err(problems))
    }

    /// Rule 3, for a Plan gone at `commit`.
    fn deletion(&mut self, commit: &str, spec: &str) -> Result<(), VerifyError> {
        let track = self.tracks.remove(spec).unwrap_or_default();
        if track.last_approval != Some(true) {
            self.problem(
                commit,
                spec,
                "deleted with no signed approval before it on this branch: a Plan is closed \
                 only after an approval by a key in the approvers list"
                    .to_owned(),
            )?;
        }
        if let Some(changed) = track.unsettled.first() {
            self.problem(
                commit,
                spec,
                format!(
                    "deleted after its body changed at {} while it was approved, with no signed \
                     approval since: what was closed is not what was signed",
                    short(changed)
                ),
            )?;
        }
        Ok(())
    }

    /// Rule 2 for what is left at `HEAD`, and rule 5.
    fn finish(&mut self, head: &str) -> Result<(), VerifyError> {
        let tracks = std::mem::take(&mut self.tracks);
        for (spec, track) in &tracks {
            for changed in &track.unsettled {
                self.problem(
                    changed,
                    spec,
                    "the body changed while the Plan was approved, and no signed approval, \
                     reset or deletion follows up to HEAD: the Plan is not what was signed"
                        .to_owned(),
                )?;
            }
        }
        for (spec, state) in self.states(head)? {
            let State::Approved(snapshot) = state else {
                continue;
            };
            let settled = tracks
                .get(&spec)
                .is_none_or(|track| track.unsettled.is_empty());
            if !settled || self.checked.contains(&snapshot) {
                continue;
            }
            if let Err(problems) = self.rule_1(&snapshot)? {
                for what in problems {
                    self.problem(head, &spec, format!("at HEAD: {what}"))?;
                }
            }
        }
        self.tracks = tracks;
        Ok(())
    }

    fn track(&mut self, spec: &str) -> &mut Track {
        self.tracks.entry(spec.to_owned()).or_default()
    }

    fn problem(&mut self, commit: &str, spec: &str, what: String) -> Result<(), VerifyError> {
        let subject = self.subject(commit)?;
        self.report.problems.push(Problem {
            commit: commit.to_owned(),
            subject,
            spec: spec.to_owned(),
            what,
        });
        Ok(())
    }

    fn subject(&mut self, commit: &str) -> Result<String, VerifyError> {
        if let Some(subject) = self.subjects.get(commit) {
            return Ok(subject.clone());
        }
        let subject = git::subject(self.dir, commit)?;
        self.subjects.insert(commit.to_owned(), subject.clone());
        Ok(subject)
    }
}

/// Every `Approval-id` a commit that a ref reaches, and that is not in `fork..HEAD`,
/// wrote into a Plan: one search of every ref (§6.3, rule 6). The base's history is
/// in it, and every other branch; commits no ref reaches are not.
fn outside_ids(
    dir: &Path,
    fork: &str,
) -> Result<HashMap<String, Vec<(String, String)>>, VerifyError> {
    let branch: HashSet<String> = git::range_commits(dir, fork)?.into_iter().collect();
    let mut outside: HashMap<String, Vec<(String, String)>> = HashMap::new();
    let pathspec = format!("{PLANS}/*/{SPEC_FILE}");
    for (commit, spec, value) in git::added_lines_on_any_ref(dir, &pathspec, "Approval-id:")? {
        let id = value.trim();
        if id.is_empty() || branch.contains(&commit) {
            continue;
        }
        outside
            .entry(id.to_owned())
            .or_default()
            .push((commit, spec));
    }
    Ok(outside)
}

/// Every `Approval-id` a `failed` entry under `.invar/decisions/` at the base
/// cancelled, with the entry's path (§6.3, rule 6; `decision-entry.md` §7). One `git
/// grep` finds the entries that name one; only those are read.
fn cancelled_ids(dir: &Path, base_commit: &str) -> Result<HashMap<String, String>, VerifyError> {
    let mut cancelled = HashMap::new();
    for path in git::files_matching_at(dir, base_commit, "^approval-id: ", DECISIONS)? {
        let Some(bytes) = git::file_at(dir, base_commit, &path)? else {
            continue;
        };
        if let Ok(entry) = decision::parse(&String::from_utf8_lossy(&bytes))
            && entry.kind == Type::Failed
            && let Some(id) = entry.approval_id
        {
            cancelled.entry(id).or_insert(path);
        }
    }
    Ok(cancelled)
}

/// `approval.sig`, beside `spec`.
fn signature_of(spec: &str) -> String {
    match spec.rsplit_once('/') {
        Some((dir, _)) => format!("{dir}/{SIGNATURE_FILE}"),
        None => SIGNATURE_FILE.to_owned(),
    }
}

// --- errors -----------------------------------------------------------------------

/// Why the check could not run.
#[derive(Debug)]
pub enum VerifyError {
    Cycle(CycleError),
    Git(GitError),
    /// The base's `invar.yaml` is not a valid config.
    BaseConfig {
        source: ConfigError,
    },
    /// A shallow checkout (§6.1).
    Shallow {
        base: String,
    },
    /// `HEAD` is a merge commit onto the base, such as the one CI checks out for a pull
    /// request: its first parent is the base, and the branch's commits are not on its
    /// first-parent line (§6.1).
    MergeCommit {
        base: String,
    },
    /// The base has no approvers list (§6.1).
    NoApprovers {
        base: String,
    },
    TempFile {
        path: PathBuf,
        source: std::io::Error,
    },
    /// `ssh-keygen` could not be run.
    SshKeygen(std::io::Error),
}

impl From<CycleError> for VerifyError {
    fn from(err: CycleError) -> Self {
        Self::Cycle(err)
    }
}

impl From<GitError> for VerifyError {
    fn from(err: GitError) -> Self {
        Self::Git(err)
    }
}

impl fmt::Display for VerifyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cycle(err) => err.fmt(f),
            Self::Git(err) => err.fmt(f),
            Self::BaseConfig { source } => {
                write!(
                    f,
                    "cannot read `approval.signed` from the base branch: {source}"
                )
            }
            Self::Shallow { base } => write!(
                f,
                "this is a shallow clone, and approval verify reads every commit of the branch \
                 since it left `{base}`. {}",
                git::shallow_advice(base)
            ),
            Self::MergeCommit { base } => write!(
                f,
                "HEAD is a merge commit whose first parent is `{base}`, such as the one CI checks \
                 out for a pull request, so the branch's own commits cannot be followed. Check \
                 out the branch itself: in GitHub Actions, `ref: ${{{{ \
                 github.event.pull_request.head.sha }}}}` on actions/checkout"
            ),
            Self::NoApprovers { base } => write!(
                f,
                "{base} has no {APPROVERS}, and `approval.signed` is on there, so no approval \
                 can be verified. Add the list on {base} first, through review, in git's \
                 allowed-signers format (docs/INSTALL.md, \"Signed approvals\")"
            ),
            Self::TempFile { path, source } => write!(
                f,
                "cannot write {} for ssh-keygen: {source}",
                path.display()
            ),
            Self::SshKeygen(source) => write!(
                f,
                "cannot run `ssh-keygen` to verify the approvals: {source}. Install OpenSSH's \
                 ssh-keygen"
            ),
        }
    }
}

impl std::error::Error for VerifyError {}
