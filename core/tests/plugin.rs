//! The plugin, as a set of files, tested against the CLI it fires (M1-17).
//!
//! The plugin is a shell (§13b): it fires CLI commands and passes the output
//! back, and everything that decides anything is in the CLI. That is a property
//! these tests can check by reading `plugin/`, not an intention:
//!
//! - **four hooks, each one CLI call** in exec form — no script, no pipe, no
//!   `&&`;
//! - **no hook ever runs what is the developer's** — `plan approve`, `done`;
//! - **no logic in the plugin** beyond the launcher that picks a binary;
//! - **the hooks and the CLI agree**: every hook's command line is run here
//!   against the real binary, so a plugin written for another version of
//!   `invar` fails in CI and not in somebody's session.
//!
//! What cannot be tested here — that Claude Code accepts the manifest, lists
//! the hooks in `/hooks` and runs them — is `claude plugin validate --strict`
//! in CI, and the walkthrough in `plugin/NOTES.md`.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use predicates::str::contains;
use serde_json::{Value, json};

fn plugin_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../plugin")
}

fn read_json(relative: &str) -> Value {
    let path = plugin_dir().join(relative);
    let text = fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("cannot read {}: {err}", path.display()));
    serde_json::from_str(&text)
        .unwrap_or_else(|err| panic!("{} is not JSON: {err}", path.display()))
}

// --- the hooks, as written ------------------------------------------------------------

#[derive(Debug)]
struct Hook {
    event: String,
    matcher: Option<String>,
    command: String,
    args: Vec<String>,
    timeout: u64,
}

/// Every hook in `hooks/hooks.json`, flattened.
fn hooks() -> Vec<Hook> {
    let json = read_json("hooks/hooks.json");
    let events = json["hooks"].as_object().expect("`hooks` is an object");
    let mut found = Vec::new();
    for (event, groups) in events {
        for group in groups.as_array().expect("an event holds a list of groups") {
            let matcher = group.get("matcher").map(|m| m.as_str().unwrap().to_owned());
            for hook in group["hooks"].as_array().expect("a group holds its hooks") {
                assert_eq!(hook["type"], "command", "{event}: only command hooks");
                found.push(Hook {
                    event: event.clone(),
                    matcher: matcher.clone(),
                    command: hook["command"].as_str().unwrap().to_owned(),
                    args: hook["args"]
                        .as_array()
                        .unwrap_or_else(|| {
                            panic!("{event}: a hook without `args` is not exec form")
                        })
                        .iter()
                        .map(|arg| arg.as_str().unwrap().to_owned())
                        .collect(),
                    timeout: hook["timeout"]
                        .as_u64()
                        .unwrap_or_else(|| panic!("{event}: a hook without a timeout")),
                });
            }
        }
    }
    found
}

fn hook_for(event: &str) -> Hook {
    hooks()
        .into_iter()
        .find(|hook| hook.event == event)
        .unwrap_or_else(|| panic!("no {event} hook"))
}

#[test]
fn there_are_exactly_five_hooks_on_exactly_these_events() {
    let events: Vec<String> = hooks().into_iter().map(|hook| hook.event).collect();

    // No lint hook and no test hook (L-3): nothing on `PostToolUse` or `Stop`.
    // `SubagentStart` is the conventions of the coder (M1-24) and of the test-writer
    // (AT-11): `SessionStart` output does not reach a sub-agent (plugin/NOTES.md §17).
    assert_eq!(
        events.iter().cloned().collect::<BTreeSet<_>>(),
        BTreeSet::from(
            [
                "PreToolUse",
                "SessionStart",
                "SubagentStart",
                "SubagentStop"
            ]
            .map(String::from)
        ),
        "{events:?}"
    );
    assert_eq!(
        events.len(),
        5,
        "one hook for each, and two sub-agents get the conventions: {events:?}"
    );
}

#[test]
fn each_hook_is_one_invocation_of_the_launcher_in_exec_form() {
    for hook in hooks() {
        assert_eq!(
            hook.command, "${CLAUDE_PLUGIN_ROOT}/bin/invar",
            "{}: a hook runs the launcher by its plugin path",
            hook.event
        );
        assert!(!hook.args.is_empty(), "{}: no subcommand", hook.event);
        // Exec form has no shell, so none of this could work; its presence
        // would mean logic crept into the plugin.
        for arg in &hook.args {
            assert!(
                !arg.contains([';', '|', '&', '<', '>', '$', '`', ' ']),
                "{}: `{arg}` looks like shell",
                hook.event
            );
        }
    }
}

#[test]
fn the_hooks_are_the_commands_the_design_names() {
    let expected: [(&str, &[&str]); 4] = [
        ("SessionStart", &["context", "show", "--binding"]),
        (
            "SubagentStart",
            &["context", "show", "--conventions", "--hook"],
        ),
        (
            "SubagentStop",
            &["plan", "validate", "--all-drafts", "--hook"],
        ),
        ("PreToolUse", &["plan", "guard"]),
    ];
    for (event, args) in expected {
        assert_eq!(hook_for(event).args, args, "{event}");
    }
}

#[test]
fn the_matchers_select_what_each_hook_is_for() {
    // Binding context in every session, whatever started it.
    assert_eq!(hook_for("SessionStart").matcher, None);
    // The edit tools, and `Bash` for `invar plan approve` (plugin/NOTES.md §11).
    assert_eq!(
        hook_for("PreToolUse").matcher.as_deref(),
        Some("Write|Edit|Bash")
    );
    // Only the planner: the coder's stops are not this hook's business.
    let planner = hook_for("SubagentStop")
        .matcher
        .expect("a matcher for the planner");
    assert!(planner.contains("planner"), "{planner}");
    // Only the coder and the test-writer get the conventions pushed: the planner and
    // the reviewer read their own context. M1-22 verified that `.*coder` selects the
    // plugin's agent by its namespaced type and no other agent.
    let coder = hook_for("SubagentStart")
        .matcher
        .expect("a matcher for the coder");
    assert_eq!(coder, ".*coder");
    let started: Vec<_> = hooks()
        .into_iter()
        .filter(|hook| hook.event == "SubagentStart")
        .collect();
    let matchers: Vec<Option<&str>> = started.iter().map(|hook| hook.matcher.as_deref()).collect();
    assert_eq!(matchers, [Some(".*coder"), Some(".*test-writer")]);
    // The same command for both.
    for hook in &started {
        assert_eq!(hook.args, ["context", "show", "--conventions", "--hook"]);
    }
}

#[test]
fn every_hook_has_a_timeout_so_a_hung_binary_cannot_hold_a_session() {
    for hook in hooks() {
        assert!(
            (1..=30).contains(&hook.timeout),
            "{}: timeout {}",
            hook.event,
            hook.timeout
        );
    }
}

#[test]
fn no_hook_ever_runs_what_is_the_developers() {
    // `plan approve` is a person's decision (§13) and `done` closes the cycle:
    // a hook that ran either would have abolished the gate.
    for hook in hooks() {
        for arg in &hook.args {
            for forbidden in [
                "approve",
                "--reapprove",
                "done",
                "confirm",
                "decision",
                "init",
                "pr-body",
                // The review is advisory and the developer's to ask for (M1-20,
                // L-7): a hook that ran it would make it a step of the cycle.
                "review-input",
            ] {
                assert_ne!(arg, forbidden, "{}: a hook runs `{forbidden}`", hook.event);
            }
        }
    }
}

// --- the manifest and the files around it -----------------------------------------------------

#[test]
fn the_manifest_is_the_real_plugin_and_not_the_spike() {
    let manifest = read_json(".claude-plugin/plugin.json");

    assert_eq!(manifest["name"], "invar");
    assert!(manifest["version"].as_str().is_some_and(|v| !v.is_empty()));
    assert!(
        manifest["description"]
            .as_str()
            .is_some_and(|d| !d.is_empty())
    );
    assert!(manifest["author"]["name"].as_str().is_some());
    assert!(!manifest.to_string().to_lowercase().contains("spike"));
}

#[test]
fn the_spike_is_deleted() {
    assert!(
        !plugin_dir().join("spike-m0-6").exists(),
        "plugin/spike-m0-6/ is still there: `git rm -r plugin/spike-m0-6`"
    );
}

#[test]
fn the_marketplace_lists_the_plugin_where_the_release_puts_it() {
    let marketplace = read_json("marketplace.json");

    assert_eq!(marketplace["name"], "invar");
    let plugins = marketplace["plugins"].as_array().unwrap();
    assert_eq!(plugins.len(), 1);
    assert_eq!(plugins[0]["name"], "invar");
    // `.github/workflows/release.yml` assembles the plugin into `invar/`.
    assert_eq!(plugins[0]["source"], "./invar");
}

#[test]
fn the_user_readme_is_long_enough_for_the_directory_and_the_release_ships_it() {
    // Anthropic's plugin directory requires a README of at least 40 words in the
    // plugin folder, words in code blocks not counted, and shows it as the
    // listing's description. `plugin/README.md` is for contributors; the release
    // copies `README.dist.md` into `invar/` as `README.md`.
    let text = fs::read_to_string(plugin_dir().join("README.dist.md")).unwrap();
    let mut in_code = false;
    let mut words = 0;
    for line in text.lines() {
        if line.trim_start().starts_with("```") {
            in_code = !in_code;
            continue;
        }
        if !in_code {
            words += line.split_whitespace().count();
        }
    }
    assert!(!in_code, "README.dist.md has an unclosed code block");
    assert!(
        words >= 40,
        "README.dist.md has {words} words outside code blocks"
    );

    let release = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../.github/workflows/release.yml"),
    )
    .unwrap();
    assert!(
        release.contains("cp plugin/README.dist.md dist/invar/README.md"),
        "release.yml does not copy README.dist.md into the plugin folder on dist"
    );
}

/// Every file under `plugin/`, relative to it, in sorted order.
fn plugin_files() -> Vec<PathBuf> {
    fn walk(dir: &Path, root: &Path, into: &mut Vec<PathBuf>) {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(&path, root, into);
            } else {
                into.push(path.strip_prefix(root).unwrap().to_owned());
            }
        }
    }
    let root = plugin_dir();
    let mut files = Vec::new();
    walk(&root, &root, &mut files);
    files.sort();
    files
}

#[test]
fn the_plugin_holds_no_script_but_the_launcher() {
    // No logic in the plugin (§13b): data (`.json`), prose (`.md`) — and the
    // one file that finds the binary. The binaries CI adds beside it are not
    // committed; if one is lying here after a local build, it is allowed
    // because it is `invar`, not because it is a script.
    for file in plugin_files() {
        let shown = file.display().to_string();
        let extension = file.extension().and_then(|e| e.to_str()).unwrap_or("");
        let is_data_or_prose = matches!(extension, "json" | "md");
        let is_launcher = shown == "bin/invar";
        let is_built_binary = shown.starts_with("bin/invar-");
        assert!(
            is_data_or_prose || is_launcher || is_built_binary,
            "plugin/{shown} is neither data, prose nor the launcher: logic does not belong in \
             the plugin (§13b)"
        );
    }
}

#[test]
fn only_the_launcher_is_committed_under_bin_never_a_binary() {
    // A binary in git is a binary nobody reviewed and every clone carries
    // forever. CI builds them and publishes them on the `dist` branch.
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let Ok(output) = std::process::Command::new("git")
        .args(["ls-files", "--", "plugin/bin"])
        .current_dir(&repo)
        .output()
    else {
        return;
    };
    if !output.status.success() {
        // Not a git checkout (an unpacked source archive): nothing to check.
        return;
    }
    let tracked = String::from_utf8(output.stdout).unwrap();
    for line in tracked.lines().filter(|line| !line.is_empty()) {
        assert_eq!(line, "plugin/bin/invar", "a binary is committed: {line}");
    }
}

// --- the hooks and the CLI agree ---------------------------------------------------------------------

/// What Claude Code pipes to each event's hook, for a session in `root` that
/// has nothing for the hook to object to.
fn stdin_for(event: &str, root: &Path) -> String {
    match event {
        "SessionStart" => json!({
            "hook_event_name": "SessionStart",
            "source": "startup",
            "cwd": root,
        }),
        "SubagentStart" => json!({
            "hook_event_name": "SubagentStart",
            "agent_id": "a1",
            "agent_type": "invar:coder",
            "cwd": root,
        }),
        "SubagentStop" => json!({
            "hook_event_name": "SubagentStop",
            "agent_type": "planner",
            "stop_hook_active": false,
            "cwd": root,
        }),
        "PreToolUse" => json!({
            "hook_event_name": "PreToolUse",
            "tool_name": "Write",
            "tool_input": { "file_path": root.join("app/Http/Controller.php") },
            "cwd": root,
        }),
        other => panic!("no payload for {other}"),
    }
    .to_string()
}

#[test]
fn every_hook_command_line_is_one_the_real_binary_accepts_and_answers_quietly() {
    // The version-skew test. A hook with arguments `invar` does not know exits
    // 1 here — and in a session it would be a hook error on every start.
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("invar.yaml"), "").unwrap();
    let root = dir.path().canonicalize().unwrap();

    for hook in hooks() {
        Command::cargo_bin("invar")
            .unwrap()
            .current_dir(&root)
            .args(&hook.args)
            .write_stdin(stdin_for(&hook.event, &root))
            .assert()
            .success()
            .stdout("")
            .stderr("");
    }
}

#[test]
fn outside_an_invar_project_the_stop_and_guard_hooks_say_nothing() {
    // The plugin is installed for every project it is enabled in, and most are
    // not invar's. (`SessionStart` is the open case: plugin/NOTES.md §13.)
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();

    for event in ["SubagentStop", "PreToolUse"] {
        let hook = hook_for(event);
        Command::cargo_bin("invar")
            .unwrap()
            .current_dir(&root)
            .args(&hook.args)
            .write_stdin(stdin_for(event, &root))
            .assert()
            .success()
            .stdout("")
            .stderr("");
    }
}

#[test]
fn the_pre_tool_use_hook_also_answers_for_a_bash_call() {
    // The matcher says `Bash`; the command has to understand what comes with it.
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("invar.yaml"), "").unwrap();
    let root = dir.path().canonicalize().unwrap();
    let hook = hook_for("PreToolUse");
    let bash = |command: &str| {
        json!({
            "hook_event_name": "PreToolUse",
            "tool_name": "Bash",
            "tool_input": { "command": command },
            "cwd": root,
        })
        .to_string()
    };

    Command::cargo_bin("invar")
        .unwrap()
        .current_dir(&root)
        .args(&hook.args)
        .write_stdin(bash("ls -la"))
        .assert()
        .success()
        .stdout("");
    // And the developer's own command is denied: exit 2 is *deny* here.
    Command::cargo_bin("invar")
        .unwrap()
        .current_dir(&root)
        .args(&hook.args)
        .write_stdin(bash("invar plan approve SHOP-214"))
        .assert()
        .failure()
        .code(2)
        .stderr(contains("developer's decision"));
    // So are the two that write the decision log: the draft's approval, and
    // the manual entry that would go around it.
    for (command, skill) in [
        (
            "invar decision approve SHOP-214",
            "/invar:log-approve SHOP-214",
        ),
        (
            "invar decision record --ticket SHOP-214 --type failed -m x",
            "/invar:log SHOP-214",
        ),
        // And closing the cycle, which deletes the approved Plan.
        ("invar done SHOP-214", "cycle is ready to close"),
    ] {
        Command::cargo_bin("invar")
            .unwrap()
            .current_dir(&root)
            .args(&hook.args)
            .write_stdin(bash(command))
            .assert()
            .failure()
            .code(2)
            .stderr(contains("developer's decision"))
            .stderr(contains(skill));
    }
}

// --- the launcher ----------------------------------------------------------------------------------------

#[cfg(unix)]
mod launcher {
    use std::os::unix::fs::PermissionsExt as _;

    use super::*;

    /// This machine's `<os>-<arch>`, as the launcher names it.
    fn platform() -> String {
        let uname = |flag: &str| {
            let output = std::process::Command::new("uname")
                .arg(flag)
                .output()
                .unwrap();
            String::from_utf8(output.stdout).unwrap().trim().to_owned()
        };
        let os = uname("-s").to_lowercase();
        let arch = match uname("-m").as_str() {
            "aarch64" => "arm64".to_owned(),
            "amd64" => "x86_64".to_owned(),
            other => other.to_owned(),
        };
        format!("{os}-{arch}")
    }

    /// Wait until `path` can be executed.
    ///
    /// A file that was just written cannot always be run at once: the tests in
    /// this file run on parallel threads, and a child forked by one thread
    /// between another's `write` and `close` still holds the write descriptor
    /// until it execs, so `exec` of the new file fails with "Text file busy"
    /// (ETXTBSY, 26). The window is a few microseconds; trying again closes it.
    /// Any other outcome — success, a non-zero exit, "permission denied" — is
    /// the file's real behaviour and ends the wait.
    fn wait_until_executable(path: &Path) {
        const ETXTBSY: i32 = 26;
        for _ in 0..100 {
            let started = std::process::Command::new(path)
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status();
            match started {
                Err(err) if err.raw_os_error() == Some(ETXTBSY) => {
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
                _ => return,
            }
        }
    }

    /// The launcher copied into `<tmp>/bin/`, ready for a fake binary to be put
    /// beside it. Returns the temporary directory and the `bin` directory.
    fn installed() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("bin");
        fs::create_dir_all(&bin).unwrap();
        fs::copy(plugin_dir().join("bin/invar"), bin.join("invar")).unwrap();
        fs::set_permissions(bin.join("invar"), fs::Permissions::from_mode(0o755)).unwrap();
        wait_until_executable(&bin.join("invar"));
        (dir, bin)
    }

    /// A stand-in for the binary: prints its arguments one to a line, then its
    /// input, and exits 7.
    fn fake_binary(bin: &Path, mode: u32) {
        let path = bin.join(format!("invar-{}", platform()));
        fs::write(&path, "#!/bin/sh\nprintf '%s\\n' \"$@\"\ncat\nexit 7\n").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(mode)).unwrap();
        wait_until_executable(&path);
    }

    #[test]
    fn the_launcher_is_executable_in_the_repository() {
        let mode = fs::metadata(plugin_dir().join("bin/invar"))
            .unwrap()
            .permissions()
            .mode();
        assert!(
            mode & 0o111 != 0,
            "plugin/bin/invar is not executable: `chmod +x plugin/bin/invar && git update-index \
             --chmod=+x plugin/bin/invar`"
        );
    }

    #[test]
    fn it_runs_the_binary_for_this_platform_and_passes_everything_through() {
        let (_dir, bin) = installed();
        fake_binary(&bin, 0o755);

        Command::new(bin.join("invar"))
            .args(["plan", "guard", "--flag", "two words"])
            .write_stdin("the hook input")
            .assert()
            // The exit code is the binary's own, not the launcher's.
            .code(7)
            .stdout("plan\nguard\n--flag\ntwo words\nthe hook input");
    }

    #[test]
    fn it_finds_its_binary_however_it_is_called_and_from_wherever() {
        let (_dir, bin) = installed();
        fake_binary(&bin, 0o755);
        let elsewhere = tempfile::tempdir().unwrap();

        // By absolute path, from another directory.
        Command::new(bin.join("invar"))
            .current_dir(elsewhere.path())
            .arg("a")
            .assert()
            .code(7)
            .stdout(contains("a\n"));

        // By name, found on PATH, as the agent runs it once the plugin's `bin/`
        // is on the Bash tool's PATH.
        let path = format!("{}:{}", bin.display(), std::env::var("PATH").unwrap());
        Command::new("invar")
            .env("PATH", path)
            .current_dir(elsewhere.path())
            .arg("b")
            .assert()
            .code(7)
            .stdout(contains("b\n"));
    }

    #[test]
    fn a_platform_without_a_binary_exits_127_and_never_2() {
        // Exit 2 from a PreToolUse hook means "deny": a missing binary must not
        // stop anybody's work. 127 is a non-blocking error.
        let (_dir, bin) = installed();

        let assert = Command::new(bin.join("invar"))
            .arg("plan")
            .assert()
            .failure()
            .code(127)
            .stdout("")
            .stderr(contains(format!("no binary for {}", platform())))
            .stderr(contains("linux-x86_64"))
            .stderr(contains("darwin-arm64"));
        assert_ne!(assert.get_output().status.code(), Some(2));
    }

    #[test]
    fn a_binary_that_is_not_executable_counts_as_missing() {
        let (_dir, bin) = installed();
        fake_binary(&bin, 0o644);

        Command::new(bin.join("invar"))
            .assert()
            .failure()
            .code(127)
            .stderr(contains("no binary for"));
    }

    #[test]
    fn the_launcher_decides_nothing_it_is_a_few_lines_that_find_a_file() {
        // The one script in the plugin stays small enough to be read at a
        // glance. If it grows, something other than "find the binary" is in it.
        let text = fs::read_to_string(plugin_dir().join("bin/invar")).unwrap();
        let code_lines = text
            .lines()
            .filter(|line| !line.trim().is_empty() && !line.trim_start().starts_with('#'))
            .count();

        assert!(
            code_lines <= 15,
            "{code_lines} lines of code in the launcher"
        );
        assert!(text.contains("exec \"$binary\" \"$@\""));
        assert!(
            !text.contains("curl") && !text.contains("wget"),
            "the launcher downloads nothing"
        );
    }
}
