//! The planner agent and the protocol skill, tested as what they can be tested
//! as: files (M1-18).
//!
//! Whether the planner *behaves* — asks the regional question, writes criteria
//! that would catch the incident, returns only a path and a summary — is a
//! property of a model reading a prompt, and the only test of it is running it
//! on real tickets: `plugin/NOTES.md` §14 is the protocol, and the iteration log
//! is where the results go. What these tests hold is everything around that
//! which can drift without anybody noticing:
//!
//! - the **example Plan in the prompt is valid** under the real `plan validate`,
//!   so a change to the Plan contract that outdates the template fails here and
//!   not in a planner that learned the old shape;
//! - **every `invar` command and flag the prompts name exists**, so a prompt
//!   cannot send an agent to a flag that was renamed or never was;
//! - the **main session is never pointed at the decision log**, and the skill
//!   says what the acceptance criteria require of it;
//! - the planner **is the agent the `SubagentStop` hook selects**.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use serde_json::Value;

fn plugin_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../plugin")
}

fn read(relative: &str) -> String {
    let path = plugin_dir().join(relative);
    fs::read_to_string(&path).unwrap_or_else(|err| panic!("cannot read {}: {err}", path.display()))
}

/// The flat `key: value` frontmatter of an agent or a skill, and what follows.
fn frontmatter(relative: &str) -> (BTreeMap<String, String>, String) {
    let text = read(relative);
    let rest = text
        .strip_prefix("---\n")
        .unwrap_or_else(|| panic!("{relative} has no frontmatter"));
    let (head, body) = rest
        .split_once("\n---\n")
        .unwrap_or_else(|| panic!("{relative}: the frontmatter is never closed"));
    let meta = head
        .lines()
        .map(|line| {
            let (key, value) = line
                .split_once(": ")
                .unwrap_or_else(|| panic!("{relative}: `{line}` is not `key: value`"));
            (key.to_owned(), value.to_owned())
        })
        .collect();
    (meta, body.to_owned())
}

// --- the planner ---------------------------------------------------------------------

#[test]
fn the_planner_is_the_agent_the_stop_hook_selects() {
    let (meta, _) = frontmatter("agents/planner.md");
    let hooks: Value = serde_json::from_str(&read("hooks/hooks.json")).unwrap();
    let matcher = hooks["hooks"]["SubagentStop"][0]["matcher"]
        .as_str()
        .unwrap();

    assert_eq!(meta["name"], "planner");
    // The matcher is `.*planner`: the name has to end the way it expects.
    assert!(
        meta["name"].ends_with(matcher.trim_start_matches(".*")),
        "the SubagentStop matcher `{matcher}` does not select `{}`",
        meta["name"]
    );
}

#[test]
fn the_frontmatter_stays_valid_yaml_without_quoting() {
    // A flat reader like this one is what an unquoted value can be read by; a
    // `: ` or a ` #` inside a value would make a real YAML parser read it
    // differently.
    for file in ["agents/planner.md", "skills/invar/SKILL.md"] {
        let (meta, _) = frontmatter(file);
        for (key, value) in &meta {
            assert!(!value.contains(": "), "{file}: `{key}` contains `: `");
            assert!(!value.contains(" #"), "{file}: `{key}` contains ` #`");
            assert!(!value.is_empty(), "{file}: `{key}` is empty");
        }
    }
}

#[test]
fn the_planner_has_the_tools_to_read_the_code_and_write_one_file_and_no_more() {
    let (meta, _) = frontmatter("agents/planner.md");
    let tools: BTreeSet<&str> = meta["tools"].split(',').map(str::trim).collect();

    // It reads (`Read`, `Grep`, `Glob`), runs `invar` (`Bash`) and writes the
    // Plan (`Write`, `Edit`).
    for needed in ["Read", "Grep", "Glob", "Bash", "Write", "Edit"] {
        assert!(tools.contains(needed), "the planner lacks {needed}");
    }
    // It does not start other agents: Phase 1 is one context.
    for forbidden in ["Agent", "Task"] {
        assert!(!tools.contains(forbidden), "the planner has {forbidden}");
    }
}

#[test]
fn the_planners_description_tells_the_main_session_what_to_give_it_and_what_it_returns() {
    let (meta, _) = frontmatter("agents/planner.md");
    let description = &meta["description"];

    assert!(description.contains("ticket id"), "{description}");
    assert!(description.contains("verbatim") || description.contains("exactly as written"));
    assert!(description.contains("QUESTIONS"), "{description}");
    // The main session is told, where it chooses the agent, that the log is not
    // its to read.
    assert!(
        description.contains("main session must not"),
        "{description}"
    );
}

#[test]
fn the_planners_prompt_has_each_part_the_pilot_depends_on() {
    let (_, body) = frontmatter("agents/planner.md");

    for heading in [
        "## What you are given",
        "## What you return",
        "## Procedure",
        "## Questions you must ask",
        "## Implementation context is a distillate",
        "## Acceptance criteria that would catch the incident",
        "### Example of a complete Plan",
        "## A Plan that came back",
        "## What you never do",
    ] {
        assert!(
            body.contains(heading),
            "the planner prompt lost `{heading}`"
        );
    }
    for phrase in [
        // The two shapes of the answer, which the skill parses by eye.
        "QUESTIONS",
        "PLAN .invar/plans/",
        // Reading is narrow.
        "Never a full scan",
        // The mandatory question, and its single exception.
        "asking is\nmandatory",
        "stated in the ticket",
        // The distillate, and what is not a source.
        "never a copy",
        "Past decisions are not a source",
        // The criterion per invariant, as the negative case.
        "gets at least one criterion that fails if the invariant is\nbroken",
        // What it must not do.
        "Run `invar plan approve`",
        "Quote the decision log",
    ] {
        assert!(
            body.contains(phrase) || body.replace("\n", " ").contains(&phrase.replace('\n', " ")),
            "the planner prompt lost: {phrase:?}"
        );
    }
}

/// The first complete Plan in the planner's prompt.
fn example_plan() -> String {
    let (_, body) = frontmatter("agents/planner.md");
    let after = body
        .split_once("### Example of a complete Plan")
        .expect("the planner prompt has an example Plan")
        .1;
    let fenced = after.split_once("```markdown\n").unwrap().1;
    fenced.split_once("\n```\n").unwrap().0.to_owned() + "\n"
}

#[test]
fn the_example_plan_in_the_prompt_passes_plan_validate() {
    // The template is what the planner imitates. If the contract moves, this is
    // where it is noticed — not in a planner that learned the old shape.
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("invar.yaml"), "").unwrap();
    let plan_dir = dir.path().join(".invar/plans/ACME-87-export-invoices-csv");
    fs::create_dir_all(&plan_dir).unwrap();
    fs::write(plan_dir.join("spec.md"), example_plan()).unwrap();

    Command::cargo_bin("invar")
        .unwrap()
        .current_dir(dir.path())
        .args(["plan", "validate", "ACME-87"])
        .assert()
        .success();
}

#[test]
fn the_example_plan_shows_what_it_asks_of_the_planner() {
    let plan = example_plan();

    // A draft: the status the planner writes, with the approval keys present
    // and empty.
    for line in [
        "Status: draft",
        "Approved-by:",
        "Approved-at:",
        "Approved-checksum:",
    ] {
        assert!(
            plan.lines().any(|l| l == line),
            "the example lacks `{line}`"
        );
    }
    // A business invariant in Implementation context, and a criterion for the
    // negative case: another account's customer is a 404.
    assert!(plan.contains("BUSINESS_CONTEXT invariant:"));
    assert!(plan.contains("belongs to another account"));
    assert!(plan.contains("the response is 404"));
    // Only the two sources the contract allows.
    let context = plan.split_once("## Implementation context\n").unwrap().1;
    for line in context.lines().filter(|l| l.starts_with("- ")) {
        assert!(
            line.starts_with("- AI_CONTEXT:") || line.starts_with("- BUSINESS_CONTEXT invariant:"),
            "{line}"
        );
    }
}

// --- the skill ---------------------------------------------------------------------------

fn skill() -> (BTreeMap<String, String>, String) {
    frontmatter("skills/invar/SKILL.md")
}

#[test]
fn the_skill_is_found_by_what_a_developer_asks() {
    let (meta, _) = skill();
    let description = &meta["description"];

    assert_eq!(meta["name"], "invar");
    // The description is what decides whether the skill is loaded at all (§5c).
    for word in ["ticket", "Plan", "invar.yaml", "skip"] {
        assert!(description.contains(word), "no `{word}` in: {description}");
    }
    assert!(
        description.len() <= 1024,
        "{} characters",
        description.len()
    );
}

#[test]
fn the_skill_has_every_part_of_the_cycle() {
    let (_, body) = skill();

    for heading in [
        "## Where the session runs",
        "## Is this a job for the cycle?",
        "## Phase 1 — plan",
        "## The developer approves",
        "## Phase 2 — the coder implements",
        "## When the coder answers BLOCKED",
        "## Closing",
        "## Models",
        "## The decision log is not yours",
        "## Never",
    ] {
        assert!(body.contains(heading), "the skill lost `{heading}`");
    }
}

#[test]
fn the_skill_tells_the_agent_to_skip_the_cycle_for_trivial_work_and_to_say_so() {
    let (_, body) = skill();

    assert!(body.contains("**Skip it** for trivial work"));
    // The exact sentence to say, so a skip is a visible decision and not an
    // oversight (L-9).
    assert!(body.contains("*\"Skipping the invar cycle:"), "{body}");
    assert!(body.contains("When you skip, say so, before you start"));
    // And the escape for the case it cannot tell.
    assert!(body.contains("Plan this, or just do it?"));
}

#[test]
fn the_skill_runs_the_cycle_as_a_dispatcher() {
    let flat = skill().1.replace('\n', " ");

    // The main session dispatches; the coder implements (M1-25).
    assert!(flat.contains("in the cycle you are the **dispatcher**"));
    assert!(flat.contains("**You do not implement an approved Plan yourself**"));
    // The gate, then the coder with the ticket line only.
    assert!(flat.contains("**Phase 2 begins with `invar plan check <TICKET>`.**"));
    assert!(flat.contains("**start the coder** (`coder`; `invar:coder` if namespaced)"));
    assert!(flat.contains("with the ticket line and nothing else"));
    assert!(flat.contains("`Ticket: SHOP-214`"));
    // Both answers are handled.
    assert!(flat.contains("**`DONE`**"));
    assert!(flat.contains("**`BLOCKED <reason>`**"));
    // An impossible Plan goes back, with a `failed` entry — not "fixed" — and
    // the entry is the developer's to record (decision-entry.md D-6).
    assert!(flat.contains("Draft the `failed` entry?"));
    assert!(flat.contains("returns the Plan to `draft`"));
    assert!(flat.contains("**The developer records it**, with `/invar:log-approve <TICKET>`"));
    assert!(flat.contains("it is their call, not yours: never record it yourself"));
    assert!(
        !flat.contains("--type failed"),
        "the old way of recording it"
    );
    assert!(flat.contains("do not weaken a criterion"));
    assert!(flat.contains("**Never edit the approved Plan**"));
    // The developer closes the cycle; the main session says it is ready.
    assert!(flat.contains("**tell the developer the cycle is ready to close**, and stop"));
    assert!(flat.contains("Never run `invar done` yourself"));
    assert!(
        !flat.contains("`invar done <TICKET> -m"),
        "the old closing step"
    );
    // The developer's step is theirs.
    assert!(flat.contains("**Stop and wait.** Never run `invar plan approve` yourself"));
    // Code of its own only when the cycle is skipped.
    assert!(flat.contains("Then you write the change yourself."));
}

#[test]
fn the_skill_never_tells_the_main_session_to_implement_an_approved_plan() {
    // What the M1-18 skill said, and the dispatcher must not.
    let flat = skill().1.replace('\n', " ");
    for gone in [
        "Implement what the Plan says",
        "write the tests that check each one",
        "you carry Phase 2",
        "your job in Phase 2 is to implement it",
    ] {
        assert!(!flat.contains(gone), "the skill still says: {gone:?}");
    }
}

#[test]
fn the_skill_chooses_models_aloud_and_handles_an_unavailable_one() {
    let flat = skill().1.replace('\n', " ");

    // The values the `Agent` tool accepts per call (M1-22).
    assert!(flat.contains("`sonnet`, `opus`, `haiku` or `fable`"));
    assert!(flat.contains("**A planner on anything but its default is said aloud**"));
    // No fallback exists, so none is invented.
    assert!(flat.contains("`model_not_found`"));
    assert!(flat.contains("offer to run the agent again with `model: sonnet`"));
    assert!(flat.contains("switch models silently."));
}

#[test]
fn the_skill_says_to_start_at_the_project_root() {
    // Started in a subdirectory, `.claude/settings.json` and its deny rule are
    // not loaded (plugin/NOTES.md §17, D-17.3).
    let flat = skill().1.replace('\n', " ");
    assert!(flat.contains("Claude Code must be started **at the project root**"));
    assert!(flat.contains("does not load the project's `.claude/settings.json`"));
}

#[test]
fn the_main_session_is_never_pointed_at_the_decision_log() {
    // "The main session never receives the decision log." Every mention of a
    // way to read it, in the skill, is a prohibition or the binding-only form.
    let (_, body) = skill();
    let mut mentions = 0;
    for line in body.lines() {
        let reads_the_log = line.contains("invar context show")
            || line.contains("invar decision log")
            || line.contains(".invar/decisions");
        if !reads_the_log {
            continue;
        }
        mentions += 1;
        assert!(
            ["never", "Never", "do not", "Do not", "--binding"]
                .iter()
                .any(|word| line.contains(word)),
            "the skill points the main session at the log: {line}"
        );
    }
    assert!(
        mentions >= 3,
        "the skill no longer says where the log is not to be read"
    );
}

#[test]
fn the_planner_alone_is_told_to_read_the_log() {
    let (_, planner) = frontmatter("agents/planner.md");
    assert!(planner.contains("`invar context show`"));
    assert!(planner.contains("the decision log"));
}

// --- the commands and flags both prompts name -----------------------------------------------

/// Every inline `` `invar …` `` span in `text`, outside fenced blocks.
fn inline_commands(text: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut in_fence = false;
    for line in text.lines() {
        if line.trim_start().starts_with("```") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }
        let mut rest = line;
        while let Some(start) = rest.find("`invar ") {
            let after = &rest[start + 1..];
            let Some(end) = after.find('`') else { break };
            found.push(after[..end].to_owned());
            rest = &after[end + 1..];
        }
    }
    found
}

/// `invar plan approve <TICKET> --reapprove` → (`plan approve`, [`--reapprove`]).
fn command_and_flags(span: &str) -> (Vec<String>, Vec<String>) {
    let tokens: Vec<&str> = span.split_whitespace().skip(1).collect();
    let path: Vec<String> = tokens
        .iter()
        .take_while(|token| {
            !token.is_empty()
                && token.chars().all(|c| c.is_ascii_lowercase() || c == '-')
                && !token.starts_with('-')
        })
        .map(|token| (*token).to_owned())
        .collect();
    let flags = tokens
        .iter()
        .filter(|token| token.starts_with("--"))
        .map(|token| {
            token
                .trim_end_matches([',', '.', ')', ';', '"'])
                .split('=')
                .next()
                .unwrap()
                .to_owned()
        })
        .collect();
    (path, flags)
}

#[test]
fn every_invar_command_and_flag_the_prompts_name_exists() {
    let mut texts = vec![
        frontmatter("agents/planner.md").1,
        frontmatter("agents/coder.md").1,
        frontmatter("agents/reviewer.md").1,
        skill().1,
        frontmatter("skills/log/SKILL.md").1,
        frontmatter("skills/log-approve/SKILL.md").1,
    ];
    texts
        .iter_mut()
        .for_each(|text| *text = text.replace("\\\"", "\""));

    let mut seen: BTreeSet<(Vec<String>, String)> = BTreeSet::new();
    let mut commands: BTreeSet<Vec<String>> = BTreeSet::new();
    for text in &texts {
        for span in inline_commands(text) {
            let (path, flags) = command_and_flags(&span);
            assert!(!path.is_empty(), "`{span}` names no command");
            commands.insert(path.clone());
            for flag in flags {
                seen.insert((path.clone(), flag));
            }
        }
    }
    // The extraction itself has to be finding things.
    assert!(commands.len() >= 6, "found only {commands:?}");
    // Since `/invar:log-approve`, no prompt spells out `decision record` with
    // its flags; `--binding` and `--conventions` are what is left.
    assert!(seen.len() >= 2, "found only {seen:?}");

    for path in &commands {
        let assert = Command::cargo_bin("invar")
            .unwrap()
            .args(path)
            .arg("--help")
            .assert()
            .success();
        let help = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
        for (_, flag) in seen.iter().filter(|(p, _)| p == path) {
            assert!(
                help.contains(flag.as_str()),
                "`invar {}` has no {flag}",
                path.join(" ")
            );
        }
    }
}

#[test]
fn the_commands_the_two_prompts_split_between_them_are_the_ones_each_is_allowed() {
    // The skill, which the main session reads, names no command only the
    // planner may run; the planner is told not to run what is the developer's
    // or the coder's.
    let skill_commands: BTreeSet<Vec<String>> = inline_commands(&skill().1)
        .iter()
        .map(|span| command_and_flags(span).0)
        .collect();
    let planner_commands: BTreeSet<Vec<String>> =
        inline_commands(&frontmatter("agents/planner.md").1)
            .iter()
            .map(|span| command_and_flags(span).0)
            .collect();

    // `plan check` and `pr-body` are the main session's (Phase 2 and
    // closing); the planner names neither.
    assert!(skill_commands.contains(&vec!["plan".to_owned(), "check".to_owned()]));
    assert!(skill_commands.contains(&vec!["pr-body".to_owned()]));
    // `plan validate` and the full `context show` are the planner's.
    assert!(planner_commands.contains(&vec!["plan".to_owned(), "validate".to_owned()]));
    assert!(planner_commands.contains(&vec!["context".to_owned(), "show".to_owned()]));
}

// --- every agent ------------------------------------------------------------------------------

/// Every agent definition under `plugin/agents/`.
fn agents() -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(plugin_dir().join("agents"))
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| name.ends_with(".md"))
        .map(|name| format!("agents/{name}"))
        .collect();
    names.sort();
    names
}

#[test]
fn every_agents_model_is_one_the_agent_tool_can_also_name() {
    // `claude plugin validate --strict` accepts any string here, even
    // `banana`, and a model the account cannot use fails the agent with a 404
    // at runtime (plugin/NOTES.md §17, D-17.4). Aliases only: they are what
    // the main session can pass per call, and they follow the current model.
    const ALLOWED: [&str; 5] = ["opus", "sonnet", "haiku", "fable", "inherit"];
    let all = agents();
    assert!(all.len() >= 3, "{all:?}");
    for file in all {
        let (meta, _) = frontmatter(&file);
        if let Some(model) = meta.get("model") {
            assert!(
                ALLOWED.contains(&model.as_str()),
                "{file}: `model: {model}` is not one of {ALLOWED:?}"
            );
        }
    }
}

#[test]
fn the_planner_runs_on_a_strong_model_by_default() {
    // DESIGN §13d: the developer approves the planner's work, so a weak Plan
    // weakens the gate itself (M1-25; NOTES §14, lever 3).
    let (meta, _) = frontmatter("agents/planner.md");
    assert_eq!(meta["model"], "opus");
}

// --- the coder (M1-24) ----------------------------------------------------------------------

#[test]
fn the_coder_writes_code_and_starts_no_one_on_a_cheaper_model() {
    let (meta, _) = frontmatter("agents/coder.md");
    let tools: BTreeSet<&str> = meta["tools"].split(',').map(str::trim).collect();

    assert_eq!(meta["name"], "coder");
    for needed in ["Read", "Grep", "Glob", "Bash", "Write", "Edit"] {
        assert!(tools.contains(needed), "the coder lacks {needed}");
    }
    for forbidden in ["Agent", "Task"] {
        assert!(!tools.contains(forbidden), "the coder has {forbidden}");
    }
    // DESIGN §13d: a cheaper model is allowed for the agent whose every output
    // is checked on disk.
    assert_eq!(meta["model"], "sonnet");
    for (key, value) in &meta {
        assert!(!value.contains(": "), "`{key}` contains `: `");
        assert!(!value.contains(" #"), "`{key}` contains ` #`");
    }
}

#[test]
fn the_coder_answers_in_one_of_two_shapes() {
    let (meta, body) = frontmatter("agents/coder.md");

    assert!(meta["description"].contains("ticket line and nothing else"));
    assert!(
        body.contains("```\nDONE\n<at most 8 lines"),
        "the DONE shape"
    );
    assert!(
        body.contains("```\nBLOCKED <the reason, in one line>\n"),
        "the BLOCKED shape"
    );
}

#[test]
fn the_coder_begins_and_ends_with_plan_check_and_stops_on_a_refusal() {
    let flat = frontmatter("agents/coder.md").1.replace('\n', " ");

    assert!(flat.contains("Run `invar plan check <TICKET>` **before anything else**."));
    assert!(flat.contains("answer `BLOCKED <what it said>` and stop. Write no code"));
    assert!(flat.contains("Run `invar plan check <TICKET>` once more"));
    // Never past a criterion by changing it.
    assert!(flat.contains("**Soften a criterion to get past it**"));
    assert!(flat.contains("Do not pick one of two contradicting criteria"));
    assert!(flat.contains("**Edit the Plan**"));
    // Outside Scope is a question for the developer, not a quiet widening.
    assert!(flat.contains("**outside Scope**, stop and answer `BLOCKED`, naming"));
}

#[test]
fn the_coder_is_never_pointed_at_business_rules_or_the_log() {
    // DESIGN §5c: the coder meets business rules only as the Plan states them,
    // and never the log. Every line that names a way to either is a
    // prohibition; `context show` appears only with `--conventions`.
    let (_, body) = frontmatter("agents/coder.md");
    let mut prohibitions = 0;
    for line in body.lines() {
        let points_at_them = line.contains("BUSINESS_CONTEXT")
            || line.contains(".invar/decisions")
            || line.contains("invar decision log")
            || (line.contains("invar context show") && !line.contains("--conventions"));
        if points_at_them {
            prohibitions += 1;
            assert!(
                line.contains("never") || line.contains("Never"),
                "the coder is pointed at business rules or the log: {line}"
            );
        }
    }
    assert!(
        prohibitions >= 2,
        "the prohibitions themselves went missing"
    );
    // Its conventions are the `--conventions` output, which the hook pushes.
    assert!(body.contains("=== invar: conventions ==="));
    assert!(body.contains("`invar context show --conventions`"));
}

#[test]
fn the_conventions_block_the_coder_looks_for_is_the_one_the_cli_prints() {
    // The prompt tells the coder to look for a heading; if the CLI's heading
    // changed, the coder would run the command again on every start.
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("invar.yaml"), "").unwrap();
    fs::write(
        dir.path().join("AI_CONTEXT.md"),
        "<!-- invar-context: v1 -->\nConvention.\n",
    )
    .unwrap();
    let assert = Command::cargo_bin("invar")
        .unwrap()
        .current_dir(dir.path())
        .args(["context", "show", "--conventions"])
        .assert()
        .success();
    let out = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    assert!(out.starts_with("=== invar: conventions ===\n"), "{out}");
}

// --- the reviewer and /invar:review (M1-20) ------------------------------------------------

#[test]
fn the_reviewer_reads_and_changes_nothing() {
    let (meta, _) = frontmatter("agents/reviewer.md");
    let tools: BTreeSet<&str> = meta["tools"].split(',').map(str::trim).collect();

    assert_eq!(meta["name"], "reviewer");
    // It reads the code and runs `invar review-input`.
    for needed in ["Read", "Grep", "Glob", "Bash"] {
        assert!(tools.contains(needed), "the reviewer lacks {needed}");
    }
    // It edits nothing and starts no one.
    for forbidden in ["Write", "Edit", "NotebookEdit", "Agent", "Task"] {
        assert!(!tools.contains(forbidden), "the reviewer has {forbidden}");
    }
    for (key, value) in &meta {
        assert!(!value.contains(": "), "`{key}` contains `: `");
        assert!(!value.contains(" #"), "`{key}` contains ` #`");
    }
}

#[test]
fn the_reviewers_output_has_the_fixed_shape() {
    let (_, body) = frontmatter("agents/reviewer.md");
    let flat = body.replace('\n', " ");

    // The three sections, in this order, in the template.
    let template = body
        .split_once("## What you return")
        .unwrap()
        .1
        .split_once("```\n")
        .unwrap()
        .1
        .split_once("\n```")
        .unwrap()
        .0;
    let order = [
        "REVIEW <TICKET>",
        "## Acceptance criteria",
        "1. met —",
        "Evidence: <file:line>",
        "2. not met —",
        "3. unclear —",
        "Why:",
        "## Invariants the Plan names that the change appears to break",
        // M1-26: a planner miss, reported apart because its fix is in Phase 1.
        "## Invariants the Plan does not name that the change appears to break",
        "## Changes outside Scope",
    ];
    let positions: Vec<usize> = order
        .iter()
        .map(|needle| {
            template
                .find(needle)
                .unwrap_or_else(|| panic!("no {needle:?} in the template"))
        })
        .collect();
    assert!(positions.windows(2).all(|w| w[0] < w[1]), "{template}");

    // The rules that make the shape worth having.
    for rule in [
        "Every `met` and every `not met` has at least one `file:line`",
        "**`unclear` is a correct answer, not a failure.**",
        "**Nothing else.**",
        "`None found.`",
    ] {
        assert!(flat.contains(rule), "the reviewer prompt lost: {rule:?}");
    }
}

#[test]
fn the_reviewer_checks_every_invariant_on_a_strong_model() {
    // M1-26: the coder never reads BUSINESS_CONTEXT.md, so the reviewer is
    // the one agent that checks the branch against every invariant in it.
    let (meta, body) = frontmatter("agents/reviewer.md");
    let flat = body.replace('\n', " ");

    assert_eq!(meta["model"], "opus");
    assert!(flat.contains("checks the branch against **every** invariant"));
    assert!(flat.contains("List every invariant in the `BUSINESS_CONTEXT.md` block"));
    assert!(flat.contains("for **each one**, named or not"));
    assert!(flat.contains("**not named by the Plan** — the planner missed it"));
    // The old instruction, which checked only what the Plan named and what the
    // change "touches", is gone.
    assert!(!flat.contains("each rule of `BUSINESS_CONTEXT.md` the change touches"));
    assert!(!flat.contains("## Invariants the change appears to break"));
}

#[test]
fn the_reviewer_is_independent_of_the_coder_and_the_log() {
    let (_, body) = frontmatter("agents/reviewer.md");
    let flat = body.replace('\n', " ");

    assert!(flat.contains("Run `invar review-input <TICKET>`"));
    assert!(
        flat.contains("**ignore it**"),
        "extra context from the caller"
    );
    assert!(flat.contains("**Change anything**"));
    // Every mention of a way to the log is a prohibition.
    for line in body.lines() {
        let reads_the_log = line.contains(".invar/decisions")
            || line.contains("invar decision log")
            || (line.contains("invar context show") && !line.contains("--binding"));
        if reads_the_log {
            assert!(
                line.contains("never") || line.contains("Never"),
                "the reviewer is pointed at the log: {line}"
            );
        }
    }
    // And `review-input` exists, with the name the prompt uses.
    Command::cargo_bin("invar")
        .unwrap()
        .args(["review-input", "--help"])
        .assert()
        .success();
}

#[test]
fn review_is_the_developers_and_passes_the_ticket_alone() {
    let (meta, body) = frontmatter("skills/review/SKILL.md");
    let flat = body.replace('\n', " ");

    assert_eq!(meta["name"], "review");
    assert_eq!(meta["disable-model-invocation"], "true");
    assert!(flat.contains("**Give it the ticket line and nothing else**"));
    assert!(flat.contains("exactly as it returns it"));
    assert!(flat.contains("It is advisory."));
}

#[test]
fn the_protocol_skill_offers_the_review_before_done_and_never_runs_it() {
    let flat = skill().1.replace('\n', " ");

    // On by default since invar-yaml.md 2.0.0 (D-8); off only when said.
    assert!(flat.contains("unless `invar.yaml` sets `review.offer: false`"));
    assert!(flat.contains("Run `/invar:review` before closing?"));
    assert!(flat.contains("do not start the reviewer yourself"));
    // Offered before the cycle is closed, which removes the Plan the review
    // needs.
    let offer = flat.find("Offer the review").unwrap();
    let done = flat.find("the cycle is ready to close**").unwrap();
    assert!(offer < done);
}

#[cfg(unix)]
#[test]
fn review_resolves_the_ticket_like_approve_does() {
    // The same block, with its own last line: it names the ticket instead of
    // approving it.
    let block = |file: &str| {
        let (_, body) = frontmatter(file);
        body.split_once("```!\n")
            .unwrap()
            .1
            .split_once("\n```")
            .unwrap()
            .0
            .to_owned()
    };
    let review = block("skills/review/SKILL.md");
    let dir = tempfile::tempdir().unwrap();
    for name in ["SHOP-214-a", "SHOP-300-b"] {
        let plan = dir.path().join(".invar/plans").join(name);
        fs::create_dir_all(&plan).unwrap();
        fs::write(plan.join("spec.md"), "").unwrap();
    }
    let run = |arguments: &str| {
        let out = std::process::Command::new("sh")
            .arg("-c")
            .arg(review.replace("$ARGUMENTS", arguments))
            .current_dir(dir.path())
            .output()
            .unwrap();
        String::from_utf8(out.stdout).unwrap()
    };

    assert_eq!(run("SHOP-300"), "Ticket: SHOP-300\n");
    let listed = run("");
    assert!(listed.contains("2 Plans exist"), "{listed}");
    assert!(!listed.contains("Ticket:"), "{listed}");
    assert!(run("x; rm -rf .").contains("is not a ticket"));
    assert!(dir.path().join(".invar/plans/SHOP-214-a/spec.md").exists());
}

// --- /invar:approve (M1-19) --------------------------------------------------------------

#[test]
fn approve_cannot_be_invoked_by_the_model() {
    // The gate is the developer's (§13). `disable-model-invocation` is the
    // field Claude Code honours; an unknown field is ignored without a word, so
    // a misspelling would abolish the gate silently. Hence the exact string.
    let (meta, body) = frontmatter("skills/approve/SKILL.md");

    assert_eq!(meta["name"], "approve");
    assert_eq!(meta["disable-model-invocation"], "true");
    assert!(
        !meta.contains_key("user-invocable"),
        "it must stay in the / menu"
    );
    // The model is told not to run it either; belt and braces.
    assert!(body.contains("Do not run `invar plan approve` yourself"));
}

#[test]
fn approve_runs_the_cli_and_lists_the_plans_instead_of_guessing() {
    let (_, body) = frontmatter("skills/approve/SKILL.md");

    assert!(body.contains("invar plan approve \"$t\""));
    assert!(body.contains("Plans are waiting; say which: /invar:approve <ticket>"));
    assert!(body.contains("there is no Plan in .invar/plans/ to approve"));
}

#[cfg(unix)]
mod approve_block {
    use super::*;

    /// The shell block of the skill, with `$ARGUMENTS` replaced, run in `dir`
    /// with a stand-in `invar` that records what it was asked to approve.
    fn run(dir: &Path, arguments: &str) -> String {
        use std::os::unix::fs::PermissionsExt as _;
        let (_, body) = frontmatter("skills/approve/SKILL.md");
        let block = body
            .split_once("```!\n")
            .unwrap()
            .1
            .split_once("\n```")
            .unwrap()
            .0
            .replace("$ARGUMENTS", arguments);
        let bin = dir.join("fakebin");
        fs::create_dir_all(&bin).unwrap();
        fs::write(bin.join("invar"), "#!/bin/sh\necho \"approved: $3\"\n").unwrap();
        fs::set_permissions(bin.join("invar"), fs::Permissions::from_mode(0o755)).unwrap();
        let out = std::process::Command::new("sh")
            .arg("-c")
            .arg(&block)
            .current_dir(dir)
            .env(
                "PATH",
                format!("{}:{}", bin.display(), std::env::var("PATH").unwrap()),
            )
            .output()
            .unwrap();
        String::from_utf8(out.stdout).unwrap()
    }

    fn plan(dir: &Path, name: &str) {
        let p = dir.join(".invar/plans").join(name);
        fs::create_dir_all(&p).unwrap();
        fs::write(p.join("spec.md"), "").unwrap();
    }

    #[test]
    fn one_plan_needs_no_argument() {
        let dir = tempfile::tempdir().unwrap();
        plan(dir.path(), "SHOP-214-cancel-unpaid-order");
        assert_eq!(
            run(dir.path(), ""),
            "approved: SHOP-214-cancel-unpaid-order\n"
        );
    }

    #[test]
    fn several_plans_are_listed_and_nothing_is_approved() {
        let dir = tempfile::tempdir().unwrap();
        plan(dir.path(), "SHOP-214-cancel-unpaid-order");
        plan(dir.path(), "SHOP-300-refunds");
        let out = run(dir.path(), "");
        assert!(out.contains("2 Plans are waiting"), "{out}");
        assert!(out.contains("  SHOP-214-cancel-unpaid-order\n"), "{out}");
        assert!(out.contains("  SHOP-300-refunds\n"), "{out}");
        assert!(!out.contains("approved:"), "{out}");
    }

    #[test]
    fn a_ticket_is_passed_through_and_shell_syntax_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        plan(dir.path(), "SHOP-214-a");
        plan(dir.path(), "SHOP-300-b");
        assert_eq!(run(dir.path(), "SHOP-300"), "approved: SHOP-300\n");
        let out = run(dir.path(), "x; touch pwned");
        assert!(out.contains("is not a ticket"), "{out}");
        assert!(!dir.path().join("pwned").exists());
    }

    #[test]
    fn no_plan_is_said_plainly() {
        let dir = tempfile::tempdir().unwrap();
        assert!(run(dir.path(), "").contains("no Plan in .invar/plans/"));
    }

    #[test]
    fn no_start_still_approves_and_says_phase_2_is_not_started() {
        let dir = tempfile::tempdir().unwrap();
        plan(dir.path(), "SHOP-214-a");
        plan(dir.path(), "SHOP-300-b");
        assert_eq!(
            run(dir.path(), "SHOP-300 --no-start"),
            "approved: SHOP-300\nPhase 2: not started (--no-start)\n"
        );
        // Either order.
        assert_eq!(
            run(dir.path(), "--no-start SHOP-214"),
            "approved: SHOP-214\nPhase 2: not started (--no-start)\n"
        );
        // Without it, no such line.
        assert_eq!(run(dir.path(), "SHOP-300"), "approved: SHOP-300\n");
    }

    #[test]
    fn no_start_works_without_a_ticket_too() {
        let dir = tempfile::tempdir().unwrap();
        plan(dir.path(), "SHOP-214-cancel-unpaid-order");
        assert_eq!(
            run(dir.path(), "--no-start"),
            "approved: SHOP-214-cancel-unpaid-order\nPhase 2: not started (--no-start)\n"
        );
    }

    #[test]
    fn another_option_or_a_second_ticket_approves_nothing() {
        let dir = tempfile::tempdir().unwrap();
        plan(dir.path(), "SHOP-214-a");
        let unknown = run(dir.path(), "SHOP-214 --reapprove");
        assert!(
            unknown.contains("the one option is --no-start"),
            "{unknown}"
        );
        assert!(!unknown.contains("approved:"), "{unknown}");
        let two = run(dir.path(), "SHOP-214 SHOP-300");
        assert!(two.contains("give one ticket"), "{two}");
        assert!(!two.contains("approved:"), "{two}");
    }
}

#[test]
fn approve_starts_phase_2_only_on_the_recorded_line() {
    // Task 6: the approval is the decision to implement, so the skill starts
    // Phase 2 — but only when the command's own success line is there, never
    // on the model's reading of the output.
    let (meta, body) = frontmatter("skills/approve/SKILL.md");
    let flat = body.replace('\n', " ");

    assert_eq!(meta["argument-hint"], "[ticket] [--no-start]");
    let condition = flat
        .find("**If the output contains the line `approved <TICKET> checksum sha256:…`**")
        .expect("the start is conditional on the recorded line");
    let no_start = flat
        .find("**and no line `Phase 2: not started (--no-start)`**")
        .expect("--no-start is part of the condition");
    let start = flat
        .find("**Start Phase 2 for that ticket now**")
        .expect("the start instruction");
    let otherwise = flat
        .find("**Otherwise, stop.** Write no code and start no coder.")
        .expect("what to do without the line");
    assert!(condition < no_start && no_start < start && start < otherwise);
    // Phase 2 as the protocol describes it: the check first, then the coder.
    assert!(flat.contains("run `invar plan check <TICKET>` first"));
    assert!(flat.contains("with the ticket line and nothing else"));
    assert!(flat.contains("never by your own reading"));
    // The block prints the --no-start line the condition names.
    assert!(body.contains("echo \"Phase 2: not started (--no-start)\""));
    // And still never runs the approval itself.
    assert!(body.contains("Do not run `invar plan approve` yourself"));

    // The protocol skill no longer waits for a second prompt after the gate.
    let protocol = skill().1.replace('\n', " ");
    assert!(protocol.contains("**The approval starts Phase 2; no second prompt is needed.**"));
    assert!(protocol.contains("`approved <TICKET> checksum sha256:…`"));
    assert!(protocol.contains("`Phase 2: not started (--no-start)`"));
    assert!(!protocol.contains("When the developer tells you the Plan is approved, go on."));
}

#[test]
fn the_line_approve_keys_on_is_the_line_the_cli_prints() {
    // The skill names `approved <TICKET> checksum sha256:…`; the real
    // `plan approve` has to end with exactly that shape.
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("invar.yaml"), "").unwrap();
    fs::write(dir.path().join(".gitconfig"), "[user]\n\tname = rade\n").unwrap();
    let plan_dir = dir.path().join(".invar/plans/ACME-87-export-invoices-csv");
    fs::create_dir_all(&plan_dir).unwrap();
    fs::write(plan_dir.join("spec.md"), example_plan()).unwrap();

    let assert = Command::cargo_bin("invar")
        .unwrap()
        .current_dir(dir.path())
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", dir.path().join(".gitconfig"))
        .env("HOME", dir.path())
        .args(["plan", "approve", "ACME-87"])
        .assert()
        .success();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let last = stdout.lines().last().unwrap();

    let (_, body) = frontmatter("skills/approve/SKILL.md");
    let named = body
        .split_once("the line `")
        .unwrap()
        .1
        .split_once("…`")
        .unwrap()
        .0
        .replace("<TICKET>", "ACME-87");
    assert_eq!(named, "approved ACME-87 checksum sha256:");
    assert!(last.starts_with(&named), "{last}");
    let hex = &last[named.len()..];
    assert!(
        hex.len() == 64 && hex.bytes().all(|b| b.is_ascii_hexdigit()),
        "{last}"
    );
}

// --- /invar:log ------------------------------------------------------------------------------

#[test]
fn log_is_the_developers_and_names_its_ticket() {
    // A draft records nothing, but whether a cycle is worth an entry is the
    // developer's call (decision-entry.md D-6, D-9): the model does not draft
    // one on its own.
    let (meta, _) = frontmatter("skills/log/SKILL.md");

    assert_eq!(meta["name"], "log");
    assert_eq!(meta["disable-model-invocation"], "true");
    assert_eq!(meta["argument-hint"], "[ticket]");
    assert!(meta["description"].contains("records nothing"));
}

#[test]
fn log_writes_a_draft_next_to_the_plan_and_records_nothing() {
    let (_, body) = frontmatter("skills/log/SKILL.md");
    let flat = body.replace('\n', " ");

    assert!(flat.contains("`.invar/plans/<TICKET>-<slug>/decision.md`"));
    assert!(flat.contains("If a draft is already there, replace it"));
    assert!(flat.contains("run `invar decision validate <TICKET>`"));
    assert!(flat.contains("**No `id` and no `date`**"));
    assert!(flat.contains("**Do not record it**"));
    // The three draft types, each with when, and not the fourth.
    for kind in ["**`confirmed`**", "**`failed`**", "**`note`**"] {
        assert!(flat.contains(kind), "no {kind}");
    }
    assert!(flat.contains("Never `correction`"));
    // What the body is, and is not.
    assert!(flat.contains("**What was decided, and why**"));
    assert!(flat.contains("**Not a summary of the diff**"));
}

#[test]
fn log_never_points_at_the_decision_log() {
    let (_, body) = frontmatter("skills/log/SKILL.md");
    let mut mentions = 0;
    for line in body.lines() {
        if line.contains(".invar/decisions") || line.contains("invar decision log") {
            mentions += 1;
            assert!(
                line.contains("Never") || line.contains("never"),
                "/invar:log points at the log: {line}"
            );
        }
    }
    assert!(
        mentions >= 1,
        "the skill no longer says the log is not to be read"
    );
}

#[test]
fn the_example_draft_in_log_passes_decision_validate() {
    // Like the planner's example Plan: a change to the draft contract that
    // outdates the skill's template fails here, not in a session.
    let (_, body) = frontmatter("skills/log/SKILL.md");
    let example = body
        .split_once("```markdown\n")
        .expect("the skill shows an example draft")
        .1
        .split_once("\n```")
        .unwrap()
        .0
        .to_owned()
        + "\n";
    let ticket = example
        .lines()
        .find_map(|line| line.strip_prefix("ticket: "))
        .expect("the example names its ticket")
        .to_owned();

    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("invar.yaml"), "").unwrap();
    let plan = dir
        .path()
        .join(".invar/plans")
        .join(format!("{ticket}-example"));
    fs::create_dir_all(&plan).unwrap();
    fs::write(plan.join("spec.md"), "").unwrap();
    fs::write(plan.join("decision.md"), &example).unwrap();

    Command::cargo_bin("invar")
        .unwrap()
        .current_dir(dir.path())
        .args(["decision", "validate", &ticket])
        .assert()
        .success();
}

#[cfg(unix)]
#[test]
fn log_resolves_the_ticket_like_review_does() {
    let (_, body) = frontmatter("skills/log/SKILL.md");
    let block = body
        .split_once("```!\n")
        .unwrap()
        .1
        .split_once("\n```")
        .unwrap()
        .0
        .to_owned();
    let dir = tempfile::tempdir().unwrap();
    let run = |arguments: &str| {
        let out = std::process::Command::new("sh")
            .arg("-c")
            .arg(block.replace("$ARGUMENTS", arguments))
            .current_dir(dir.path())
            .output()
            .unwrap();
        String::from_utf8(out.stdout).unwrap()
    };

    assert!(run("").contains("no Plan in .invar/plans/"));
    let plan = dir.path().join(".invar/plans/SHOP-214-a");
    fs::create_dir_all(&plan).unwrap();
    fs::write(plan.join("spec.md"), "").unwrap();
    assert_eq!(run(""), "Ticket: SHOP-214-a\n");
    let other = dir.path().join(".invar/plans/SHOP-300-b");
    fs::create_dir_all(&other).unwrap();
    fs::write(other.join("spec.md"), "").unwrap();
    assert!(run("").contains("2 Plans exist"));
    assert_eq!(run("SHOP-300"), "Ticket: SHOP-300\n");
    assert!(run("x; touch pwned").contains("is not a ticket"));
    assert!(!dir.path().join("pwned").exists());
}

#[test]
fn the_protocol_skill_offers_log_after_blocked_and_before_done() {
    let flat = skill().1.replace('\n', " ");

    assert_eq!(
        flat.matches("Run `/invar:log`.").count(),
        2,
        "offered once after BLOCKED and once before closing"
    );
    // After BLOCKED: before the developer records the `failed` entry.
    let blocked = flat.find("Draft the `failed` entry?").unwrap();
    let failed = flat.find("`/invar:log-approve <TICKET>`").unwrap();
    assert!(blocked < failed);
    // Closing: after the review, before `done`, which removes the Plan.
    let review = flat.find("Offer the review").unwrap();
    let draft = flat.find("Offer to draft the decision entry").unwrap();
    let done = flat.find("the cycle is ready to close**").unwrap();
    assert!(review < draft && draft < done);
    // When an entry is probably not worth drafting, said once, and the
    // developer decides.
    assert!(flat.contains("probably needs no more than a one-line message"));
    assert!(flat.contains("leave the choice to the developer"));
    assert!(flat.contains("Do not draft it yourself without `/invar:log`"));
}

// --- invar done is the developer's --------------------------------------------------------

#[test]
fn the_protocol_skill_tells_the_developer_the_cycle_is_ready_and_never_closes_it() {
    let flat = skill().1.replace('\n', " ");

    // The PR description is rendered while the Plan exists, by the session.
    assert!(flat.contains("`invar pr-body <TICKET> --out .invar/out/<TICKET>-pr.md`"));
    let pr = flat.find("Render the PR description**").unwrap();
    let ready = flat.find("the cycle is ready to close**").unwrap();
    assert!(pr < ready);
    // Closing is the developer's, from a terminal or CI; a draft entry blocks it.
    assert!(flat.contains("Closing is theirs"));
    assert!(flat.contains("or CI runs it for them"));
    assert!(flat.contains("never delete the Plan another way"));
    assert!(flat.contains("It refuses while a draft entry is still beside the Plan"));
    assert!(flat.contains("- run `invar plan approve` or `invar done`;"));
}

#[test]
fn no_agent_is_told_to_run_done() {
    // Every agent names `invar done` only among what it never runs.
    for file in agents() {
        let (_, body) = frontmatter(&file);
        for line in body.lines().filter(|line| line.contains("invar done")) {
            assert!(
                [
                    "never",
                    "Never",
                    "no `invar",
                    "**Run `invar plan approve` or"
                ]
                .iter()
                .any(|word| line.contains(word)),
                "{file} names `invar done` without forbidding it: {line}"
            );
        }
    }
}

// --- the end of a cycle (task 5) ---------------------------------------------------------

#[test]
fn the_skill_ends_a_cycle_in_the_new_order() {
    let flat = skill().1.replace('\n', " ");

    // The order, stated once, as the developer will live it.
    assert!(flat.contains(
        "**commit → `/invar:review` (optional) → push → PR review → `/invar:log` and \
         `/invar:log-approve` (if an entry is wanted) → `invar done`**"
    ));
    // And the steps in that order.
    let steps: Vec<usize> = [
        "**1. Commit.**",
        "**2. Offer the review**",
        "**3. Render the PR description**",
        "**4. Push, and the PR is reviewed**",
        "**5. Offer to draft the decision entry**",
        "**6. `invar done` is the developer's.**",
    ]
    .iter()
    .map(|step| flat.find(step).unwrap_or_else(|| panic!("no {step:?}")))
    .collect();
    assert!(steps.windows(2).all(|w| w[0] < w[1]), "{steps:?}");
    // The draft is written before or during the PR review, recorded after it.
    assert!(flat.contains("before the PR review or during it"));
    assert!(flat.contains("with `/invar:log-approve`, after the review"));
    assert!(flat.contains("the developer edits the draft or runs `/invar:log` again"));
}

#[test]
fn after_blocked_the_skill_offers_a_failed_draft_the_developer_approves() {
    let flat = skill().1.replace('\n', " ");
    let blocked = flat
        .split_once("## When the coder answers BLOCKED")
        .unwrap()
        .1
        .split_once("## Closing")
        .unwrap()
        .0;

    assert!(blocked.contains("Run `/invar:log`."));
    assert!(blocked.contains("with `type: failed`"));
    assert!(blocked.contains("**The developer records it**, with `/invar:log-approve <TICKET>`"));
    assert!(blocked.contains("never record it yourself"));
}

#[test]
fn the_skill_names_both_commands_and_never_tells_the_agent_to_run_what_is_the_developers() {
    let (_, body) = skill();
    let flat = body.replace('\n', " ");

    assert!(flat.contains("`/invar:log`"));
    assert!(flat.contains("`/invar:log-approve`"));

    // Each of the developer's commands appears after "run" only as "never run".
    for command in [
        "invar done",
        "invar decision approve",
        "invar decision record",
    ] {
        for verb in ["run `", "Run `"] {
            let needle = format!("{verb}{command}");
            for (at, _) in flat.match_indices(&needle) {
                let before = &flat[..at];
                assert!(
                    before.ends_with("never ") || before.ends_with("Never "),
                    "the skill tells the agent to run `{command}`: …{}",
                    &flat[at.saturating_sub(60)..(at + needle.len() + 20).min(flat.len())]
                );
            }
        }
        // Never written as a command line for the agent to copy.
        for line in body.lines() {
            let trimmed = line.trim();
            assert!(
                !(trimmed.starts_with(&format!("`{command} ")) && trimmed.ends_with('`')),
                "a command line to copy: {line}"
            );
        }
    }
    // The prohibitions themselves.
    assert!(flat.contains("Never run `invar done` yourself"));
    assert!(flat.contains("- run `invar plan approve` or `invar done`;"));
    assert!(flat.contains("never `invar decision approve`, never `invar decision record`"));
}

// --- /invar:log-approve ----------------------------------------------------------------------

#[test]
fn log_approve_cannot_be_invoked_by_the_model() {
    // Like /invar:approve: the entry is the developer's (decision-entry.md D-6),
    // and a misspelt field would be ignored without a word, so the exact
    // string.
    let (meta, body) = frontmatter("skills/log-approve/SKILL.md");

    assert_eq!(meta["name"], "log-approve");
    assert_eq!(meta["disable-model-invocation"], "true");
    assert_eq!(meta["argument-hint"], "[ticket]");
    assert!(
        !meta.contains_key("user-invocable"),
        "it must stay in the / menu"
    );
    for (key, value) in &meta {
        assert!(!value.contains(": "), "`{key}` contains `: `");
        assert!(!value.contains(" #"), "`{key}` contains ` #`");
    }
    assert!(body.contains("Do not run `invar decision approve` yourself"));
    assert!(body.contains("never run `invar decision record`"));
}

#[test]
fn log_approve_runs_the_cli_through_shell_injection() {
    let (_, body) = frontmatter("skills/log-approve/SKILL.md");
    let block = body
        .split_once("```!\n")
        .expect("the command runs before the model, in a ```! block")
        .1
        .split_once("\n```")
        .unwrap()
        .0;

    // The same guard on `$ARGUMENTS` as /invar:approve, and the command last.
    assert!(block.contains("*[!A-Za-z0-9._/-]*)"));
    assert!(
        block.ends_with("invar decision approve \"$t\" 2>&1"),
        "{block}"
    );
    assert!(body.contains("Show the output above to the developer exactly as it is, and stop."));
}

#[cfg(unix)]
#[test]
fn log_approve_finds_the_one_draft_and_lists_several() {
    use std::os::unix::fs::PermissionsExt as _;
    let (_, body) = frontmatter("skills/log-approve/SKILL.md");
    let block = body
        .split_once("```!\n")
        .unwrap()
        .1
        .split_once("\n```")
        .unwrap()
        .0
        .to_owned();
    let dir = tempfile::tempdir().unwrap();
    let bin = dir.path().join("fakebin");
    fs::create_dir_all(&bin).unwrap();
    fs::write(bin.join("invar"), "#!/bin/sh\necho \"$1 $2: $3\"\n").unwrap();
    fs::set_permissions(bin.join("invar"), fs::Permissions::from_mode(0o755)).unwrap();
    let run = |arguments: &str| {
        let out = std::process::Command::new("sh")
            .arg("-c")
            .arg(block.replace("$ARGUMENTS", arguments))
            .current_dir(dir.path())
            .env(
                "PATH",
                format!("{}:{}", bin.display(), std::env::var("PATH").unwrap()),
            )
            .output()
            .unwrap();
        String::from_utf8(out.stdout).unwrap()
    };
    let plan = |name: &str, draft: bool| {
        let p = dir.path().join(".invar/plans").join(name);
        fs::create_dir_all(&p).unwrap();
        fs::write(p.join("spec.md"), "").unwrap();
        if draft {
            fs::write(p.join("decision.md"), "").unwrap();
        }
    };

    // A Plan without a draft has nothing to record.
    plan("SHOP-100-no-draft", false);
    let none = run("");
    assert!(none.contains("no draft entry"), "{none}");
    assert!(none.contains("/invar:log"), "{none}");
    // The one draft is found by itself, whatever other Plans exist.
    plan("SHOP-214-a", true);
    assert_eq!(run(""), "decision approve: SHOP-214-a\n");
    // Several are listed, and nothing is recorded.
    plan("SHOP-300-b", true);
    let several = run("");
    assert!(several.contains("2 drafts are waiting"), "{several}");
    assert!(several.contains("  SHOP-300-b\n"), "{several}");
    assert!(!several.contains("decision approve:"), "{several}");
    // A ticket is passed through; shell syntax is refused.
    assert_eq!(run("SHOP-300"), "decision approve: SHOP-300\n");
    assert!(run("x; touch pwned").contains("is not a ticket"));
    assert!(!dir.path().join("pwned").exists());
}

#[test]
fn log_approve_is_offered_by_log_and_by_the_protocol_skill() {
    let log = frontmatter("skills/log/SKILL.md").1.replace('\n', " ");
    assert!(log.contains("record it with `/invar:log-approve <TICKET>`"));
    // The old instructions, which had the developer record by hand, are gone.
    assert!(!log.contains("--type failed"));
    assert!(!log.contains("`invar done <TICKET> -m"));

    let flat = skill().1.replace('\n', " ");
    assert!(flat.contains("| `/invar:log-approve` | the developer's:"));
    assert!(flat.contains("**The developer records the draft** with `/invar:log-approve`"));
}

#[test]
fn no_prompt_tells_an_agent_to_record_a_decision_entry() {
    // `decision approve` is the developer's gate and `decision record` would
    // go around it; `plan guard` denies both to the agent. Every line of every
    // prompt that names either is a prohibition — the command itself runs only
    // inside /invar:log-approve's ```! block, before the model.
    let mut files: Vec<String> = agents();
    for skill in fs::read_dir(plugin_dir().join("skills")).unwrap() {
        let name = skill.unwrap().file_name().to_string_lossy().into_owned();
        if plugin_dir()
            .join("skills")
            .join(&name)
            .join("SKILL.md")
            .is_file()
        {
            files.push(format!("skills/{name}/SKILL.md"));
        }
    }
    let mut mentions = 0;
    for file in &files {
        let mut in_fence = false;
        for line in read(file).lines() {
            if line.trim_start().starts_with("```") {
                in_fence = !in_fence;
                continue;
            }
            if in_fence
                || !(line.contains("invar decision approve")
                    || line.contains("invar decision record"))
            {
                continue;
            }
            mentions += 1;
            assert!(
                ["never", "Never", "Do not", "do not"]
                    .iter()
                    .any(|word| line.contains(word)),
                "{file} names a decision-recording command without forbidding it: {line}"
            );
        }
    }
    // The prohibitions themselves are there: planner, coder, reviewer, the
    // protocol skill, /invar:log and /invar:log-approve.
    assert!(mentions >= 6, "only {mentions} mentions");
}
