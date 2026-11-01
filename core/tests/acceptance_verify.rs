//! End-to-end tests for verifying locks (AT-9): the real binary, in real git
//! repositories. `src/acceptance/verify/tests.rs` covers each rule on its own.

use std::fs;
use std::path::PathBuf;

use assert_cmd::Command;
use assert_cmd::assert::Assert;
use tempfile::TempDir;

const SPEC: &str = ".invar/plans/SHOP-214-cancel-unpaid-order/spec.md";
const PLAN_DIR: &str = ".invar/plans/SHOP-214-cancel-unpaid-order";

const CONFIG: &str = "acceptance:\n  dir: tests/Acceptance\n  \
    run: [\"sh\", \"runner.sh\", \"{report}\", \"{files}\"]\n";

/// Locked for another ticket on `main`.
const OTHERS: &str = "tests/Acceptance/others_test.php";
/// This ticket's.
const MINE: &str = "tests/Acceptance/mine_test.php";
const TEXT: &str = "it('AC1 works', fn () => expect(false)->toBeTrue());\n";

fn body(to_change: &str) -> String {
    let section = if to_change.is_empty() {
        String::new()
    } else {
        format!("## Acceptance tests to change\n\n{to_change}\n\n")
    };
    format!(
        "## Ticket\nSHOP-214 — Cancel an unpaid order.\n\n\
         ## Intent\nA customer can cancel an order they have not paid for.\n\n\
         ## Scope\n`Orders` only.\n\n\
         ## Open questions / answers\nNone.\n\n\
         ## Rejected alternatives\nNone.\n\n\
         ## Acceptance criteria\n\n\
         - AC1: Given an unpaid order, when its owner cancels it, then it is cancelled.\n\n\
         {section}\
         ## Implementation context\n- AI_CONTEXT: controllers stay thin\n"
    )
}

struct Project {
    dir: TempDir,
}

impl Project {
    /// On `feat/SHOP-214-cancel`, from a `main` that holds [`OTHERS`], with an approved
    /// Plan that says `required` and declares nothing.
    fn new() -> Self {
        Self::with_config(CONFIG)
    }

    fn with_config(config: &str) -> Self {
        let project = Self {
            dir: tempfile::tempdir().unwrap(),
        };
        project.write("invar.yaml", config);
        project.write(
            ".gitconfig-test",
            "[user]\n\tname = Test\n\temail = test@example.com\n[commit]\n\tgpgsign = false\n",
        );
        project.git(&["init", "-q", "-b", "main"]);
        let locked = project.locked("SHOP-1", TEXT);
        project.write(OTHERS, &locked);
        project.commit("tests of another ticket");
        project.git(&["checkout", "-q", "-b", "feat/SHOP-214-cancel"]);
        project.approve("required", "");
        project
    }

    fn root(&self) -> PathBuf {
        self.dir.path().canonicalize().unwrap()
    }

    fn write(&self, path: &str, text: &str) {
        let path = self.root().join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    fn read(&self, path: &str) -> String {
        fs::read_to_string(self.root().join(path)).unwrap()
    }

    fn exists(&self, path: &str) -> bool {
        self.root().join(path).exists()
    }

    fn git(&self, args: &[&str]) -> String {
        let output = std::process::Command::new("git")
            .args(args)
            .current_dir(self.root())
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", self.root().join(".gitconfig-test"))
            .env("HOME", self.root())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap()
    }

    fn commit(&self, message: &str) {
        self.git(&["add", "-A"]);
        self.git(&["commit", "-q", "--allow-empty", "-m", message]);
    }

    fn invar(&self, args: &[&str]) -> Assert {
        Command::cargo_bin("invar")
            .unwrap()
            .current_dir(self.root())
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", self.root().join(".gitconfig-test"))
            .env("HOME", self.root())
            .args(args)
            .assert()
    }

    /// The text of a file locked for `ticket`, with the digest `invar hash --lock`
    /// prints for it: exactly what someone writing it by hand would do.
    fn locked(&self, ticket: &str, text: &str) -> String {
        let pending = format!("<?php\n// invar-lock: {ticket}\n{text}");
        let file = self.dir.path().join("to-hash.php");
        fs::write(&file, &pending).unwrap();
        let output = Command::cargo_bin("invar")
            .unwrap()
            .args(["hash", "--lock"])
            .arg(&file)
            .output()
            .unwrap();
        fs::remove_file(&file).unwrap();
        let digest = String::from_utf8(output.stdout).unwrap();
        pending.replace(
            &format!("invar-lock: {ticket}"),
            &format!("invar-lock: {ticket} {}", digest.trim()),
        )
    }

    /// The Plan approved as `plan approve` records it.
    fn approve(&self, acceptance: &str, to_change: &str) {
        let body = body(to_change);
        let file = self.dir.path().join("to-hash.txt");
        fs::write(&file, format!("Acceptance: {acceptance}\n{body}")).unwrap();
        let output = Command::cargo_bin("invar")
            .unwrap()
            .arg("hash")
            .arg(&file)
            .output()
            .unwrap();
        fs::remove_file(&file).unwrap();
        let checksum = String::from_utf8(output.stdout).unwrap();
        self.write(
            SPEC,
            &format!(
                "---\nTicket: SHOP-214\nModule: orders\nStatus: approved\nApproved-by: rade\n\
                 Approved-at: 2026-10-05\nApproved-checksum: {}\nAcceptance: {acceptance}\n---\n\n\
                 {body}",
                checksum.trim()
            ),
        );
    }

    /// One more locked test of this ticket, so that §10.3 holds.
    fn lock_mine(&self) {
        let locked = self.locked("SHOP-214", TEXT);
        self.write(MINE, &locked);
    }

    fn plan_check(&self) -> Assert {
        self.invar(&["plan", "check", "SHOP-214"])
    }

    fn done(&self) -> Assert {
        self.invar(&["done", "SHOP-214"])
    }
}

fn stderr_of(assert: &Assert) -> String {
    String::from_utf8(assert.get_output().stderr.clone()).unwrap()
}

fn stdout_of(assert: &Assert) -> String {
    String::from_utf8(assert.get_output().stdout.clone()).unwrap()
}

/// A refusal: non-zero, nothing on stdout, and every part on stderr.
fn refused(assert: Assert, parts: &[&str]) {
    let assert = assert.failure().stdout("");
    let message = stderr_of(&assert);
    for part in parts {
        assert!(message.contains(part), "{part}:\n{message}");
    }
}

/// `done` refused, and nothing deleted or staged.
fn done_refused(project: &Project, parts: &[&str]) {
    refused(project.done(), parts);
    assert!(project.exists(SPEC), "done deleted the Plan");
    let staged = project.git(&["diff", "--cached", "--name-only"]);
    assert!(!staged.contains(PLAN_DIR), "{staged}");
}

// --- a one-character change -------------------------------------------------------

/// [`MINE`] locked, and one character changed in [`OTHERS`].
fn with_one_character_changed() -> Project {
    let project = Project::new();
    project.lock_mine();
    let changed = project.read(OTHERS).replace("toBeTrue", "toBeTruE");
    project.write(OTHERS, &changed);
    project
}

#[test]
fn a_one_character_change_makes_plan_check_refuse_naming_the_file_and_its_ticket() {
    let project = with_one_character_changed();
    refused(
        project.plan_check(),
        &[
            OTHERS,
            "locked for SHOP-1",
            "changed after it was locked",
            "Restore",
        ],
    );
}

#[test]
fn a_one_character_change_makes_done_refuse_and_delete_nothing() {
    let project = with_one_character_changed();
    done_refused(
        &project,
        &[OTHERS, "locked for SHOP-1", "Nothing was changed"],
    );
}

// --- another ticket's test ---------------------------------------------------------

#[test]
fn another_tickets_test_changed_is_refused_until_the_approved_plan_declares_it() {
    let project = Project::new();
    project.lock_mine();
    // The digest holds, so only the branch rule can see this (D-20).
    let weaker = project.locked("SHOP-1", "it('AC1 works', fn () => true);\n");
    project.write(OTHERS, &weaker);
    project.commit("weaken");

    refused(
        project.plan_check(),
        &[
            OTHERS,
            "locked for SHOP-1",
            "changed on this branch",
            "— change —",
        ],
    );
    done_refused(&project, &[OTHERS, "changed on this branch"]);

    project.approve(
        "required",
        &format!("- {OTHERS} — change — the rule for paid orders moved"),
    );
    project.plan_check().success();
    project.done().success();
    assert!(!project.exists(SPEC));
}

#[test]
fn a_declaration_written_in_without_approval_changes_nothing() {
    let project = Project::new();
    let weaker = project.locked("SHOP-1", "it('AC1 works', fn () => true);\n");
    project.write(OTHERS, &weaker);
    let spec = project.read(SPEC).replace(
        "## Implementation context",
        &format!("## Acceptance tests to change\n\n- {OTHERS} — change — trust me\n\n## Implementation context"),
    );
    project.write(SPEC, &spec);
    refused(project.plan_check(), &["changed after it was approved"]);
}

#[test]
fn another_tickets_test_deleted_is_refused_until_declared_with_delete() {
    let project = Project::new();
    project.lock_mine();
    project.git(&["rm", "-q", OTHERS]);
    project.commit("delete");

    refused(
        project.plan_check(),
        &[OTHERS, "locked for SHOP-1", "deleted on this branch"],
    );
    done_refused(&project, &[OTHERS, "deleted on this branch"]);

    project.approve("required", &format!("- {OTHERS} — change — wrong action"));
    refused(project.plan_check(), &["only with `change`"]);

    project.approve(
        "required",
        &format!("- {OTHERS} — delete — the feature is gone"),
    );
    project.plan_check().success();
    project.done().success();
}

#[test]
fn a_rename_is_refused_as_a_deletion() {
    let project = Project::new();
    project.git(&["mv", OTHERS, "tests/Acceptance/renamed_test.php"]);
    project.commit("rename");
    refused(project.plan_check(), &[OTHERS, "deleted on this branch"]);
    project.approve("required", &format!("- {OTHERS} — delete — moved"));
    project.plan_check().success();
}

#[test]
fn an_uncommitted_change_counts_like_a_committed_one() {
    let project = Project::new();
    let weaker = project.locked("SHOP-1", "it('AC1 works', fn () => true);\n");
    project.write(OTHERS, &weaker);
    refused(project.plan_check(), &[OTHERS, "changed on this branch"]);
}

#[test]
fn the_tickets_own_file_locked_at_the_merge_base_needs_no_declaration() {
    let project = Project::new();
    project.lock_mine();
    project.commit("first cycle");
    project.git(&["branch", "-q", "-f", "main"]);

    let second = project.locked(
        "SHOP-214",
        "it('AC1 second', fn () => expect(1)->toBe(2));\n",
    );
    project.write(MINE, &second);
    project.plan_check().success().stderr("");
    project.done().success();
}

#[test]
fn a_ticket_that_takes_over_a_declared_test_may_leave_it_pending_until_done() {
    let project = Project::new();
    project.approve("required", &format!("- {OTHERS} — change — the rule moved"));
    project.write(OTHERS, &format!("<?php\n// invar-lock: SHOP-214\n{TEXT}"));
    project.plan_check().success();
    done_refused(&project, &[OTHERS, "pending line for SHOP-214"]);

    let relocked = project.locked("SHOP-214", TEXT);
    project.write(OTHERS, &relocked);
    project.plan_check().success();
    project.done().success();
}

// --- the declarations --------------------------------------------------------------

#[test]
fn a_declared_path_that_is_not_a_locked_test_is_only_a_warning() {
    let project = Project::new();
    let typo = "tests/Acceptance/othres_test.php";
    project.approve("required", &format!("- {typo} — change — a typo"));
    let assert = project.plan_check().success();
    let message = stderr_of(&assert);
    assert!(message.contains(&format!("warning: {typo}")), "{message}");
    assert!(message.contains("not a locked test"), "{message}");
}

#[test]
fn a_declared_path_that_did_not_change_is_only_a_warning() {
    let project = Project::new();
    project.approve("required", &format!("- {OTHERS} — change — not done yet"));
    let assert = project.plan_check().success();
    assert!(
        stderr_of(&assert).contains("did not change"),
        "{}",
        stderr_of(&assert)
    );
}

#[test]
fn a_declared_path_with_dot_dot_is_refused() {
    let project = Project::new();
    project.approve("required", "- tests/Acceptance/../secret.php — change — no");
    refused(
        project.plan_check(),
        &["tests/Acceptance/../secret.php", "`..`"],
    );
}

// --- closing ---------------------------------------------------------------------

#[test]
fn done_refuses_required_with_no_locked_test() {
    let project = Project::new();
    done_refused(
        &project,
        &[
            "`Acceptance: required`",
            "no file under tests/Acceptance is locked for it",
        ],
    );
}

#[test]
fn done_refuses_required_with_a_pending_line_left() {
    let project = Project::new();
    project.lock_mine();
    project.write(
        "tests/Acceptance/more_test.php",
        &format!("// invar-lock: SHOP-214\n{TEXT}"),
    );
    done_refused(&project, &["more_test.php", "pending line for SHOP-214"]);
}

#[test]
fn done_refuses_skipped_with_a_pending_line_and_otherwise_asks_for_nothing() {
    let project = Project::new();
    project.approve("skipped", "");
    project.write(
        "tests/Acceptance/more_test.php",
        &format!("// invar-lock: SHOP-214\n{TEXT}"),
    );
    done_refused(&project, &["more_test.php", "pending line for SHOP-214"]);

    fs::remove_file(project.root().join("tests/Acceptance/more_test.php")).unwrap();
    project.done().success();
}

#[test]
fn done_leaves_another_tickets_pending_line_to_ci() {
    let project = Project::new();
    project.lock_mine();
    project.write(
        "tests/Acceptance/other_pending.php",
        &format!("// invar-lock: SHOP-9\n{TEXT}"),
    );
    project.done().success();
}

#[test]
fn done_reports_every_problem_at_once() {
    let project = Project::new();
    let changed = project.read(OTHERS).replace("toBeTrue", "toBeFalse");
    project.write(OTHERS, &changed);
    project.write(
        "tests/Acceptance/more_test.php",
        &format!("// invar-lock: SHOP-214\n{TEXT}"),
    );
    done_refused(
        &project,
        &[
            "changed after it was locked",
            "changed on this branch",
            "no file under tests/Acceptance is locked",
            "more_test.php",
        ],
    );
}

// --- plan check --------------------------------------------------------------------

#[test]
fn plan_check_passes_with_a_pending_line_and_prints_its_lines_as_before() {
    let project = Project::new();
    project.write(MINE, &format!("// invar-lock: SHOP-214\n{TEXT}"));
    let assert = project.plan_check().success().stderr("");
    let out = stdout_of(&assert);
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines.len(), 2, "{out}");
    assert!(lines[0].starts_with("ok: "), "{out}");
    assert_eq!(lines[1], "acceptance: required");
}

#[test]
fn plan_check_holds_a_skipped_ticket_to_the_locks_too() {
    let project = Project::new();
    project.approve("skipped", "");
    let weaker = project.locked("SHOP-1", "it('AC1 works', fn () => true);\n");
    project.write(OTHERS, &weaker);
    refused(project.plan_check(), &[OTHERS, "changed on this branch"]);
}

#[test]
fn a_draft_is_refused_for_being_a_draft_before_any_lock_is_looked_at() {
    let project = Project::new();
    project.write(
        SPEC,
        &format!(
            "---\nTicket: SHOP-214\nModule: orders\nStatus: draft\nApproved-by:\nApproved-at:\n\
             Approved-checksum:\n---\n\n{}",
            body("")
        ),
    );
    let changed = project.read(OTHERS).replace("toBeTrue", "toBeFalse");
    project.write(OTHERS, &changed);
    let assert = project.plan_check().failure();
    let message = stderr_of(&assert);
    assert!(message.contains("draft"), "{message}");
    assert!(!message.contains(OTHERS), "{message}");
}

#[test]
fn a_base_branch_that_is_not_there_refuses_plan_check() {
    let project = Project::with_config(&format!("git:\n  base_branch: develop\n{CONFIG}"));
    refused(project.plan_check(), &["cannot be compared", "develop"]);
}

#[test]
fn outside_git_plan_check_and_done_warn_and_still_check_integrity() {
    let project = Project::new();
    project.lock_mine();
    fs::remove_dir_all(project.root().join(".git")).unwrap();
    let assert = project.plan_check().success();
    assert!(
        stderr_of(&assert).contains("not in a git repository"),
        "{}",
        stderr_of(&assert)
    );

    let changed = project.read(OTHERS).replace("toBeTrue", "toBeFalse");
    project.write(OTHERS, &changed);
    refused(
        project.plan_check(),
        &[OTHERS, "changed after it was locked"],
    );
    refused(project.done(), &[OTHERS]);
    assert!(project.exists(SPEC));
}

// --- no section ------------------------------------------------------------------

#[test]
fn with_no_section_plan_check_is_silent_and_done_warns_only_for_a_required_plan() {
    let project = Project::new();
    project.write("invar.yaml", "");
    project.plan_check().success().stderr("");
    let assert = project.done().success();
    assert!(
        stderr_of(&assert).contains("nothing to verify"),
        "{}",
        stderr_of(&assert)
    );

    let project = Project::new();
    project.write("invar.yaml", "");
    project.approve("skipped", "");
    project.plan_check().success().stderr("");
    project.done().success().stderr("");
}

// --- invar acceptance verify --------------------------------------------------------

impl Project {
    fn verify(&self) -> Assert {
        self.invar(&["acceptance", "verify"])
    }

    fn verify_ticket(&self) -> Assert {
        self.invar(&["acceptance", "verify", "SHOP-214"])
    }
}

#[test]
fn verify_with_no_ticket_counts_the_locked_files_on_one_line() {
    let project = Project::new();
    project.lock_mine();
    project
        .verify()
        .success()
        .stdout("verified 2 locked file(s)\n")
        .stderr("");
}

#[test]
fn a_one_character_change_makes_both_forms_of_verify_refuse_naming_the_file_and_its_ticket() {
    let project = with_one_character_changed();
    refused(
        project.verify(),
        &[OTHERS, "locked for SHOP-1", "changed after it was locked"],
    );
    refused(project.verify_ticket(), &[OTHERS, "locked for SHOP-1"]);
    assert!(project.exists(SPEC));
}

#[test]
fn verify_with_no_ticket_refuses_a_pending_line_of_any_ticket() {
    let project = Project::new();
    project.write(
        "tests/Acceptance/more_test.php",
        &format!("// invar-lock: SHOP-9\n{TEXT}"),
    );
    refused(
        project.verify(),
        &["more_test.php", "pending line for SHOP-9"],
    );
}

#[test]
fn verify_with_no_ticket_works_with_no_git_repository_and_no_plan() {
    let project = Project::new();
    fs::remove_dir_all(project.root().join(".git")).unwrap();
    fs::remove_dir_all(project.root().join(".invar")).unwrap();
    project
        .verify()
        .success()
        .stdout("verified 1 locked file(s)\n")
        .stderr("");

    project.write(MINE, &format!("// invar-lock: SHOP-214\n{TEXT}"));
    refused(project.verify(), &[MINE, "pending line for SHOP-214"]);
}

#[test]
fn verify_with_no_ticket_works_in_a_shallow_clone_after_the_plan_is_gone() {
    let project = Project::new();
    project.lock_mine();
    project.done().success();
    project.commit("the work, with the Plan gone");

    let clone = tempfile::tempdir().unwrap();
    let url = format!("file://{}", project.root().display());
    let output = std::process::Command::new("git")
        .args([
            "clone",
            "-q",
            "--depth",
            "1",
            "--branch",
            "feat/SHOP-214-cancel",
        ])
        .arg(&url)
        .arg(clone.path())
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", project.root().join(".gitconfig-test"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    Command::cargo_bin("invar")
        .unwrap()
        .current_dir(clone.path())
        .args(["acceptance", "verify"])
        .assert()
        .success()
        .stdout("verified 2 locked file(s)\n");
}

#[test]
fn verify_with_a_missing_directory_verifies_nothing() {
    let project = Project::new();
    fs::remove_dir_all(project.root().join("tests")).unwrap();
    project
        .verify()
        .success()
        .stdout("verified 0 locked file(s)\n");
}

#[test]
fn verify_with_a_ticket_makes_the_check_done_makes_and_deletes_nothing() {
    let project = Project::new();
    // No locked test.
    refused(
        project.verify_ticket(),
        &["no file under tests/Acceptance is locked"],
    );

    // A pending line left.
    project.write(MINE, &format!("// invar-lock: SHOP-214\n{TEXT}"));
    refused(
        project.verify_ticket(),
        &[MINE, "pending line for SHOP-214"],
    );

    // Another ticket's test changed, with the digest written by hand.
    project.lock_mine();
    let weaker = project.locked("SHOP-1", "it('AC1 works', fn () => true);\n");
    project.write(OTHERS, &weaker);
    refused(project.verify_ticket(), &[OTHERS, "changed on this branch"]);
    // The form CI runs cannot see that one, which is why the other callers exist (D-17).
    project.verify().success();

    // Declared.
    project.approve("required", &format!("- {OTHERS} — change — the rule moved"));
    project
        .verify_ticket()
        .success()
        .stdout("verified 2 locked file(s)\n");
    assert!(project.exists(SPEC));
    assert_eq!(project.git(&["diff", "--cached", "--name-only"]), "");
}

#[test]
fn verify_with_a_ticket_refuses_a_plan_that_is_not_approved() {
    let project = Project::new();
    project.lock_mine();
    let edited = project
        .read(SPEC)
        .replace("controllers stay thin", "controllers grow");
    project.write(SPEC, &edited);
    refused(project.verify_ticket(), &["changed after it was approved"]);
}

#[test]
fn verify_says_every_problem_at_once() {
    let project = Project::new();
    let changed = project.read(OTHERS).replace("toBeTrue", "toBeFalse");
    project.write(OTHERS, &changed);
    project.write("tests/Acceptance/bad_test.php", "// invar-lock: shop-1\n");
    project.write(
        "tests/Acceptance/more_test.php",
        &format!("// invar-lock: SHOP-9\n{TEXT}"),
    );
    refused(
        project.verify(),
        &[
            "(3 problems)",
            OTHERS,
            "bad_test.php: line 1",
            "more_test.php",
        ],
    );
}

#[test]
fn with_no_section_verify_warns_and_exits_zero_in_both_forms() {
    let project = Project::new();
    project.write("invar.yaml", "");
    for assert in [project.verify(), project.verify_ticket()] {
        let assert = assert.success().stdout("");
        assert!(
            stderr_of(&assert).contains("nothing to verify"),
            "{}",
            stderr_of(&assert)
        );
    }
    // The Plan is not looked for either, since it may be gone by then.
    fs::remove_dir_all(project.root().join(".invar")).unwrap();
    project.verify_ticket().success().stdout("");
}

#[test]
fn verify_works_from_a_subdirectory() {
    let project = Project::new();
    project.lock_mine();
    Command::cargo_bin("invar")
        .unwrap()
        .current_dir(project.root().join("tests/Acceptance"))
        .args(["acceptance", "verify"])
        .assert()
        .success()
        .stdout("verified 2 locked file(s)\n");
}
