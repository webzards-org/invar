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
