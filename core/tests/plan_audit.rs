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

    /// The gate's command, `plan approve`, as a developer named Rade, with a git config
    /// of its own.
    fn approve(&self, args: &[&str]) -> assert_cmd::assert::Assert {
        let gitconfig = self.root().join(".test-gitconfig");
        fs::write(&gitconfig, "[user]\n\tname = Rade\n").unwrap();
        Command::cargo_bin("invar")
            .unwrap()
            .current_dir(self.root())
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env_remove("XDG_CONFIG_HOME")
            .env("GIT_CONFIG_GLOBAL", &gitconfig)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("HOME", self.root())
            .env("GIT_COMMITTER_DATE", "2026-10-05T00:30:00+02:00")
            .args(["plan", "approve"])
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

// --- report -------------------------------------------------------------------------

const FINDINGS: &str = ".invar/out/SHOP-214-audit/findings.md";
const REPORT: &str = ".invar/out/SHOP-214-audit/report.txt";

/// A project whose audit has begun, with `plan` as the Plan now and `findings`.
fn audited(plan: &str, findings: &str) -> Project {
    let project = Project::new(&draft());
    project.audit(&["begin", "SHOP-214"]).success();
    project.write(SPEC, plan);
    project.write(FINDINGS, findings);
    project
}

/// The draft with the two facts an auditor corrects.
fn corrected() -> String {
    draft()
        .replace("canceled_at", "cancelled_at")
        .replace("OrdersController.php", "OrderController.php")
}

const TWO_EDITS: &str = "\
# SHOP-214
edit: Scope | app/Http/Controllers/OrderController.php:1 | the controller is OrderController.php
edit: Implementation context | database/migrations/2026_01_01_create_orders.php:18 | the column is `cancelled_at`
";

#[test]
fn report_shows_each_edit_as_a_hunk_with_its_reason() {
    let project = audited(&corrected(), TWO_EDITS);
    let out = stdout(&project.audit(&["report", "SHOP-214"]).success());
    assert!(
        out.contains(
            "changes:\n\
             \x20 ## Scope\n\
             \x20   -- app/Http/Controllers/OrdersController.php\n\
             \x20   +- app/Http/Controllers/OrderController.php\n\
             \x20     reason: the controller is OrderController.php \
             (app/Http/Controllers/OrderController.php:1)\n\
             \x20 ## Implementation context\n\
             \x20   -- The order records the time in `canceled_at`.\n\
             \x20   +- The order records the time in `cancelled_at`.\n\
             \x20     reason: the column is `cancelled_at` \
             (database/migrations/2026_01_01_create_orders.php:18)\n\
             plan validate: ok\n"
        ),
        "{out}"
    );
    assert!(
        out.starts_with(&format!("plan audit SHOP-214: {SPEC}\n\n")),
        "{out}"
    );
    assert_eq!(project.read(REPORT).unwrap(), out);
}

#[test]
fn an_edited_section_without_an_edit_entry_is_reported_and_an_extra_reason_too() {
    let project = audited(
        &draft().replace("canceled_at", "cancelled_at"),
        "edit: Scope | app/Models/Order.php:1 | the model is Order.php\n",
    );
    let out = stdout(&project.audit(&["report", "SHOP-214"]).success());
    assert!(
        out.contains(
            "  ## Scope\n\
             \x20   reason with no change: the model is Order.php (app/Models/Order.php:1)\n\
             \x20 ## Implementation context\n\
             \x20   -- The order records the time in `canceled_at`.\n\
             \x20   +- The order records the time in `cancelled_at`.\n\
             \x20     edited without a reason\n"
        ),
        "{out}"
    );
}

#[test]
fn no_change_and_no_finding_is_ready() {
    let project = audited(&draft(), "# nothing to correct\n");
    let out = stdout(&project.audit(&["report", "SHOP-214"]).success());
    assert!(
        out.contains(
            "changes:\n  none\nplan validate: ok\nblocking findings:\n  none\nnotes:\n  none\n"
        ),
        "{out}"
    );
    assert!(
        out.ends_with(
            "plan audit SHOP-214: READY\n\
             this is an agent's assessment of the Plan, not a check; approving is yours.\n"
        ),
        "{out}"
    );
}

#[test]
fn one_blocking_finding_is_not_ready_and_notes_follow_it() {
    let project = audited(
        &draft(),
        "finding: wording | note | Intent | ticket | \"own\" could mean the team's\n\
         finding: criterion-cannot-pass | blocking | Acceptance criteria | app/Models/Order.php:12 | AC1 needs a customer, and orders | have none\n",
    );
    let out = stdout(&project.audit(&["report", "SHOP-214"]).success());
    assert!(
        out.contains(
            "blocking findings:\n\
             \x20 criterion-cannot-pass (Acceptance criteria, app/Models/Order.php:12): AC1 \
             needs a customer, and orders | have none\n\
             notes:\n\
             \x20 wording (Intent, ticket): \"own\" could mean the team's\n\
             body: sha256:"
        ),
        "{out}"
    );
    assert!(
        out.ends_with(
            "plan audit SHOP-214: NOT READY — 1 blocking finding(s)\n\
             this is an agent's assessment of the Plan, not a check; approving is yours.\n"
        ),
        "{out}"
    );
}

#[test]
fn the_report_is_byte_identical_for_the_same_files() {
    let project = audited(&corrected(), TWO_EDITS);
    let first = stdout(&project.audit(&["report", "SHOP-214"]).success());
    let second = stdout(&project.audit(&["report", "SHOP-214"]).success());
    assert_eq!(first, second);
    // And from another directory: paths are the project's, not the shell's.
    let elsewhere = Command::cargo_bin("invar")
        .unwrap()
        .current_dir(project.root().join(".invar"))
        .args(["plan", "audit", "report", "SHOP-214"])
        .assert()
        .success();
    assert_eq!(stdout(&elsewhere), first);
}

#[test]
fn the_report_includes_what_plan_validate_finds() {
    // The auditor left Implementation context empty: reported, not refused.
    let emptied = draft().replace(
        "- AI_CONTEXT: controllers stay thin\n- The order records the time in `canceled_at`.\n",
        "",
    );
    let project = audited(&emptied, "");
    let out = stdout(&project.audit(&["report", "SHOP-214"]).success());
    assert!(
        out.contains(&format!("plan validate: 1 problem(s)\n  {SPEC}:")),
        "{out}"
    );
    assert!(out.contains("Implementation context"), "{out}");
    assert!(out.ends_with("approving is yours.\n"), "{out}");
}

#[test]
fn the_body_line_is_the_digest_of_the_plan_now() {
    let project = audited(&corrected(), TWO_EDITS);
    let out = stdout(&project.audit(&["report", "SHOP-214"]).success());
    let body = out
        .lines()
        .find_map(|line| line.strip_prefix("body: sha256:"))
        .unwrap();
    assert_eq!(body.len(), 64);
    // Another body, another digest.
    project.write(SPEC, &draft());
    let again = stdout(&project.audit(&["report", "SHOP-214"]).success());
    assert!(!again.contains(body), "{again}");
}

#[test]
fn an_edit_of_the_acceptance_criteria_is_refused_and_named() {
    let project = audited(
        &corrected().replace(
            "then it is cancelled.",
            "then it is cancelled and refunded.",
        ),
        TWO_EDITS,
    );
    project
        .audit(&["report", "SHOP-214"])
        .failure()
        .stdout("")
        .stderr(contains("changed what an audit may not"))
        .stderr(contains("`## Acceptance criteria` changed"))
        .stderr(contains(format!("`git diff {SPEC}`")))
        .stderr(contains("--restart"));
    assert!(!project.exists(REPORT));
    // Nothing is restored.
    assert!(project.read(SPEC).unwrap().contains("and refunded"));
}

#[test]
fn a_header_change_is_refused() {
    let project = audited(&draft().replace("Module: orders", "Module: billing"), "");
    project
        .audit(&["report", "SHOP-214"])
        .failure()
        .stderr(contains("the header changed"));
    assert!(!project.exists(REPORT));
}

#[test]
fn a_plan_that_is_no_longer_a_draft_is_refused() {
    let approved = draft()
        .replace("Status: draft", "Status: approved")
        .replace("Approved-by:", "Approved-by: rade")
        .replace("Approved-at:", "Approved-at: 2026-10-05")
        .replace(
            "Approved-checksum:",
            &format!("Approved-checksum: sha256:{}", "ab".repeat(32)),
        );
    let project = audited(&approved, "");
    project
        .audit(&["report", "SHOP-214"])
        .failure()
        .stderr(contains("no longer a draft"))
        .stderr(contains("the header changed"));
}

#[test]
fn a_section_removed_added_or_moved_is_refused() {
    let removed = draft().replace("## Rejected alternatives\nNone considered.\n\n", "");
    let project = audited(&removed, "");
    project
        .audit(&["report", "SHOP-214"])
        .failure()
        .stderr(contains(
            "a section was removed or renamed: `## Rejected alternatives`",
        ));

    let added = format!("{}\n## Notes\nAn aside.\n", draft());
    let project = audited(&added, "");
    project
        .audit(&["report", "SHOP-214"])
        .failure()
        .stderr(contains("a section was added: `## Notes`"));

    // A heading slipped into Implementation context is a section added, not an edit.
    let inside = draft().replace(
        "- The order records",
        "## Scope\n- app/Jobs/Refund.php\n\n- The order records",
    );
    let project = audited(&inside, "");
    project
        .audit(&["report", "SHOP-214"])
        .failure()
        .stderr(contains("moved or repeated"));
}

#[test]
fn report_is_refused_before_begin_and_without_findings() {
    let project = Project::new(&draft());
    project
        .audit(&["report", "SHOP-214"])
        .failure()
        .stderr(contains("no audit of SHOP-214 has begun"));
    project.audit(&["begin", "SHOP-214"]).success();
    project
        .audit(&["report", "SHOP-214"])
        .failure()
        .stderr(contains("has written no findings"));
}

#[test]
fn malformed_findings_are_refused_each_with_its_line() {
    let project = audited(
        &draft(),
        "edit: Acceptance criteria | a.php:1 | reworded\n\
         finding: wording | blocking | Intent | ticket | vague\n",
    );
    project
        .audit(&["report", "SHOP-214"])
        .failure()
        .stderr(contains(format!("{FINDINGS}:1:")))
        .stderr(contains(format!("{FINDINGS}:2:")))
        .stderr(contains("not a blocking kind"));
    assert!(!project.exists(REPORT));
}

// --- the line at the gate -----------------------------------------------------------

/// The lines from the audit line to `Acceptance:`: where the gate shows it.
fn gate(out: &str) -> &str {
    let start = out
        .find("plan audit: ")
        .unwrap_or_else(|| panic!("no audit line in:\n{out}"));
    let end = out[start..].find("Acceptance: ").unwrap() + start;
    &out[start..end]
}

#[test]
fn the_gate_says_not_run_when_there_is_no_report() {
    let project = Project::new(&draft());
    let out = stdout(&project.approve(&["SHOP-214"]).success());
    assert_eq!(gate(&out), "plan audit: not run\n\n");
    // After the criteria and the invariants.
    assert!(
        out.find("INV-2").unwrap() < out.find("plan audit: ").unwrap(),
        "{out}"
    );
    assert!(out.contains("approved SHOP-214 checksum sha256:"), "{out}");

    // An audit begun and never reported is not run either, and nor is a report.txt
    // whose last lines are not a report's.
    let project = Project::new(&draft());
    project.audit(&["begin", "SHOP-214"]).success();
    project.write(REPORT, "plan audit SHOP-214: READY\n");
    let out = stdout(&project.approve(&["SHOP-214"]).success());
    assert_eq!(gate(&out), "plan audit: not run\n\n");
}

#[test]
fn the_gate_says_ready_for_this_version_of_the_plan() {
    let project = audited(&corrected(), TWO_EDITS);
    project.audit(&["report", "SHOP-214"]).success();
    let out = stdout(&project.approve(&["SHOP-214"]).success());
    assert_eq!(
        gate(&out),
        "plan audit: READY, for this version of the Plan\n\n"
    );
}

#[test]
fn the_gate_says_not_ready_and_approves_all_the_same() {
    let project = audited(
        &draft(),
        "finding: criterion-cannot-pass | blocking | Acceptance criteria | a.php:1 | no\n\
         finding: invariant-not-named | blocking | Implementation context | BUSINESS_CONTEXT.md:7 | INV-2\n",
    );
    project.audit(&["report", "SHOP-214"]).success();
    let out = stdout(&project.approve(&["SHOP-214", "--skill"]).success());
    assert_eq!(
        gate(&out),
        "plan audit: NOT READY (2 blocking), for this version of the Plan\n\n"
    );
    assert!(out.contains("approved SHOP-214 checksum sha256:"), "{out}");
    assert!(project.read(SPEC).unwrap().contains("Status: approved"));
}

#[test]
fn the_gate_says_when_the_plan_changed_after_the_audit() {
    let project = audited(
        &draft(),
        "finding: criteria-contradict | blocking | Acceptance criteria | ticket | AC1 and AC1\n",
    );
    project.audit(&["report", "SHOP-214"]).success();
    project.write(
        SPEC,
        &draft().replace("None considered.", "A queue, rejected."),
    );
    let out = stdout(&project.approve(&["SHOP-214"]).success());
    assert_eq!(
        gate(&out),
        "plan audit: NOT READY (1 blocking), but the Plan changed after it\n\n"
    );

    let project = audited(&draft(), "");
    project.audit(&["report", "SHOP-214"]).success();
    project.write(
        SPEC,
        &draft().replace("None considered.", "A queue, rejected."),
    );
    let out = stdout(&project.approve(&["SHOP-214"]).success());
    assert_eq!(
        gate(&out),
        "plan audit: READY, but the Plan changed after it\n\n"
    );
}
