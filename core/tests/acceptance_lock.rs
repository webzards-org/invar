//! End-to-end tests for `invar acceptance lock` (AT-8): the real binary, on a tiny
//! project, with a fake `run` command that copies a canned JUnit report.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use assert_cmd::assert::Assert;
use predicates::str::contains;
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

/// Copies `canned.xml` to `{report}`.
const COPY: &str = "cp canned.xml \"$1\"\n";

const A: &str = "tests/Acceptance/a_test.php";
const B: &str = "tests/Acceptance/sub/b_test.php";
const PENDING: &str = "<?php\n// invar-lock: SHOP-214 written by the test-writer\n\
    it('works', fn () => expect(false)->toBeTrue());\n";

struct Project {
    dir: TempDir,
    /// `TMPDIR` for the command: the report goes here, and must not stay.
    temp: TempDir,
}

impl Project {
    /// An approved Plan, the fake runner, and the two files of [`A`] and [`B`].
    fn new() -> Self {
        let project = Self::bare(CONFIG);
        project.approve("required");
        project.runner(COPY);
        project.write(A, PENDING);
        project.write(B, PENDING);
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

    fn runner(&self, script: &str) {
        self.write("fake-runner.sh", script);
    }

    fn canned(&self, xml: &str) {
        self.write("canned.xml", xml);
    }

    /// The digest `invar hash` gives `text`, as an independent `Approved-checksum`.
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

    fn approve(&self, acceptance: &str) {
        let checksum = self.hash_of(&format!("Acceptance: {acceptance}\n{BODY}"));
        self.write(
            SPEC,
            &format!(
                "---\nTicket: SHOP-214\nModule: orders\nStatus: approved\nApproved-by: rade\n\
                 Approved-at: 2026-10-05\nApproved-checksum: {checksum}\n\
                 Acceptance: {acceptance}\n---\n\n{BODY}"
            ),
        );
    }

    fn lock(&self) -> Assert {
        Command::cargo_bin("invar")
            .unwrap()
            .current_dir(self.root())
            .env("TMPDIR", self.temp.path())
            .args(["acceptance", "lock", "SHOP-214"])
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

    fn temp_is_empty(&self) -> bool {
        fs::read_dir(self.temp.path()).unwrap().next().is_none()
    }

    /// A refusal: non-zero, nothing on stdout, every part on stderr, not one byte of
    /// the project changed, and no report left behind. Returns the message.
    fn refuses(&self, parts: &[&str]) -> String {
        let before = self.tree();
        let assert = self.lock().failure().stdout("");
        let message = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
        for part in parts {
            assert!(message.contains(part), "{part}:\n{message}");
        }
        assert_eq!(self.tree(), before, "a refusal changed a file:\n{message}");
        assert!(self.temp_is_empty(), "the report stayed:\n{message}");
        message
    }
}

/// A `<testcase>`: `child` is `failure`, `error`, `skipped`, or empty for a pass.
fn case(file: &str, name: &str, child: &str) -> String {
    let inner = match child {
        "" => String::new(),
        "error" => "<error type=\"Error\">Class \"App\\Cancel\" not found\nat a.php:3\n\
                    third\nfourth\nfifth\nsixth line is cut</error>"
            .to_owned(),
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

/// Every test red on an assertion, with both criteria covered.
fn all_red() -> String {
    report(&[
        case(A, "it AC1 cancels", "failure"),
        case(B, "it AC2 refuses", "failure"),
    ])
}

// --- the refusal chain ----------------------------------------------------------

#[test]
fn a_draft_is_refused_before_anything_runs() {
    let project = Project::new();
    project.write(
        SPEC,
        &format!(
            "---\nTicket: SHOP-214\nModule: orders\nStatus: draft\nApproved-by:\nApproved-at:\n\
             Approved-checksum:\n---\n\n{BODY}"
        ),
    );
    // A runner that would be seen to run.
    project.runner("touch ran\n");
    project.refuses(&["draft"]);
    assert!(!project.root().join("ran").exists());
}

#[test]
fn a_plan_changed_since_approval_is_refused() {
    let project = Project::new();
    let spec = project.root().join(SPEC);
    let edited = fs::read_to_string(&spec)
        .unwrap()
        .replace("refused.", "denied.");
    fs::write(spec, edited).unwrap();
    project.refuses(&["changed"]);
}

#[test]
fn skipped_is_refused_in_the_words_of_acceptance_spec() {
    let project = Project::new();
    project.approve("skipped");
    project.refuses(&[
        "approved without acceptance tests",
        "--reapprove --acceptance",
    ]);
}

#[test]
fn no_section_is_refused() {
    let project = Project::new();
    project.write("invar.yaml", "");
    project.refuses(&["no `acceptance:` section"]);
}

#[test]
fn criteria_without_ids_are_refused() {
    let project = Project::new();
    let body = BODY.replace("- AC1: ", "- ").replace("- AC2: ", "- ");
    let checksum = project.hash_of(&format!("Acceptance: required\n{body}"));
    project.write(
        SPEC,
        &format!(
            "---\nTicket: SHOP-214\nModule: orders\nStatus: approved\nApproved-by: rade\n\
             Approved-at: 2026-10-05\nApproved-checksum: {checksum}\n\
             Acceptance: required\n---\n\n{body}"
        ),
    );
    project.refuses(&["no ids"]);
}

// --- finding the files ----------------------------------------------------------

#[test]
fn nothing_to_lock_is_refused() {
    let project = Project::bare(CONFIG);
    project.approve("required");
    project.write(
        "tests/Acceptance/other_test.php",
        "// invar-lock: SHOP-999\n",
    );
    project.refuses(&[
        "nothing to lock for SHOP-214 under tests/Acceptance",
        "`invar-lock: SHOP-214`",
    ]);
}

#[test]
fn a_missing_directory_is_refused_naming_it() {
    let project = Project::bare(CONFIG);
    project.approve("required");
    project.refuses(&["nothing to lock for SHOP-214 under tests/Acceptance"]);
}

#[test]
fn a_malformed_line_is_refused_naming_the_file_before_anything_runs() {
    let project = Project::new();
    project.runner("touch ran\n");
    project.write(A, "// invar-lock: shop-214\n");
    project.refuses(&[A, "line 1"]);
    assert!(!project.root().join("ran").exists());
}

// --- the run --------------------------------------------------------------------

#[test]
fn the_files_come_relative_sorted_and_separate_and_the_report_stays_out_of_the_project() {
    let project = Project::new();
    // A space in a name stays one argument: there is no shell.
    let spaced = "tests/Acceptance/c d_test.php";
    project.write(spaced, PENDING);
    project.runner(
        "printf '%s\\n' \"$@\" > \"$(dirname \"$1\")/../argv.log\"\ncp canned.xml \"$1\"\n",
    );
    project.canned(&report(&[
        case(A, "it AC1 cancels", "failure"),
        case(B, "it AC2 refuses", "failure"),
        case(spaced, "it AC2 too", "failure"),
    ]));

    project.lock().success();

    let log = fs::read_to_string(project.temp.path().join("argv.log")).unwrap();
    let lines: Vec<&str> = log.lines().collect();
    assert_eq!(lines[1..], [A, spaced, B]);
    let report = Path::new(lines[0]);
    assert!(
        report.starts_with(project.temp.path().canonicalize().unwrap()),
        "{log}"
    );
    assert!(!report.starts_with(project.root()), "{log}");
    // Only what the fake wrote is left: the report's own directory is gone.
    let left: Vec<_> = fs::read_dir(project.temp.path()).unwrap().collect();
    assert_eq!(left.len(), 1, "{left:?}");
}

#[test]
fn a_temp_directory_inside_the_project_is_refused() {
    let project = Project::new();
    project.runner("touch ran\n");
    project.write("tmp/keep", "");
    let inside = project.root().join("tmp");
    let before = project.tree();

    let assert = Command::cargo_bin("invar")
        .unwrap()
        .current_dir(project.root())
        .env("TMPDIR", &inside)
        .args(["acceptance", "lock", "SHOP-214"])
        .assert()
        .failure()
        .stdout("");

    let message = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
    assert!(message.contains("inside the project"), "{message}");
    assert_eq!(project.tree(), before);
}

#[test]
fn a_program_that_does_not_exist_is_refused_naming_it() {
    let project = Project::new();
    project.write(
        "invar.yaml",
        "acceptance:\n  dir: tests/Acceptance\n  \
         run: [\"./vendor/bin/nope\", \"{report}\", \"{files}\"]\n",
    );
    project.refuses(&["`./vendor/bin/nope`", "acceptance.run"]);
}

#[test]
fn an_empty_report_is_the_tests_did_not_run_with_stderr_and_nothing_about_tests() {
    let project = Project::new();
    project.runner(
        "echo 'PHP Fatal: Class \"Pest\" not found' >&2\necho progress\n: > \"$1\"\nexit 255\n",
    );
    let message = project.refuses(&[
        "the tests did not run",
        "exit status: 255",
        "PHP Fatal: Class \"Pest\" not found",
        "progress",
    ]);
    for absent in ["covers", "passes before", "no test from"] {
        assert!(!message.contains(absent), "{absent}:\n{message}");
    }
}

#[test]
fn no_report_written_is_the_tests_did_not_run() {
    let project = Project::new();
    project.runner("echo 'cannot boot' >&2\nexit 1\n");
    let message = project.refuses(&[
        "the tests did not run",
        "no report was written",
        "exit status: 1",
        "cannot boot",
    ]);
    assert!(!message.contains("covers"), "{message}");
}

#[test]
fn the_exit_code_is_not_read() {
    // Exit 0, with a report that says a test passed: the report decides.
    let project = Project::new();
    project.canned(&report(&[
        case(A, "it AC1 cancels", ""),
        case(B, "it AC2 refuses", "failure"),
    ]));
    project.refuses(&["passes before the code exists"]);

    // Exit 1, with a report that says every test is red: the report decides.
    project.runner("cp canned.xml \"$1\"\nexit 1\n");
    project.canned(&all_red());
    project.lock().success();
}

#[test]
fn a_report_that_is_not_xml_is_refused_with_the_readers_error() {
    let project = Project::new();
    project.canned("this is not xml");
    project.refuses(&["the report", "exit status: 0"]);
}

#[test]
fn a_file_with_no_test_case_in_the_report_is_refused_naming_it() {
    let project = Project::new();
    project.canned(&report(&[
        case(A, "it AC1 cancels", "failure"),
        case(A, "it AC2 refuses", "failure"),
    ]));
    project.refuses(&[B, "no test from this file ran"]);
}

#[test]
fn a_report_with_no_tests_names_every_file() {
    let project = Project::new();
    project.canned("<testsuites/>\n");
    project.refuses(&[A, B, "no test from this file ran", "no test covers"]);
}

#[test]
fn test_cases_of_other_files_are_ignored() {
    let project = Project::new();
    project.canned(&report(&[
        case(A, "it AC1 cancels", "failure"),
        case(B, "it AC2 refuses", "failure"),
        // No token, and green: neither matters for a file nobody passed.
        case("tests/Bootstrap.php", "the runner's own check", ""),
    ]));
    project.lock().success();
}

// --- the checks -----------------------------------------------------------------

#[test]
fn a_passing_test_is_refused_with_no_override() {
    let project = Project::new();
    project.canned(&report(&[
        case(A, "it AC1 cancels", ""),
        case(B, "it AC2 refuses", "failure"),
    ]));
    project.refuses(&[
        A,
        "`it AC1 cancels` (AC1)",
        "passes before the code exists",
        "no override",
        "`BLOCKED`",
    ]);
}

#[test]
fn an_error_is_refused_with_its_type_and_first_lines() {
    let project = Project::new();
    project.canned(&report(&[
        case(A, "it AC1 cancels", "error"),
        case(B, "it AC2 refuses", "failure"),
    ]));
    let message = project.refuses(&[
        "`it AC1 cancels` (AC1)",
        "fails for the wrong reason (an error, not an assertion): Error",
        "Class \"App\\Cancel\" not found",
        "fifth",
    ]);
    assert!(!message.contains("sixth line"), "{message}");
}

#[test]
fn a_skipped_test_is_refused() {
    let project = Project::new();
    project.canned(&report(&[
        case(A, "it AC1 cancels", "skipped"),
        case(B, "it AC2 refuses", "failure"),
    ]));
    project.refuses(&["`it AC1 cancels` (AC1)", "a skipped test proves nothing"]);
}

#[test]
fn a_test_with_no_token_is_refused_by_name() {
    let project = Project::new();
    project.canned(&report(&[
        case(A, "it cancels", "failure"),
        case(A, "it AC1 cancels", "failure"),
        case(B, "it AC2 refuses", "failure"),
    ]));
    project.refuses(&["`it cancels`", "no `AC<n>` token"]);
}

#[test]
fn a_test_with_two_tokens_is_refused_and_covers_neither() {
    let project = Project::new();
    project.canned(&report(&[
        case(A, "it AC1 and AC2", "failure"),
        case(B, "it AC2 refuses", "failure"),
    ]));
    project.refuses(&[
        "`it AC1 and AC2`",
        "AC1, AC2",
        "no test covers",
        "AC1: Given an unpaid",
    ]);
}

#[test]
fn an_id_the_plan_does_not_have_is_refused() {
    let project = Project::new();
    project.canned(&report(&[
        case(A, "it AC1 cancels", "failure"),
        case(A, "it AC9 flies", "failure"),
        case(B, "it AC2 refuses", "failure"),
    ]));
    project.refuses(&["`it AC9 flies`", "AC9", "does not have", "AC1, AC2"]);
}

#[test]
fn an_uncovered_criterion_is_refused_with_its_text() {
    let project = Project::new();
    project.canned(&report(&[
        case(A, "it AC1 cancels", "failure"),
        case(B, "it AC1 again", "failure"),
    ]));
    let message = project.refuses(&["no test covers", "AC2: Given a paid order"]);
    assert!(!message.contains("AC1: Given"), "{message}");
}

#[test]
fn every_problem_is_reported_in_one_run_grouped_by_file() {
    let project = Project::new();
    project.canned(&report(&[
        case(B, "it AC1 passes", ""),
        case(A, "no token at all", "error"),
        case(A, "it AC7 unknown and skipped", "skipped"),
    ]));
    let message = project.refuses(&[
        "`it AC1 passes` (AC1)",
        "passes before the code exists",
        "`no token at all`",
        "no `AC<n>` token",
        "fails for the wrong reason",
        "AC7",
        "a skipped test proves nothing",
        "AC2: Given a paid order",
    ]);
    // By file, in order, whatever order the report had.
    let first = message.find(A).unwrap();
    let second = message.find(B).unwrap();
    assert!(first < second, "{message}");
    assert!(
        message.find("no token at all").unwrap() < second,
        "{message}"
    );
}

#[test]
fn the_success_output_names_the_tests_of_each_criterion_in_the_plans_order() {
    let project = Project::new();
    project.canned(&report(&[
        case(B, "it AC2 refuses", "failure"),
        case(A, "it AC1 cancels", "failure"),
    ]));
    let assert = project.lock().success().stderr("");
    let out = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    assert!(
        out.starts_with(&format!(
            "AC1:\n  {A}: it AC1 cancels\nAC2:\n  {B}: it AC2 refuses\n"
        )),
        "{out}"
    );
    assert!(project.temp_is_empty());
}

#[test]
fn help_says_what_it_does_and_there_is_no_relock_flag_yet() {
    Command::cargo_bin("invar")
        .unwrap()
        .args(["acceptance", "lock", "--help"])
        .assert()
        .success()
        .stdout(contains("no override"));
    let project = Project::new();
    Command::cargo_bin("invar")
        .unwrap()
        .current_dir(project.root())
        .args(["acceptance", "lock", "SHOP-214", "--relock"])
        .assert()
        .failure();
}
