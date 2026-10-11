//! End-to-end tests for `invar plan validate` (M1-8): the arguments, what is
//! printed where, and the exit code — the surface the planner and the
//! `SubagentStop` hook act on.

use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use predicates::prelude::*;
use predicates::str::contains;
use tempfile::TempDir;

fn invar() -> Command {
    Command::cargo_bin("invar").expect("the `invar` binary should be built")
}

/// A valid light-mode draft for `ticket`.
fn plan_text(ticket: &str) -> String {
    format!(
        "---\nTicket: {ticket}\nModule: orders\nStatus: draft\nApproved-by:\n\
         Approved-at:\nApproved-checksum:\n---\n\n\
         ## Ticket\n{ticket} — Cancel an unpaid order.\n\n\
         ## Intent\nCustomers cancel their own unpaid orders.\n\n\
         ## Scope\n`orders` only.\n\n\
         ## Open questions / answers\nNone were needed.\n\n\
         ## Rejected alternatives\nNone considered.\n\n\
         ## Acceptance criteria\n- Given an unpaid order, when I cancel it, then it is cancelled.\n\n\
         ## Implementation context\n- AI_CONTEXT: controllers stay thin\n"
    )
}

/// A temporary project: `invar.yaml` with `config`, and `.invar/plans/`.
struct Project {
    dir: TempDir,
}

impl Project {
    fn new(config: &str) -> Self {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("invar.yaml"), config).unwrap();
        fs::create_dir_all(dir.path().join(".invar/plans")).unwrap();
        Self { dir }
    }

    fn root(&self) -> PathBuf {
        self.dir.path().canonicalize().unwrap()
    }

    fn plan(&self, name: &str, text: &str) -> PathBuf {
        let dir = self.root().join(".invar/plans").join(name);
        fs::create_dir_all(&dir).unwrap();
        let spec = dir.join("spec.md");
        fs::write(&spec, text).unwrap();
        spec
    }

    fn validate(&self, args: &[&str]) -> assert_cmd::assert::Assert {
        self.validate_in(&self.root(), args)
    }

    fn validate_in(&self, cwd: &Path, args: &[&str]) -> assert_cmd::assert::Assert {
        invar()
            .current_dir(cwd)
            .args(["plan", "validate"])
            .args(args)
            .assert()
    }
}

const SPEC: &str = ".invar/plans/PROJ-412-reserve-stock/spec.md";

// --- valid ------------------------------------------------------------------

#[test]
fn a_valid_plan_by_ticket_exits_zero() {
    let project = Project::new("");
    project.plan("PROJ-412-reserve-stock", &plan_text("PROJ-412"));
    project
        .validate(&["PROJ-412"])
        .success()
        .stdout(format!("ok: {SPEC}\n"))
        .stderr("");
}

#[test]
fn an_approved_plan_with_an_empty_checksum_is_valid_in_form() {
    let project = Project::new("");
    let text = plan_text("PROJ-412").replace("Status: draft", "Status: approved");
    project.plan("PROJ-412-reserve-stock", &text);
    project.validate(&["PROJ-412"]).success();
}

// --- every problem, one run --------------------------------------------------

#[test]
fn every_problem_is_printed_as_path_line_message_in_one_run() {
    let project = Project::new("");
    let text = plan_text("PROJ-413")
        .replace("Module: orders", "Module: or-ders")
        .replace(
            "## Implementation context\n- AI_CONTEXT: controllers stay thin\n",
            "",
        )
        .replace(
            "- Given an unpaid order, when I cancel it, then it is cancelled.\n",
            "",
        );
    project.plan("PROJ-412-reserve-stock", &text);
    let assert = project.validate(&["PROJ-412"]).failure().code(1);
    let output = assert.get_output();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines.len(), 4, "{stdout}");
    for line in &lines {
        assert!(line.starts_with(&format!("{SPEC}:")), "{line}");
    }
    assert!(stdout.contains(&format!(
        "{SPEC}: missing required section `## Implementation context`"
    )));
    assert!(stdout.contains(&format!("{SPEC}:2: the header says `Ticket: PROJ-413`")));
    assert!(stdout.contains(&format!("{SPEC}:3: `Module` must be")));
    assert!(stdout.contains("`## Acceptance criteria` is empty"));

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        stderr,
        "error: the Plan is not valid (4 problems); fix every problem listed above, \
         then run `invar plan validate PROJ-412` again\n"
    );
}

#[test]
fn a_malformed_directory_name_is_reported_with_the_content_problems() {
    let project = Project::new("");
    let text = plan_text("PROJ-7").replace("## Scope\n", "### Scope\n");
    project.plan("PROJ-7-Reserve_Stock", &text);
    project
        .validate(&["PROJ-7"])
        .failure()
        .stdout(contains(
            "directory name `PROJ-7-Reserve_Stock` is not `<TICKET>-<slug>`",
        ))
        .stdout(contains("`### Scope` should be `## Scope`"));
}

// --- explicit paths ----------------------------------------------------------

#[test]
fn an_explicit_file_or_directory_path_works() {
    let project = Project::new("");
    project.plan("PROJ-412-reserve-stock", &plan_text("PROJ-412"));
    project
        .validate(&[SPEC])
        .success()
        .stdout(format!("ok: {SPEC}\n"));
    project
        .validate(&[".invar/plans/PROJ-412-reserve-stock"])
        .success()
        .stdout(format!("ok: {SPEC}\n"));
}

#[test]
fn a_path_finds_its_own_project_from_any_directory() {
    let project = Project::new("");
    let spec = project.plan("PROJ-412-reserve-stock", &plan_text("PROJ-412"));
    let elsewhere = tempfile::tempdir().unwrap();
    project
        .validate_in(elsewhere.path(), &[spec.to_str().unwrap()])
        .success()
        .stdout(format!("ok: {}\n", spec.display()));
}

#[test]
fn a_plan_outside_the_plans_directory_is_told_where_to_move() {
    let project = Project::new("");
    fs::write(project.root().join("spec.md"), plan_text("PROJ-1")).unwrap();
    project.validate(&["./spec.md"]).failure().stdout(contains(
        "spec.md: a Plan lives at `.invar/plans/<TICKET>-<slug>/spec.md`",
    ));
}

#[test]
fn a_path_that_does_not_exist_is_an_error() {
    let project = Project::new("");
    project
        .validate(&["no/such/dir/spec.md"])
        .failure()
        .stderr(contains("does not exist"));
}

// --- --all-drafts ---------------------------------------------------------------

#[test]
fn all_drafts_validates_every_draft_and_skips_approved_plans() {
    let project = Project::new("");
    project.plan("PROJ-1-valid", &plan_text("PROJ-1"));
    project.plan(
        "PROJ-2-broken",
        &plan_text("PROJ-2").replace("## Intent\n", ""),
    );
    let approved = plan_text("PROJ-3")
        .replace("Status: draft", "Status: approved")
        .replace("## Intent\n", "");
    project.plan("PROJ-3-approved", &approved);

    project
        .validate(&["--all-drafts"])
        .failure()
        .stdout(contains("ok: .invar/plans/PROJ-1-valid/spec.md"))
        .stdout(contains(
            ".invar/plans/PROJ-2-broken/spec.md: missing required section `## Intent`",
        ))
        .stdout(contains("PROJ-3").not())
        .stderr(contains("1 of 2 Plans are not valid (1 problem)"))
        .stderr(contains("`invar plan validate --all-drafts`"));
}

#[test]
fn all_drafts_with_no_drafts_exits_zero() {
    let project = Project::new("");
    project
        .validate(&["--all-drafts"])
        .success()
        .stdout(contains("no draft Plans in .invar/plans"));

    let approved = plan_text("PROJ-3").replace("Status: draft", "Status: approved");
    project.plan("PROJ-3-approved", &approved);
    project.validate(&["--all-drafts"]).success();

    fs::remove_dir_all(project.root().join(".invar")).unwrap();
    project
        .validate(&["--all-drafts"])
        .success()
        .stdout(contains("does not exist"));
}

#[test]
fn a_target_and_all_drafts_are_mutually_exclusive_and_one_is_required() {
    let project = Project::new("");
    // A usage error exits 1, never 2: 2 is reserved for `plan guard`'s denial
    // (tests/cli.rs, `a_usage_error_never_exits_with_code_2`).
    project.validate(&[]).failure().code(1);
    project
        .validate(&["PROJ-1", "--all-drafts"])
        .failure()
        .code(1);
}

// --- project and config ---------------------------------------------------------

#[test]
fn it_runs_from_a_subdirectory_of_the_project() {
    let project = Project::new("");
    project.plan("PROJ-412-reserve-stock", &plan_text("PROJ-412"));
    let sub = project.root().join("app/Http");
    fs::create_dir_all(&sub).unwrap();
    // Outside the current directory, the path is printed in full.
    project
        .validate_in(&sub, &["PROJ-412"])
        .success()
        .stdout(format!("ok: {}\n", project.root().join(SPEC).display()));
}

#[test]
fn lookup_errors_are_reported() {
    let project = Project::new("");
    project.plan("PROJ-412-reserve-stock", &plan_text("PROJ-412"));
    project.plan("PROJ-412-other", &plan_text("PROJ-412"));
    project
        .validate(&["PROJ-412"])
        .failure()
        .stderr(contains("matches 2 Plans"));
    project
        .validate(&["PROJ-9"])
        .failure()
        .stderr(contains("no Plan for `PROJ-9`"));
}

#[test]
fn scenarios_true_is_refused() {
    let project = Project::new("scenarios: true\n");
    project.plan("PROJ-412-reserve-stock", &plan_text("PROJ-412"));
    project
        .validate(&["PROJ-412"])
        .failure()
        .stderr(contains("light mode only"));
}

#[test]
fn outside_a_project_it_says_to_run_init() {
    let dir = tempfile::tempdir().unwrap();
    invar()
        .current_dir(dir.path())
        .args(["plan", "validate", "PROJ-1"])
        .assert()
        .failure()
        .stderr(contains("invar init"));
}

#[test]
fn an_unknown_config_key_is_a_warning_not_a_failure() {
    let project = Project::new("senarios: false\n");
    project.plan("PROJ-412-reserve-stock", &plan_text("PROJ-412"));
    project
        .validate(&["PROJ-412"])
        .success()
        .stderr(contains("warning:").and(contains("`senarios`")));
}
