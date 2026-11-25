//! End-to-end tests for `invar approval verify` (`approval-signature.md` §6): the real
//! binary, real `git` repositories, real keys made by `ssh-keygen` in each test, and
//! approvals made by the real `invar plan approve` wherever a person would make them.
//! A forgery is made the way an agent with a shell could make it. Nothing reaches the
//! network or the developer's own agent, keys or git config.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command as Process, Stdio};

use assert_cmd::Command;
use predicates::str::contains;
use tempfile::TempDir;

const DIR: &str = ".invar/plans/SHOP-214-cancel-unpaid-order";
const SPEC: &str = ".invar/plans/SHOP-214-cancel-unpaid-order/spec.md";
const SIGNATURE: &str = ".invar/plans/SHOP-214-cancel-unpaid-order/approval.sig";
const SIGNED: &str = "approval:\n  signed: true\n";
const BRANCH: &str = "feat/SHOP-214-cancel";

fn draft() -> String {
    "---\nTicket: SHOP-214\nModule: orders\nStatus: draft\nApproved-by:\n\
     Approved-at:\nApproved-checksum:\n---\n\n\
     ## Ticket\nSHOP-214 — Cancel an unpaid order.\n\n\
     ## Intent\nCustomers cancel their own unpaid orders.\n\n\
     ## Scope\n`orders` only.\n\n\
     ## Open questions / answers\nNone were needed.\n\n\
     ## Rejected alternatives\nNone considered.\n\n\
     ## Acceptance criteria\n\n\
     - AC1: Given an unpaid order, when its customer cancels it, then it is cancelled.\n\n\
     ## Implementation context\n- AI_CONTEXT: controllers stay thin\n"
        .to_owned()
}

/// A repository whose `main` has `config` as `invar.yaml` and, when given, an
/// approvers list, with a branch checked out from it; a git configuration and SSH
/// keys of its own.
struct Project {
    dir: TempDir,
}

impl Project {
    fn new(config: &str, approvers: Option<&str>) -> Self {
        let project = Self {
            dir: tempfile::tempdir().unwrap(),
        };
        fs::create_dir_all(project.keys()).unwrap();
        fs::write(
            project.gitconfig(),
            "[user]\n\tname = dev\n\temail = dev@example.com\n",
        )
        .unwrap();
        project.git(&["init", "-q", "-b", "main", "."]);
        project.write("invar.yaml", config);
        if let Some(approvers) = approvers {
            project.write(".invar/approvers", approvers);
        }
        project.git(&["add", "-A"]);
        project.git(&["commit", "-q", "-m", "base"]);
        project.git(&["checkout", "-q", "-b", BRANCH]);
        project
    }

    /// Signed approvals on `main`, with `approver` listed as `dev@example.com`, and
    /// git set to sign with it.
    fn signed() -> (Self, PathBuf) {
        let project = Self::new(SIGNED, None);
        let key = project.key("approver");
        project.git(&["checkout", "-q", "main"]);
        project.write(
            ".invar/approvers",
            &format!("{}\n", project.approver_line("dev@example.com", &key)),
        );
        project.commit_all("chore: list the approvers");
        project.git(&["checkout", "-q", "-B", BRANCH]);
        project.sign_with(&key);
        (project, key)
    }

    fn root(&self) -> PathBuf {
        self.dir.path().canonicalize().unwrap()
    }

    fn keys(&self) -> PathBuf {
        self.dir.path().join(".test-keys")
    }

    fn gitconfig(&self) -> PathBuf {
        self.dir.path().join(".test-gitconfig")
    }

    fn write(&self, relative: &str, text: &str) {
        let path = self.root().join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    fn read(&self, relative: &str) -> Option<String> {
        fs::read_to_string(self.root().join(relative)).ok()
    }

    /// A new ed25519 key with no passphrase, `name` and `name.pub`.
    fn key(&self, name: &str) -> PathBuf {
        let path = self.keys().join(name);
        let status = Process::new("ssh-keygen")
            .args(["-q", "-t", "ed25519", "-C", name, "-N", "", "-f"])
            .arg(&path)
            .stdin(Stdio::null())
            .status()
            .expect("ssh-keygen must be installed to run these tests");
        assert!(status.success());
        path
    }

    fn approver_line(&self, principal: &str, key: &Path) -> String {
        let public = fs::read_to_string(key.with_extension("pub")).unwrap();
        format!(
            "{principal} namespaces=\"invar-approval\" {}",
            public.trim()
        )
    }

    fn sign_with(&self, key: &Path) {
        self.git(&["config", "--global", "gpg.format", "ssh"]);
        self.git(&[
            "config",
            "--global",
            "user.signingkey",
            &key.display().to_string(),
        ]);
    }

    fn env(&self) -> Vec<(&'static str, String)> {
        vec![
            ("GIT_CONFIG_GLOBAL", self.gitconfig().display().to_string()),
            ("GIT_CONFIG_NOSYSTEM", "1".into()),
            ("HOME", self.root().display().to_string()),
            ("GIT_COMMITTER_DATE", "2026-10-05T10:00:00+02:00".into()),
            ("GIT_AUTHOR_DATE", "2026-10-05T10:00:00+02:00".into()),
        ]
    }

    fn git(&self, args: &[&str]) -> String {
        self.git_in(&self.root(), args)
    }

    fn git_in(&self, dir: &Path, args: &[&str]) -> String {
        let output = Process::new("git")
            .args(args)
            .current_dir(dir)
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .envs(self.env())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap()
    }

    fn invar_in(&self, dir: &Path, args: &[&str]) -> assert_cmd::assert::Assert {
        Command::cargo_bin("invar")
            .unwrap()
            .current_dir(dir)
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env_remove("XDG_CONFIG_HOME")
            .env_remove("SSH_AUTH_SOCK")
            .envs(self.env())
            .args(args)
            .write_stdin("")
            .assert()
    }

    fn invar(&self, args: &[&str]) -> assert_cmd::assert::Assert {
        self.invar_in(&self.root(), args)
    }

    fn commit_all(&self, message: &str) {
        self.git(&["add", "-A"]);
        self.git(&["commit", "-q", "--allow-empty", "-m", message]);
    }

    /// The planner's draft, committed.
    fn draft(&self) {
        self.write(SPEC, &draft());
        self.commit_all("docs: plan SHOP-214");
    }

    /// `plan approve`, as the developer runs it, without a commit.
    fn approve(&self, args: &[&str]) {
        let mut all = vec!["plan", "approve", "SHOP-214"];
        all.extend_from_slice(args);
        self.invar(&all).success();
    }

    /// The reset of a failed cycle.
    fn fail(&self) {
        self.invar(&[
            "decision",
            "record",
            "--ticket",
            "SHOP-214",
            "--type",
            "failed",
            "-m",
            "did not hold",
        ])
        .success();
        self.commit_all("chore: SHOP-214 failed");
    }

    fn code(&self, message: &str) {
        let count = self.read("app/orders.txt").unwrap_or_default();
        self.write("app/orders.txt", &format!("{count}{message}\n"));
        self.commit_all(message);
    }

    fn done(&self) {
        self.invar(&["done", "SHOP-214", "--commit", "chore: close SHOP-214"])
            .success();
    }

    fn field(&self, name: &str) -> String {
        self.read(SPEC)
            .unwrap()
            .lines()
            .find_map(|line| line.strip_prefix(&format!("{name}:")))
            .unwrap()
            .trim()
            .to_owned()
    }

    /// Replace a header line's value by hand, as an agent's shell could.
    fn set_field(&self, name: &str, value: &str) {
        let spec = self.read(SPEC).unwrap();
        let new = if value.is_empty() {
            format!("{name}:")
        } else {
            format!("{name}: {value}")
        };
        let lines: Vec<String> = spec
            .lines()
            .map(|line| match line.strip_prefix(&format!("{name}:")) {
                Some(_) => new.clone(),
                None => line.to_owned(),
            })
            .collect();
        self.write(SPEC, &(lines.join("\n") + "\n"));
    }

    fn verify(&self) -> assert_cmd::assert::Assert {
        self.invar(&["approval", "verify", "--base", "main"])
    }
}

fn fingerprint(key: &Path) -> String {
    let output = Process::new("ssh-keygen")
        .arg("-lf")
        .arg(key.with_extension("pub"))
        .output()
        .unwrap();
    String::from_utf8(output.stdout)
        .unwrap()
        .split_whitespace()
        .nth(1)
        .unwrap()
        .to_owned()
}

fn stdout(assert: &assert_cmd::assert::Assert) -> String {
    String::from_utf8(assert.get_output().stdout.clone()).unwrap()
}

fn last_line(assert: &assert_cmd::assert::Assert) -> String {
    stdout(assert).lines().last().unwrap_or_default().to_owned()
}

fn failed(project: &Project) -> assert_cmd::assert::Assert {
    let assert = project.verify().failure();
    assert_eq!(
        last_line(&assert),
        "approval verify: failed",
        "{}",
        stdout(&assert)
    );
    assert
}

// --- passes -----------------------------------------------------------------------

#[test]
fn a_signed_approval_then_done_passes_and_names_who_signed() {
    let (project, key) = Project::signed();
    project.draft();
    project.approve(&["--commit", "chore: approve SHOP-214"]);
    project.code("feat(orders): cancel an unpaid order");
    project.done();

    let assert = project.verify().success();
    let out = stdout(&assert);
    assert!(
        out.contains(&format!(
            "{SPEC}: SHOP-214 approved by dev@example.com ({})",
            fingerprint(&key)
        )),
        "{out}"
    );
    assert!(out.contains("(chore: approve SHOP-214)"), "{out}");
    assert!(
        last_line(&assert).starts_with("approval verify: passed: 1 signed approval(s)"),
        "{out}"
    );
    // `Approved-by` is git's `user.name`, and the signature is the record.
    assert!(
        out.contains("`Approved-by` says `dev`, and the signature is by dev@example.com"),
        "{out}"
    );
}

#[test]
fn a_reset_then_a_signed_reapproval_passes() {
    let (project, _) = Project::signed();
    project.draft();
    project.approve(&["--commit", "chore: approve SHOP-214"]);
    project.fail();
    let spec = project.read(SPEC).unwrap();
    project.write(
        SPEC,
        &spec.replace("Customers cancel", "Customers may cancel"),
    );
    project.commit_all("docs: revise SHOP-214");
    project.approve(&["--commit", "chore: approve SHOP-214 again"]);
    project.done();

    let assert = project.verify().success();
    assert!(
        last_line(&assert).starts_with("approval verify: passed: 2 signed approval(s)"),
        "{}",
        stdout(&assert)
    );
}

#[test]
fn a_branch_with_no_plan_passes_with_a_note() {
    let (project, _) = Project::signed();
    project.code("fix: a typo");
    project
        .verify()
        .success()
        .stdout(contains(
            "note: this branch has no Plan at any commit, so there is nothing to verify",
        ))
        .stdout(contains("approval verify: passed: nothing to verify"));
}

#[test]
fn with_approval_signed_off_at_the_base_the_check_is_off() {
    let project = Project::new("", None);
    project.draft();
    project.write(SPEC, &draft().replace("Status: draft", "Status: approved"));
    project.commit_all("approved by hand");
    let assert = project.verify().success();
    assert_eq!(
        last_line(&assert),
        "approval verify: off: main:invar.yaml does not set `approval.signed: true`, so \
         approvals are not signed, and none is verified"
    );
}

// --- refused ----------------------------------------------------------------------

#[test]
fn an_approval_with_no_signature_is_refused() {
    let (project, _) = Project::signed();
    project.draft();
    project.approve(&[]);
    fs::remove_file(project.root().join(SIGNATURE)).unwrap();
    project.commit_all("chore: approve SHOP-214");
    failed(&project).stdout(contains(format!(
        "(chore: approve SHOP-214): {SPEC}: is approved, and has no approval.sig"
    )));
}

#[test]
fn a_signer_not_in_the_bases_list_is_refused() {
    let (project, _) = Project::signed();
    let stranger = project.key("stranger");
    project.sign_with(&stranger);
    project.draft();
    project.approve(&["--commit", "chore: approve SHOP-214"]);
    failed(&project).stdout(contains(format!(
        "signed by a key that is not in the approvers list ({}) (main:.invar/approvers)",
        fingerprint(&stranger)
    )));
}

#[test]
fn a_branch_that_adds_its_own_key_to_the_list_is_refused() {
    let (project, _) = Project::signed();
    let stranger = project.key("stranger");
    let list = project.read(".invar/approvers").unwrap();
    project.write(
        ".invar/approvers",
        &format!(
            "{list}\n{}\n",
            project.approver_line("agent@example.com", &stranger)
        ),
    );
    project.commit_all("chore: add an approver");
    project.sign_with(&stranger);
    project.draft();
    project.approve(&["--commit", "chore: approve SHOP-214"]);
    failed(&project)
        .stdout(contains(
            "signed by a key that is not in the approvers list",
        ))
        .stdout(contains(
            "note: this branch changes .invar/approvers; the approvals were verified against \
             main:.invar/approvers, and the change applies once it is on main",
        ));
}

#[test]
fn a_signature_over_another_statement_is_refused() {
    // The body edited after signing, and `Approved-checksum` rewritten to match it:
    // what the agent would do. The signature is over the old checksum.
    let (project, _) = Project::signed();
    project.draft();
    project.approve(&[]);
    let (id, signature) = (
        project.field("Approval-id"),
        project.read(SIGNATURE).unwrap(),
    );
    let spec = project.read(SPEC).unwrap();
    project.write(SPEC, &spec.replace("Customers cancel", "Anyone cancels"));
    // An unsigned re-approval computes the checksum of the new body; the id and the
    // signature are then put back as they were.
    project.write("invar.yaml", "");
    project.approve(&["--reapprove"]);
    project.write("invar.yaml", SIGNED);
    project.set_field("Approval-id", &id);
    project.write(SIGNATURE, &signature);
    project.commit_all("chore: approve SHOP-214");
    failed(&project).stdout(contains(
        "the signature in approval.sig is not over this approval (Signature verification \
         failed: incorrect signature)",
    ));
}

#[test]
fn a_body_edited_after_signing_with_the_header_unchanged_is_refused() {
    let (project, _) = Project::signed();
    project.draft();
    project.approve(&[]);
    let spec = project.read(SPEC).unwrap();
    project.write(SPEC, &spec.replace("Customers cancel", "Anyone cancels"));
    project.commit_all("chore: approve SHOP-214");
    failed(&project).stdout(contains(
        "`Approved-checksum` does not match the Plan at this commit",
    ));
}

#[test]
fn a_body_change_while_approved_with_nothing_after_it_is_refused() {
    let (project, _) = Project::signed();
    project.draft();
    project.approve(&["--commit", "chore: approve SHOP-214"]);
    let spec = project.read(SPEC).unwrap();
    project.write(SPEC, &spec.replace("Customers cancel", "Anyone cancels"));
    project.commit_all("docs: widen SHOP-214");
    let assert = failed(&project).stdout(contains(format!(
        "(docs: widen SHOP-214): {SPEC}: the body changed while the Plan was approved, and no \
         signed approval, reset or deletion follows up to HEAD"
    )));
    // The approval itself was signed, and is reported as such.
    assert!(stdout(&assert).contains("SHOP-214 approved by dev@example.com"));
}

#[test]
fn a_body_change_then_done_is_refused() {
    let (project, _) = Project::signed();
    project.draft();
    project.approve(&["--commit", "chore: approve SHOP-214"]);
    let spec = project.read(SPEC).unwrap();
    project.write(SPEC, &spec.replace("Customers cancel", "Anyone cancels"));
    project.commit_all("docs: widen SHOP-214");
    // `done` refuses a changed Plan; an agent deletes it by hand.
    project.git(&["rm", "-q", "-r", DIR]);
    project.commit_all("chore: close SHOP-214");
    failed(&project).stdout(contains("deleted after its body changed at "));
}

#[test]
fn a_plan_created_and_deleted_with_no_valid_approval_is_refused() {
    let (project, _) = Project::signed();
    project.draft();
    project.write(SPEC, &draft().replace("Status: draft", "Status: approved"));
    project.commit_all("approved by hand");
    project.code("feat(orders): cancel");
    project.git(&["rm", "-q", "-r", DIR]);
    project.commit_all("chore: close SHOP-214");
    failed(&project)
        .stdout(contains(format!(
            "(approved by hand): {SPEC}: is approved, and has no approval.sig"
        )))
        .stdout(contains(format!(
            "(chore: close SHOP-214): {SPEC}: deleted with no signed approval before it on this \
             branch"
        )));
}

#[test]
fn a_draft_created_and_deleted_is_refused() {
    let (project, _) = Project::signed();
    project.draft();
    project.git(&["rm", "-q", "-r", DIR]);
    project.commit_all("chore: drop SHOP-214");
    failed(&project).stdout(contains("deleted with no signed approval before it"));
}

#[test]
fn a_replayed_approval_after_a_reset_is_refused() {
    let (project, _) = Project::signed();
    project.draft();
    project.approve(&["--commit", "chore: approve SHOP-214"]);
    let approved = project.git(&["rev-parse", "HEAD"]).trim().to_owned();
    project.fail();
    assert_eq!(
        project.read(SIGNATURE),
        None,
        "the reset removes the signature"
    );
    // Back from history: the old body, header and signature, all valid on their own.
    project.git(&["checkout", &approved, "--", SPEC, SIGNATURE]);
    project.commit_all("chore: restore SHOP-214");
    let assert = failed(&project);
    let out = stdout(&assert);
    assert!(
        out.contains(&format!(
            "(chore: restore SHOP-214): {SPEC}: this approval is replayed: its `Approval-id` {} is \
             the one of the approval at {}",
            project.field("Approval-id"),
            &approved[..12]
        )),
        "{out}"
    );
    // At HEAD it is the same replayed approval, reported once.
    assert_eq!(out.matches("is replayed").count(), 1, "{out}");
}

#[test]
fn an_approval_without_an_approval_id_is_refused() {
    let (project, _) = Project::signed();
    project.draft();
    project.approve(&[]);
    project.set_field("Approval-id", "");
    project.commit_all("chore: approve SHOP-214");
    failed(&project).stdout(contains(
        "is approved, and has no `Approval-id`, which every signed approval names",
    ));
}

#[test]
fn a_branch_that_turns_signing_off_is_checked_all_the_same() {
    let (project, _) = Project::signed();
    project.write("invar.yaml", "");
    project.commit_all("chore: stop signing");
    project.draft();
    // Unsigned now, since the work tree's invar.yaml says so.
    project.approve(&["--commit", "chore: approve SHOP-214"]);
    failed(&project)
        .stdout(contains(
            "note: this branch turns `approval.signed` off in invar.yaml; main has it on, so the \
             branch's approvals are verified all the same",
        ))
        .stdout(contains("is approved, and has no approval.sig"));
}

#[test]
fn every_problem_is_listed_in_one_run() {
    let (project, _) = Project::signed();
    project.draft();
    project.write(SPEC, &draft().replace("Status: draft", "Status: approved"));
    project.commit_all("approved by hand");
    project.git(&["rm", "-q", "-r", DIR]);
    project.commit_all("chore: close SHOP-214");
    project.write(
        ".invar/plans/SHOP-300-refunds/spec.md",
        &draft()
            .replace("SHOP-214", "SHOP-300")
            .replace("Status: draft", "Status: approved"),
    );
    project.commit_all("approved SHOP-300 by hand");
    let assert = failed(&project);
    let out = stdout(&assert);
    let problems = out
        .split_once("problems:\n")
        .unwrap()
        .1
        .lines()
        .filter(|line| line.starts_with("  "))
        .count();
    // SHOP-214: no signature, no checksum, no id, deleted without a signed approval;
    // SHOP-300: the same three at its approval.
    assert!(problems >= 6, "{out}");
    assert!(out.contains("SHOP-300-refunds/spec.md"), "{out}");
}

// --- preconditions ----------------------------------------------------------------

#[test]
fn a_shallow_checkout_is_refused() {
    let (project, _) = Project::signed();
    project.draft();
    project.approve(&["--commit", "chore: approve SHOP-214"]);
    let clone = project.dir.path().join("clone");
    project.git(&[
        "clone",
        "-q",
        "--depth",
        "1",
        "--no-single-branch",
        "--branch",
        BRANCH,
        &format!("file://{}", project.root().display()),
        &clone.display().to_string(),
    ]);
    project
        .invar_in(&clone, &["approval", "verify", "--base", "origin/main"])
        .failure()
        .stderr(contains("this is a shallow clone"))
        .stderr(contains("fetch-depth: 0"));
}

#[test]
fn the_base_is_never_the_one_the_branch_names() {
    let (project, _) = Project::signed();
    // The branch points `git.base_branch` at itself and forges an approval.
    project.write(
        "invar.yaml",
        &format!("{SIGNED}git:\n  base_branch: {BRANCH}\n"),
    );
    project.draft();
    project.write(SPEC, &draft().replace("Status: draft", "Status: approved"));
    project.commit_all("approved by hand");
    let assert = project
        .invar(&["approval", "verify"])
        .failure()
        .stdout(contains("base: main"))
        .stdout(contains(format!(
            "invar.yaml on this branch sets `git.base_branch: {BRANCH}`"
        )));
    assert_eq!(last_line(&assert), "approval verify: failed");
}

#[test]
fn a_base_with_no_approvers_list_is_refused() {
    let project = Project::new(SIGNED, None);
    project.code("fix: a typo");
    project.verify().failure().stderr(contains(
        "main has no .invar/approvers, and `approval.signed` is on there",
    ));
}

#[test]
fn a_merge_commit_onto_the_base_is_refused() {
    // What actions/checkout gives a pull request: `refs/pull/N/merge`.
    let (project, _) = Project::signed();
    project.draft();
    project.approve(&["--commit", "chore: approve SHOP-214"]);
    project.git(&["checkout", "-q", "-b", "ci-merge", "main"]);
    project.git(&["merge", "-q", "--no-ff", "-m", "Merge", BRANCH]);
    project
        .verify()
        .failure()
        .stderr(contains(
            "HEAD is a merge commit whose first parent is `main`",
        ))
        .stderr(contains("github.event.pull_request.head.sha"));
}

#[test]
fn it_writes_nothing() {
    let (project, _) = Project::signed();
    project.draft();
    project.approve(&["--commit", "chore: approve SHOP-214"]);
    let before = project.git(&["status", "--porcelain", "--ignored"]);
    project.verify().success();
    assert_eq!(project.git(&["status", "--porcelain", "--ignored"]), before);
}

#[test]
fn a_merge_of_the_base_into_the_branch_is_followed_along_the_branch() {
    // The branch's own first-parent line holds the approval; a merge of `main` into
    // it is one more commit of the branch, not CI's merge commit.
    let (project, _) = Project::signed();
    project.draft();
    project.approve(&["--commit", "chore: approve SHOP-214"]);
    project.git(&["checkout", "-q", "main"]);
    project.code("chore: main moves on");
    project.git(&["checkout", "-q", BRANCH]);
    project.git(&["merge", "-q", "--no-ff", "-m", "Merge main", "main"]);
    project.done();
    let assert = project.verify().success();
    assert!(
        last_line(&assert).starts_with("approval verify: passed: 1 signed approval(s)"),
        "{}",
        stdout(&assert)
    );
}
