//! End-to-end tests for `invar acceptance unlock` (AT-13): the real binary, on a tiny
//! project, with files locked by the real `invar acceptance lock` and a fake `run`
//! command that copies a canned JUnit report.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use assert_cmd::assert::Assert;
use predicates::prelude::*;
use predicates::str::{contains, starts_with};
use tempfile::TempDir;

const PLAN_DIR: &str = ".invar/plans/SHOP-214-cancel-unpaid-order";
const SPEC: &str = ".invar/plans/SHOP-214-cancel-unpaid-order/spec.md";
const RECORD: &str = ".invar/plans/SHOP-214-cancel-unpaid-order/unlocked";

const CONFIG: &str = "acceptance:\n  dir: tests/Acceptance\n  \
    run: [\"sh\", \"fake-runner.sh\", \"{report}\", \"{files}\"]\n";

const BODY: &str = "## Ticket\nSHOP-214 — Cancel an unpaid order.\n\n\
    ## Intent\nA customer can cancel an order they have not paid for.\n\n\
    ## Scope\n`Orders` only.\n\n\
    ## Open questions / answers\nNone.\n\n\
    ## Rejected alternatives\nNone.\n\n\
    ## Acceptance criteria\n\n\
    - AC1: Given an unpaid order, when its owner cancels it, then it is cancelled.\n\
    - AC2: Given a paid order, when its owner cancels it, then it is refused.\n\n\
    ## Implementation context\n- AI_CONTEXT: controllers stay thin\n";

const A: &str = "tests/Acceptance/a_test.php";
const B: &str = "tests/Acceptance/sub/b_test.php";
const OTHERS: &str = "tests/Acceptance/others_test.php";
/// The pending text of [`A`] and [`B`], which is what an unlocked file goes back to.
const PENDING: &str = "<?php\n// invar-lock: SHOP-214 written by the test-writer\n\
    it('works', fn () => expect(false)->toBeTrue());\n";

struct Project {
    dir: TempDir,
    temp: TempDir,
}

impl Project {
    /// An approved Plan, and [`A`] and [`B`] locked by the real command.
    fn new() -> Self {
        let project = Self::approved("required");
        project.write(A, PENDING);
        project.write(B, PENDING);
        project.write("fake-runner.sh", "cp canned.xml \"$1\"\n");
        project.write(
            "canned.xml",
            &report(&[
                case(A, "it AC1 cancels", "failure"),
                case(B, "it AC2 refuses", "failure"),
            ]),
        );
        project.run(&["acceptance", "lock", "SHOP-214"]).success();
        project
    }

    fn approved(acceptance: &str) -> Self {
        let project = Self {
            dir: tempfile::tempdir().unwrap(),
            temp: tempfile::tempdir().unwrap(),
        };
        project.write("invar.yaml", CONFIG);
        let checksum = project.hash_of(&format!("Acceptance: {acceptance}\n{BODY}"));
        project.write(
            SPEC,
            &format!(
                "---\nTicket: SHOP-214\nModule: orders\nStatus: approved\nApproved-by: rade\n\
                 Approved-at: 2026-10-05\nApproved-checksum: {checksum}\n\
                 Acceptance: {acceptance}\n---\n\n{BODY}"
            ),
        );
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

    fn hash_of(&self, text: &str) -> String {
        let file = self.temp.path().join("to-hash.txt");
        fs::write(&file, text).unwrap();
        let output = Command::cargo_bin("invar")
            .unwrap()
            .arg("hash")
            .arg(&file)
            .output()
            .unwrap();
        fs::remove_file(&file).unwrap();
        String::from_utf8(output.stdout).unwrap().trim().to_owned()
    }

    fn run(&self, args: &[&str]) -> Assert {
        Command::cargo_bin("invar")
            .unwrap()
            .current_dir(self.root())
            .env("TMPDIR", self.temp.path())
            .args(args)
            .assert()
    }

    fn unlock(&self, files: &[&str]) -> Assert {
        let mut args = vec!["acceptance", "unlock", "SHOP-214"];
        args.extend_from_slice(files);
        self.run(&args)
    }

    fn tree(&self) -> BTreeMap<PathBuf, Vec<u8>> {
        fn walk(dir: &Path, tree: &mut BTreeMap<PathBuf, Vec<u8>>) {
            for entry in fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    walk(&path, tree);
                } else {
                    tree.insert(path.clone(), fs::read(&path).unwrap());
                }
            }
        }
        let mut tree = BTreeMap::new();
        walk(&self.root(), &mut tree);
        tree
    }

    /// A refusal: non-zero, nothing on stdout, every part on stderr, and not one byte of
    /// the project changed.
    fn refuses(&self, files: &[&str], parts: &[&str]) {
        let before = self.tree();
        let assert = self.unlock(files).failure().stdout("");
        let message = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
        for part in parts {
            assert!(message.contains(part), "{part}:\n{message}");
        }
        assert_eq!(self.tree(), before, "a refusal changed a file:\n{message}");
    }

    /// Lock [`A`] again, as the test-writer would, so that it can be unlocked again.
    fn relock_a(&self) {
        self.run(&["acceptance", "lock", "SHOP-214"]).success();
    }
}

fn case(file: &str, name: &str, child: &str) -> String {
    let inner = if child.is_empty() {
        String::new()
    } else {
        format!("<{child}/>")
    };
    format!("<testcase name=\"{name}\" file=\"{file}\">{inner}</testcase>\n")
}

fn report(cases: &[String]) -> String {
    format!(
        "<testsuites><testsuite>\n{}</testsuite></testsuites>\n",
        cases.concat()
    )
}

// --- what it writes ---------------------------------------------------------------------------

#[test]
fn the_line_goes_back_to_pending_and_nothing_else_in_the_file_changes() {
    let project = Project::new();
    assert!(project.read(A).contains("sha256:"));
    let b = project.read(B);

    project
        .unlock(&[A])
        .success()
        .stdout(contains(format!("unlocked: {A}\n")))
        .stdout(predicate::str::ends_with(
            "unlocked SHOP-214 1 file(s), round 1 of 2\n",
        ));

    assert_eq!(project.read(A), PENDING);
    assert_eq!(project.read(B), b, "a file not named stays locked");
}

#[test]
fn the_record_holds_the_files_and_the_round() {
    let project = Project::new();
    project.unlock(&[A, B]).success();
    assert_eq!(
        project.read(RECORD),
        format!("round: 1\nopen 1 {A}\nopen 1 {B}\n")
    );
    // No temporary file stays beside it.
    let names: Vec<String> = fs::read_dir(project.root().join(PLAN_DIR))
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names.len(), 2, "{names:?}");
}

#[test]
fn a_file_may_be_named_from_anywhere_and_twice_and_counts_once() {
    let project = Project::new();
    let absolute = project.root().join(A).display().to_string();
    project
        .unlock(&[&format!("./{A}"), &absolute])
        .success()
        .stdout(contains("unlocked SHOP-214 1 file(s), round 1 of 2"));
    assert_eq!(project.read(RECORD), format!("round: 1\nopen 1 {A}\n"));

    let project = Project::new();
    Command::cargo_bin("invar")
        .unwrap()
        .current_dir(project.root().join("tests/Acceptance"))
        .args(["acceptance", "unlock", "SHOP-214", "a_test.php"])
        .assert()
        .success();
    assert_eq!(project.read(RECORD), format!("round: 1\nopen 1 {A}\n"));
}

#[test]
fn the_second_round_is_allowed_and_the_third_refused_saying_how_to_end_the_cycle() {
    let project = Project::new();
    project.unlock(&[A]).success();
    project.relock_a();
    project
        .unlock(&[A])
        .success()
        .stdout(contains("unlocked SHOP-214 1 file(s), round 2 of 2"));
    project.relock_a();

    project.refuses(
        &[A],
        &[
            "both rounds",
            "`failed`",
            "`/invar:log`",
            "nothing was unlocked",
        ],
    );
    assert!(project.read(A).contains("sha256:"));
}

// --- what it refuses --------------------------------------------------------------------------

#[test]
fn no_file_named_is_refused_and_names_the_ticket() {
    let project = Project::new();
    project.refuses(&[], &["name the files", "SHOP-214"]);
}

#[test]
fn a_draft_plan_is_refused() {
    let project = Project::new();
    let text = project
        .read(SPEC)
        .replace("Status: approved", "Status: draft");
    project.write(SPEC, &text);
    project.refuses(&[A], &["draft"]);
}

#[test]
fn a_plan_changed_since_approval_is_refused() {
    let project = Project::new();
    let text = project
        .read(SPEC)
        .replace("it is refused", "it is refused quietly");
    project.write(SPEC, &text);
    project.refuses(&[A], &["changed"]);
}

#[test]
fn a_skipped_ticket_is_refused() {
    let project = Project::approved("skipped");
    project.write(A, PENDING);
    project.refuses(&[A], &["approved without acceptance tests"]);
}

#[test]
fn a_pending_file_is_refused() {
    let project = Project::new();
    project.unlock(&[A]).success();
    project.refuses(&[A], &[A, "it is pending"]);
}

#[test]
fn another_tickets_file_is_refused() {
    let project = Project::new();
    project.write(
        OTHERS,
        "<?php\n// invar-lock: SHOP-300 sha256:5d5da6ccd3116bee608b2a6c15d4c9f0d20f32b17b220cc13a643169ab874e07\n",
    );
    project.refuses(&[OTHERS], &[OTHERS, "it belongs to SHOP-300"]);
}

#[test]
fn a_tampered_file_is_refused() {
    let project = Project::new();
    let text = project.read(A) + "// a later edit\n";
    project.write(A, &text);
    project.refuses(&[A], &[A, "it changed after it was locked"]);
}

#[test]
fn a_path_outside_the_acceptance_dir_is_refused() {
    let project = Project::new();
    project.write("app/Order.php", "<?php\n");
    project.refuses(
        &["app/Order.php"],
        &["app/Order.php", "not under `tests/Acceptance`"],
    );
    project.refuses(&["../elsewhere.php"], &["outside the project"]);
    project.refuses(&["/etc/passwd"], &["outside the project"]);
    project.refuses(&["tests/Acceptance/../../app/Order.php"], &["not under"]);
}

#[test]
fn a_file_with_no_line_and_a_file_that_is_not_there_are_refused() {
    let project = Project::new();
    project.write("tests/Acceptance/helper.php", "<?php\n");
    project.refuses(&["tests/Acceptance/helper.php"], &["no `invar-lock:` line"]);
    project.refuses(&["tests/Acceptance/none.php"], &["no such file"]);
}

#[test]
fn every_problem_is_named_and_a_good_file_beside_a_bad_one_stays_locked() {
    let project = Project::new();
    project.refuses(
        &[A, "tests/Acceptance/none.php", "app/Order.php"],
        &[
            "tests/Acceptance/none.php",
            "app/Order.php",
            "nothing was unlocked",
        ],
    );
    assert!(project.read(A).contains("sha256:"));
    assert!(!project.root().join(RECORD).exists());
}

#[test]
fn a_record_that_cannot_be_read_is_refused_and_grants_nothing() {
    let project = Project::new();
    project.write(RECORD, "round: 9\n");
    project.refuses(&[A], &["cannot read the unlock record", "round: 9"]);
}

#[cfg(unix)]
#[test]
fn a_record_that_is_a_link_is_refused() {
    let project = Project::new();
    project.write("elsewhere", "round: 1\n");
    std::os::unix::fs::symlink(
        project.root().join("elsewhere"),
        project.root().join(RECORD),
    )
    .unwrap();
    project.refuses(
        &[A],
        &["cannot read the unlock record", "not a regular file"],
    );
}

#[cfg(unix)]
#[test]
fn a_test_reached_through_a_link_is_refused() {
    let project = Project::new();
    std::os::unix::fs::symlink(
        project.root().join("tests/Acceptance/sub"),
        project.root().join("tests/Acceptance/link"),
    )
    .unwrap();
    project.refuses(
        &["tests/Acceptance/link/b_test.php"],
        &["reached through a link", "nothing"],
    );
}

// --- --skill ----------------------------------------------------------------------------------

#[test]
fn under_skill_every_refusal_is_an_error_line_with_exit_0() {
    let project = Project::new();
    let before = project.tree();
    for args in [
        vec!["acceptance", "unlock", "--skill", "SHOP-214", "none.php"],
        vec!["acceptance", "unlock", "--skill", "SHOP-214"],
        vec!["acceptance", "unlock", "--skill", "SHOP-999", A],
        // What clap refuses, too.
        vec!["acceptance", "unlock", "--skill"],
        vec!["acceptance", "unlock", "--skill", "SHOP-214", A, "--bogus"],
    ] {
        project
            .run(&args)
            .success()
            .stdout(starts_with("error: "))
            .stdout(contains("round").not());
    }
    assert_eq!(project.tree(), before);
}

#[test]
fn under_skill_the_success_line_is_the_same() {
    let project = Project::new();
    project
        .run(&["acceptance", "unlock", "--skill", "SHOP-214", A])
        .success()
        .stdout(contains("unlocked SHOP-214 1 file(s), round 1 of 2\n"));
}

#[test]
fn an_unknown_flag_without_skill_stays_a_failure_on_stderr() {
    let project = Project::new();
    project
        .run(&["acceptance", "unlock", "SHOP-214", A, "--bogus"])
        .failure()
        .code(1)
        .stdout("")
        .stderr(contains("--bogus"));
}

// --- `lock` with a record ---------------------------------------------------------------------

impl Project {
    fn lock(&self) -> Assert {
        self.run(&["acceptance", "lock", "SHOP-214"])
    }

    /// What the next run reports, for [`A`] and [`B`].
    fn outcomes(&self, a: &str, b: &str) {
        self.write(
            "canned.xml",
            &report(&[case(A, "it AC1 cancels", a), case(B, "it AC2 refuses", b)]),
        );
    }
}

const SKIPPED_A: &str =
    "red check skipped for tests/Acceptance/a_test.php (unlocked by the developer, round 1)\n";

#[test]
fn an_unlocked_file_that_now_passes_locks_and_the_output_names_it() {
    let project = Project::new();
    project.unlock(&[A]).success();
    project.outcomes("", "");

    project
        .lock()
        .success()
        .stdout(contains(SKIPPED_A))
        .stdout(contains("sub/b_test.php (unlocked").not())
        .stdout(predicate::str::ends_with("locked SHOP-214 1 file(s)\n"));
    assert!(project.read(A).contains("sha256:"));
}

#[test]
fn an_unlocked_file_that_still_fails_on_an_assertion_locks_too() {
    let project = Project::new();
    project.unlock(&[A]).success();
    project.outcomes("failure", "");
    project.lock().success().stdout(contains(SKIPPED_A));
}

#[test]
fn an_unlocked_file_with_an_error_or_a_skip_is_refused_and_stays_pending() {
    for outcome in ["error", "skipped"] {
        let project = Project::new();
        project.unlock(&[A]).success();
        project.outcomes(outcome, "");
        let record = project.read(RECORD);

        project.lock().failure().stdout("").stderr(contains(A));
        assert_eq!(project.read(A), PENDING, "{outcome}");
        assert_eq!(
            project.read(RECORD),
            record,
            "{outcome}: a refusal keeps it"
        );
    }
}

#[test]
fn the_token_checks_and_coverage_still_apply_to_an_unlocked_file() {
    let project = Project::new();
    project.unlock(&[A]).success();
    project.write(
        "canned.xml",
        &report(&[
            case(A, "it cancels with no token", ""),
            case(B, "it AC2 refuses", ""),
        ]),
    );
    project
        .lock()
        .failure()
        .stderr(contains("no `AC<n>` token"))
        .stderr(contains("no test covers"));
    assert_eq!(project.read(A), PENDING);
}

#[test]
fn another_file_in_the_same_run_still_gets_the_red_check() {
    let project = Project::new();
    project.unlock(&[A]).success();
    let new = "tests/Acceptance/new_test.php";
    project.write(new, PENDING);
    project.write(
        "canned.xml",
        &report(&[
            case(A, "it AC1 cancels", ""),
            case(B, "it AC2 refuses", ""),
            case(new, "it AC2 also refuses", ""),
        ]),
    );
    project
        .lock()
        .failure()
        .stdout("")
        .stderr(contains(new))
        .stderr(contains("passes before the code exists"));
    assert_eq!(project.read(A), PENDING, "nothing is locked on a refusal");

    // Red, it locks, beside the file that passed.
    project.write(
        "canned.xml",
        &report(&[
            case(A, "it AC1 cancels", ""),
            case(B, "it AC2 refuses", ""),
            case(new, "it AC2 also refuses", "failure"),
        ]),
    );
    project
        .lock()
        .success()
        .stdout(contains(SKIPPED_A))
        .stdout(contains("new_test.php (unlocked").not())
        .stdout(contains("locked SHOP-214 2 file(s)\n"));
}

#[test]
fn a_pending_file_the_record_does_not_list_gets_the_red_check() {
    // Taking the digest off by hand grants nothing.
    let project = Project::new();
    project.unlock(&[B]).success();
    project.write(A, PENDING);
    project.outcomes("", "");
    project
        .lock()
        .failure()
        .stderr(contains(A))
        .stderr(contains("passes before the code exists"));
}

#[test]
fn the_record_is_updated_once_locked_and_the_file_is_unlocked_once() {
    let project = Project::new();
    project.unlock(&[A]).success();
    project.outcomes("", "");
    project.lock().success();
    assert_eq!(project.read(RECORD), format!("round: 1\nclosed 1 {A}\n"));

    // Taking the digest off again by hand, now that the entry is closed, grants nothing.
    project.write(A, PENDING);
    project
        .lock()
        .failure()
        .stderr(contains("passes before the code exists"));
}

#[test]
fn an_open_entry_whose_file_is_not_pending_is_ignored_with_a_warning_and_closed() {
    for gone in [false, true] {
        let project = Project::new();
        let locked = project.read(A);
        project.unlock(&[A, B]).success();
        if gone {
            fs::remove_file(project.root().join(A)).unwrap();
            project.write(
                "canned.xml",
                &report(&[case(B, "it AC2 refuses", ""), case(B, "it AC1 cancels", "")]),
            );
        } else {
            project.write(A, &locked);
            project.outcomes("", "");
        }

        project
            .lock()
            .success()
            .stderr(contains(format!("warning: {A}: in the unlock record")))
            .stdout(contains(format!("red check skipped for {B}")))
            .stdout(contains("a_test.php (unlocked").not());
        let record = project.read(RECORD);
        assert!(!record.contains("open "), "{record}");
    }
}

#[test]
fn a_record_that_cannot_be_read_stops_lock_and_runs_nothing() {
    let project = Project::new();
    project.unlock(&[A]).success();
    project.write(RECORD, "round: 1\nopen 1\n");
    project.write("fake-runner.sh", "touch ran\n");
    project
        .lock()
        .failure()
        .stdout("")
        .stderr(contains("cannot read the unlock record"));
    assert!(!project.root().join("ran").exists());
    assert_eq!(project.read(A), PENDING);
}

#[test]
fn a_ticket_with_no_record_writes_none() {
    let project = Project::new();
    assert!(!project.root().join(RECORD).exists());
}

// --- who says so ------------------------------------------------------------------------------

#[test]
fn spec_prints_the_unlocked_files_only_while_they_wait() {
    let project = Project::new();
    let spec = || {
        let assert = project.run(&["acceptance", "spec", "SHOP-214"]).success();
        String::from_utf8(assert.get_output().stdout.clone()).unwrap()
    };
    let before = spec();
    assert!(!before.contains("=== Unlocked tests ==="), "{before}");

    project.unlock(&[A]).success();
    let out = spec();
    let section = out.split_once("=== Unlocked tests ===").unwrap().1;
    assert!(section.contains(&format!("- {A} (round 1 of 2)")), "{out}");
    assert!(!section.contains("b_test.php"), "{out}");
    assert!(section.contains("Fix exactly these files"), "{out}");
    assert!(section.contains("word for word"), "{out}");
    assert!(
        section.contains("Do not weaken them to pass the code"),
        "{out}"
    );
    // The coder may have stopped before writing any code, and the last step says so too.
    assert!(
        section.contains("The code may or may not exist yet"),
        "{out}"
    );
    assert!(
        section.contains("every test outside the unlocked files fails on an assertion"),
        "{out}"
    );
    assert!(
        out.find("=== Unlocked tests ===") < out.find("=== Next step ==="),
        "{out}"
    );

    project.outcomes("", "");
    project.lock().success();
    assert_eq!(spec(), before, "once locked, the spec is what it was");
}

#[test]
fn verify_says_which_files_were_unlocked_on_stderr_and_keeps_its_one_line() {
    let project = Project::new();
    project
        .run(&["acceptance", "verify", "SHOP-214"])
        .success()
        .stdout("verified 2 locked file(s)\n")
        .stderr(contains("unlocked by the developer").not());

    project.unlock(&[A]).success();
    // Pending: `verify` refuses, as for any pending line (§10.3).
    project
        .run(&["acceptance", "verify", "SHOP-214"])
        .failure()
        .stderr(contains("a pending line for SHOP-214"));

    project.outcomes("", "");
    project.lock().success();
    project
        .run(&["acceptance", "verify", "SHOP-214"])
        .success()
        .stdout("verified 2 locked file(s)\n")
        .stderr(contains(format!(
            "unlocked by the developer: {A} (round 1)\n"
        )));
    // With no ticket there is no Plan to read a record beside.
    project
        .run(&["acceptance", "verify"])
        .success()
        .stderr(contains("unlocked by the developer").not());
}

#[test]
fn done_says_which_files_were_unlocked_and_the_record_goes_with_the_plan() {
    let project = Project::new();
    project.unlock(&[A]).success();
    project.outcomes("", "");
    project.lock().success();

    project
        .run(&["done", "SHOP-214"])
        .success()
        .stdout(contains(format!(
            "unlocked by the developer: {A} (round 1)\nremoved: "
        )));
    assert!(!project.root().join(PLAN_DIR).exists());
}

#[test]
fn done_refuses_an_unlocked_file_that_was_never_locked_again() {
    let project = Project::new();
    project.unlock(&[A]).success();
    project
        .run(&["done", "SHOP-214"])
        .failure()
        .stderr(contains("a pending line for SHOP-214"));
    assert!(project.root().join(RECORD).exists());
}

#[test]
fn a_record_that_cannot_be_read_is_a_warning_for_verify_and_done() {
    let project = Project::new();
    project.write(RECORD, "nonsense\n");
    project
        .run(&["acceptance", "verify", "SHOP-214"])
        .success()
        .stdout("verified 2 locked file(s)\n")
        .stderr(contains("warning: cannot read the unlock record"));
    project
        .run(&["done", "SHOP-214"])
        .success()
        .stderr(contains("warning: cannot read the unlock record"));
}
