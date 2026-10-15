//! The gate-refusal suite (M1-21): every refusal and every boundary the
//! milestone promises, as a test running the **real binary** against a real
//! temporary tree — no agent, no plugin. Each test reads as a sentence about
//! the system.
//!
//! The suite starts here with the one entry M1-23 adds; M1-21 brings the rest
//! (`plan approve` refusing an invalid Plan, `done` refusing a changed one, …),
//! some of which are already covered command by command in their own files.

use std::fs;
use std::path::PathBuf;

use assert_cmd::Command;
use tempfile::TempDir;

const MARKER: &str = "<!-- invar-context: v1 -->";

struct Project {
    dir: TempDir,
}

impl Project {
    fn new() -> Self {
        let project = Self {
            dir: tempfile::tempdir().unwrap(),
        };
        fs::write(project.root().join("invar.yaml"), "").unwrap();
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

    fn invar(&self, args: &[&str]) -> assert_cmd::assert::Assert {
        Command::cargo_bin("invar")
            .unwrap()
            .current_dir(self.root())
            .args(args)
            .assert()
    }
}

/// The coder reads `invar context show --conventions` (DESIGN §5c, §13c; M1-23,
/// M1-24). Business rules reach it only through the approved Plan, and the
/// decision log never does.
#[test]
fn the_coders_context_never_contains_a_business_rule_or_a_decision_entry() {
    let project = Project::new();
    project.write(
        "AI_CONTEXT.md",
        &format!("{MARKER}\n## Layers\n- Controllers stay thin.\n"),
    );
    project.write(
        "BUSINESS_CONTEXT.md",
        &format!("{MARKER}\n## Invariants\n- BUSINESS-RULE-7: every query is scoped.\n"),
    );
    project.write(
        ".invar/decisions/2026-10-06-163712-global-shop-1.md",
        "---\nid: d-1234abcd\ndate: 2026-10-06T18:37:12+02:00\nticket: SHOP-1\n\
         module: global\ntype: note\n---\n\nDECISION-ENTRY-9.\n",
    );

    // Present in the sources: the planner's context carries both.
    let planner = project.invar(&["context", "show"]).success();
    let planner = String::from_utf8(planner.get_output().stdout.clone()).unwrap();
    assert!(planner.contains("BUSINESS-RULE-7"), "{planner}");
    assert!(planner.contains("DECISION-ENTRY-9"), "{planner}");

    // Absent from the coder's, on stdout and stderr alike.
    let coder = project
        .invar(&["context", "show", "--conventions"])
        .success();
    let output = coder.get_output();
    for stream in [&output.stdout, &output.stderr] {
        let text = String::from_utf8(stream.clone()).unwrap();
        for forbidden in ["BUSINESS-RULE-7", "BUSINESS_CONTEXT.md", "DECISION-ENTRY-9"] {
            assert!(!text.contains(forbidden), "{forbidden:?} in:\n{text}");
        }
    }
    assert!(
        String::from_utf8(output.stdout.clone())
            .unwrap()
            .contains("Controllers stay thin."),
        "the conventions themselves are delivered"
    );
}
