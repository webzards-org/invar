//! End-to-end tests for `invar pr-body` (M1-16): the real binary, a real
//! project.

use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use predicates::str::contains;
use tempfile::TempDir;

const SPEC: &str = ".invar/plans/SHOP-214-cancel-unpaid-order/spec.md";

fn contract_example(number: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../contracts/formats/plan-header.md");
    let text = fs::read_to_string(path).unwrap();
    let after = text.split_once(&format!("### {number} ")).unwrap().1;
    let body = after.split_once("```markdown\n").unwrap().1;
    body.split_once("\n```\n").unwrap().0.to_owned() + "\n"
}

fn draft_plan() -> String {
    contract_example("9.1")
}

fn approved_plan() -> String {
    let draft = draft_plan();
    contract_example("9.2") + draft.split_once("\n---\n").unwrap().1
}

struct Project {
    dir: TempDir,
}

impl Project {
    fn new(plan: &str) -> Self {
        let project = Self {
            dir: tempfile::tempdir().unwrap(),
        };
        fs::write(project.root().join("invar.yaml"), "").unwrap();
        let spec = project.root().join(SPEC);
        fs::create_dir_all(spec.parent().unwrap()).unwrap();
        fs::write(spec, plan).unwrap();
        project
    }

    fn root(&self) -> PathBuf {
        self.dir.path().canonicalize().unwrap()
    }

    fn pr_body(&self, args: &[&str]) -> assert_cmd::assert::Assert {
        let mut all = vec!["pr-body"];
        all.extend_from_slice(args);
        Command::cargo_bin("invar")
            .unwrap()
            .current_dir(self.root())
            .args(&all)
            .assert()
    }
}

#[test]
fn renders_the_fixed_sections_in_order_to_stdout() {
    let project = Project::new(&approved_plan());
    let assert = project.pr_body(&["SHOP-214"]).success();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();

    assert!(
        stdout.starts_with("# SHOP-214\n\nApproved by rade on 2026-10-05.\n"),
        "{stdout}"
    );
    assert!(!stdout.contains("## Ticket\n"), "{stdout}");
    let order = [
        "## Intent",
        "## Scope",
        "## Open questions / answers",
        "## Rejected alternatives",
        "## Acceptance criteria",
        "## Implementation context",
    ];
    let positions: Vec<usize> = order.iter().map(|h| stdout.find(h).unwrap()).collect();
    assert!(positions.windows(2).all(|w| w[0] < w[1]), "{stdout}");
    assert!(
        stdout.contains("Given a pending, unpaid order of mine"),
        "{stdout}"
    );
}

#[test]
fn with_ticket_includes_the_pasted_text_first() {
    let project = Project::new(&approved_plan());
    let assert = project.pr_body(&["SHOP-214", "--with-ticket"]).success();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();

    let ticket_pos = stdout.find("## Ticket").unwrap();
    let intent_pos = stdout.find("## Intent").unwrap();
    assert!(ticket_pos < intent_pos, "{stdout}");
    assert!(
        stdout.contains("As a customer I want to cancel"),
        "{stdout}"
    );
}

#[test]
fn a_draft_plan_is_refused() {
    let project = Project::new(&draft_plan());
    project
        .pr_body(&["SHOP-214"])
        .failure()
        .stderr(contains("is still a draft"))
        .stderr(contains("invar plan approve SHOP-214"));
}

#[test]
fn renders_to_a_file_with_out() {
    let project = Project::new(&approved_plan());
    let out = project.root().join("pr.md");
    let out_str = out.display().to_string();
    project
        .pr_body(&["SHOP-214", "--out", &out_str])
        .success()
        .stdout(contains("pr.md"));
    let text = fs::read_to_string(&out).unwrap();
    assert!(text.starts_with("# SHOP-214"), "{text}");
}

#[test]
fn rendering_the_same_plan_twice_is_byte_identical() {
    let project = Project::new(&approved_plan());
    let a = project
        .pr_body(&["SHOP-214"])
        .success()
        .get_output()
        .stdout
        .clone();
    let b = project
        .pr_body(&["SHOP-214"])
        .success()
        .get_output()
        .stdout
        .clone();
    assert_eq!(a, b);
}

#[test]
fn it_works_by_path_too() {
    let project = Project::new(&approved_plan());
    let spec = project.root().join(SPEC).display().to_string();
    project
        .pr_body(&[&spec])
        .success()
        .stdout(contains("# SHOP-214"));
}

// --- the exact output, for the contract's own example -----------------------------------

/// Compare `actual` with the stored snapshot `tests/snapshots/<name>`.
///
/// The snapshots are what a reviewer and CodeRabbit read, byte for byte, so a
/// change to them is a change to the product and has to show up in a diff:
/// re-run with `INVAR_BLESS_SNAPSHOTS=1` to rewrite them, and read the diff.
/// Line endings in the stored file are not compared: a checkout with `autocrlf`
/// must not fail a test about content.
fn assert_snapshot(name: &str, actual: &str) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/snapshots")
        .join(name);
    if std::env::var_os("INVAR_BLESS_SNAPSHOTS").is_some() {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, actual).unwrap();
        return;
    }
    let expected = fs::read_to_string(&path)
        .unwrap_or_else(|err| {
            panic!(
                "cannot read the snapshot {} ({err}); run with INVAR_BLESS_SNAPSHOTS=1 to \
                 create it",
                path.display()
            )
        })
        .replace("\r\n", "\n");
    assert_eq!(
        actual,
        expected,
        "the output differs from {}; if that is intended, run with INVAR_BLESS_SNAPSHOTS=1 \
         and review the diff",
        path.display()
    );
}

fn stdout_of(assert: &assert_cmd::assert::Assert) -> String {
    String::from_utf8(assert.get_output().stdout.clone()).unwrap()
}

#[test]
fn the_contracts_example_plan_renders_exactly_the_stored_description() {
    // Example 9.1's body under example 9.2's header: the approved Plan.
    let project = Project::new(&approved_plan());

    let stdout = stdout_of(&project.pr_body(&["SHOP-214"]).success());

    assert_snapshot("pr-body-shop-214.md", &stdout);
}

#[test]
fn the_same_example_with_the_ticket_text_renders_exactly_the_stored_description() {
    let project = Project::new(&approved_plan());

    let stdout = stdout_of(&project.pr_body(&["SHOP-214", "--with-ticket"]).success());

    assert_snapshot("pr-body-shop-214-with-ticket.md", &stdout);
}

#[test]
fn the_file_out_writes_is_byte_for_byte_what_stdout_prints() {
    let project = Project::new(&approved_plan());
    let printed = project
        .pr_body(&["SHOP-214"])
        .success()
        .get_output()
        .stdout
        .clone();

    project.pr_body(&["SHOP-214", "--out", "pr.md"]).success();

    assert_eq!(fs::read(project.root().join("pr.md")).unwrap(), printed);
}

#[test]
fn out_creates_the_directory_it_is_asked_to_write_into() {
    // `.invar/out/` is gitignored and `init` does not make it.
    let project = Project::new(&approved_plan());

    project
        .pr_body(&["SHOP-214", "--out", ".invar/out/SHOP-214-pr.md"])
        .success()
        .stdout(".invar/out/SHOP-214-pr.md\n");

    let text = fs::read_to_string(project.root().join(".invar/out/SHOP-214-pr.md")).unwrap();
    assert!(text.starts_with("# SHOP-214\n"), "{text}");
}

#[test]
fn a_draft_that_is_refused_writes_no_file_even_when_asked_to() {
    let project = Project::new(&draft_plan());
    let out = project.root().join("pr.md");

    project
        .pr_body(&["SHOP-214", "--out", "pr.md"])
        .failure()
        .stderr(contains("is still a draft"));

    assert!(
        !out.exists(),
        "a description of an unapproved Plan was written"
    );
    assert!(!project.root().join(".invar/out").exists());
}

// --- only a Plan that is approved, and unchanged since, is described ----------------------
//
// The same check `plan check` and `invar done` make: a description written
// under an "Approved by …" line has to be true of the text below it.

const WHY: &str = "a PR description describes an approved specification, and this Plan is not one";

#[test]
fn a_draft_is_refused_with_the_reason_once() {
    let project = Project::new(&draft_plan());

    let assert = project
        .pr_body(&["SHOP-214"])
        .failure()
        .stdout("")
        .stderr(contains(WHY))
        .stderr(contains("invar plan approve SHOP-214"));

    // Not printed twice, as an error whose cause repeats it would be.
    let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
    assert_eq!(stderr.matches("is still a draft").count(), 1, "{stderr}");
}

#[test]
fn a_plan_edited_after_approval_is_refused_and_nothing_is_written() {
    // The case a rule that only refuses drafts lets through: approved, then
    // changed. Its approval is not an approval of this text.
    let project = Project::new(&approved_plan());
    let edited = approved_plan().replace("refused with 403", "refused with 409");
    assert_ne!(edited, approved_plan());
    fs::write(project.root().join(SPEC), edited).unwrap();

    project
        .pr_body(&["SHOP-214", "--out", "pr.md"])
        .failure()
        .stdout("")
        .stderr(contains(WHY))
        .stderr(contains("changed after it was approved"))
        .stderr(contains("rade approved it on 2026-10-05"));

    assert!(!project.root().join("pr.md").exists());
    assert!(!project.root().join(".invar/out").exists());
}

#[test]
fn an_approval_nobody_recorded_is_refused() {
    // plan-header.md example 9.3: `Status: approved`, and no checksum.
    let unrecorded = approved_plan().replace(
        "Approved-checksum: sha256:b017737bb6bfa67784b6eb2979946c4703c7e28f37ee363501ca0b886978aea5",
        "Approved-checksum:",
    );
    assert_ne!(unrecorded, approved_plan());
    let project = Project::new(&unrecorded);

    project
        .pr_body(&["SHOP-214"])
        .failure()
        .stdout("")
        .stderr(contains(WHY))
        .stderr(contains(
            "says `Status: approved`, but `Approved-checksum` is empty",
        ));
}

#[test]
fn a_plan_that_is_not_valid_lists_its_problems_and_is_refused() {
    // A misspelled heading: its approval cannot even be read.
    let project = Project::new(
        &approved_plan().replace("## Acceptance criteria\n", "## Acceptance criterion\n"),
    );

    project
        .pr_body(&["SHOP-214"])
        .failure()
        // What the error points at when it says "listed above".
        .stdout(contains("spec.md:"))
        .stdout(contains("Acceptance criteria"))
        .stderr(contains(WHY))
        .stderr(contains("the Plan is not valid"))
        .stderr(contains("listed above"));
}

#[test]
fn an_approved_plan_checked_out_with_crlf_is_still_approved_and_renders_the_same_bytes() {
    // Approval is over canonical text, so a checkout that turned LF into CRLF
    // does not undo it, and the description does not depend on line endings.
    let project = Project::new(&approved_plan().replace('\n', "\r\n"));

    let stdout = stdout_of(&project.pr_body(&["SHOP-214"]).success());

    assert_snapshot("pr-body-shop-214.md", &stdout);
}

// --- `## Acceptance tests to change` (plan-header.md §6.5) -----------------------

/// Example 9.1 with `content` as its `## Acceptance tests to change`, under
/// example 9.2's header with the checksum of this body: approved, and current.
fn approved_with_tests_to_change(content: &str) -> String {
    let context = "## Implementation context\n";
    let draft = draft_plan().replacen(
        context,
        &format!("## Acceptance tests to change\n{content}\n{context}"),
        1,
    );
    let body = draft.split_once("\n---\n").unwrap().1;

    let scratch = tempfile::tempdir().unwrap();
    let file = scratch.path().join("body.md");
    fs::write(&file, body.trim_start_matches('\n')).unwrap();
    let output = Command::cargo_bin("invar")
        .unwrap()
        .arg("hash")
        .arg(&file)
        .output()
        .unwrap();
    let checksum = String::from_utf8(output.stdout).unwrap();

    let header = contract_example("9.12")
        .lines()
        .map(|line| match line.strip_prefix("Approved-checksum:") {
            Some(_) => format!("Approved-checksum: {}\n", checksum.trim()),
            None => format!("{line}\n"),
        })
        .collect::<String>();
    header + body
}

const TESTS_TO_CHANGE: &str = "- tests/Acceptance/Shop190PaidTest.php — change — its 403 no longer holds within 24 hours\n\
     - tests/Acceptance/Shop177RefundTest.php — delete — refunds moved to SHOP-301\n";

#[test]
fn a_plan_with_tests_to_change_renders_exactly_the_stored_description() {
    let project = Project::new(&approved_with_tests_to_change(TESTS_TO_CHANGE));
    let stdout = stdout_of(&project.pr_body(&["SHOP-214"]).success().stderr(""));
    assert_snapshot("pr-body-shop-214-tests-to-change.md", &stdout);
}

#[test]
fn a_section_that_says_none_renders_as_a_plan_without_it() {
    // Example 9.12's header, as `approved_with_tests_to_change` writes: no
    // `Acceptance`, so the Plan without the section is the one of 9.12 too.
    let project = Project::new(&approved_with_tests_to_change("_none_\n"));
    let stdout = stdout_of(&project.pr_body(&["SHOP-214"]).success());
    assert_snapshot("pr-body-shop-214-no-acceptance-field.md", &stdout);
}

// --- how acceptance was decided (AT-15) ------------------------------------------

const ACCEPTANCE_CONFIG: &str = "acceptance:\n  dir: tests/Acceptance\n  \
    run: [\"sh\", \"runner.sh\", \"{report}\", \"{files}\"]\n";

/// Example 9.1 approved with `--acceptance` (example 9.10): `required`, and current.
fn required_plan() -> String {
    let draft = draft_plan();
    contract_example("9.10") + draft.split_once("\n---\n").unwrap().1
}

/// Example 9.1 approved under 1.6.0 (example 9.12): no `Acceptance` at all.
fn plan_without_the_field() -> String {
    let draft = draft_plan();
    contract_example("9.12") + draft.split_once("\n---\n").unwrap().1
}

impl Project {
    fn write(&self, path: &str, text: &str) {
        let path = self.root().join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    /// `text` under a lock line for `ticket`, with the digest `invar hash --lock`
    /// prints for it.
    fn write_locked(&self, path: &str, ticket: &str, text: &str) {
        let pending = format!("<?php\n// invar-lock: {ticket}\n{text}");
        self.write(path, &pending);
        let output = Command::cargo_bin("invar")
            .unwrap()
            .args(["hash", "--lock"])
            .arg(self.root().join(path))
            .output()
            .unwrap();
        let digest = String::from_utf8(output.stdout).unwrap();
        self.write(
            path,
            &pending.replace(
                &format!("invar-lock: {ticket}"),
                &format!("invar-lock: {ticket} {}", digest.trim()),
            ),
        );
    }

    /// An approved `required` Plan with two locked files that name different
    /// criteria, a pending one, and one locked for another ticket.
    fn with_locked_tests() -> Self {
        let project = Self::new(&required_plan());
        project.write("invar.yaml", ACCEPTANCE_CONFIG);
        project.write_locked(
            "tests/Acceptance/Shop214OtherTeamTest.php",
            "SHOP-214",
            "it('ac_3: another team gets 404', fn () => ...);\n\
             it('AC4 the history lists it', fn () => ...);\n",
        );
        project.write_locked(
            "tests/Acceptance/Shop214CancelTest.php",
            "SHOP-214",
            "it('AC2 a paid order is refused', fn () => ...);\n\
             it('AC1: cancels and releases the stock', fn () => ...);\n\
             // ACME and vac3 are not tokens; AC1 again is the same one.\n",
        );
        // Neither is this ticket's lock: one is not locked yet, one is another ticket's.
        project.write(
            "tests/Acceptance/Shop214PendingTest.php",
            "<?php\n// invar-lock: SHOP-214\nit('AC9 later');\n",
        );
        project.write_locked(
            "tests/Acceptance/Shop190PaidTest.php",
            "SHOP-190",
            "it('AC7 is another ticket');\n",
        );
        project
    }
}

#[test]
fn required_lists_the_locked_files_and_the_criteria_each_names() {
    let project = Project::with_locked_tests();
    let stdout = stdout_of(&project.pr_body(&["SHOP-214"]).success().stderr(""));
    assert_snapshot("pr-body-shop-214-required.md", &stdout);
}

#[test]
fn required_with_an_unlock_record_lists_what_the_developer_unlocked() {
    let project = Project::with_locked_tests();
    project.write(
        ".invar/plans/SHOP-214-cancel-unpaid-order/unlocked",
        "round: 2\nclosed 1 tests/Acceptance/Shop214CancelTest.php\n\
         closed 2 tests/Acceptance/Shop214OtherTeamTest.php\n",
    );
    let stdout = stdout_of(&project.pr_body(&["SHOP-214"]).success().stderr(""));
    assert_snapshot("pr-body-shop-214-required-unlocked.md", &stdout);
}

#[test]
fn an_unlock_record_that_cannot_be_read_is_warned_about_and_left_out() {
    let project = Project::with_locked_tests();
    project.write(
        ".invar/plans/SHOP-214-cancel-unpaid-order/unlocked",
        "round: 7\n",
    );
    let stdout = stdout_of(
        &project
            .pr_body(&["SHOP-214"])
            .success()
            .stderr(contains("warning: "))
            .stderr(contains("says nothing about unlocking")),
    );
    assert_snapshot("pr-body-shop-214-required.md", &stdout);
}

#[test]
fn skipped_says_so_in_the_first_lines() {
    // Example 9.2 says `Acceptance: skipped`.
    let project = Project::new(&approved_plan());
    project.write("invar.yaml", ACCEPTANCE_CONFIG);
    let stdout = stdout_of(&project.pr_body(&["SHOP-214"]).success());
    assert!(
        stdout.starts_with(
            "# SHOP-214\n\nApproved by rade on 2026-10-05.\n\n\
             **No acceptance tests:** this ticket was approved without them.\n\n## Intent\n"
        ),
        "{stdout}"
    );
    assert_snapshot("pr-body-shop-214.md", &stdout);
}

#[test]
fn a_plan_approved_before_the_field_says_there_were_no_tests() {
    let project = Project::new(&plan_without_the_field());
    let stdout = stdout_of(&project.pr_body(&["SHOP-214"]).success());
    assert_snapshot("pr-body-shop-214-no-acceptance-field.md", &stdout);
}

#[test]
fn required_without_a_config_section_warns_and_lists_no_files() {
    let project = Project::new(&required_plan());
    // A locked file is there, and there is no `acceptance.dir` to find it by.
    project.write_locked(
        "tests/Acceptance/Shop214CancelTest.php",
        "SHOP-214",
        "it('AC1 cancels');\n",
    );
    let stdout = stdout_of(&project.pr_body(&["SHOP-214"]).success());
    assert_snapshot("pr-body-shop-214-required-no-config.md", &stdout);
}

#[test]
fn the_same_files_render_the_same_bytes() {
    let project = Project::with_locked_tests();
    let a = stdout_of(&project.pr_body(&["SHOP-214"]).success());
    let b = stdout_of(&project.pr_body(&["SHOP-214"]).success());
    assert_eq!(a, b);
}
