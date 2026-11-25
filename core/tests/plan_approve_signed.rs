//! End-to-end tests for a signed approval (`approval-signature.md` §5) and for the
//! approval's commit (`invar-yaml.md` §3.7): the real binary, real `git`, and real
//! SSH keys made by `ssh-keygen` in each test. Nothing reaches the network or the
//! developer's own agent, keys or git config.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command as Process, Stdio};

use assert_cmd::Command;
use predicates::prelude::*;
use predicates::str::contains;
use tempfile::TempDir;

const SPEC: &str = ".invar/plans/SHOP-214-cancel-unpaid-order/spec.md";
const SIGNATURE: &str = ".invar/plans/SHOP-214-cancel-unpaid-order/approval.sig";

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

/// A git repository with one commit, a draft Plan not yet committed, and a git
/// configuration and SSH keys of its own.
struct Project {
    dir: TempDir,
}

impl Project {
    fn new(config: &str) -> Self {
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
        fs::write(project.root().join("invar.yaml"), config).unwrap();
        project.git(&["add", "invar.yaml"]);
        project.git(&["commit", "-q", "-m", "base"]);
        let spec = project.root().join(SPEC);
        fs::create_dir_all(spec.parent().unwrap()).unwrap();
        fs::write(spec, draft()).unwrap();
        project
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

    /// A new ed25519 key, `name` and `name.pub`, with `passphrase`.
    fn key(&self, name: &str, passphrase: &str) -> PathBuf {
        let path = self.keys().join(name);
        let status = Process::new("ssh-keygen")
            .args(["-q", "-t", "ed25519", "-C", name, "-N", passphrase, "-f"])
            .arg(&path)
            .stdin(Stdio::null())
            .status()
            .expect("ssh-keygen must be installed to run these tests");
        assert!(status.success());
        path
    }

    /// Set git's signing up as a developer would for signed commits.
    fn sign_with(&self, key: &str) {
        self.git(&["config", "--global", "gpg.format", "ssh"]);
        self.git(&["config", "--global", "user.signingkey", key]);
    }

    fn env(&self) -> Vec<(&'static str, String)> {
        // An askpass that refuses: the person cancelling the prompt. It keeps a
        // passphrase prompt from reaching a terminal the tests may run in.
        let askpass = self.keys().join("askpass-refuses");
        if !askpass.exists() {
            fs::write(&askpass, "#!/bin/sh\nexit 1\n").unwrap();
            make_executable(&askpass);
        }
        vec![
            ("GIT_CONFIG_GLOBAL", self.gitconfig().display().to_string()),
            ("GIT_CONFIG_NOSYSTEM", "1".into()),
            ("HOME", self.root().display().to_string()),
            ("GIT_COMMITTER_DATE", "2026-10-05T10:00:00+02:00".into()),
            ("GIT_AUTHOR_DATE", "2026-10-05T10:00:00+02:00".into()),
            ("SSH_ASKPASS", askpass.display().to_string()),
            ("SSH_ASKPASS_REQUIRE", "force".into()),
            ("DISPLAY", ":invar-test".into()),
        ]
    }

    fn git(&self, args: &[&str]) -> String {
        let output = Process::new("git")
            .args(args)
            .current_dir(self.root())
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

    fn approve(&self, args: &[&str]) -> assert_cmd::assert::Assert {
        self.invar(&["plan", "approve"], args)
    }

    fn invar(&self, command: &[&str], args: &[&str]) -> assert_cmd::assert::Assert {
        Command::cargo_bin("invar")
            .unwrap()
            .current_dir(self.root())
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env_remove("XDG_CONFIG_HOME")
            .env_remove("SSH_AUTH_SOCK")
            .envs(self.env())
            .args(command)
            .args(args)
            .write_stdin("")
            .assert()
    }

    /// The value of `name` in the Plan's header, if it has one.
    fn field(&self, name: &str) -> Option<String> {
        self.read(SPEC)?
            .split("\n---\n")
            .next()?
            .lines()
            .find_map(|line| line.strip_prefix(&format!("{name}:")))
            .map(|value| value.trim().to_owned())
    }

    fn read(&self, relative: &str) -> Option<String> {
        fs::read_to_string(self.root().join(relative)).ok()
    }

    /// `ssh-keygen -Y verify` of `approval.sig` over the statement built from the
    /// header, four lines (`approval-signature.md` §3), against an allowed-signers
    /// line for `principal` and `key`: an independent check of what `plan approve`
    /// wrote.
    fn verify(&self, principal: &str, key: &Path) -> std::process::Output {
        let field = |name: &str| self.field(name).unwrap();
        let statement = format!(
            "invar-approval 1\nticket: {}\napproved-checksum: {}\napproval-id: {}\n",
            field("Ticket"),
            field("Approved-checksum"),
            field("Approval-id")
        );
        let public = fs::read_to_string(key.with_extension("pub")).unwrap();
        let allowed = self.keys().join("allowed");
        fs::write(
            &allowed,
            format!("{principal} namespaces=\"invar-approval\" {public}"),
        )
        .unwrap();
        let mut child = Process::new("ssh-keygen")
            .args([
                "-Y",
                "verify",
                "-n",
                "invar-approval",
                "-I",
                principal,
                "-f",
            ])
            .arg(&allowed)
            .arg("-s")
            .arg(self.root().join(SIGNATURE))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        use std::io::Write as _;
        child
            .stdin
            .take()
            .unwrap()
            .write_all(statement.as_bytes())
            .unwrap();
        child.wait_with_output().unwrap()
    }
}

#[cfg(unix)]
fn make_executable(path: &Path) {
    use std::os::unix::fs::PermissionsExt as _;
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

#[cfg(not(unix))]
fn make_executable(_: &Path) {}

const SIGNED: &str = "approval:\n  signed: true\n";

/// Refused before or at signing: the Plan is the draft it was, and there is no
/// signature.
fn nothing_was_written(project: &Project) {
    assert_eq!(project.read(SPEC).unwrap(), draft());
    assert_eq!(project.read(SIGNATURE), None);
}

// --- signed ------------------------------------------------------------------------

#[test]
fn a_signed_approval_writes_a_signature_ssh_keygen_verifies() {
    let project = Project::new(SIGNED);
    let key = project.key("approver", "");
    project.sign_with(&key.display().to_string());

    let assert = project
        .approve(&["SHOP-214"])
        .success()
        .stdout(contains("signing with "))
        .stdout(contains(format!("  Signature: {SIGNATURE}")));
    // The line `/invar:approve` keys on is still the last one.
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let last = stdout.lines().last().unwrap();
    assert!(
        last.starts_with("approved SHOP-214 checksum sha256:"),
        "{stdout}"
    );

    let signature = project.read(SIGNATURE).unwrap();
    assert!(
        signature.starts_with("-----BEGIN SSH SIGNATURE-----\n"),
        "{signature}"
    );
    assert!(project.read(SPEC).unwrap().contains("Status: approved\n"));

    let verified = project.verify("dev@example.com", &key);
    assert!(
        verified.status.success(),
        "{}",
        String::from_utf8_lossy(&verified.stderr)
    );
    assert!(
        String::from_utf8_lossy(&verified.stdout).contains("Good \"invar-approval\" signature")
    );
}

#[test]
fn the_signature_is_over_the_approval_and_no_other() {
    // A body changed after signing makes another statement, which the same
    // signature does not cover.
    let project = Project::new(SIGNED);
    let key = project.key("approver", "");
    project.sign_with(&key.display().to_string());
    project.approve(&["SHOP-214"]).success();
    let spec = project.read(SPEC).unwrap();
    let checksum = spec
        .lines()
        .find_map(|line| line.strip_prefix("Approved-checksum: "))
        .unwrap()
        .to_owned();
    fs::write(
        project.root().join(SPEC),
        spec.replace(&checksum, &format!("sha256:{}", "0".repeat(64))),
    )
    .unwrap();
    assert!(!project.verify("dev@example.com", &key).status.success());
}

#[test]
fn the_statement_names_the_approval_id_written_in_the_header() {
    let project = Project::new(SIGNED);
    let key = project.key("approver", "");
    project.sign_with(&key.display().to_string());
    project.approve(&["SHOP-214"]).success();
    let id = project.field("Approval-id").unwrap();
    assert_eq!(id.len(), 32, "{id}");
    assert!(
        id.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
        "{id}"
    );
    // Right after `Acceptance`, which `plan approve` added after the checksum.
    assert!(
        project
            .read(SPEC)
            .unwrap()
            .contains(&format!("\nAcceptance: skipped\nApproval-id: {id}\n---\n")),
        "{}",
        project.read(SPEC).unwrap()
    );
    assert!(project.verify("dev@example.com", &key).status.success());
}

/// `decision record --type failed`: the reset of a failed cycle.
fn fail(project: &Project) {
    project
        .invar(
            &["decision", "record"],
            &[
                "--ticket",
                "SHOP-214",
                "--type",
                "failed",
                "-m",
                "did not hold",
            ],
        )
        .success()
        .stdout(contains(format!("reset: {SIGNATURE} removed")));
}

#[test]
fn a_failed_reset_clears_the_id_and_removes_the_signature() {
    let project = Project::new(SIGNED);
    let key = project.key("approver", "");
    project.sign_with(&key.display().to_string());
    project.approve(&["SHOP-214"]).success();
    fail(&project);
    assert_eq!(project.read(SIGNATURE), None);
    assert_eq!(project.field("Status").as_deref(), Some("draft"));
    assert_eq!(project.field("Approval-id").as_deref(), Some(""));
}

#[test]
fn two_approvals_of_the_same_body_get_different_ids_and_signatures() {
    // The same ticket and checksum, approved, reset, approved again: without the
    // id, ed25519 would sign the same statement to the same signature, and the
    // first could be put back after the reset (approval-signature.md D-7).
    let project = Project::new(SIGNED);
    let key = project.key("approver", "");
    project.sign_with(&key.display().to_string());
    project.approve(&["SHOP-214"]).success();
    let (first_id, first_signature) = (
        project.field("Approval-id").unwrap(),
        project.read(SIGNATURE).unwrap(),
    );
    let checksum = project.field("Approved-checksum").unwrap();
    fail(&project);
    project.approve(&["SHOP-214"]).success();
    assert_eq!(project.field("Approved-checksum").unwrap(), checksum);
    assert_ne!(project.field("Approval-id").unwrap(), first_id);
    assert_ne!(project.read(SIGNATURE).unwrap(), first_signature);
    assert!(project.verify("dev@example.com", &key).status.success());
}

#[test]
fn a_reapproval_replaces_the_signature() {
    let project = Project::new(SIGNED);
    let key = project.key("approver", "");
    project.sign_with(&key.display().to_string());
    project.approve(&["SHOP-214"]).success();
    let first = project.read(SIGNATURE).unwrap();

    let spec = project.read(SPEC).unwrap();
    fs::write(
        project.root().join(SPEC),
        spec.replace("Customers cancel", "Customers may cancel"),
    )
    .unwrap();
    project.approve(&["SHOP-214", "--reapprove"]).success();
    let second = project.read(SIGNATURE).unwrap();
    assert_ne!(first, second);
    assert!(project.verify("dev@example.com", &key).status.success());
}

#[test]
fn without_an_ssh_signing_setup_the_approval_is_refused_before_the_criteria() {
    let project = Project::new(SIGNED);
    project
        .approve(&["SHOP-214"])
        .failure()
        .stdout(contains("Acceptance criteria").not())
        .stderr(contains("`approval.signed` is on"))
        .stderr(contains("git has no `gpg.format` set"))
        .stderr(contains("git config gpg.format ssh"));
    nothing_was_written(&project);

    project.git(&["config", "--global", "gpg.format", "openpgp"]);
    project
        .approve(&["SHOP-214"])
        .failure()
        .stderr(contains("`gpg.format = openpgp`"));
    nothing_was_written(&project);

    project.git(&["config", "--global", "gpg.format", "ssh"]);
    project
        .approve(&["SHOP-214"])
        .failure()
        .stderr(contains("git has no `user.signingkey`"));
    nothing_was_written(&project);
}

#[test]
fn a_refusal_reaches_the_skill_on_stdout() {
    let project = Project::new(SIGNED);
    project
        .approve(&["SHOP-214", "--skill"])
        .code(0)
        .stdout(contains("error: `approval.signed` is on"))
        .stdout(contains("approved SHOP-214 checksum").not());
    nothing_was_written(&project);
}

#[test]
fn a_key_that_cannot_sign_refuses_with_its_own_words_and_writes_nothing() {
    // A passphrase key, no agent, and the askpass refusing: the person
    // cancelled the prompt.
    let project = Project::new(SIGNED);
    let key = project.key("locked", "a passphrase");
    project.sign_with(&key.display().to_string());
    project
        .approve(&["SHOP-214"])
        .failure()
        .stdout(contains("Acceptance criteria"))
        .stderr(contains("`ssh-keygen` did not sign the approval"))
        .stderr(contains("passphrase"));
    nothing_was_written(&project);
}

#[test]
fn a_public_key_written_in_place_is_used_as_git_uses_it() {
    // `key::ssh-ed25519 …` signs through the agent; with no agent it fails, and
    // the temporary key file it was written to is gone.
    let project = Project::new(SIGNED);
    let key = project.key("approver", "");
    let public = fs::read_to_string(key.with_extension("pub")).unwrap();
    project.sign_with(&format!("key::{}", public.trim()));
    project
        .approve(&["SHOP-214"])
        .failure()
        .stdout(contains("signing with ssh-ed25519 "))
        .stderr(contains("`ssh-keygen` did not sign the approval"));
    nothing_was_written(&project);
    let left: Vec<_> = fs::read_dir(std::env::temp_dir())
        .unwrap()
        .filter_map(|entry| entry.ok())
        .filter(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            name.starts_with(".invar-signing-key-")
                && fs::read_to_string(entry.path()).is_ok_and(|t| t.trim() == public.trim())
        })
        .collect();
    assert!(left.is_empty(), "a temporary key was left behind");
}

#[test]
fn without_the_section_nothing_is_signed_as_before() {
    let project = Project::new("");
    let key = project.key("approver", "");
    // A signing setup that would work: it is not asked for.
    project.sign_with(&key.display().to_string());
    project
        .approve(&["SHOP-214"])
        .success()
        .stdout(contains("signing with").not())
        .stdout(contains("Signature:").not())
        .stdout(contains("approved SHOP-214 checksum sha256:"));
    assert_eq!(project.read(SIGNATURE), None);
    assert!(project.read(SPEC).unwrap().contains("Status: approved\n"));
    // An unsigned approval writes no `Approval-id`.
    assert_eq!(project.field("Approval-id"), None);
}

// --- committed ---------------------------------------------------------------------

fn files_in_head(project: &Project) -> Vec<String> {
    let mut files: Vec<String> = project
        .git(&["show", "--name-only", "--format=", "HEAD"])
        .lines()
        .map(str::to_owned)
        .collect();
    files.sort();
    files
}

#[test]
fn commit_makes_one_commit_of_the_plan_and_its_signature_only() {
    let project = Project::new(SIGNED);
    let key = project.key("approver", "");
    project.sign_with(&key.display().to_string());
    fs::write(project.root().join("other.txt"), "staged, not approved\n").unwrap();
    project.git(&["add", "other.txt"]);

    project
        .approve(&[
            "SHOP-214",
            "--commit",
            "chore: approve the Plan of SHOP-214",
        ])
        .success()
        .stdout(contains("  committed: chore: approve the Plan of SHOP-214"));
    assert_eq!(
        project.git(&["log", "-1", "--format=%s"]).trim(),
        "chore: approve the Plan of SHOP-214"
    );
    assert_eq!(files_in_head(&project), [SIGNATURE, SPEC]);
    // What else was staged stays staged, and out of the commit.
    assert_eq!(
        project.git(&["diff", "--cached", "--name-only"]).trim(),
        "other.txt"
    );
    assert_eq!(project.git(&["rev-list", "--count", "HEAD"]).trim(), "2");
}

#[test]
fn approval_commit_in_the_config_commits_under_the_default_message() {
    let project = Project::new("approval:\n  commit: true\n");
    project.approve(&["SHOP-214"]).success();
    assert_eq!(
        project.git(&["log", "-1", "--format=%s"]).trim(),
        "Approve plan SHOP-214"
    );
    // Unsigned: the Plan alone.
    assert_eq!(files_in_head(&project), [SPEC]);
}

#[test]
fn no_commit_wins_over_the_config_and_commit_with_no_commit_is_refused() {
    let project = Project::new("approval:\n  commit: true\n");
    project
        .approve(&["SHOP-214", "--commit", "x", "--no-commit"])
        .failure()
        .stderr(contains("`--commit` cannot be used with `--no-commit`"));
    assert_eq!(project.read(SPEC).unwrap(), draft());

    project.approve(&["SHOP-214", "--no-commit"]).success();
    assert_eq!(project.git(&["rev-list", "--count", "HEAD"]).trim(), "1");
}

#[test]
fn a_commit_that_fails_leaves_the_approval_and_says_so() {
    let project = Project::new("");
    let hook = project.root().join(".git/hooks/pre-commit");
    fs::write(&hook, "#!/bin/sh\necho 'refused by the hook' >&2\nexit 1\n").unwrap();
    make_executable(&hook);
    project
        .approve(&["SHOP-214", "--commit", "Approve plan SHOP-214"])
        .failure()
        .stdout(contains("approved SHOP-214 checksum").not())
        .stderr(contains(
            "the approval of SHOP-214 is recorded, and committing it failed",
        ))
        .stderr(contains(format!(
            "commit `{SPEC}` yourself, then ask the session to start Phase 2"
        )))
        .stderr(contains("refused by the hook"));
    assert!(project.read(SPEC).unwrap().contains("Status: approved\n"));
    assert_eq!(project.git(&["rev-list", "--count", "HEAD"]).trim(), "1");
    // A retry finds the approval current: it changes nothing, and starts nothing.
    project
        .approve(&["SHOP-214", "--commit", "Approve plan SHOP-214"])
        .success()
        .stdout(contains("already approved"))
        .stdout(contains("approved SHOP-214 checksum").not());
}

#[test]
fn a_signed_commit_that_fails_names_the_plan_and_its_signature() {
    let project = Project::new(SIGNED);
    let key = project.key("approver", "");
    project.sign_with(&key.display().to_string());
    let hook = project.root().join(".git/hooks/pre-commit");
    fs::write(&hook, "#!/bin/sh\nexit 1\n").unwrap();
    make_executable(&hook);
    project
        .approve(&["SHOP-214", "--commit", "Approve plan SHOP-214"])
        .failure()
        .stdout(contains("approved SHOP-214 checksum").not())
        .stderr(contains(format!(
            "commit `{SPEC}` and `{SIGNATURE}` yourself, then ask the session to start Phase 2"
        )));
}
