//! End-to-end tests for the Plan audit (`contracts/formats/plan-audit.md`): the real
//! binary in throwaway project trees.

use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use predicates::prelude::*;
use predicates::str::contains;
use tempfile::TempDir;

const SPEC: &str = ".invar/plans/SHOP-214-cancel-unpaid-order/spec.md";
const AUDIT: &str = ".invar/out/SHOP-214-audit";

fn draft() -> String {
    "---\nTicket: SHOP-214\nModule: orders\nStatus: draft\nApproved-by:\n\
     Approved-at:\nApproved-checksum:\n---\n\n\
     ## Ticket\nSHOP-214 — Customers cancel their own unpaid orders.\n\n\
     ## Intent\nCustomers cancel their own unpaid orders.\n\n\
     ## Scope\n- app/Http/Controllers/OrdersController.php\n- app/Models/Order.php\n\n\
     ## Open questions / answers\nNone were needed.\n\n\
     ## Rejected alternatives\nNone considered.\n\n\
     ## Acceptance criteria\n\n\
     - AC1: Given an unpaid order, when its customer cancels it, then it is cancelled.\n\n\
     ## Implementation context\n\
     - AI_CONTEXT: controllers stay thin\n\
     - The order records the time in `canceled_at`.\n"
        .to_owned()
}

const BUSINESS: &str = "<!-- invar-context: v1 -->\n# Business context\n\n## Invariants\n\n\
    - INV-1: every tenant-scoped query is filtered by the account\n\
    - INV-2: cancelling an order releases its reserved stock\n";

const AI: &str = "<!-- invar-context: v1 -->\n# AI context\n\n## Conventions\n\n\
    - Controllers stay thin.\n";

/// A project with the draft Plan, both context files and one decision entry.
struct Project {
    dir: TempDir,
}

impl Project {
    fn new(plan: &str) -> Self {
        let project = Self {
            dir: tempfile::tempdir().unwrap(),
        };
        project.write("invar.yaml", "scenarios: false\n");
        project.write(SPEC, plan);
        project.write("BUSINESS_CONTEXT.md", BUSINESS);
        project.write("AI_CONTEXT.md", AI);
        project.write(
            ".invar/decisions/2026-10-06-163712-orders-shop-100.md",
            "---\nid: d-7c41e09a\ndate: 2026-10-06T18:37:12+02:00\nticket: SHOP-100\n\
             module: orders\ntype: note\n---\n\nA SECRET FROM THE DECISION LOG.\n",
        );
        project
    }

    fn root(&self) -> PathBuf {
        self.dir.path().canonicalize().unwrap()
    }

    fn write(&self, relative: &str, text: &str) {
        let path = self.root().join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    fn read(&self, relative: &str) -> Option<String> {
        fs::read_to_string(self.root().join(relative)).ok()
    }

    fn exists(&self, relative: &str) -> bool {
        self.root().join(relative).exists()
    }

    fn invar(&self, args: &[&str]) -> assert_cmd::assert::Assert {
        Command::cargo_bin("invar")
            .unwrap()
            .current_dir(self.root())
            .args(args)
            .assert()
    }

    fn audit(&self, args: &[&str]) -> assert_cmd::assert::Assert {
        let mut all = vec!["plan", "audit"];
        all.extend_from_slice(args);
        self.invar(&all)
    }
}

fn stdout(assert: &assert_cmd::assert::Assert) -> String {
    String::from_utf8(assert.get_output().stdout.clone()).unwrap()
}

/// Compare with `tests/snapshots/<name>`; `INVAR_BLESS_SNAPSHOTS=1` writes it.
fn assert_snapshot(name: &str, actual: &str) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/snapshots")
        .join(name);
    if std::env::var_os("INVAR_BLESS_SNAPSHOTS").is_some() {
        fs::write(&path, actual).unwrap();
        return;
    }
    let expected = fs::read_to_string(&path).unwrap_or_else(|err| {
        panic!(
            "cannot read the snapshot {} ({err}); run with INVAR_BLESS_SNAPSHOTS=1 to write it",
            path.display()
        )
    });
    assert_eq!(actual, expected, "the output differs from {name}");
}

// --- begin --------------------------------------------------------------------------

#[test]
fn begin_records_the_plan_as_the_baseline() {
    let project = Project::new(&draft());
    project
        .audit(&["begin", "SHOP-214"])
        .success()
        .stdout(contains("plan audit begun: SHOP-214"))
        .stdout(contains(format!("  baseline: {AUDIT}/baseline.md")));
    assert_eq!(
        project.read(&format!("{AUDIT}/baseline.md")).unwrap(),
        draft()
    );
}

#[test]
fn begin_refuses_an_approved_plan_and_an_invalid_one() {
    let approved = draft()
        .replace("Status: draft", "Status: approved")
        .replace("Approved-by:", "Approved-by: rade")
        .replace("Approved-at:", "Approved-at: 2026-10-05")
        .replace(
            "Approved-checksum:",
            &format!("Approved-checksum: sha256:{}", "ab".repeat(32)),
        );
    let project = Project::new(&approved);
    project
        .audit(&["begin", "SHOP-214"])
        .failure()
        .stderr(contains("is approved, and an audit is of a draft"));
    assert!(!project.exists(AUDIT));

    let project = Project::new(&draft().replace("## Intent\n", "## Intnet\n"));
    project
        .audit(&["begin", "SHOP-214"])
        .failure()
        .stdout(contains("Intent"))
        .stderr(contains("is not a valid Plan"));
    assert!(!project.exists(AUDIT));
}

#[test]
fn a_second_begin_is_refused_unless_restarted() {
    let project = Project::new(&draft());
    project.audit(&["begin", "SHOP-214"]).success();
    project.write(&format!("{AUDIT}/findings.md"), "# old\n");
    project.write(&format!("{AUDIT}/report.txt"), "old\n");
    // An edit, then begin again: the baseline must not move past it.
    project.write(SPEC, &draft().replace("None considered.", "Edited."));
    project
        .audit(&["begin", "SHOP-214"])
        .failure()
        .stderr(contains("an audit of this Plan has begun already"))
        .stderr(contains("`--restart`"));
    assert_eq!(
        project.read(&format!("{AUDIT}/baseline.md")).unwrap(),
        draft()
    );

    project.audit(&["begin", "SHOP-214", "--restart"]).success();
    assert!(
        project
            .read(&format!("{AUDIT}/baseline.md"))
            .unwrap()
            .contains("Edited.")
    );
    assert!(!project.exists(&format!("{AUDIT}/findings.md")));
    assert!(!project.exists(&format!("{AUDIT}/report.txt")));
}

#[test]
fn a_refusal_reaches_the_skill_on_stdout() {
    let project = Project::new(&draft());
    project.audit(&["begin", "SHOP-214"]).success();
    project
        .audit(&["begin", "--skill", "SHOP-214"])
        .code(0)
        .stdout(contains("error: an audit of this Plan has begun already"))
        .stdout(contains("plan audit begun").not());
    // An argument clap refuses reaches the skill too.
    project
        .audit(&["begin", "--skill", "SHOP-214", "--bogus"])
        .code(0)
        .stdout(contains("error:"));
}

// --- input --------------------------------------------------------------------------

#[test]
fn input_is_refused_before_begin() {
    let project = Project::new(&draft());
    project
        .audit(&["input", "SHOP-214"])
        .failure()
        .stderr(contains("no audit of SHOP-214 has begun"))
        .stderr(contains("/invar:plan-audit SHOP-214"));
}

#[test]
fn input_is_the_plan_the_context_and_the_rules_and_nothing_from_the_log() {
    let project = Project::new(&draft());
    project.audit(&["begin", "SHOP-214"]).success();
    let first = stdout(&project.audit(&["input", "SHOP-214"]).success());
    let second = stdout(&project.audit(&["input", "SHOP-214"]).success());
    assert_eq!(first, second);
    assert!(!first.contains("A SECRET FROM THE DECISION LOG"), "{first}");
    assert!(!first.contains(".invar/decisions"), "{first}");
    assert_snapshot("plan-audit-input-shop-214.txt", &first);
}
