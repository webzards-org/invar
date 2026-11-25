//! A signed `Invar-Skip` (`approval-signature.md` §6.5, `invar-yaml.md` §3.8): made by
//! the real `invar cycle skip` with a real key, checked by `invar cycle check`.

mod common;

use common::{BRANCH, Project, fingerprint, last_line, stdout};
use predicates::prelude::*;
use predicates::str::contains;

const REQUIRED: &str = "cycle:\n  required_paths:\n    - \"app/Models/**\"\n";

fn signed_config() -> String {
    format!("approval:\n  signed: true\n{REQUIRED}")
}

/// A signed project, and a branch with one commit to a required path.
fn changed() -> (Project, std::path::PathBuf) {
    let (project, key) = Project::signed(&signed_config());
    project.write("app/Models/Order.php", "<?php // v2\n");
    project.commit_all("fix(orders): round the total");
    (project, key)
}

fn skip(project: &Project, args: &[&str]) -> assert_cmd::assert::Assert {
    let mut all = vec![
        "cycle",
        "skip",
        "--reason",
        "a rounding fix, no behavior change",
    ];
    all.extend_from_slice(args);
    project.invar(&all)
}

fn check(project: &Project) -> assert_cmd::assert::Assert {
    project.invar(&["cycle", "check", "--base", "main"])
}

fn fails(project: &Project) -> String {
    let assert = check(project).failure();
    assert_eq!(
        last_line(&assert),
        "cycle check: failed",
        "{}",
        stdout(&assert)
    );
    stdout(&assert)
}

#[test]
fn a_signed_skip_passes_and_names_who_signed() {
    let (project, key) = changed();
    // What else is staged stays staged, and out of the commit.
    project.write("notes.txt", "staged\n");
    project.git(&["add", "notes.txt"]);
    skip(&project, &[])
        .success()
        .stdout(contains("Required changes this skip covers, against main:"))
        .stdout(contains("  app/Models/Order.php (app/Models/**)"))
        .stdout(contains("skip signed: 1 required change(s)"));
    assert_eq!(
        project.git(&["diff", "--cached", "--name-only"]).trim(),
        "notes.txt"
    );
    assert_eq!(
        project
            .git(&["show", "--name-only", "--format=", "HEAD"])
            .trim(),
        ""
    );

    let assert = check(&project).success();
    let out = stdout(&assert);
    assert!(
        out.contains(&format!(
            ": Invar-Skip: a rounding fix, no behavior change (signed by dev@example.com, {})",
            fingerprint(&key)
        )),
        "{out}"
    );
    assert_eq!(
        last_line(&assert),
        "cycle check: passed: skipped by `Invar-Skip:` (1 signed trailer(s))"
    );
}

#[test]
fn print_gives_the_two_trailers_and_commits_nothing() {
    let (project, _) = changed();
    let before = project.head();
    let assert = skip(&project, &["--print"]).success();
    let out = stdout(&assert);
    let lines: Vec<&str> = out.lines().rev().take(2).collect();
    assert!(
        lines[0].starts_with("Invar-Skip-Signature: U1NIU0lH"),
        "{out}"
    );
    assert_eq!(lines[1], "Invar-Skip: a rounding fix, no behavior change");
    assert_eq!(project.head(), before);
}

#[test]
fn a_bare_trailer_does_not_count_when_signing_is_on() {
    let (project, _) = changed();
    project.git(&[
        "commit",
        "-q",
        "--allow-empty",
        "-m",
        "chore: skip",
        "--trailer",
        "Invar-Skip: trust me",
    ]);
    let out = fails(&project);
    assert!(
        out.contains(
            "Invar-Skip: trust me: does not count: not signed: `approval.signed` is on at the \
             base, so `Invar-Skip:` counts only with an `Invar-Skip-Signature:` beside it"
        ),
        "{out}"
    );
    assert!(out.contains("counts only signed by an approver"), "{out}");
}

#[test]
fn a_bare_trailer_counts_when_signing_is_off() {
    let project = Project::new(REQUIRED);
    project.write("app/Models/Order.php", "<?php // v2\n");
    project.git(&["add", "-A"]);
    project.git(&[
        "commit",
        "-q",
        "-m",
        "fix(orders): round",
        "--trailer",
        "Invar-Skip: a rounding fix",
    ]);
    check(&project).success().stdout(contains(
        "cycle check: passed: skipped by `Invar-Skip:` (1 trailer(s))",
    ));
    // And `cycle skip` is not the way there: a plain trailer is.
    project
        .invar(&["cycle", "skip", "--reason", "x"])
        .failure()
        .stderr(contains(
            "does not set `approval.signed: true`, so a skip is not signed",
        ));
}

#[test]
fn a_skip_signed_by_a_key_not_in_the_bases_list_does_not_count() {
    let (project, _) = changed();
    let stranger = project.key("stranger");
    project.sign_with(&stranger);
    skip(&project, &[]).success();
    let out = fails(&project);
    assert!(
        out.contains(&format!(
            "signed by a key that is not in the approvers list ({}) (main:.invar/approvers)",
            fingerprint(&stranger)
        )),
        "{out}"
    );
}

#[test]
fn a_line_restricted_to_approvals_does_not_sign_a_skip() {
    let (project, key) = changed();
    project.git(&["checkout", "-q", "main"]);
    project.write(
        ".invar/approvers",
        &format!(
            "{}\n",
            project.approver_line("dev@example.com", &key, Some("invar-approval"))
        ),
    );
    project.commit_all("chore: approvals only");
    project.git(&["checkout", "-q", BRANCH]);
    project.git(&["rebase", "-q", "main"]);
    skip(&project, &[]).success();
    let out = fails(&project);
    assert!(
        out.contains("not permitted for use in signature namespace \"invar-skip\""),
        "{out}"
    );
}

#[test]
fn a_later_change_to_a_required_path_needs_a_new_skip() {
    let (project, _) = changed();
    skip(&project, &[]).success();
    project.write(
        "app/Models/Order.php",
        "<?php // v3, more than was signed\n",
    );
    project.commit_all("fix(orders): and something else");
    let out = fails(&project);
    assert!(
        out.contains("does not count: the signature is not over the change as it is now"),
        "{out}"
    );
    // A new skip covers it again.
    skip(&project, &[]).success();
    check(&project).success();
}

#[test]
fn a_new_required_path_after_the_skip_needs_a_new_skip() {
    let (project, _) = changed();
    skip(&project, &[]).success();
    project.write("app/Models/Refund.php", "<?php\n");
    project.commit_all("feat(refunds): a model");
    fails(&project);
}

#[test]
fn a_skip_taken_to_another_branch_with_another_change_does_not_count() {
    let (project, _) = changed();
    skip(&project, &[]).success();
    let signed = project.head();
    project.git(&["checkout", "-q", "-b", "feat/SHOP-300-other", "main"]);
    project.write("app/Models/Order.php", "<?php // another change\n");
    project.commit_all("feat(orders): something else");
    project.git(&[
        "cherry-pick",
        "--allow-empty",
        "--keep-redundant-commits",
        &signed,
    ]);
    let out = fails(&project);
    assert!(
        out.contains("the signature is not over the change as it is now"),
        "{out}"
    );
}

#[test]
fn a_signed_skip_survives_a_rebase_with_signoff_onto_a_moved_main() {
    let (project, _) = changed();
    skip(&project, &[]).success();
    project.git(&["checkout", "-q", "main"]);
    project.write("README.md", "Shop, moved on\n");
    project.commit_all("docs: main moves on");
    project.git(&["checkout", "-q", BRANCH]);
    project.git(&["rebase", "-q", "--signoff", "main"]);
    // The empty commit is kept, with its trailers and the sign-off after them.
    let message = project.git(&["log", "-1", "--format=%B"]);
    assert!(message.contains("Invar-Skip: a rounding fix"), "{message}");
    assert!(message.contains("Invar-Skip-Signature: "), "{message}");
    assert!(
        message.contains("Signed-off-by: dev <dev@example.com>"),
        "{message}"
    );
    check(&project)
        .success()
        .stdout(contains("(1 signed trailer(s))"));
}

#[test]
fn a_skip_with_nothing_to_cover_or_no_reason_is_refused() {
    let (project, _) = Project::signed(&signed_config());
    project.write("README.md", "docs only\n");
    project.commit_all("docs: a typo");
    skip(&project, &[]).failure().stderr(contains(
        "changes no path that main:invar.yaml requires a cycle for",
    ));
    project
        .invar(&["cycle", "skip", "--reason", "  "])
        .failure()
        .stderr(contains("the reason is empty"));
}

#[test]
fn a_refusal_reaches_the_skill_on_stdout() {
    let (project, _) = Project::signed(&signed_config());
    project
        .invar(&["cycle", "skip", "--skill", "--reason", "x"])
        .code(0)
        .stdout(contains("error: this branch changes no path"))
        .stdout(contains("skip signed").not());
}
