//! End-to-end tests for whether locked acceptance tests pass (#156,
//! `acceptance-lock.md` §10.5): `invar acceptance verify --results`, `invar acceptance
//! run` and `invar done --results`. The real binary, on a tiny project outside git, with
//! canned JUnit reports and a fake `run` command that copies one.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use assert_cmd::assert::Assert;
use tempfile::TempDir;

const SPEC: &str = ".invar/plans/SHOP-214-cancel-unpaid-order/spec.md";

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

/// Copies `canned.xml` to `{report}`, and writes the files it was given to `ran-with`.
const RUNNER: &str = "shift_report=\"$1\"; shift; printf '%s\\n' \"$@\" > ran-with\n\
    cp canned.xml \"$shift_report\"\n";

/// Locked for this ticket.
const A: &str = "tests/Acceptance/a_test.php";
const B: &str = "tests/Acceptance/sub/b_test.php";
/// Locked for another ticket.
const OTHER: &str = "tests/Acceptance/other_test.php";
const TEXT: &str = "it('AC1 works', fn () => expect(false)->toBeTrue());\n";

struct Project {
    dir: TempDir,
    temp: TempDir,
}

impl Project {
    /// An approved Plan that says `required`, [`A`] and [`B`] locked for it, and the
    /// fake runner.
    fn new() -> Self {
        let project = Self::bare(CONFIG);
        project.approve("required");
        project.lock(A, "SHOP-214");
        project.lock(B, "SHOP-214");
        project.write("fake-runner.sh", RUNNER);
        project
    }

    fn bare(config: &str) -> Self {
        let project = Self {
            dir: tempfile::tempdir().unwrap(),
            temp: tempfile::tempdir().unwrap(),
        };
        project.write("invar.yaml", config);
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

    fn exists(&self, path: &str) -> bool {
        self.root().join(path).exists()
    }

    fn hash(&self, args: &[&str], text: &str) -> String {
        let file = self.temp.path().join("to-hash.txt");
        fs::write(&file, text).unwrap();
        let output = Command::cargo_bin("invar")
            .unwrap()
            .args(args)
            .arg(&file)
            .output()
            .unwrap();
        fs::remove_file(&file).unwrap();
        String::from_utf8(output.stdout).unwrap().trim().to_owned()
    }

    fn approve(&self, acceptance: &str) {
        let checksum = self.hash(&["hash"], &format!("Acceptance: {acceptance}\n{BODY}"));
        self.write(
            SPEC,
            &format!(
                "---\nTicket: SHOP-214\nModule: orders\nStatus: approved\nApproved-by: rade\n\
                 Approved-at: 2026-10-05\nApproved-checksum: {checksum}\n\
                 Acceptance: {acceptance}\n---\n\n{BODY}"
            ),
        );
    }

    /// `path`, locked for `ticket` with the digest `invar hash --lock` prints.
    fn lock(&self, path: &str, ticket: &str) {
        let pending = format!("<?php\n// invar-lock: {ticket}\n{TEXT}");
        let digest = self.hash(&["hash", "--lock"], &pending);
        self.write(
            path,
            &pending.replace(
                &format!("invar-lock: {ticket}"),
                &format!("invar-lock: {ticket} {digest}"),
            ),
        );
    }

    fn invar(&self, args: &[&str]) -> Assert {
        Command::cargo_bin("invar")
            .unwrap()
            .current_dir(self.root())
            .env("TMPDIR", self.temp.path())
            .env("GIT_CEILING_DIRECTORIES", self.dir.path().parent().unwrap())
            .args(args)
            .assert()
    }

    /// Every file of the project, by path, with its bytes.
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
}

/// A `<testcase>`: `child` is `failure`, `error`, `skipped`, or empty for a pass.
fn case(file: &str, name: &str, child: &str) -> String {
    let inner = match child {
        "" => String::new(),
        "failure" => "<failure type=\"ExpectationFailedException\">Failed asserting that \
                      false is true.\nat a_test.php:3</failure>"
            .to_owned(),
        "error" => "<error type=\"Error\">Class \"App\\Cancel\" not found</error>".to_owned(),
        other => format!("<{other}/>"),
    };
    format!("<testcase name=\"{name}\" file=\"{file}\">{inner}</testcase>\n")
}

fn report(cases: &[String]) -> String {
    format!(
        "<testsuites><testsuite>\n{}</testsuite></testsuites>\n",
        cases.concat()
    )
}

fn green() -> String {
    report(&[case(A, "it AC1 cancels", ""), case(B, "it AC2 refuses", "")])
}

fn stdout_of(assert: &Assert) -> String {
    String::from_utf8(assert.get_output().stdout.clone()).unwrap()
}

fn stderr_of(assert: &Assert) -> String {
    String::from_utf8(assert.get_output().stderr.clone()).unwrap()
}

/// A refusal: non-zero, nothing on stdout, and every part on stderr.
fn refused(assert: Assert, parts: &[&str]) -> String {
    let assert = assert.failure().stdout("");
    let message = stderr_of(&assert);
    for part in parts {
        assert!(message.contains(part), "{part}:\n{message}");
    }
    message
}

// --- verify --results -------------------------------------------------------------------

#[test]
fn verify_results_passes_when_every_locked_test_passed() {
    let project = Project::new();
    project.write("junit.xml", &green());
    project
        .invar(&["acceptance", "verify", "--results", "junit.xml"])
        .success()
        .stdout("verified 2 locked file(s), and 2 locked test(s) passed\n")
        .stderr("");
}

#[test]
fn without_results_verify_prints_exactly_what_it_did_before() {
    let project = Project::new();
    // A failing report beside it changes nothing: it is not read.
    project.write("junit.xml", &report(&[case(A, "it AC1", "failure")]));
    project
        .invar(&["acceptance", "verify"])
        .success()
        .stdout("verified 2 locked file(s)\n")
        .stderr("");
    project.write(A, "<?php\n// invar-lock: SHOP-214 sha256:0000000000000000000000000000000000000000000000000000000000000000\n");
    let message = refused(project.invar(&["acceptance", "verify"]), &[A]);
    assert!(message.starts_with("error: the acceptance locks do not hold (1 problem):\n"));
    assert!(!message.contains("not run by your test suite"), "{message}");
}

#[test]
fn verify_results_fails_on_a_locked_file_missing_from_the_report() {
    let project = Project::new();
    project.write("junit.xml", &report(&[case(A, "it AC1 cancels", "")]));
    refused(
        project.invar(&["acceptance", "verify", "--results", "junit.xml"]),
        &[
            "the locked acceptance tests did not all pass (1 problem):",
            &format!("{B}: not run by your test suite"),
            "Add tests/Acceptance to a test suite",
        ],
    );
}

#[test]
fn verify_results_fails_on_failure_error_and_skipped_naming_file_test_and_criterion() {
    let project = Project::new();
    project.write(
        "junit.xml",
        &report(&[
            case(A, "it AC1 cancels", "failure"),
            case(A, "it AC1 cancels twice", "error"),
            case(B, "it AC2 refuses", "skipped"),
        ]),
    );
    refused(
        project.invar(&["acceptance", "verify", "--results", "junit.xml"]),
        &[
            "(3 problems)",
            &format!("{A}: `it AC1 cancels` (AC1): failure: Failed asserting that false is true."),
            &format!("{A}: `it AC1 cancels twice` (AC1): error: Class \"App\\Cancel\" not found"),
            &format!("{B}: `it AC2 refuses` (AC2): skipped"),
        ],
    );
}

#[test]
fn verify_results_fails_on_a_test_passed_in_one_report_and_failed_in_another() {
    let project = Project::new();
    project.write(
        "first.xml",
        &report(&[case(A, "it AC1 cancels", "failure")]),
    );
    project.write("retry.xml", &green());
    refused(
        project.invar(&[
            "acceptance",
            "verify",
            "--results",
            "first.xml",
            "retry.xml",
        ]),
        &[
            "(1 problem)",
            "`it AC1 cancels` (AC1): passed in one run and failure in another",
            "only on a retry is not accepted",
        ],
    );
}

#[test]
fn verify_results_ignores_test_cases_of_files_that_are_not_locked() {
    let project = Project::new();
    project.write("tests/Acceptance/helper.php", "<?php\n// no lock line\n");
    project.write(
        "junit.xml",
        &report(&[
            case(A, "it AC1 cancels", ""),
            case(B, "it AC2 refuses", ""),
            case("tests/Feature/OrderTest.php", "it fails", "failure"),
            case("tests/Acceptance/helper.php", "it errors", "error"),
        ]),
    );
    project
        .invar(&["acceptance", "verify", "--results", "junit.xml"])
        .success()
        .stdout("verified 2 locked file(s), and 2 locked test(s) passed\n");
}

#[test]
fn verify_results_without_a_ticket_checks_every_ticket_and_with_one_only_its_own() {
    let project = Project::new();
    project.lock(OTHER, "SHOP-1");
    project.write("junit.xml", &green());
    // SHOP-1's file is in no report.
    refused(
        project.invar(&["acceptance", "verify", "--results", "junit.xml"]),
        &[&format!("{OTHER}: not run by your test suite")],
    );
    // With the ticket, only its own; outside git the branch rule only warns.
    let assert = project
        .invar(&["acceptance", "verify", "SHOP-214", "--results", "junit.xml"])
        .success()
        .stdout("verified 3 locked file(s), and 2 locked test(s) passed\n");
    assert!(!stderr_of(&assert).contains(OTHER));
}

#[test]
fn verify_results_maps_paths_under_the_report_root() {
    let project = Project::new();
    let sail = |file: &str| format!("/var/www/html/{file}");
    project.write(
        "junit.xml",
        &report(&[
            case(&sail(A), "it AC1 cancels", ""),
            case(&sail(B), "it AC2 refuses", ""),
        ]),
    );
    // Without it, the path is outside the root, and the message says what to do.
    refused(
        project.invar(&["acceptance", "verify", "--results", "junit.xml"]),
        &[
            "junit.xml",
            "/var/www/html/tests/Acceptance/a_test.php",
            "`--report-root`",
        ],
    );
    project
        .invar(&[
            "acceptance",
            "verify",
            "--results",
            "junit.xml",
            "--report-root",
            "/var/www/html",
        ])
        .success()
        .stdout("verified 2 locked file(s), and 2 locked test(s) passed\n");
}

#[test]
fn verify_results_refuses_a_report_that_is_missing_empty_or_not_xml_naming_each() {
    let project = Project::new();
    project.write("empty.xml", "");
    project.write("html.xml", "this is not xml");
    refused(
        project.invar(&[
            "acceptance",
            "verify",
            "--results",
            "empty.xml",
            "html.xml",
            "nowhere.xml",
        ]),
        &[
            "cannot read the test results, and nothing was checked",
            "empty.xml: the report is empty",
            "html.xml: the report is not well-formed XML",
            "nowhere.xml: there is no such file",
        ],
    );
}

#[test]
fn a_report_root_needs_results_and_an_absolute_path() {
    let project = Project::new();
    project.write("junit.xml", &green());
    refused(
        project.invar(&["acceptance", "verify", "--report-root", "/var/www/html"]),
        &["--results"],
    );
    refused(
        project.invar(&[
            "acceptance",
            "verify",
            "--results",
            "junit.xml",
            "--report-root",
            "var/www/html",
        ]),
        &["is not an absolute path"],
    );
}

#[test]
fn verify_results_checks_the_locks_first() {
    let project = Project::new();
    project.write("junit.xml", &green());
    project.write(A, "<?php\n// invar-lock: SHOP-214 sha256:0000000000000000000000000000000000000000000000000000000000000000\n");
    let message = refused(
        project.invar(&["acceptance", "verify", "--results", "junit.xml"]),
        &["the acceptance locks do not hold", A],
    );
    assert!(!message.contains("did not all pass"), "{message}");
}

// --- acceptance run ---------------------------------------------------------------------

#[test]
fn run_green_prints_each_criterion_and_the_line_to_key_on() {
    let project = Project::new();
    project.write("canned.xml", &green());
    project
        .invar(&["acceptance", "run", "SHOP-214"])
        .success()
        .stdout("AC1: passed\nAC2: passed\nacceptance: green SHOP-214\n")
        .stderr("");
}

#[test]
fn run_red_says_what_did_not_pass_and_exits_non_zero() {
    let project = Project::new();
    project.write(
        "canned.xml",
        &report(&[
            case(A, "it AC1 cancels", ""),
            case(B, "it AC2 refuses", "failure"),
        ]),
    );
    let assert = project.invar(&["acceptance", "run", "SHOP-214"]).failure();
    assert_eq!(
        stdout_of(&assert),
        format!(
            "AC1: passed\nAC2: not passed\n  {B}: `it AC2 refuses` (AC2): failure: Failed \
             asserting that false is true.\nacceptance: red SHOP-214\n"
        )
    );
    assert!(stderr_of(&assert).contains("did not all pass"));
}

#[test]
fn run_red_on_skipped_and_on_a_criterion_no_test_ran() {
    let project = Project::new();
    project.write(
        "canned.xml",
        &report(&[case(A, "it AC1 cancels", "skipped")]),
    );
    let assert = project.invar(&["acceptance", "run", "SHOP-214"]).failure();
    let out = stdout_of(&assert);
    assert!(out.contains("AC1: not passed\n"), "{out}");
    assert!(out.contains("(AC1): skipped"), "{out}");
    assert!(
        out.contains("AC2: not passed\n  no test of AC2 ran\n"),
        "{out}"
    );
    assert!(
        out.contains(&format!("  {B}: not run by your test suite")),
        "{out}"
    );
    assert!(out.ends_with("acceptance: red SHOP-214\n"), "{out}");
}

#[test]
fn run_passes_only_the_tickets_locked_files_and_writes_nothing_else() {
    let project = Project::new();
    project.lock(OTHER, "SHOP-1");
    project.write(
        "tests/Acceptance/pending_test.php",
        "<?php\n// invar-lock: SHOP-214\nit('AC1 new', fn () => true);\n",
    );
    project.write("tests/Acceptance/helper.php", "<?php\n");
    project.write("canned.xml", &green());
    let before = project.tree();
    project.invar(&["acceptance", "run", "SHOP-214"]).success();
    let ran_with = fs::read_to_string(project.root().join("ran-with")).unwrap();
    assert_eq!(ran_with, format!("{A}\n{B}\n"));
    // The runner's own file is the only change, and the report did not stay.
    let mut after = project.tree();
    after.remove(&project.root().join("ran-with"));
    assert_eq!(after, before);
    assert!(fs::read_dir(project.temp.path()).unwrap().next().is_none());
}

#[test]
fn run_refuses_a_draft_before_anything_runs() {
    let project = Project::new();
    project.write(
        SPEC,
        &format!(
            "---\nTicket: SHOP-214\nModule: orders\nStatus: draft\nApproved-by:\nApproved-at:\n\
             Approved-checksum:\n---\n\n{BODY}"
        ),
    );
    refused(
        project.invar(&["acceptance", "run", "SHOP-214"]),
        &["draft"],
    );
    assert!(!project.exists("ran-with"));
}

#[test]
fn run_refuses_a_plan_changed_since_approval() {
    let project = Project::new();
    let spec = project.root().join(SPEC);
    let edited = fs::read_to_string(&spec)
        .unwrap()
        .replace("is refused", "is refused politely");
    fs::write(&spec, edited).unwrap();
    refused(
        project.invar(&["acceptance", "run", "SHOP-214"]),
        &["changed"],
    );
    assert!(!project.exists("ran-with"));
}

#[test]
fn run_refuses_a_skipped_ticket() {
    let project = Project::new();
    project.approve("skipped");
    refused(
        project.invar(&["acceptance", "run", "SHOP-214"]),
        &["approved without acceptance tests"],
    );
    assert!(!project.exists("ran-with"));
}

#[test]
fn run_refuses_with_no_acceptance_section() {
    let project = Project::new();
    project.write("invar.yaml", "");
    refused(
        project.invar(&["acceptance", "run", "SHOP-214"]),
        &["no `acceptance:` section"],
    );
}

#[test]
fn run_refuses_a_ticket_with_no_locked_file() {
    let project = Project::bare(CONFIG);
    project.approve("required");
    project.write("fake-runner.sh", RUNNER);
    project.write(
        "tests/Acceptance/pending_test.php",
        "<?php\n// invar-lock: SHOP-214\nit('AC1 new', fn () => true);\n",
    );
    project.lock(OTHER, "SHOP-1");
    refused(
        project.invar(&["acceptance", "run", "SHOP-214"]),
        &["no file under tests/Acceptance is locked for it"],
    );
    assert!(!project.exists("ran-with"));
}

#[test]
fn run_refuses_a_locked_file_that_changed() {
    let project = Project::new();
    let path = project.root().join(A);
    let text = fs::read_to_string(&path).unwrap() + "// one more line\n";
    fs::write(&path, text).unwrap();
    refused(
        project.invar(&["acceptance", "run", "SHOP-214"]),
        &[A, "it changed after it was locked"],
    );
    assert!(!project.exists("ran-with"));
}

#[test]
fn run_with_no_report_says_the_tests_did_not_run() {
    let project = Project::new();
    project.write("fake-runner.sh", "echo boom >&2\nexit 3\n");
    refused(
        project.invar(&["acceptance", "run", "SHOP-214"]),
        &["the tests did not run", "no report was written", "boom"],
    );
}

// --- done --results ---------------------------------------------------------------------

#[test]
fn done_results_refuses_on_red_and_deletes_nothing() {
    let project = Project::new();
    project.write("junit.xml", &report(&[case(A, "it AC1 cancels", "")]));
    let before = project.tree();
    refused(
        project.invar(&["done", "SHOP-214", "--results", "junit.xml"]),
        &[
            &format!("{B}: not run by your test suite"),
            "Nothing was changed",
        ],
    );
    assert_eq!(project.tree(), before);
}

#[test]
fn done_results_closes_on_green() {
    let project = Project::new();
    project.write("junit.xml", &green());
    project
        .invar(&["done", "SHOP-214", "--results", "junit.xml"])
        .success();
    assert!(!project.exists(SPEC));
}

#[test]
fn done_without_results_reads_no_report() {
    let project = Project::new();
    project.write(
        "junit.xml",
        &report(&[case(A, "it AC1 cancels", "failure")]),
    );
    project.invar(&["done", "SHOP-214"]).success();
    assert!(!project.exists(SPEC));
}
