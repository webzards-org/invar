//! End-to-end tests for `invar acceptance spec` (AT-7): the real binary, on a project
//! with an approved Plan.

use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use assert_cmd::assert::Assert;
use predicates::prelude::*;
use predicates::str::contains;
use tempfile::TempDir;

const SPEC: &str = ".invar/plans/SHOP-214-cancel-unpaid-order/spec.md";

const CONFIG: &str = "acceptance:\n  dir: tests/Acceptance\n  \
    run: [\"vendor/bin/pest\", \"--log-junit\", \"{report}\", \"{files}\"]\n  \
    fakes_allowed: [http, mail]\n";

/// Absent from every expected output, and present in everything that must stay out.
const SENTINEL: &str = "SENTINEL-MUST-NOT-APPEAR";

fn body(criteria: &str, context: &str) -> String {
    format!(
        "## Ticket\nSHOP-214 — Cancel an unpaid order. {SENTINEL} pasted\n\n\
         ## Intent\nA customer can cancel an order they have not paid for.\n\n\
         ## Scope\n`Orders` only: `CancelOrder`, and its route.\n\n\
         ## Open questions / answers\nQ: refunds? A: {SENTINEL} not here.\n\n\
         ## Rejected alternatives\nA queue, {SENTINEL} too heavy.\n\n\
         ## Acceptance criteria\n\n{criteria}\n\n\
         ## Implementation context\n{context}\n"
    )
}

const CRITERIA: &str = "- AC1: Given an unpaid order, when its owner cancels it,\n  \
    then it is cancelled and its stock is released.\n\
    - AC2: Given a paid order, when its owner cancels it, then the request is refused with 403.";

const CONTEXT: &str = "- AI_CONTEXT: controllers stay thin\n\
    - BUSINESS_CONTEXT: a paid order is never cancelled here";

struct Project {
    dir: TempDir,
}

impl Project {
    fn new(config: &str) -> Self {
        let project = Self {
            dir: tempfile::tempdir().unwrap(),
        };
        fs::write(project.root().join("invar.yaml"), config).unwrap();
        project
    }

    fn root(&self) -> PathBuf {
        self.dir.path().canonicalize().unwrap()
    }

    fn spec_path(&self) -> PathBuf {
        self.root().join(SPEC)
    }

    fn write_spec(&self, text: &str) {
        fs::create_dir_all(self.spec_path().parent().unwrap()).unwrap();
        fs::write(self.spec_path(), text).unwrap();
    }

    /// The digest `invar hash` gives `text`, as an independent `Approved-checksum`.
    fn hash_of(&self, text: &str) -> String {
        let file = self.root().join("to-hash.txt");
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

    /// `body` approved, as `plan approve` records it: with `acceptance` in the header
    /// and in the digest, or with neither.
    fn approve(&self, body: &str, acceptance: Option<&str>) {
        let (covered, line) = match acceptance {
            Some(value) => (
                format!("Acceptance: {value}\n{body}"),
                format!("Acceptance: {value}\n"),
            ),
            None => (body.to_owned(), String::new()),
        };
        let checksum = self.hash_of(&covered);
        self.write_spec(&format!(
            "---\nTicket: SHOP-214\nModule: orders\nStatus: approved\nApproved-by: rade\n\
             Approved-at: 2026-10-05\nApproved-checksum: {checksum}\n{line}---\n\n{body}"
        ));
    }

    fn spec(&self) -> Assert {
        Command::cargo_bin("invar")
            .unwrap()
            .current_dir(self.root())
            .args(["acceptance", "spec", "SHOP-214"])
            .assert()
    }

    fn plan_check(&self) -> Assert {
        Command::cargo_bin("invar")
            .unwrap()
            .current_dir(self.root())
            .args(["plan", "check", "SHOP-214"])
            .assert()
    }
}

fn required() -> Project {
    let project = Project::new(CONFIG);
    project.approve(&body(CRITERIA, CONTEXT), Some("required"));
    project
}

fn stdout_of(assert: Assert) -> String {
    String::from_utf8(assert.get_output().stdout.clone()).unwrap()
}

fn stderr_of(assert: Assert) -> String {
    String::from_utf8(assert.get_output().stderr.clone()).unwrap()
}

/// Compare `actual` with `tests/snapshots/<name>`, as `tests/review_input.rs` does:
/// `INVAR_BLESS_SNAPSHOTS=1` rewrites it, and the diff is what gets read.
fn assert_snapshot(name: &str, actual: &str) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/snapshots")
        .join(name);
    if std::env::var_os("INVAR_BLESS_SNAPSHOTS").is_some() {
        fs::write(&path, actual).unwrap();
        return;
    }
    let expected = fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("cannot read the snapshot {} ({err})", path.display()))
        .replace("\r\n", "\n");
    assert_eq!(
        actual,
        expected,
        "the output differs from {}",
        path.display()
    );
}

// --- the output -----------------------------------------------------------------

#[test]
fn the_full_output_is_pinned_byte_for_byte() {
    let output = stdout_of(required().spec().success().stderr(""));
    assert_snapshot("acceptance-spec-shop-214.txt", &output);
}

#[test]
fn the_same_plan_gives_the_same_bytes() {
    let project = required();
    let first = project.spec().success().get_output().stdout.clone();
    let second = project.spec().success().get_output().stdout.clone();
    assert_eq!(first, second);
}

#[test]
fn a_wrapped_criterion_is_printed_as_one_line() {
    required().spec().success().stdout(contains(
        "\nAC1: Given an unpaid order, when its owner cancels it, then it is cancelled and its \
         stock is released.\n",
    ));
}

#[test]
fn a_path_resolves_as_for_plan_check() {
    let project = required();
    let by_ticket = project.spec().success().get_output().stdout.clone();
    for target in [SPEC, ".invar/plans/SHOP-214-cancel-unpaid-order"] {
        Command::cargo_bin("invar")
            .unwrap()
            .current_dir(project.root())
            .args(["acceptance", "spec", target])
            .assert()
            .success()
            .stdout(predicate::eq(by_ticket.clone()));
    }
}

#[test]
fn no_context_file_decision_or_how_it_came_about_reaches_the_output() {
    let project = required();
    let marker = "<!-- invar-context: v1 -->";
    fs::write(
        project.root().join("BUSINESS_CONTEXT.md"),
        format!("{marker}\n# BUSINESS_CONTEXT.md\n\n## Invariants (always true, no exceptions)\n- {SENTINEL} business\n"),
    )
    .unwrap();
    fs::write(
        project.root().join("AI_CONTEXT.md"),
        format!("{marker}\n# AI_CONTEXT.md\n\n## Naming\n- {SENTINEL} conventions\n"),
    )
    .unwrap();
    let decisions = project.root().join(".invar/decisions/orders");
    fs::create_dir_all(&decisions).unwrap();
    fs::write(
        decisions.join("2026-10-01-SHOP-1.md"),
        format!("---\nticket: SHOP-1\n---\n{SENTINEL} decision\n"),
    )
    .unwrap();

    let output = stdout_of(project.spec().success());
    assert!(!output.contains(SENTINEL), "{output}");
    // What the Plan itself quotes is approved text, and stays.
    assert!(output.contains("BUSINESS_CONTEXT: a paid order is never cancelled here"));
}

#[test]
fn nothing_is_cut_for_size() {
    let long: Vec<String> = (0..3000)
        .map(|n| format!("- context line {n} that was approved"))
        .collect();
    let context = long.join("\n");
    assert!(context.chars().count() > 10_000);
    let project = Project::new(CONFIG);
    project.approve(&body(CRITERIA, &context), Some("required"));

    let output = stdout_of(project.spec().success().stderr(""));
    assert!(output.chars().count() > 100_000, "{}", output.len());
    assert!(output.contains(&context));
}

// --- refusals -------------------------------------------------------------------

/// A refusal: non-zero, nothing on stdout, and this on stderr.
fn refused(assert: Assert, parts: &[&str]) {
    let assert = assert.failure().stdout("");
    let message = stderr_of(assert);
    for part in parts {
        assert!(message.contains(part), "{part}: {message}");
    }
}

/// `plan check`'s refusal, which the command repeats in the same words.
fn refuses_as_plan_check_does(project: &Project) {
    let expected = stderr_of(project.plan_check().failure());
    assert!(!expected.is_empty());
    let got = stderr_of(project.spec().failure().stdout(""));
    assert_eq!(got, expected);
}

#[test]
fn a_draft_is_refused_as_plan_check_refuses_it() {
    let project = Project::new(CONFIG);
    project.write_spec(&format!(
        "---\nTicket: SHOP-214\nModule: orders\nStatus: draft\nApproved-by:\nApproved-at:\n\
         Approved-checksum:\n---\n\n{}",
        body(CRITERIA, CONTEXT)
    ));
    refuses_as_plan_check_does(&project);
    project.spec().failure().stderr(contains("draft"));
}

#[test]
fn a_changed_plan_is_refused_as_plan_check_refuses_it() {
    let project = required();
    let edited = fs::read_to_string(project.spec_path())
        .unwrap()
        .replace("refused with 403", "refused with 404");
    project.write_spec(&edited);
    refuses_as_plan_check_does(&project);
    project.spec().failure().stderr(contains("changed"));
}

#[test]
fn a_flipped_value_is_refused_as_plan_check_refuses_it() {
    let project = Project::new(CONFIG);
    project.approve(&body(CRITERIA, CONTEXT), Some("skipped"));
    let flipped = fs::read_to_string(project.spec_path())
        .unwrap()
        .replace("Acceptance: skipped", "Acceptance: required");
    project.write_spec(&flipped);
    refuses_as_plan_check_does(&project);
}

#[test]
fn an_approval_nobody_recorded_is_refused_as_plan_check_refuses_it() {
    let project = Project::new(CONFIG);
    project.write_spec(&format!(
        "---\nTicket: SHOP-214\nModule: orders\nStatus: approved\nApproved-by:\nApproved-at:\n\
         Approved-checksum:\nAcceptance: required\n---\n\n{}",
        body(CRITERIA, CONTEXT)
    ));
    refuses_as_plan_check_does(&project);
}

#[test]
fn a_plan_that_is_not_valid_is_listed_on_stderr_with_stdout_empty() {
    let project = required();
    let broken = fs::read_to_string(project.spec_path())
        .unwrap()
        .replace("## Scope\n", "## Scoop\n");
    project.write_spec(&broken);
    refused(
        project.spec(),
        &["`## Scope`", "spec.md:", "Fix every problem listed above"],
    );
}

#[test]
fn skipped_is_refused() {
    let project = Project::new(CONFIG);
    project.approve(&body(CRITERIA, CONTEXT), Some("skipped"));
    project.plan_check().success();
    refused(
        project.spec(),
        &[
            "SHOP-214",
            "this ticket was approved without acceptance tests; there is nothing for a \
             test-writer to do",
            "--reapprove --acceptance",
        ],
    );
}

#[test]
fn an_approval_with_no_value_reads_as_skipped_and_is_refused() {
    let project = Project::new(CONFIG);
    project.approve(&body(CRITERIA, CONTEXT), None);
    project.plan_check().success();
    refused(
        project.spec(),
        &["this ticket was approved without acceptance tests"],
    );
}

#[test]
fn no_section_is_refused_with_what_to_add() {
    for config in ["", "acceptance:\n"] {
        let project = Project::new(config);
        project.approve(&body(CRITERIA, CONTEXT), Some("required"));
        project.plan_check().success();
        refused(
            project.spec(),
            &[
                "no `acceptance:` section",
                "\nacceptance:\n  dir: ",
                "{report}",
                "{files}",
            ],
        );
    }
}

#[test]
fn criteria_with_no_ids_are_refused_rather_than_printed_as_something_else() {
    let project = Project::new(CONFIG);
    project.approve(
        &body(
            "- Given a paid order, when its owner cancels it, then it is refused.",
            CONTEXT,
        ),
        Some("required"),
    );
    project.plan_check().success();
    refused(
        project.spec(),
        &["SHOP-214", "no ids", "`- AC1: Given …`", "--reapprove"],
    );
}

#[test]
fn an_invalid_section_is_reported_as_it_is_everywhere_else() {
    let project = Project::new("acceptance:\n  dir: tests\n");
    project.approve(&body(CRITERIA, CONTEXT), Some("required"));
    refused(project.spec(), &["`acceptance.run` is required"]);
}

#[test]
fn a_missing_plan_is_reported_with_nothing_on_stdout() {
    refused(Project::new(CONFIG).spec(), &["SHOP-214"]);
}

#[test]
fn help_says_who_runs_it_and_when() {
    Command::cargo_bin("invar")
        .unwrap()
        .args(["acceptance", "spec", "--help"])
        .assert()
        .success()
        .stdout(contains("test-writer agent"))
        .stdout(contains("right after `invar plan check`"))
        .stdout(contains("--skill").not());
}
