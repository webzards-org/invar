//! End-to-end tests for `invar init` (M1-15): the real binary, in real git
//! repositories.
//!
//! Each test is a sentence about the command: *it creates everything on an
//! empty repository*, *it merges into what a Laravel project already has*, *a
//! second run reports unchanged for every file*, *it refuses outside a
//! repository*. The rules behind them are tested in `init/tests.rs` and, for the
//! two merge algorithms, in `init/settings/tests.rs` and `init/lines/tests.rs`.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command as Process;

use assert_cmd::Command;
use predicates::prelude::*;
use predicates::str::contains;
use serde_json::{Value, json};
use tempfile::TempDir;

const RULE: &str = "Read(/.invar/decisions/**)";

/// What `init` reports, in order.
const FILES: [&str; 8] = [
    ".invar/plans/",
    ".invar/decisions/",
    "invar.yaml",
    "AI_CONTEXT.md",
    "BUSINESS_CONTEXT.md",
    ".gitattributes",
    ".gitignore",
    ".claude/settings.json",
];

/// What a Laravel project's own files look like.
const LARAVEL_GITIGNORE: &str = "/vendor\n/node_modules\n/public/build\n/public/hot\n\
                                 /storage/*.key\n.env\n.env.backup\n.phpunit.result.cache\n";
const LARAVEL_GITATTRIBUTES: &str = "* text=auto eol=lf\n\n*.blade.php diff=html\n*.css diff=css\n\
                                     *.md diff=markdown\n*.php diff=php\n\n\
                                     /.github export-ignore\n";
const TEAM_SETTINGS: &str = r#"{
  "model": "claude-opus",
  "env": {
    "APP_ENV": "local"
  },
  "permissions": {
    "allow": [
      "Bash(php artisan test:*)",
      "Bash(composer install)"
    ],
    "deny": [
      "Read(./.env)",
      "Read(./storage/**)"
    ],
    "ask": ["Bash(git push:*)"],
    "defaultMode": "acceptEdits"
  },
  "hooks": {
    "PostToolUse": [
      {
        "matcher": "Write|Edit",
        "hooks": [{ "type": "command", "command": "vendor/bin/pint --dirty" }]
      }
    ]
  }
}
"#;

/// A directory, and optionally a git repository in it.
struct Project {
    dir: TempDir,
}

impl Project {
    fn repository() -> Self {
        let project = Self::plain();
        project.git(&["init", "--quiet", "-b", "main"]);
        project
    }

    /// Not a repository.
    fn plain() -> Self {
        let project = Self {
            dir: tempfile::tempdir().unwrap(),
        };
        fs::write(project.root().join(".gitconfig-test"), "").unwrap();
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

    fn read(&self, relative: &str) -> String {
        fs::read_to_string(self.root().join(relative)).unwrap()
    }

    fn git(&self, args: &[&str]) {
        let output = Process::new("git")
            .args(args)
            .current_dir(self.root())
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", self.root().join(".gitconfig-test"))
            .env("HOME", self.root())
            .output()
            .expect("git runs");
        assert!(
            output.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn command(&self, dir: &Path, args: &[&str]) -> assert_cmd::assert::Assert {
        Command::cargo_bin("invar")
            .unwrap()
            .current_dir(dir)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", self.root().join(".gitconfig-test"))
            .env("HOME", self.root())
            .args(args)
            .assert()
    }

    fn init(&self) -> assert_cmd::assert::Assert {
        self.command(&self.root(), &["init"])
    }

    fn invar(&self, args: &[&str]) -> assert_cmd::assert::Assert {
        self.command(&self.root(), args)
    }
}

/// Every file and directory under `root` with each file's bytes; git's own
/// directory is left out.
fn snapshot(root: &Path) -> BTreeMap<String, Option<Vec<u8>>> {
    fn walk(dir: &Path, root: &Path, into: &mut BTreeMap<String, Option<Vec<u8>>>) {
        for entry in fs::read_dir(dir).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            let relative = path
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            if relative == ".git" {
                continue;
            }
            if entry.file_type().unwrap().is_dir() {
                into.insert(format!("{relative}/"), None);
                walk(&path, root, into);
            } else {
                into.insert(relative, Some(fs::read(&path).unwrap()));
            }
        }
    }
    let mut into = BTreeMap::new();
    walk(root, root, &mut into);
    into
}

fn stdout_of(assert: &assert_cmd::assert::Assert) -> String {
    String::from_utf8(assert.get_output().stdout.clone()).unwrap()
}

// --- an empty repository ------------------------------------------------------------

#[test]
fn it_creates_every_item_on_an_empty_git_repository() {
    let project = Project::repository();

    let assert = project.init().success().stderr("");

    // One line per file, in a fixed order, each saying what happened.
    let out = stdout_of(&assert);
    let reported: Vec<&str> = out
        .lines()
        .filter(|line| line.starts_with("created"))
        .collect();
    let expected: Vec<String> = FILES
        .iter()
        .map(|path| format!("created    {path}"))
        .collect();
    assert_eq!(reported, expected, "{out}");

    assert!(project.root().join(".invar/plans").is_dir());
    assert!(project.root().join(".invar/decisions").is_dir());
    for file in &FILES[2..] {
        assert!(project.root().join(file).is_file(), "{file}");
    }
    assert!(
        project
            .read("invar.yaml")
            .lines()
            .any(|line| line == "scenarios: false")
    );
    assert!(
        project
            .read(".gitattributes")
            .contains("\n.invar/plans/**/*.md text eol=lf\n")
    );
    assert!(project.read(".gitignore").contains("\n.invar/out/\n"));
    assert!(
        out.contains("Next: fill in AI_CONTEXT.md and BUSINESS_CONTEXT.md"),
        "{out}"
    );
}

#[test]
fn the_context_files_hold_the_marker_and_headings_and_no_content() {
    let project = Project::repository();
    project.init().success();

    for name in ["AI_CONTEXT.md", "BUSINESS_CONTEXT.md"] {
        let text = project.read(name);
        assert_eq!(
            text.lines().next(),
            Some("<!-- invar-context: v1 -->"),
            "{name}"
        );
        for line in text.lines() {
            assert!(
                line.is_empty() || line.starts_with("<!--") || line.starts_with('#'),
                "{name} holds content: {line:?}"
            );
        }
    }
    assert!(
        project
            .read("AI_CONTEXT.md")
            .contains("## Layers and boundaries")
    );
    assert!(
        project
            .read("BUSINESS_CONTEXT.md")
            .contains("## Invariants")
    );
}

#[test]
fn the_settings_file_denies_reading_the_decision_log() {
    let project = Project::repository();
    project.init().success();

    let settings: Value = serde_json::from_str(&project.read(".claude/settings.json")).unwrap();

    assert_eq!(settings, json!({ "permissions": { "deny": [RULE] } }));
}

#[test]
fn the_scaffolded_project_is_one_the_other_commands_accept() {
    // `init` pulled forward is for hand-testing everything else.
    let project = Project::repository();
    project.init().success();

    // Both context files are empty, so they behave as if they were absent.
    project
        .invar(&["context", "show", "--binding"])
        .success()
        .stdout("")
        .stderr("");
    project
        .invar(&["context", "show"])
        .success()
        .stdout(contains("nothing to show"))
        .stderr("");
    // The decision log it created is one `decision log` reads.
    project.invar(&["decision", "log"]).success().stderr("");
}

// --- running it again -------------------------------------------------------------------

#[test]
fn a_second_run_reports_unchanged_for_every_file_and_changes_nothing() {
    let project = Project::repository();
    project.init().success();
    let before = snapshot(&project.root());

    let assert = project.init().success().stderr("");

    let out = stdout_of(&assert);
    let reported: Vec<&str> = out
        .lines()
        .filter(|line| line.starts_with("unchanged"))
        .collect();
    // `unchanged` is nine letters, so the column after it is two spaces wide.
    let expected: Vec<String> = FILES
        .iter()
        .map(|path| format!("unchanged  {path}"))
        .collect();
    assert_eq!(reported, expected, "{out}");
    assert!(
        out.contains("nothing to do: everything is already in place"),
        "{out}"
    );
    assert!(!out.contains("created"), "{out}");
    assert_eq!(
        snapshot(&project.root()),
        before,
        "the second run changed the tree"
    );
}

// --- merging into what the team has ------------------------------------------------------

/// A project with Laravel's own `.gitignore`, a `.gitattributes`, and a
/// `.claude/settings.json` with permissions, an environment and a hook.
fn laravel_project() -> Project {
    let project = Project::repository();
    project.write(".gitignore", LARAVEL_GITIGNORE);
    project.write(".gitattributes", LARAVEL_GITATTRIBUTES);
    project.write(".claude/settings.json", TEAM_SETTINGS);
    project.write("composer.json", "{ \"name\": \"acme/orders\" }\n");
    project.write("artisan", "#!/usr/bin/env php\n");
    project
}

#[test]
fn it_merges_into_the_files_a_laravel_project_already_has() {
    let project = laravel_project();

    let assert = project.init().success().stderr("");

    // Reported as updates, with what was added.
    let out = stdout_of(&assert);
    assert!(
        out.contains("updated    .gitignore — added `.invar/out/`"),
        "{out}"
    );
    assert!(
        out.contains("updated    .gitattributes — added `.invar/plans/**/*.md text eol=lf`"),
        "{out}"
    );
    assert!(
        out.contains(
            "updated    .claude/settings.json — added the deny rule `Read(/.invar/decisions/**)`"
        ),
        "{out}"
    );

    // What the team had is there, in front, byte for byte.
    assert!(project.read(".gitignore").starts_with(LARAVEL_GITIGNORE));
    assert!(
        project
            .read(".gitattributes")
            .starts_with(LARAVEL_GITATTRIBUTES)
    );
    assert_eq!(
        project.read("composer.json"),
        "{ \"name\": \"acme/orders\" }\n"
    );
    assert_eq!(project.read("artisan"), "#!/usr/bin/env php\n");
}

#[test]
fn a_settings_file_with_existing_permissions_keeps_all_of_them() {
    let project = laravel_project();

    project.init().success();

    let new = project.read(".claude/settings.json");
    let mut expected: Value = serde_json::from_str(TEAM_SETTINGS).unwrap();
    expected["permissions"]["deny"]
        .as_array_mut()
        .unwrap()
        .push(json!(RULE));
    assert_eq!(
        serde_json::from_str::<Value>(&new).unwrap(),
        expected,
        "{new}"
    );

    // Not rewritten: the team's layout is its own, and the only line that is
    // new is the rule.
    assert_eq!(
        new,
        TEAM_SETTINGS.replace(
            "      \"Read(./storage/**)\"\n",
            "      \"Read(./storage/**)\",\n      \"Read(/.invar/decisions/**)\"\n"
        )
    );
}

#[test]
fn merging_into_a_populated_project_is_idempotent_too() {
    let project = laravel_project();
    project.init().success();
    let before = snapshot(&project.root());

    project
        .init()
        .success()
        .stdout(contains("nothing to do: everything is already in place"))
        .stdout(contains("created").not())
        .stdout(contains("updated").not());

    assert_eq!(snapshot(&project.root()), before);
}

#[test]
fn files_the_team_already_wrote_are_left_exactly_as_they_are() {
    let project = Project::repository();
    project.write("invar.yaml", "# mine\nreview:\n  offer: true\n");
    project.write(
        "AI_CONTEXT.md",
        "<!-- invar-context: v1 -->\n# Conventions\n\nControllers stay thin.\n",
    );

    project
        .init()
        .success()
        .stdout(contains("unchanged  invar.yaml"))
        .stdout(contains("unchanged  AI_CONTEXT.md"))
        .stdout(contains("created    BUSINESS_CONTEXT.md"));

    assert_eq!(
        project.read("invar.yaml"),
        "# mine\nreview:\n  offer: true\n"
    );
    assert_eq!(
        project.read("AI_CONTEXT.md"),
        "<!-- invar-context: v1 -->\n# Conventions\n\nControllers stay thin.\n"
    );
}

// --- what it warns about ------------------------------------------------------------------

#[test]
fn a_context_file_without_the_marker_is_reported_and_left_alone() {
    let project = Project::repository();
    project.write("AI_CONTEXT.md", "# Conventions\n\nControllers stay thin.\n");

    project
        .init()
        .success()
        .stderr(contains(
            "warning: AI_CONTEXT.md already exists without the marker",
        ))
        .stderr(contains("<!-- invar-context: v1 -->"));

    assert_eq!(
        project.read("AI_CONTEXT.md"),
        "# Conventions\n\nControllers stay thin.\n"
    );
}

#[test]
fn a_gitignore_that_hides_the_whole_invar_directory_is_reported() {
    let project = Project::repository();
    project.write(".gitignore", "/vendor\n.invar/\n");

    project.init().success().stderr(contains(
        "warning: .gitignore ignores the whole of `.invar/`",
    ));
}

// --- refusing ---------------------------------------------------------------------------------

#[test]
fn it_refuses_outside_a_git_repository_and_creates_nothing() {
    let project = Project::plain();
    let before = snapshot(&project.root());

    project
        .init()
        .failure()
        .code(1)
        .stdout("")
        .stderr(contains("`invar init` must run inside a git repository"))
        .stderr(contains("git init"))
        .stderr(contains("git said:"));

    assert_eq!(snapshot(&project.root()), before, "something was created");
    assert!(!project.root().join("invar.yaml").exists());
}

#[test]
fn it_refuses_a_settings_file_it_cannot_read_and_creates_nothing_at_all() {
    let project = Project::repository();
    project.write(
        ".claude/settings.json",
        "{ \"permissions\": { \"deny\": [ }",
    );
    let before = snapshot(&project.root());

    project
        .init()
        .failure()
        .code(1)
        .stdout("")
        .stderr(contains(".claude/settings.json is not valid JSON"))
        .stderr(contains("add the rule by hand"))
        .stderr(contains("Nothing was changed."));

    // Not even the files that had nothing to do with it.
    assert_eq!(
        snapshot(&project.root()),
        before,
        "something was written before refusing"
    );
    assert!(!project.root().join("invar.yaml").exists());
}

#[test]
fn a_settings_file_that_is_not_an_object_is_refused_too() {
    let project = Project::repository();
    project.write(".claude/settings.json", "[]\n");

    project
        .init()
        .failure()
        .stderr(contains("is JSON, but not an object"))
        .stderr(contains("Nothing was changed."));

    assert_eq!(project.read(".claude/settings.json"), "[]\n");
}

// --- where it runs ---------------------------------------------------------------------------------

#[test]
fn a_subdirectory_of_a_repository_becomes_the_project_root_and_the_command_says_so() {
    // A monorepo project; or an accident in `app/Http/`. Either way: say which.
    let project = Project::repository();
    let backend = project.root().join("backend");
    fs::create_dir_all(&backend).unwrap();

    project
        .command(&backend, &["init"])
        .success()
        .stdout(contains("created    invar.yaml"))
        .stderr(contains("note: this directory is not the repository root"));

    assert!(backend.join("invar.yaml").is_file());
    assert!(backend.join(".invar/plans").is_dir());
    assert!(!project.root().join("invar.yaml").exists());
}

#[test]
fn at_the_repository_root_there_is_no_such_note() {
    let project = Project::repository();

    project
        .init()
        .success()
        .stderr(contains("not the repository root").not());
}

// --- the command's own surface -----------------------------------------------------------------------

#[test]
fn the_help_says_what_it_creates_and_that_it_merges() {
    Command::cargo_bin("invar")
        .unwrap()
        .args(["init", "--help"])
        .assert()
        .success()
        .stdout(contains("invar.yaml"))
        .stdout(contains(".claude/settings.json"))
        .stdout(contains("never"))
        .stdout(contains("unchanged"));

    Command::cargo_bin("invar")
        .unwrap()
        .arg("--help")
        .assert()
        .success()
        .stdout(contains("init"));
}
