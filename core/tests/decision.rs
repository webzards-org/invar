//! End-to-end tests for `invar decision record` and `invar decision log`
//! (M1-11): the real binary, a real project, and git's clock fixed through
//! `GIT_COMMITTER_DATE`.

use std::fs;
use std::path::{Path, PathBuf};

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
        .code(2)
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
    assert!(reset.contains("Status: draft\nApproved-by:\nApproved-at:\nApproved-checksum:\n---\n"));
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
