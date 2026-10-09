//! End-to-end tests for `invar context show` (M1-14): the real binary and a
//! real project.
//!
//! The thing under test is what an agent receives — stdout, and nothing else on
//! it — and what only a person sees: stderr and the exit code. The rules
//! behind them (the marker, the budget, the order) are tested in
//! `context/tests.rs`; here they are tested as the command shows them.

use std::fs;
use std::path::PathBuf;

use assert_cmd::Command;
use predicates::prelude::*;
use predicates::str::contains;
use tempfile::TempDir;

const MARKER: &str = "<!-- invar-context: v1 -->";

struct Project {
    dir: TempDir,
}

impl Project {
    fn new(config: &str) -> Self {
        let project = Self {
            dir: tempfile::tempdir().unwrap(),
        };
        fs::write(project.root().join("invar.yaml"), config).unwrap();
        project
    }

    fn root(&self) -> PathBuf {
        self.dir.path().canonicalize().unwrap()
    }

    /// A context file, marked, with `body` under the marker.
    fn context(&self, name: &str, body: &str) {
        fs::write(self.root().join(name), format!("{MARKER}\n{body}")).unwrap();
    }

    /// A decision entry written the way the core writes one: at 16:37:`second`
    /// UTC, for `module`.
    fn entry(&self, module: &str, second: u32, body: &str) {
        let dir = self.root().join(".invar/decisions");
        fs::create_dir_all(&dir).unwrap();
        let text = format!(
            "---\nid: d-{:08x}\ndate: 2026-10-06T18:37:{second:02}+02:00\nticket: SHOP-1\n\
             module: {module}\ntype: note\n---\n\n{body}\n",
            0xabc0_0000 + second
        );
        fs::write(
            dir.join(format!("2026-10-06-1637{second:02}-{module}-shop-1.md")),
            text,
        )
        .unwrap();
    }

    fn show(&self, args: &[&str]) -> assert_cmd::assert::Assert {
        let mut all = vec!["context", "show"];
        all.extend_from_slice(args);
        Command::cargo_bin("invar")
            .unwrap()
            .current_dir(self.root())
            .args(all)
            .assert()
    }
}

fn stdout_of(assert: &assert_cmd::assert::Assert) -> String {
    String::from_utf8(assert.get_output().stdout.clone()).unwrap()
}

// --- --binding ---------------------------------------------------------------------

#[test]
fn binding_prints_both_files_with_their_origin_and_nothing_from_the_decision_log() {
    let project = Project::new("");
    project.context("AI_CONTEXT.md", "Convention: thin controllers.\n");
    project.context(
        "BUSINESS_CONTEXT.md",
        "Invariant: scope every query by account.\n",
    );
    project.entry("global", 10, "LOG-ONLY-TEXT");

    let assert = project
        .show(&["--binding"])
        .success()
        .stdout(contains(
            "=== AI_CONTEXT.md — this project's own conventions; binding ===",
        ))
        .stdout(contains("Convention: thin controllers."))
        .stdout(contains(
            "=== BUSINESS_CONTEXT.md — business rules the application must obey; binding ===",
        ))
        .stdout(contains("Invariant: scope every query by account."))
        .stdout(contains("LOG-ONLY-TEXT").not())
        .stdout(contains("decision log").not())
        .stderr("");

    // The first line of hook output must not look like JSON, and the marker is
    // administration, not content.
    let out = stdout_of(&assert);
    assert!(out.starts_with("=== invar: binding context ===\n"), "{out}");
    assert!(!out.contains("invar-context"), "{out}");
}

#[test]
fn binding_with_no_context_files_prints_nothing_at_all() {
    let project = Project::new("");

    project.show(&["--binding"]).success().stdout("").stderr("");
}

#[test]
fn binding_runs_from_a_subdirectory_of_the_project() {
    let project = Project::new("");
    project.context("AI_CONTEXT.md", "Convention: thin controllers.\n");
    let deep = project.root().join("app/Http/Controllers");
    fs::create_dir_all(&deep).unwrap();

    Command::cargo_bin("invar")
        .unwrap()
        .current_dir(&deep)
        .args(["context", "show", "--binding"])
        .assert()
        .success()
        .stdout(contains("Convention: thin controllers."));
}

// --- the limit -----------------------------------------------------------------------

#[test]
fn above_the_limit_it_fails_with_both_sizes_and_the_limit_and_prints_nothing() {
    let project = Project::new("context:\n  binding_limit: 300\n");
    project.context("AI_CONTEXT.md", &"a".repeat(411));
    project.context("BUSINESS_CONTEXT.md", &"b".repeat(123));

    project
        .show(&["--binding"])
        .failure()
        .code(1)
        .stdout("")
        .stderr(contains("over the limit of 300"))
        .stderr(contains("`context.binding_limit` in invar.yaml"))
        .stderr(contains("AI_CONTEXT.md"))
        .stderr(contains("411 characters"))
        .stderr(contains("BUSINESS_CONTEXT.md"))
        .stderr(contains("123 characters"))
        .stderr(contains("labels and framing"));
}

#[test]
fn the_limit_is_on_exactly_what_is_printed() {
    let project = Project::new("");
    project.context("AI_CONTEXT.md", &"a".repeat(200));
    let printed = stdout_of(&project.show(&["--binding"]).success());
    let characters = printed.chars().count();

    // Exactly as long as the limit: fine. One character longer: refused.
    fs::write(
        project.root().join("invar.yaml"),
        format!("context:\n  binding_limit: {characters}\n"),
    )
    .unwrap();
    project
        .show(&["--binding"])
        .success()
        .stdout(printed.clone());

    fs::write(
        project.root().join("invar.yaml"),
        format!("context:\n  binding_limit: {}\n", characters - 1),
    )
    .unwrap();
    project
        .show(&["--binding"])
        .failure()
        .code(1)
        .stdout("")
        .stderr(contains(format!(
            "binding context is {characters} characters"
        )));
}

#[test]
fn a_limit_above_what_claude_code_delivers_is_refused_by_the_config() {
    // invar-yaml.md §3.4: the cap is the hook's, not a preference.
    let project = Project::new("context:\n  binding_limit: 12000\n");
    project.context("AI_CONTEXT.md", "Convention: thin controllers.\n");

    project
        .show(&["--binding"])
        .failure()
        .code(1)
        .stderr(contains(
            "`context.binding_limit` must be between 1 and 10000",
        ));
}

// --- the planner's output ---------------------------------------------------------------

#[test]
fn by_default_the_decision_log_follows_the_binding_context() {
    let project = Project::new("");
    project.context("AI_CONTEXT.md", "Convention: thin controllers.\n");
    project.context("BUSINESS_CONTEXT.md", "Invariant: scope by account.\n");
    project.entry("global", 10, "global-old");
    project.entry("global", 20, "global-new");

    let assert = project.show(&[]).success().stderr("");

    let out = stdout_of(&assert);
    let positions: Vec<usize> = [
        "Convention: thin controllers.",
        "Invariant: scope by account.",
        "=== decision log — advisory, newest first ===",
        "global-new",
        "global-old",
        "=== end of decision log ===",
    ]
    .iter()
    .map(|needle| {
        out.find(needle)
            .unwrap_or_else(|| panic!("no {needle:?} in:\n{out}"))
    })
    .collect();
    assert!(positions.windows(2).all(|pair| pair[0] < pair[1]), "{out}");
}

#[test]
fn a_module_selects_its_entries_and_global_always_follows() {
    let project = Project::new("");
    project.entry("orders", 10, "orders-old");
    project.entry("global", 20, "global-a");
    project.entry("orders", 30, "orders-new");
    project.entry("payments", 40, "payments-a");

    let assert = project.show(&["--module", "orders"]).success();

    let out = stdout_of(&assert);
    let positions: Vec<usize> = ["orders-new", "orders-old", "global-a"]
        .iter()
        .map(|needle| {
            out.find(needle)
                .unwrap_or_else(|| panic!("no {needle:?} in:\n{out}"))
        })
        .collect();
    assert!(positions.windows(2).all(|pair| pair[0] < pair[1]), "{out}");
    assert!(!out.contains("payments-a"), "another module's entry: {out}");
    assert!(
        out.contains("Other modules have entries too: payments (1)."),
        "{out}"
    );
}

#[test]
fn without_a_module_the_global_entries_are_shown_and_the_other_modules_named() {
    let project = Project::new("");
    project.entry("orders", 10, "orders-a");
    project.entry("global", 20, "global-a");

    project
        .show(&[])
        .success()
        .stdout(contains("global-a"))
        .stdout(contains("orders-a").not())
        .stdout(contains("Other modules have entries too: orders (1)."));
}

#[test]
fn only_fifty_entries_are_shown_and_the_rest_are_counted() {
    let project = Project::new("");
    for second in 0..51 {
        project.entry("global", second, &format!("Entry number {second}."));
    }

    let assert = project.show(&[]).success();

    let out = stdout_of(&assert);
    assert_eq!(out.matches("Entry number ").count(), 50, "{out}");
    assert!(out.contains("Entry number 50."), "the newest is shown");
    assert!(!out.contains("Entry number 0."), "the oldest is not");
    assert!(
        out.contains("(1 older entry is not shown: the limit is 50 entries or about 4000 tokens.)"),
        "{out}"
    );
}

#[test]
fn an_entry_that_cannot_be_read_is_reported_on_stderr_and_the_rest_are_shown() {
    let project = Project::new("");
    project.entry("global", 10, "A good entry.");
    fs::write(
        project
            .root()
            .join(".invar/decisions/2026-10-06-163799-global-broken.md"),
        "no front matter\n",
    )
    .unwrap();

    project
        .show(&[])
        .success()
        .stdout(contains("A good entry."))
        .stderr(contains(
            "2026-10-06-163799-global-broken.md is not a valid decision entry",
        ));
}

#[test]
fn with_nothing_to_show_the_planner_is_told_so() {
    let project = Project::new("");

    project
        .show(&[])
        .success()
        .stdout(contains("nothing to show"))
        .stderr("");
}

// --- files that are not loaded ---------------------------------------------------------------

#[test]
fn a_file_without_the_marker_is_reported_on_stderr_and_not_loaded() {
    let project = Project::new("");
    fs::write(
        project.root().join("AI_CONTEXT.md"),
        "# Conventions\n\nSECRET-RULE: never loaded.\n",
    )
    .unwrap();
    project.context("BUSINESS_CONTEXT.md", "Invariant: scope by account.\n");

    for args in [&["--binding"][..], &[]] {
        project
            .show(args)
            .success()
            .stdout(contains("Invariant: scope by account."))
            .stdout(contains("SECRET-RULE").not())
            .stdout(contains("AI_CONTEXT.md").not())
            .stderr(contains(
                "warning: AI_CONTEXT.md exists but is not recognized, so it is not loaded",
            ))
            .stderr(contains(MARKER));
    }
}

#[test]
fn an_empty_marked_file_and_an_absent_one_give_identical_output() {
    let absent = Project::new("");
    let empty = Project::new("");
    empty.context("AI_CONTEXT.md", "# AI_CONTEXT.md\n\n## Layers\n");
    empty.context(
        "BUSINESS_CONTEXT.md",
        "# BUSINESS_CONTEXT.md\n\n## Invariants\n\n## Regional / parameterized facts\n",
    );
    for project in [&absent, &empty] {
        project.entry("orders", 10, "Same entry in both.");
        project.entry("global", 20, "Same global entry in both.");
    }

    for args in [&["--binding"][..], &[], &["--module", "orders"]] {
        let (a, e) = (absent.show(args).success(), empty.show(args).success());
        assert_eq!(
            a.get_output().stdout,
            e.get_output().stdout,
            "stdout differs for {args:?}"
        );
        assert_eq!(
            a.get_output().stderr,
            e.get_output().stderr,
            "stderr differs for {args:?}"
        );
    }
}

#[test]
fn a_file_that_cannot_be_read_is_an_error_not_a_quiet_absence() {
    let project = Project::new("");
    fs::create_dir(project.root().join("AI_CONTEXT.md")).unwrap();

    project
        .show(&["--binding"])
        .failure()
        .code(1)
        .stdout("")
        .stderr(contains("cannot read"))
        .stderr(contains("AI_CONTEXT.md"));
}

// --- arguments ---------------------------------------------------------------------------------

#[test]
fn binding_and_a_module_do_not_go_together() {
    // `--module` selects decision entries, which `--binding` leaves out.
    let project = Project::new("");

    project
        .show(&["--binding", "--module", "orders"])
        .failure()
        .code(1)
        .stderr(contains("cannot be used with"));
}

#[test]
fn a_name_that_is_not_a_module_is_refused() {
    let project = Project::new("");

    project
        .show(&["--module", "Orders"])
        .failure()
        .code(1)
        .stderr(contains("`Orders` is not a module name"));
}

#[test]
fn outside_a_project_it_says_to_run_init() {
    let dir = tempfile::tempdir().unwrap();

    Command::cargo_bin("invar")
        .unwrap()
        .current_dir(dir.path())
        .args(["context", "show", "--binding"])
        .assert()
        .failure()
        .stderr(contains("invar init"));
}

#[test]
fn the_help_describes_both_outputs() {
    Command::cargo_bin("invar")
        .unwrap()
        .args(["context", "show", "--help"])
        .assert()
        .success()
        .stdout(contains("--binding"))
        .stdout(contains("--module"))
        .stdout(contains("binding_limit"));

    Command::cargo_bin("invar")
        .unwrap()
        .arg("--help")
        .assert()
        .success()
        .stdout(contains("context"));
}
