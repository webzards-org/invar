//! End-to-end tests for `invar decision record` and `invar decision log`
//! (M1-11): the real binary, a real project, and git's clock fixed through
//! `GIT_COMMITTER_DATE`.

use std::fs;
use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::process::{Command as Process, Stdio};
use std::time::{Duration, Instant};

use assert_cmd::Command;
use predicates::prelude::*;
use predicates::str::contains;
use tempfile::TempDir;

const SPEC: &str = ".invar/plans/SHOP-214-cancel-unpaid-order/spec.md";
/// 16:37:12 UTC.
const NOW: &str = "2026-10-06T18:37:12+02:00";
const ENTRY: &str = ".invar/decisions/2026-10-06-163712-orders-cancel-unpaid-order.md";

fn contract_example(number: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../contracts/formats/plan-header.md");
    let text = fs::read_to_string(path).unwrap();
    let after = text.split_once(&format!("### {number} ")).unwrap().1;
    let body = after.split_once("```markdown\n").unwrap().1;
    body.split_once("\n```\n").unwrap().0.to_owned() + "\n"
}

fn approved_plan() -> String {
    let draft = contract_example("9.1");
    contract_example("9.2") + draft.split_once("\n---\n").unwrap().1
}

struct Project {
    dir: TempDir,
}

impl Project {
    fn new(plan: Option<&str>) -> Self {
        let project = Self {
            dir: tempfile::tempdir().unwrap(),
        };
        fs::write(project.root().join("invar.yaml"), "").unwrap();
        fs::write(project.root().join(".gitconfig-test"), "").unwrap();
        if let Some(plan) = plan {
            let spec = project.root().join(SPEC);
            fs::create_dir_all(spec.parent().unwrap()).unwrap();
            fs::write(spec, plan).unwrap();
        }
        project
    }

    fn root(&self) -> PathBuf {
        self.dir.path().canonicalize().unwrap()
    }

    fn read(&self, relative: &str) -> String {
        fs::read_to_string(self.root().join(relative)).unwrap()
    }

    fn entries(&self) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(self.root().join(".invar/decisions"))
            .map(|dir| {
                dir.map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        names
    }

    fn invar(&self, args: &[&str]) -> Command {
        let mut command = Command::cargo_bin("invar").unwrap();
        command
            .current_dir(self.root())
            .env("GIT_COMMITTER_DATE", NOW)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", self.root().join(".gitconfig-test"))
            .env("HOME", self.root())
            .args(args);
        command
    }

    fn record(&self, args: &[&str]) -> assert_cmd::assert::Assert {
        let mut all = vec!["decision", "record"];
        all.extend_from_slice(args);
        self.invar(&all).assert()
    }
}

// --- record -----------------------------------------------------------------------

#[test]
fn record_writes_one_entry_from_message() {
    let project = Project::new(Some(&approved_plan()));
    project
        .record(&[
            "--ticket",
            "SHOP-214",
            "--type",
            "note",
            "--message",
            "Warehouse owns reservations.",
        ])
        .success()
        .stdout(contains(format!("recorded: {ENTRY} (d-")))
        .stdout(contains(", note, orders)"));
    let text = project.read(ENTRY);
    assert!(text.starts_with("---\nid: d-"), "{text}");
    assert!(text.contains(&format!("\ndate: {NOW}\nticket: SHOP-214\nmodule: orders\ntype: note\n---\n\nWarehouse owns reservations.\n")), "{text}");
    // The Plan is not touched by a note.
    assert_eq!(project.read(SPEC), approved_plan());
}

#[test]
fn record_reads_the_body_from_stdin() {
    let project = Project::new(Some(&approved_plan()));
    project
        .invar(&[
            "decision",
            "record",
            "--ticket",
            "SHOP-214",
            "--type",
            "correction",
        ])
        .write_stdin("Fixed the rounding by hand.\r\nIt was off by one cent.\r\n")
        .assert()
        .success();
    let text = project.read(ENTRY);
    assert!(
        text.ends_with("---\n\nFixed the rounding by hand.\nIt was off by one cent.\n"),
        "{text:?}"
    );
}

#[test]
fn an_empty_body_is_refused() {
    let project = Project::new(Some(&approved_plan()));
    project
        .invar(&[
            "decision", "record", "--ticket", "SHOP-214", "--type", "note",
        ])
        .write_stdin("  \n")
        .assert()
        .failure()
        .stderr(contains("the entry has no text"));
    assert!(project.entries().is_empty());
}

#[test]
fn two_entries_in_the_same_second_do_not_collide() {
    let project = Project::new(Some(&approved_plan()));
    for text in ["first", "second", "third"] {
        project
            .record(&["--ticket", "SHOP-214", "--type", "note", "-m", text])
            .success();
    }
    assert_eq!(
        project.entries(),
        [
            "2026-10-06-163712-orders-cancel-unpaid-order-2.md",
            "2026-10-06-163712-orders-cancel-unpaid-order-3.md",
            "2026-10-06-163712-orders-cancel-unpaid-order.md",
        ]
    );
    let ids: Vec<String> = project
        .entries()
        .iter()
        .map(|name| {
            project
                .read(&format!(".invar/decisions/{name}"))
                .lines()
                .find_map(|l| l.strip_prefix("id: ").map(str::to_owned))
                .unwrap()
        })
        .collect();
    assert_ne!(ids[0], ids[1]);
    assert_ne!(ids[1], ids[2]);
    assert_ne!(ids[0], ids[2]);
}

#[test]
fn without_a_plan_the_module_is_required_and_global_is_accepted() {
    let project = Project::new(None);
    project
        .record(&["--ticket", "SHOP-9", "--type", "note", "-m", "x"])
        .failure()
        .stderr(contains("there is no Plan for SHOP-9"))
        .stderr(contains("`--module global`"));
    project
        .record(&[
            "--ticket", "SHOP-9", "--type", "note", "--module", "global", "-m", "x",
        ])
        .success();
    assert_eq!(project.entries(), ["2026-10-06-163712-global-shop-9.md"]);
}

#[test]
fn a_module_that_contradicts_the_plan_is_refused() {
    let project = Project::new(Some(&approved_plan()));
    project
        .record(&[
            "--ticket", "SHOP-214", "--type", "note", "--module", "payments", "-m", "x",
        ])
        .failure()
        .stderr(contains(
            "`--module payments` disagrees with the Plan for SHOP-214",
        ));
    assert!(project.entries().is_empty());
}

#[test]
fn an_unknown_type_is_refused_by_the_parser() {
    let project = Project::new(Some(&approved_plan()));
    project
        .record(&["--ticket", "SHOP-214", "--type", "fixed", "-m", "x"])
        .failure()
        .code(1)
        .stderr(contains(
            "possible values: confirmed, correction, failed, note",
        ));
}

// --- failed -----------------------------------------------------------------------

#[test]
fn failed_resets_the_plan_to_draft_and_plan_check_then_refuses() {
    let original = approved_plan();
    let project = Project::new(Some(&original));
    project
        .invar(&["plan", "check", "SHOP-214"])
        .assert()
        .success();

    project
        .record(&[
            "--ticket",
            "SHOP-214",
            "--type",
            "failed",
            "-m",
            "Reservations belong to the warehouse.",
        ])
        .success()
        .stdout(contains(", failed, orders)"))
        .stdout(contains(format!("reset: {SPEC} is `draft` again")));

    let reset = project.read(SPEC);
    assert!(reset.contains(
        "Status: draft\nApproved-by:\nApproved-at:\nApproved-checksum:\nAcceptance:\n---\n"
    ));
    assert_eq!(
        reset.split_once("\n---\n").unwrap().1,
        original.split_once("\n---\n").unwrap().1,
        "the body changed"
    );
    project
        .invar(&["plan", "check", "SHOP-214"])
        .assert()
        .failure()
        .stderr(contains("is still a draft"));
}

#[test]
fn failed_without_a_plan_is_refused_with_a_clear_message() {
    let project = Project::new(None);
    project
        .record(&[
            "--ticket", "SHOP-9", "--type", "failed", "--module", "orders", "-m", "x",
        ])
        .failure()
        .stderr(contains("there is no Plan for SHOP-9"))
        .stderr(contains("has nothing to reset"))
        .stderr(contains("`--type note`"));
    assert!(project.entries().is_empty());
}

#[test]
fn record_never_modifies_an_existing_entry() {
    let project = Project::new(Some(&approved_plan()));
    project
        .record(&["--ticket", "SHOP-214", "--type", "note", "-m", "first"])
        .success();
    let first = project.read(ENTRY);
    project
        .record(&["--ticket", "SHOP-214", "--type", "failed", "-m", "second"])
        .success();
    assert_eq!(project.read(ENTRY), first);
}

#[test]
fn the_help_says_who_records() {
    Command::cargo_bin("invar")
        .unwrap()
        .args(["decision", "record", "--help"])
        .assert()
        .success()
        .stdout(contains("when the developer decides it is worth"))
        .stdout(contains("no hook runs this command"));
}

// --- log ------------------------------------------------------------------------------

#[test]
fn log_prints_entries_newest_first() {
    let project = Project::new(Some(&approved_plan()));
    project
        .record(&[
            "--ticket",
            "SHOP-214",
            "--type",
            "note",
            "-m",
            "Same second, first.",
        ])
        .success();
    project
        .record(&[
            "--ticket",
            "SHOP-214",
            "--type",
            "note",
            "-m",
            "Same second, second.",
        ])
        .success();
    project
        .invar(&[
            "decision",
            "record",
            "--ticket",
            "SHOP-300",
            "--type",
            "note",
            "--module",
            "global",
            "-m",
            "An hour later.",
        ])
        .env("GIT_COMMITTER_DATE", "2026-10-06T19:37:12+02:00")
        .assert()
        .success();
    project
        .invar(&[
            "decision",
            "record",
            "--ticket",
            "SHOP-100",
            "--type",
            "correction",
            "--module",
            "billing",
            "-m",
            "A day earlier.\n\nTwo paragraphs.",
        ])
        .env("GIT_COMMITTER_DATE", "2026-10-05T18:37:12+02:00")
        .assert()
        .success();

    let assert = project.invar(&["decision", "log"]).assert().success();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let order = [
        "An hour later.",
        "Same second, second.",
        "Same second, first.",
        "A day earlier.",
    ];
    let positions: Vec<usize> = order
        .iter()
        .map(|text| stdout.find(text).unwrap())
        .collect();
    assert!(positions.windows(2).all(|w| w[0] < w[1]), "{stdout}");
    assert!(
        stdout.starts_with("2026-10-06T19:37:12+02:00  note  SHOP-300  global  d-"),
        "{stdout}"
    );
    assert!(
        stdout.contains("    A day earlier.\n\n    Two paragraphs.\n"),
        "{stdout}"
    );

    project
        .invar(&["decision", "log", "--ticket", "SHOP-100"])
        .assert()
        .success()
        .stdout(contains("A day earlier.").and(contains("Same second").not()));
    project
        .invar(&["decision", "log", "--module", "nothing"])
        .assert()
        .success()
        .stdout("no decision entries match\n");
}

#[test]
fn log_reports_a_broken_entry_and_shows_the_rest() {
    let project = Project::new(Some(&approved_plan()));
    project
        .record(&[
            "--ticket",
            "SHOP-214",
            "--type",
            "note",
            "-m",
            "A good entry.",
        ])
        .success();
    fs::write(
        project
            .root()
            .join(".invar/decisions/2026-10-07-090000-orders-edited.md"),
        "---\nid: d-1234\n---\n\nedited by hand\n",
    )
    .unwrap();
    project
        .invar(&["decision", "log"])
        .assert()
        .success()
        .stdout(contains("A good entry."))
        .stderr(contains(
            "2026-10-07-090000-orders-edited.md is not a valid decision entry",
        ))
        .stderr(contains("has no `date`"));
}

#[test]
fn log_without_a_decisions_directory_says_so() {
    Project::new(None)
        .invar(&["decision", "log"])
        .assert()
        .success()
        .stdout(contains("no decision entries:").and(contains("does not exist")));
}

// --- order of work -----------------------------------------------------------

#[test]
fn the_project_is_looked_up_before_stdin_is_read() {
    // Outside any project, with a stdin that stays open and never ends. The
    // command has to say what is wrong, not wait for text it cannot use: when
    // it read stdin first, this hung.
    let dir = tempfile::tempdir().unwrap();
    let mut child = Process::new(env!("CARGO_BIN_EXE_invar"))
        .args(["decision", "record", "--ticket", "SHOP-1", "--type", "note"])
        .current_dir(dir.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    // Held, never written to and never closed.
    let _open_stdin = child.stdin.take();

    let deadline = Instant::now() + Duration::from_secs(10);
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if Instant::now() > deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!(
                "still waiting for stdin after 10 seconds: the command reads the entry's \
                 text before it looks for the project"
            );
        }
        std::thread::sleep(Duration::from_millis(20));
    };

    assert!(!status.success());
    let mut stderr = String::new();
    child
        .stderr
        .take()
        .unwrap()
        .read_to_string(&mut stderr)
        .unwrap();
    assert!(stderr.contains("invar init"), "{stderr}");
}

// --- validate: the draft (decision-entry.md §8.1) --------------------------------------

const DRAFT: &str = ".invar/plans/SHOP-214-cancel-unpaid-order/decision.md";
const VALID_DRAFT: &str = "---\nticket: SHOP-214\nmodule: orders\ntype: confirmed\n---\n\n\
                           Stock is released inside the cancel transaction, through \
                           `StockLedger`.\n";

impl Project {
    fn validate(&self, target: &str) -> assert_cmd::assert::Assert {
        self.invar(&["decision", "validate", target]).assert()
    }

    fn draft(&self, text: &str) {
        fs::write(self.root().join(DRAFT), text).unwrap();
    }
}

#[test]
fn validate_passes_a_valid_draft_by_ticket_and_by_path() {
    let project = Project::new(Some(&approved_plan()));
    project.draft(VALID_DRAFT);
    for target in [
        "SHOP-214",
        DRAFT,
        ".invar/plans/SHOP-214-cancel-unpaid-order",
    ] {
        project.validate(target).success().stdout(contains(format!(
            "ok: {DRAFT} (SHOP-214, confirmed, orders)"
        )));
    }
}

#[test]
fn validate_reports_every_problem_with_its_line_and_fails() {
    let project = Project::new(Some(&approved_plan()));
    project.draft(
        "---\nid: d-1234\ndate: 2026-10-06T18:37:12+02:00\nticket: SHOP-300\nmodule: orders\n\
         type: correction\n---\n",
    );
    project
        .validate("SHOP-214")
        .failure()
        .stdout(contains(format!("{DRAFT}:2: a draft has no `id`")))
        .stdout(contains(format!("{DRAFT}:3: a draft has no `date`")))
        .stdout(contains(format!(
            "{DRAFT}:6: a draft is not a `correction`"
        )))
        .stdout(contains(format!("{DRAFT}: the body is empty")))
        .stderr(contains("the draft is not valid (4 problems)"))
        .stderr(contains("invar decision validate SHOP-214"));
}

#[test]
fn validate_refuses_a_draft_that_names_another_ticket() {
    let project = Project::new(Some(&approved_plan()));
    project.draft(&VALID_DRAFT.replace("SHOP-214", "SHOP-300"));
    project
        .validate("SHOP-214")
        .failure()
        .stdout(contains("in the Plan directory of SHOP-214"));
}

#[test]
fn validate_without_a_draft_says_how_to_write_one() {
    let project = Project::new(Some(&approved_plan()));
    project
        .validate("SHOP-214")
        .failure()
        .stdout(contains("there is no `decision.md` yet"))
        .stdout(contains("/invar:log"));
}

#[test]
fn validate_for_a_ticket_with_no_plan_is_a_lookup_error() {
    let project = Project::new(None);
    project
        .validate("SHOP-214")
        .failure()
        .stdout(predicate::str::is_empty());
}

#[test]
fn validate_writes_nothing_and_records_nothing() {
    let project = Project::new(Some(&approved_plan()));
    project.draft(VALID_DRAFT);
    project.validate("SHOP-214").success();
    assert_eq!(project.read(DRAFT), VALID_DRAFT);
    assert_eq!(project.read(SPEC), approved_plan());
    assert!(project.entries().is_empty());
}

#[test]
fn a_draft_does_not_count_as_an_entry() {
    // A draft is not in the log: `decision log` does not show it, and
    // `clean-spec` still sees a Plan with no entry.
    let project = Project::new(Some(&approved_plan()));
    project.draft(VALID_DRAFT);
    project
        .invar(&["decision", "log"])
        .assert()
        .success()
        .stdout(contains("StockLedger").not());
}

#[test]
fn the_help_says_what_a_draft_is_and_that_it_records_nothing() {
    Command::cargo_bin("invar")
        .unwrap()
        .args(["decision", "validate", "--help"])
        .assert()
        .success()
        .stdout(contains("no `id` and no `date`"))
        .stdout(contains("Writes nothing; records nothing"));
}

// --- approve: the developer records the draft --------------------------------------

impl Project {
    fn approve(&self, target: &str) -> assert_cmd::assert::Assert {
        self.invar(&["decision", "approve", target]).assert()
    }

    fn exists(&self, relative: &str) -> bool {
        self.root().join(relative).exists()
    }
}

#[test]
fn approve_records_the_draft_with_a_new_id_and_date_and_removes_it() {
    let project = Project::new(Some(&approved_plan()));
    project.draft(VALID_DRAFT);
    project
        .approve("SHOP-214")
        .success()
        .stdout(contains(format!("recorded: {ENTRY} (d-")))
        .stdout(contains(", confirmed, orders)"))
        // The draft did not say; the Plan of example 9.2 does (decision-entry.md §8.1).
        .stdout(contains(
            "acceptance: skipped — the draft did not say, so it is the Plan's",
        ))
        .stdout(contains(format!("removed: {DRAFT}")));
    let text = project.read(ENTRY);
    assert!(text.starts_with("---\nid: d-"), "{text}");
    assert!(
        text.contains(&format!(
            "\ndate: {NOW}\nticket: SHOP-214\nmodule: orders\ntype: confirmed\n\
             acceptance: skipped\n---\n\n\
             Stock is released inside the cancel transaction, through `StockLedger`.\n"
        )),
        "{text}"
    );
    assert!(!project.exists(DRAFT));
    assert_eq!(project.read(SPEC), approved_plan());
}

#[test]
fn approve_records_what_the_developer_edited_by_ticket_or_by_path() {
    let project = Project::new(Some(&approved_plan()));
    // The agent's draft, as the developer left it after reading it.
    project.draft(
        &VALID_DRAFT
            .replace("module: orders", "module: global")
            .replace("type: confirmed", "type: note"),
    );
    project
        .approve(".invar/plans/SHOP-214-cancel-unpaid-order")
        .success()
        .stdout(contains(", note, global)"));
    assert_eq!(
        project.entries(),
        ["2026-10-06-163712-global-cancel-unpaid-order.md"]
    );
    assert!(!project.exists(DRAFT));
}

#[test]
fn approve_of_a_failed_draft_returns_the_plan_to_draft() {
    let project = Project::new(Some(&approved_plan()));
    project.draft(&VALID_DRAFT.replace("type: confirmed", "type: failed"));
    project
        .approve("SHOP-214")
        .success()
        .stdout(contains(", failed, orders)"))
        .stdout(contains(format!("reset: {SPEC} is `draft` again")));
    assert!(project.read(SPEC).contains("Status: draft"));
    assert_eq!(project.entries().len(), 1);
    assert!(!project.exists(DRAFT));
}

#[test]
fn approve_without_a_draft_refuses_and_writes_nothing() {
    let project = Project::new(Some(&approved_plan()));
    project
        .approve("SHOP-214")
        .failure()
        .stderr(contains("there is no draft to record"))
        .stderr(contains("/invar:log"));
    assert!(project.entries().is_empty());
    assert_eq!(project.read(SPEC), approved_plan());
}

#[test]
fn approve_of_an_invalid_draft_lists_every_problem_and_keeps_it() {
    let project = Project::new(Some(&approved_plan()));
    let invalid = "---\nid: d-1234\nticket: SHOP-214\nmodule: orders\ntype: failed\n---\n";
    project.draft(invalid);
    project
        .approve("SHOP-214")
        .failure()
        .stdout(contains(format!("{DRAFT}:2: a draft has no `id`")))
        .stdout(contains(format!("{DRAFT}: the body is empty")))
        .stderr(contains(
            "the draft is not valid (2 problems), so nothing was recorded",
        ));
    assert_eq!(project.read(DRAFT), invalid);
    assert!(project.entries().is_empty());
    // `failed`, but not valid: the Plan is not reset either.
    assert_eq!(project.read(SPEC), approved_plan());
}

#[test]
fn approve_for_a_ticket_with_no_plan_is_a_lookup_error() {
    Project::new(None).approve("SHOP-214").failure();
}

#[test]
fn the_help_says_approve_is_the_developers_gate() {
    Command::cargo_bin("invar")
        .unwrap()
        .args(["decision", "approve", "--help"])
        .assert()
        .success()
        .stdout(contains("/invar:log-approve"))
        .stdout(contains("deletes `decision.md`"));
}

// --- approve without a ticket, and --skill (what /invar:log-approve runs) --------------

#[test]
fn approve_without_a_ticket_records_the_one_draft() {
    let project = Project::new(Some(&approved_plan()));
    project.draft(VALID_DRAFT);
    project
        .invar(&["decision", "approve"])
        .assert()
        .success()
        .stdout(contains(format!("removed: {DRAFT}")));
    assert_eq!(project.entries().len(), 1);
}

#[test]
fn approve_without_a_ticket_lists_several_drafts_and_records_none() {
    let project = Project::new(Some(&approved_plan()));
    project.draft(VALID_DRAFT);
    let other = project.root().join(".invar/plans/SHOP-300-refunds");
    fs::create_dir_all(&other).unwrap();
    fs::write(other.join("spec.md"), "").unwrap();
    fs::write(other.join("decision.md"), "").unwrap();
    project
        .invar(&["decision", "approve"])
        .assert()
        .failure()
        .stderr(contains("2 drafts are waiting"))
        .stderr(contains("  SHOP-300-refunds"))
        .stderr(contains("/invar:log-approve <ticket>"));
    assert!(project.entries().is_empty());
}

#[test]
fn approve_without_a_ticket_and_without_a_draft_says_how_to_write_one() {
    let project = Project::new(Some(&approved_plan()));
    project
        .invar(&["decision", "approve"])
        .assert()
        .failure()
        .stderr(contains(
            "there is no draft entry in .invar/plans/ to record",
        ))
        .stderr(contains("/invar:log"));
}

#[test]
fn approve_skill_reports_an_invalid_draft_on_stdout_and_exits_0() {
    let project = Project::new(Some(&approved_plan()));
    project.draft("---\nticket: SHOP-214\n---\n");
    project
        .invar(&["decision", "approve", "--skill", "SHOP-214"])
        .assert()
        .success()
        .stdout(contains(format!(
            "{DRAFT}: the front matter has no `module`"
        )))
        .stdout(contains("error: the draft is not valid"))
        .stderr(predicate::str::is_empty());
    assert!(project.entries().is_empty());
    assert!(project.exists(DRAFT));
}

// --- `acceptance` (decision-entry.md §4.2) ----------------------------------------------

#[test]
fn approve_refuses_a_draft_whose_acceptance_disagrees_with_the_plan() {
    // Example 9.2 says `Acceptance: skipped`.
    let project = Project::new(Some(&approved_plan()));
    project.draft(&VALID_DRAFT.replace(
        "type: confirmed\n",
        "type: confirmed\nacceptance: required\n",
    ));
    project
        .approve("SHOP-214")
        .failure()
        .stderr(contains("the draft says `acceptance: required`"))
        .stderr(contains("approved with `acceptance: skipped`"));
    assert!(project.entries().is_empty());
    assert!(project.exists(DRAFT));
}

#[test]
fn approve_with_the_plans_value_records_it_without_saying_more() {
    let project = Project::new(Some(&approved_plan()));
    project.draft(&VALID_DRAFT.replace(
        "type: confirmed\n",
        "type: confirmed\nacceptance: skipped\n",
    ));
    project
        .approve("SHOP-214")
        .success()
        .stdout(contains("recorded: "))
        .stdout(contains("the Plan's").not());
    assert!(project.read(ENTRY).contains("\nacceptance: skipped\n---\n"));
}

#[test]
fn record_takes_acceptance_as_a_flag_and_leaves_it_out_without_one() {
    let project = Project::new(None);
    project
        .record(&[
            "--ticket",
            "SHOP-9",
            "--type",
            "note",
            "--module",
            "orders",
            "--acceptance",
            "required",
            "-m",
            "With tests.",
        ])
        .success();
    project
        .record(&[
            "--ticket", "SHOP-10", "--type", "note", "--module", "orders", "-m", "Unknown.",
        ])
        .success();
    let texts: Vec<String> = project
        .entries()
        .iter()
        .map(|name| project.read(&format!(".invar/decisions/{name}")))
        .collect();
    assert_eq!(texts.len(), 2);
    assert_eq!(
        texts
            .iter()
            .filter(|text| text.contains("\nacceptance: required\n"))
            .count(),
        1,
        "{texts:?}"
    );
    assert_eq!(
        texts
            .iter()
            .filter(|text| text.contains("acceptance"))
            .count(),
        1,
        "{texts:?}"
    );
}

#[test]
fn record_refuses_an_acceptance_value_that_is_neither() {
    let project = Project::new(None);
    project
        .record(&[
            "--ticket",
            "SHOP-9",
            "--type",
            "note",
            "--module",
            "orders",
            "--acceptance",
            "none",
            "-m",
            "x",
        ])
        .failure()
        .stderr(contains("required"))
        .stderr(contains("skipped"));
    assert!(project.entries().is_empty());
}
