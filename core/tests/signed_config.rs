//! A signed `Invar-Config` (`approval-signature.md` §6.5, `invar-yaml.md` §3.9): made by
//! the real `invar config accept` with a real key, checked by `invar config check`.

mod common;

use common::{Project, fingerprint, last_line, stdout};
use predicates::str::contains;

const SIGNED: &str = "approval:\n  signed: true\nreview:\n  offer: true\n";
/// `review.offer` from true to false: one weakening (`invar-yaml.md` §3.9).
const WEAKENED: &str = "approval:\n  signed: true\nreview:\n  offer: false\n";

/// A signed project, and a branch that weakens one check.
fn weakened() -> (Project, std::path::PathBuf) {
    let (project, key) = Project::signed(SIGNED);
    project.write("invar.yaml", WEAKENED);
    project.commit_all("chore: stop offering the review");
    (project, key)
}

fn accept(project: &Project, args: &[&str]) -> assert_cmd::assert::Assert {
    let mut all = vec!["config", "accept", "--reason", "the team reviews by hand"];
    all.extend_from_slice(args);
    project.invar(&all)
}

fn check(project: &Project) -> assert_cmd::assert::Assert {
    project.invar(&["config", "check", "--base", "main"])
}

fn fails(project: &Project) -> String {
    let assert = check(project).failure();
    assert_eq!(
        last_line(&assert),
        "config check: failed",
        "{}",
        stdout(&assert)
    );
    stdout(&assert)
}

#[test]
fn a_signed_configuration_change_passes_and_names_who_signed() {
    let (project, key) = weakened();
    accept(&project, &[])
        .success()
        .stdout(contains("  review.offer: true -> false"))
        .stdout(contains("configuration change signed: 1 weakening(s)"));
    let assert = check(&project).success();
    let out = stdout(&assert);
    assert!(
        out.contains(&format!(
            ": Invar-Config: the team reviews by hand (signed by dev@example.com, {})",
            fingerprint(&key)
        )),
        "{out}"
    );
    assert_eq!(
        last_line(&assert),
        "config check: passed: weakened with `Invar-Config:` (1 signed trailer(s))"
    );
}

#[test]
fn a_bare_trailer_does_not_count_when_signing_is_on_and_counts_when_off() {
    let (project, _) = weakened();
    project.git(&[
        "commit",
        "-q",
        "--allow-empty",
        "-m",
        "chore: weaken",
        "--trailer",
        "Invar-Config: trust me",
    ]);
    let out = fails(&project);
    assert!(
        out.contains("Invar-Config: trust me: does not count: not signed"),
        "{out}"
    );

    let project = Project::new("review:\n  offer: true\n");
    project.write("invar.yaml", "review:\n  offer: false\n");
    project.git(&["add", "-A"]);
    project.git(&[
        "commit",
        "-q",
        "-m",
        "chore: weaken",
        "--trailer",
        "Invar-Config: the team reviews by hand",
    ]);
    check(&project).success().stdout(contains("(1 trailer(s))"));
}

#[test]
fn a_change_signed_by_a_key_not_in_the_bases_list_does_not_count() {
    let (project, _) = weakened();
    let stranger = project.key("stranger");
    project.sign_with(&stranger);
    accept(&project, &[]).success();
    let out = fails(&project);
    assert!(
        out.contains("signed by a key that is not in the approvers list"),
        "{out}"
    );
}

#[test]
fn a_later_change_to_invar_yaml_needs_a_new_signature() {
    let (project, _) = weakened();
    accept(&project, &[]).success();
    // Not a weakening of its own; still not the invar.yaml that was signed.
    project.write("invar.yaml", &format!("{WEAKENED}done:\n  commit: true\n"));
    project.commit_all("chore: and more");
    let out = fails(&project);
    assert!(
        out.contains("the signature is not over the change as it is now"),
        "{out}"
    );
}

#[test]
fn an_acceptance_taken_to_another_branch_with_another_change_does_not_count() {
    let (project, _) = weakened();
    accept(&project, &[]).success();
    let signed = project.head();
    project.git(&["checkout", "-q", "-b", "chore/other", "main"]);
    project.write("invar.yaml", "review:\n  offer: false\n");
    project.commit_all("chore: weaken more");
    project.git(&[
        "cherry-pick",
        "--allow-empty",
        "--keep-redundant-commits",
        &signed,
    ]);
    fails(&project);
}

#[test]
fn nothing_weakened_or_signing_off_is_refused() {
    let (project, _) = Project::signed(SIGNED);
    project.code_change();
    accept(&project, &[])
        .failure()
        .stderr(contains("weakens nothing in invar.yaml against main"));

    let project = Project::new("review:\n  offer: true\n");
    project.write("invar.yaml", "review:\n  offer: false\n");
    project.commit_all("chore: weaken");
    accept(&project, &[]).failure().stderr(contains(
        "does not set `approval.signed: true`, so a configuration change",
    ));
}

trait CodeChange {
    fn code_change(&self);
}

impl CodeChange for Project {
    fn code_change(&self) {
        self.write("README.md", "Shop, again\n");
        self.commit_all("docs: a typo");
    }
}
