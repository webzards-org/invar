//! Tests for `init` (M1-15): what it creates, that it merges and never
//! clobbers, that a second run changes nothing, and that it refuses before it
//! writes.
//!
//! No git here — checking for a repository is the command's job, and
//! `tests/init.rs` does it against a real one. These run on plain temporary
//! directories, which is all `apply` needs.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use serde_json::{Value, json};
use tempfile::TempDir;

use super::{
    Change, DENY_RULE, INVAR_YAML, InitError, Report, ai_context, apply, business_context,
};
use crate::context::{self, MARKER, State};
use crate::format::config;
use crate::format::plan::Mode;
use crate::fs::ProjectRoot;

// --- fixtures -------------------------------------------------------------------------

/// Every file and directory under `dir` with each file's bytes.
fn snapshot(dir: &Path) -> BTreeMap<String, Option<Vec<u8>>> {
    fn walk(dir: &Path, root: &Path, into: &mut BTreeMap<String, Option<Vec<u8>>>) {
        for entry in fs::read_dir(dir).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            let relative = path
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            if entry.file_type().unwrap().is_dir() {
                into.insert(format!("{relative}/"), None);
                walk(&path, root, into);
            } else {
                into.insert(relative, Some(fs::read(&path).unwrap()));
            }
        }
    }
    let mut into = BTreeMap::new();
    walk(dir, dir, &mut into);
    into
}

fn read(dir: &TempDir, relative: &str) -> String {
    fs::read_to_string(dir.path().join(relative)).unwrap()
}

fn write(dir: &TempDir, relative: &str, text: &str) {
    let path = dir.path().join(relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

fn changes(report: &Report) -> Vec<(&str, Change)> {
    report
        .items
        .iter()
        .map(|item| (item.path.as_str(), item.change))
        .collect()
}

const EVERYTHING: [&str; 8] = [
    ".invar/plans/",
    ".invar/decisions/",
    "invar.yaml",
    "AI_CONTEXT.md",
    "BUSINESS_CONTEXT.md",
    ".gitattributes",
    ".gitignore",
    ".claude/settings.json",
];

/// A Laravel project's own files, as a team would have them.
const LARAVEL_GITIGNORE: &str =
    "/vendor\n/node_modules\n/public/build\n.env\n.phpunit.result.cache\n";
const LARAVEL_GITATTRIBUTES: &str =
    "* text=auto eol=lf\n\n*.blade.php diff=html\n*.php diff=php\n\n/.github export-ignore\n";
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

// --- an empty project ------------------------------------------------------------------

#[test]
fn it_creates_everything_in_an_empty_directory() {
    let dir = tempfile::tempdir().unwrap();

    let report = apply(dir.path()).unwrap();

    assert_eq!(
        changes(&report),
        EVERYTHING.map(|path| (path, Change::Created)),
        "{report:#?}"
    );
    assert!(report.changed());
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    assert!(dir.path().join(".invar/plans").is_dir());
    assert!(dir.path().join(".invar/decisions").is_dir());
    for file in &EVERYTHING[2..] {
        assert!(dir.path().join(file).is_file(), "{file}");
    }
}

#[test]
fn invar_yaml_states_light_mode_outright_and_the_core_reads_it_cleanly() {
    let dir = tempfile::tempdir().unwrap();
    apply(dir.path()).unwrap();

    let text = read(&dir, "invar.yaml");
    assert!(
        text.lines().any(|line| line == "scenarios: false"),
        "`scenarios: false` is not stated as a line of its own:\n{text}"
    );
    assert_eq!(text, INVAR_YAML);

    // The reader the other commands use accepts it: light mode, no warnings.
    let root = ProjectRoot::for_test(dir.path());
    let loaded = config::load_for_command(&root).unwrap();
    assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
    assert!(matches!(Mode::of(&loaded.config), Mode::Light));
}

#[test]
fn the_context_files_hold_the_marker_and_headings_and_never_any_content() {
    let dir = tempfile::tempdir().unwrap();
    apply(dir.path()).unwrap();

    for (name, headings) in [
        (
            "AI_CONTEXT.md",
            [
                "Layers and boundaries",
                "Naming",
                "Module boundaries",
                "Tests",
            ]
            .as_slice(),
        ),
        (
            "BUSINESS_CONTEXT.md",
            [
                "Invariants (always true, no exceptions)",
                "Regional / parameterized facts",
            ]
            .as_slice(),
        ),
    ] {
        let text = read(&dir, name);
        assert_eq!(text.lines().next(), Some(MARKER), "{name}");
        // Every line is the marker, blank, or a heading: nothing a team did
        // not write itself.
        for line in text.lines() {
            assert!(
                line.is_empty() || line == MARKER || line.starts_with('#'),
                "{name} holds content: {line:?}"
            );
        }
        for heading in headings {
            assert!(text.contains(&format!("## {heading}")), "{name}: {heading}");
        }
    }
    assert_eq!(read(&dir, "AI_CONTEXT.md"), ai_context());
    assert_eq!(read(&dir, "BUSINESS_CONTEXT.md"), business_context());
}

#[test]
fn the_scaffolded_context_files_behave_exactly_like_absent_ones() {
    // §5: an empty marked file switches nothing on. So `init` writes them
    // recognized, and empty.
    let dir = tempfile::tempdir().unwrap();
    apply(dir.path()).unwrap();
    let root = ProjectRoot::for_test(dir.path());

    let binding = context::load_binding(&root).unwrap();

    assert!(
        binding.files.iter().all(|file| file.state == State::Empty),
        "{binding:?}"
    );
    assert_eq!(binding.render(), "");
    assert!(binding.unrecognized().is_empty());
}

#[test]
fn the_ai_context_headings_say_nothing_about_a_language_or_a_split() {
    // Whether a team separates backend from frontend is its own decision.
    let text = ai_context().to_lowercase();

    for word in [
        "php", "laravel", "vue", "backend", "frontend", "python", "fastapi",
    ] {
        assert!(!text.contains(word), "AI_CONTEXT.md names {word}");
    }
}

#[test]
fn the_other_files_get_exactly_the_documented_entries() {
    let dir = tempfile::tempdir().unwrap();
    apply(dir.path()).unwrap();

    assert!(read(&dir, ".gitattributes").contains("\n.invar/plans/**/*.md text eol=lf\n"));
    assert!(read(&dir, ".gitignore").contains("\n.invar/out/\n"));
    assert_eq!(read(&dir, ".claude/settings.json"), {
        "{\n  \"permissions\": {\n    \"deny\": [\n      \"Read(/.invar/decisions/**)\"\n    ]\n  }\n}\n"
    });
}

// --- running it again -----------------------------------------------------------------------

#[test]
fn a_second_run_changes_nothing_and_says_so_for_every_file() {
    let dir = tempfile::tempdir().unwrap();
    apply(dir.path()).unwrap();
    let before = snapshot(dir.path());

    let second = apply(dir.path()).unwrap();

    assert_eq!(
        changes(&second),
        EVERYTHING.map(|path| (path, Change::Unchanged)),
        "{second:#?}"
    );
    assert!(!second.changed());
    assert_eq!(snapshot(dir.path()), before, "the tree changed");
}

#[test]
fn running_it_on_a_populated_project_twice_changes_nothing_the_second_time() {
    let dir = tempfile::tempdir().unwrap();
    write(&dir, ".gitignore", LARAVEL_GITIGNORE);
    write(&dir, ".gitattributes", LARAVEL_GITATTRIBUTES);
    write(&dir, ".claude/settings.json", TEAM_SETTINGS);
    apply(dir.path()).unwrap();
    let before = snapshot(dir.path());

    let second = apply(dir.path()).unwrap();

    assert!(!second.changed(), "{second:#?}");
    assert_eq!(snapshot(dir.path()), before);
}

// --- merging into what the team has ----------------------------------------------------------

#[test]
fn existing_gitignore_and_gitattributes_keep_every_line_and_get_ours_after_them() {
    let dir = tempfile::tempdir().unwrap();
    write(&dir, ".gitignore", LARAVEL_GITIGNORE);
    write(&dir, ".gitattributes", LARAVEL_GITATTRIBUTES);

    let report = apply(dir.path()).unwrap();

    let ignore = read(&dir, ".gitignore");
    assert!(ignore.starts_with(LARAVEL_GITIGNORE), "{ignore}");
    assert!(ignore.ends_with("\n.invar/out/\n"), "{ignore}");
    let attributes = read(&dir, ".gitattributes");
    assert!(
        attributes.starts_with(LARAVEL_GITATTRIBUTES),
        "{attributes}"
    );
    assert!(
        attributes.ends_with("\n.invar/plans/**/*.md text eol=lf\n"),
        "{attributes}"
    );

    let by_path: BTreeMap<_, _> = report
        .items
        .iter()
        .map(|item| (item.path.as_str(), item))
        .collect();
    for path in [".gitignore", ".gitattributes"] {
        assert_eq!(by_path[path].change, Change::Updated, "{path}");
        assert!(
            by_path[path]
                .note
                .as_deref()
                .is_some_and(|note| note.starts_with("added `"))
        );
    }
}

#[test]
fn an_existing_settings_file_keeps_its_permissions_and_everything_else() {
    let dir = tempfile::tempdir().unwrap();
    write(&dir, ".claude/settings.json", TEAM_SETTINGS);

    let report = apply(dir.path()).unwrap();

    let new = read(&dir, ".claude/settings.json");
    let mut expected: Value = serde_json::from_str(TEAM_SETTINGS).unwrap();
    expected["permissions"]["deny"]
        .as_array_mut()
        .unwrap()
        .push(json!(DENY_RULE));
    assert_eq!(
        serde_json::from_str::<Value>(&new).unwrap(),
        expected,
        "{new}"
    );

    // The team's own deny rules, allow rules and hooks are all still there, in
    // the order they were in, and the one new line is the rule.
    for kept in [
        "\"Read(./.env)\",",
        "\"Read(./storage/**)\",",
        "\"Bash(php artisan test:*)\",",
        "\"ask\": [\"Bash(git push:*)\"],",
        "\"defaultMode\": \"acceptEdits\"",
        "\"command\": \"vendor/bin/pint --dirty\"",
    ] {
        assert!(new.contains(kept), "lost {kept}:\n{new}");
    }
    assert_eq!(
        new,
        TEAM_SETTINGS.replace(
            "      \"Read(./storage/**)\"\n",
            "      \"Read(./storage/**)\",\n      \"Read(/.invar/decisions/**)\"\n"
        )
    );
    let settings = report
        .items
        .iter()
        .find(|item| item.path == ".claude/settings.json")
        .unwrap();
    assert_eq!(settings.change, Change::Updated);
    assert!(settings.note.as_deref().unwrap().contains(DENY_RULE));
}

#[test]
fn a_settings_file_without_a_permissions_section_gets_one_and_keeps_its_key_order() {
    let dir = tempfile::tempdir().unwrap();
    write(
        &dir,
        ".claude/settings.json",
        "{\n  \"zebra\": 1,\n  \"apple\": 2,\n  \"mango\": 3\n}\n",
    );

    apply(dir.path()).unwrap();

    let new = read(&dir, ".claude/settings.json");
    // Not sorted: `zebra` is still first.
    assert!(
        new.find("zebra").unwrap() < new.find("apple").unwrap()
            && new.find("apple").unwrap() < new.find("mango").unwrap(),
        "{new}"
    );
    assert!(new.starts_with("{\n  \"zebra\": 1,\n  \"apple\": 2,\n  \"mango\": 3,\n"));
}

#[test]
fn files_that_are_the_teams_are_never_touched_once_they_exist() {
    let dir = tempfile::tempdir().unwrap();
    write(&dir, "invar.yaml", "# mine\nreview:\n  offer: true\n");
    write(
        &dir,
        "AI_CONTEXT.md",
        &format!("{MARKER}\n# Conventions\n\nControllers stay thin.\n"),
    );
    write(&dir, "BUSINESS_CONTEXT.md", &format!("{MARKER}\n"));
    let before = [
        read(&dir, "invar.yaml"),
        read(&dir, "AI_CONTEXT.md"),
        read(&dir, "BUSINESS_CONTEXT.md"),
    ];

    let report = apply(dir.path()).unwrap();

    assert_eq!(read(&dir, "invar.yaml"), before[0]);
    assert_eq!(read(&dir, "AI_CONTEXT.md"), before[1]);
    assert_eq!(read(&dir, "BUSINESS_CONTEXT.md"), before[2]);
    let by_path: BTreeMap<_, _> = changes(&report).into_iter().collect();
    for path in ["invar.yaml", "AI_CONTEXT.md", "BUSINESS_CONTEXT.md"] {
        assert_eq!(by_path[path], Change::Unchanged, "{path}");
    }
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
}

#[test]
fn a_context_file_without_the_marker_is_left_alone_and_reported() {
    // A project that already has an AI_CONTEXT.md — written by hand, or by a
    // tool — would otherwise believe it is being delivered.
    let dir = tempfile::tempdir().unwrap();
    write(
        &dir,
        "AI_CONTEXT.md",
        "# Conventions\n\nControllers stay thin.\n",
    );

    let report = apply(dir.path()).unwrap();

    assert_eq!(
        read(&dir, "AI_CONTEXT.md"),
        "# Conventions\n\nControllers stay thin.\n"
    );
    assert_eq!(report.warnings.len(), 1, "{:?}", report.warnings);
    assert!(report.warnings[0].contains("AI_CONTEXT.md already exists without the marker"));
    assert!(report.warnings[0].contains(MARKER));
}

#[test]
fn a_gitignore_that_hides_the_whole_invar_directory_is_reported_and_not_changed_further() {
    let dir = tempfile::tempdir().unwrap();
    write(&dir, ".gitignore", "/vendor\n.invar/\n");

    let report = apply(dir.path()).unwrap();

    assert!(
        report
            .warnings
            .iter()
            .any(|w| w.contains("ignores the whole of `.invar/`")),
        "{:?}",
        report.warnings
    );
    // It is still added to, in the usual way; the team decides what to do about
    // its own `.invar/` line.
    assert!(read(&dir, ".gitignore").starts_with("/vendor\n.invar/\n"));
}

// --- refusing before writing --------------------------------------------------------------------

#[test]
fn a_settings_file_that_cannot_be_merged_into_stops_everything_with_nothing_created() {
    let dir = tempfile::tempdir().unwrap();
    write(
        &dir,
        ".claude/settings.json",
        "{ \"permissions\": { \"deny\": [ }",
    );
    let before = snapshot(dir.path());

    let err = apply(dir.path()).unwrap_err();

    assert!(matches!(err, InitError::Settings { .. }), "{err}");
    let message = err.to_string();
    assert!(
        message.contains(".claude/settings.json is not valid JSON"),
        "{message}"
    );
    assert!(message.contains("Nothing was changed."), "{message}");
    assert_eq!(
        snapshot(dir.path()),
        before,
        "something was written before refusing"
    );
}

#[test]
fn other_files_that_cannot_be_used_also_stop_everything_before_a_byte_is_written() {
    // A directory where a file belongs, a file where a directory belongs, text
    // that is not UTF-8.
    for (path, make_it_wrong) in [
        (".gitattributes", Wrong::Directory),
        ("invar.yaml", Wrong::Directory),
        (".invar", Wrong::File),
        (".gitignore", Wrong::NotUtf8),
    ] {
        let dir = tempfile::tempdir().unwrap();
        match make_it_wrong {
            Wrong::Directory => fs::create_dir_all(dir.path().join(path)).unwrap(),
            Wrong::File => fs::write(dir.path().join(path), "not a directory").unwrap(),
            Wrong::NotUtf8 => fs::write(dir.path().join(path), b"\xFF\xFE").unwrap(),
        }
        let before = snapshot(dir.path());

        let err = apply(dir.path());

        assert!(err.is_err(), "{path}");
        assert_eq!(
            snapshot(dir.path()),
            before,
            "{path}: something was written"
        );
    }
}

enum Wrong {
    Directory,
    File,
    NotUtf8,
}

// --- what is replaced, and how --------------------------------------------------------------------

#[cfg(unix)]
#[test]
fn a_symlinked_settings_file_is_updated_through_the_link_and_the_link_stays() {
    // Dotfiles managers do this: `.claude/settings.json` is a link into a repo.
    let dir = tempfile::tempdir().unwrap();
    let dotfiles = dir.path().join("dotfiles");
    fs::create_dir_all(&dotfiles).unwrap();
    fs::write(dotfiles.join("settings.json"), TEAM_SETTINGS).unwrap();
    fs::create_dir_all(dir.path().join(".claude")).unwrap();
    std::os::unix::fs::symlink(
        dotfiles.join("settings.json"),
        dir.path().join(".claude/settings.json"),
    )
    .unwrap();

    apply(dir.path()).unwrap();

    let link = dir.path().join(".claude/settings.json");
    assert!(
        link.symlink_metadata().unwrap().file_type().is_symlink(),
        "the link was replaced"
    );
    assert!(
        fs::read_to_string(dotfiles.join("settings.json"))
            .unwrap()
            .contains(DENY_RULE)
    );
}

#[cfg(unix)]
#[test]
fn a_replaced_file_keeps_its_permissions_and_leaves_nothing_beside_it() {
    use std::os::unix::fs::PermissionsExt as _;

    let dir = tempfile::tempdir().unwrap();
    write(&dir, ".claude/settings.json", TEAM_SETTINGS);
    let path = dir.path().join(".claude/settings.json");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();

    apply(dir.path()).unwrap();

    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    let beside: Vec<String> = fs::read_dir(dir.path().join(".claude"))
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        beside,
        ["settings.json"],
        "a temporary file was left behind"
    );
}

#[test]
fn a_leftover_temporary_file_from_an_interrupted_run_does_not_get_in_the_way() {
    let dir = tempfile::tempdir().unwrap();
    write(&dir, ".claude/settings.json", TEAM_SETTINGS);
    write(&dir, ".claude/.settings.json.invar-tmp", "half a file");

    apply(dir.path()).unwrap();

    assert!(read(&dir, ".claude/settings.json").contains(DENY_RULE));
    assert!(!dir.path().join(".claude/.settings.json.invar-tmp").exists());
}

// --- the report -------------------------------------------------------------------------------------

#[test]
fn the_report_lists_every_file_in_a_fixed_order() {
    let dir = tempfile::tempdir().unwrap();

    let report = apply(dir.path()).unwrap();

    let paths: Vec<&str> = report.items.iter().map(|item| item.path.as_str()).collect();
    assert_eq!(paths, EVERYTHING);
}

#[test]
fn only_updates_carry_a_note() {
    let dir = tempfile::tempdir().unwrap();
    write(&dir, ".gitignore", LARAVEL_GITIGNORE);

    let report = apply(dir.path()).unwrap();

    for item in &report.items {
        assert_eq!(
            item.note.is_some(),
            item.change == Change::Updated,
            "{}: {item:?}",
            item.path
        );
    }
}
