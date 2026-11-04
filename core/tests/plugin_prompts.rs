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
    assert!(description.contains("SPLIT"), "{description}");
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
        "## A ticket too complex for one cycle",
        "## A Plan that came back",
        "## What you never do",
    ] {
        assert!(
            body.contains(heading),
            "the planner prompt lost `{heading}`"
        );
    }
    for phrase in [
        // The three shapes of the answer, which the skill parses by eye.
        "QUESTIONS",
        "SPLIT\nWhy:",
        "PLAN .invar/plans/",
        // Reading is narrow.
        "Never a full scan",
        // ...and complete where it matters: every caller of a changed
        // signature, and the installed framework code, not a memory of it.
        "Find every caller of what you change",
        "Read the framework code you rely on, not your memory of it",
        "A changed\nsignature whose callers are not all in Scope?",
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
fn the_planner_decides_whether_a_ticket_is_one_cycle_before_it_asks_or_writes() {
    let (_, body) = frontmatter("agents/planner.md");
    let flat = body.replace('\n', " ");

    // The analysis comes after reading the code and before the questions and the
    // Plan: questions about the parts belong to the smaller tickets.
    let decide = body
        .find("### 3. Decide whether the ticket is one cycle")
        .expect("the planner prompt lost the step that decides it");
    let read = body.find("### 2. Read the code, narrowly").unwrap();
    let ask = body.find("### 4. Decide what you cannot know").unwrap();
    let write = body.find("### 5. Write the Plan").unwrap();
    assert!(read < decide && decide < ask && ask < write);
    for part in [
        "**behaviors**",
        "**modules and layers**",
        "**invariants**",
        "**unknown**",
    ] {
        assert!(flat.contains(part), "the analysis lost {part}");
    }
    // Size alone is not the measure; several things in one ticket are.
    assert!(flat.contains("**Size alone is not a reason.**"));
}

#[test]
fn a_split_is_recommended_and_the_developer_decides() {
    let (_, body) = frontmatter("agents/planner.md");
    let flat = body.replace('\n', " ");

    // No Plan on disk for a ticket that is not planned.
    assert!(flat.contains("**Write no Plan file and create no directory.**"));
    // The tasks only on request, each with a title and a description to paste.
    assert!(flat.contains("**Propose the tasks only when the developer asks you to.**"));
    assert!(
        flat.contains(
            "Tasks:\n1. <title>\n   <ticket description>"
                .replace('\n', " ")
                .as_str()
        )
    );
    // The decision and the tracker are the developer's.
    assert!(flat.contains("**Whether to follow it is the developer's decision**"));
    assert!(flat.contains("**Never give a task a ticket id**"));
    assert!(flat.contains("(`SHOP-214-1`)"));
    assert!(flat.contains("**Create a ticket, or give one an id**"));
    // And "plan it as one" ends the question for this ticket.
    assert!(flat.contains("do not return `SPLIT` again for this ticket"));
}

#[test]
fn the_planner_asks_for_numbered_criteria_and_knows_the_tests_to_change_section() {
    let (_, body) = frontmatter("agents/planner.md");

    assert!(
        body.contains("`- AC1: Given <state>, when <action>, then <observable outcome>.`"),
        "the planner prompt lost the `AC<n>:` form"
    );
    assert!(
        body.contains("## Acceptance tests to change"),
        "the planner prompt lost the new heading"
    );
    assert!(
        body.contains("only when the ticket must change or\ndelete another ticket's locked"),
        "the planner prompt lost when the section is written"
    );
    // The example shows the form it asks for; `plan validate` holds the rest.
    assert!(example_plan().contains("\n- AC1: Given "));
}

#[test]
fn the_reviewer_reports_each_criterion_by_its_id() {
    let (_, body) = frontmatter("agents/reviewer.md");

    assert!(body.contains("Every criterion appears, by its id, exactly once"));
    assert!(body.contains("A Plan approved before ids existed has none"));
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
        "## Phase 2 — the test-writer if required, then the coder",
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
    assert!(flat.contains("**Start the coder** (`coder`; `invar:coder` if namespaced)"));
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
    assert!(flat.contains(
        "**tell the developer the cycle is ready to close** — *\"Ready to close: run \
         `/invar:done`.\"* — and stop"
    ));
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
    let flat = one_line(&skill().1);

    // The values the `Agent` tool accepts per call (M1-22).
    assert!(flat.contains("`sonnet`, `opus`, `haiku` or `fable`"));
    assert!(flat.contains("**A planner on anything but its default is said aloud**"));
    // No fallback exists, so none is invented.
    assert!(flat.contains("`model_not_found`"));
    assert!(flat.contains("offer to run the agent again with `model: sonnet`"));
    assert!(flat.contains("switch models silently."));
    // The coder will be held to the test-writer's tests.
    assert!(flat.contains("**A test-writer on anything but its default is said aloud**"));
    assert!(flat.contains(
        "*\"The acceptance tests were written by sonnet, not by the test-writer's default.\"*"
    ));
}

// --- the dispatch (AT-12) ---------------------------------------------------------------------

/// `text` with every run of whitespace a single space, so that a phrase matches
/// however it is wrapped and indented in a list.
fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The protocol skill's Phase 2, on one line.
fn phase_2() -> String {
    one_line(&skill().1)
        .split_once("## Phase 2 — ")
        .unwrap()
        .1
        .split_once("## When the coder answers BLOCKED")
        .unwrap()
        .0
        .to_owned()
}

/// The part of Phase 2 between two markers, each of which has to be there.
fn between(flat: &str, from: &str, to: &str) -> String {
    let (_, after) = flat
        .split_once(from)
        .unwrap_or_else(|| panic!("no {from:?} in Phase 2"));
    let (part, _) = after
        .split_once(to)
        .unwrap_or_else(|| panic!("no {to:?} after {from:?} in Phase 2"));
    part.to_owned()
}

#[test]
fn the_dispatch_is_decided_by_the_acceptance_line_alone() {
    let flat = phase_2();

    let check = flat
        .find("**Phase 2 begins with `invar plan check <TICKET>`.**")
        .unwrap();
    let line = flat.find("Read its **`acceptance:` line**").unwrap();
    let skipped = flat.find("### `acceptance: skipped`").unwrap();
    let required = flat.find("### `acceptance: required`").unwrap();
    assert!(check < line && line < skipped && skipped < required);
    assert!(flat.contains("If it refuses, start nothing"));
    assert!(flat.contains(
        "**The decision comes from this line**, never from the conversation, the ticket or your \
         own judgement"
    ));
    // A CLI too old to print the line decides nothing either.
    assert!(flat.contains("If the check passes with no such line"));
}

#[test]
fn the_acceptance_line_the_dispatch_reads_is_the_one_plan_check_prints() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("invar.yaml"), "").unwrap();
    fs::write(dir.path().join(".gitconfig"), "[user]\n\tname = rade\n").unwrap();
    let plan_dir = dir.path().join(".invar/plans/ACME-87-export-invoices-csv");
    fs::create_dir_all(&plan_dir).unwrap();
    fs::write(plan_dir.join("spec.md"), example_plan()).unwrap();
    let invar = |args: &[&str]| {
        Command::cargo_bin("invar")
            .unwrap()
            .current_dir(dir.path())
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", dir.path().join(".gitconfig"))
            .env("HOME", dir.path())
            .args(args)
            .assert()
            .success()
    };

    invar(&["plan", "approve", "ACME-87", "--no-acceptance"]);
    let check = invar(&["plan", "check", "ACME-87"]);
    let stdout = String::from_utf8(check.get_output().stdout.clone()).unwrap();
    assert!(
        stdout.lines().any(|l| l == "acceptance: skipped"),
        "{stdout}"
    );
    assert!(phase_2().contains("### `acceptance: skipped`"));
}

#[test]
fn a_skipped_ticket_starts_the_coder_directly() {
    let flat = phase_2();
    let skipped = between(
        &flat,
        "### `acceptance: skipped`",
        "### `acceptance: required`",
    );
    assert!(skipped.contains("Start the coder"), "{skipped}");
    assert!(!skipped.contains("test-writer"), "{skipped}");
    assert!(!skipped.contains("verify"), "{skipped}");
}

#[test]
fn a_required_ticket_starts_the_test_writer_then_verify_then_the_coder() {
    let flat = phase_2();
    let required = between(&flat, "### `acceptance: required`", "### The coder");

    let writer = required
        .find("**Start the test-writer** (`test-writer`; `invar:test-writer` if namespaced)")
        .expect("the test-writer is started");
    let locked = required.find("**`LOCKED`**").expect("LOCKED is handled");
    let verify = required
        .find("run `invar acceptance verify <TICKET>`")
        .expect("LOCKED is followed by the command");
    let only_if = required.find("**Only if it passes**").unwrap();
    let coder = required
        .find("start the coder.")
        .expect("the coder comes after the command");
    assert!(writer < locked && locked < verify && verify < only_if && only_if < coder);
    assert!(required.contains("Do not start the coder yet."));
    assert!(required.contains("The agent's word is not the check; the command is."));
    // `verify` refusing stops the cycle. With no `acceptance:` section it exits 0 having
    // checked nothing, so the pass is the line it prints.
    assert!(required.contains("printing `verified <N> locked file(s)`"));
    assert!(required.contains("If `verify` refuses, or that line is not there"));
    // A relocked ticket's `verify` says so beside that line (acceptance-lock.md D-31).
    assert!(required.contains("saying `unlocked by the developer:` are information"));
    assert!(required.contains("start no coder, and stop"));

    // The coder's section comes after the test-writer's.
    assert!(flat.find("### `acceptance: required`") < flat.find("**Start the coder**"));
    assert!(flat.contains("one per work unit when the Plan declares them"));
}

#[test]
fn the_line_the_dispatch_keys_on_after_locked_is_the_one_verify_prints() {
    // Nothing locked, for a `required` Plan, is a refusal: `verify` is not vacuous. The
    // whole-directory form, with nothing to check, shows the shape of the line.
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("invar.yaml"),
        "acceptance:\n  dir: tests/Acceptance\n  run: [pest, \"{report}\", \"{files}\"]\n",
    )
    .unwrap();
    Command::cargo_bin("invar")
        .unwrap()
        .current_dir(dir.path())
        .args(["acceptance", "verify"])
        .assert()
        .success()
        .stdout("verified 0 locked file(s)\n");
}

#[test]
fn the_test_writer_is_started_with_the_ticket_line_only() {
    let flat = phase_2();
    assert!(
        flat.contains("Every agent in this phase is started with the ticket line and nothing else")
    );
    assert!(flat.contains("`Ticket: SHOP-214`"));
    let required = between(&flat, "### `acceptance: required`", "### The coder");
    assert!(required.contains("with the ticket line and nothing else"));
    // Before both of them.
    assert!(flat.find("`Ticket: SHOP-214`") < flat.find("**Start the test-writer**"));
}

#[test]
fn a_test_writers_blocked_starts_no_coder_and_goes_to_the_developer() {
    let flat = phase_2();
    let blocked = between(&flat, "**`BLOCKED <reason>`**", "### The coder");

    assert!(blocked.contains("put it to the developer, as it gave it, and stop."));
    assert!(blocked.contains("Start no coder."));
    assert!(!blocked.contains("start the coder"), "{blocked}");
    // What the developer can do.
    assert!(blocked.contains("fix the cause (test infrastructure, a factory)"));
    assert!(blocked.contains("`/invar:approve <TICKET> --reapprove --no-acceptance`"));
    assert!(blocked.contains("revise the Plan"));
    // A file left pending would stop `done` later, and is not the main session's to touch.
    assert!(blocked.contains("pending `invar-lock:` line"));
    assert!(blocked.contains("the developer's to remove, not yours"));
    // A `failed` entry only when the Plan is the cause.
    assert!(blocked.contains("Do not draft a `failed` entry by default"));
    assert!(blocked.contains("Only if the reason is that the Plan cannot be tested as approved"));
}

#[test]
fn every_flag_the_protocol_skill_gives_invar_approve_exists() {
    // The helper below reads `` `invar …` `` spans, and these start `/invar:approve`.
    let help = Command::cargo_bin("invar")
        .unwrap()
        .args(["plan", "approve", "--help"])
        .assert()
        .success();
    let help = String::from_utf8(help.get_output().stdout.clone()).unwrap();
    let body = skill().1;
    let mut seen = 0;
    for span in body.split('`').filter(|s| s.starts_with("/invar:approve ")) {
        for flag in span.split_whitespace().filter(|w| w.starts_with("--")) {
            seen += 1;
            assert!(help.contains(flag), "`plan approve` has no {flag}");
        }
    }
    assert!(seen >= 2, "found only {seen}");
    // The flags on their own, in the gate and in the table.
    for flag in ["--acceptance", "--no-acceptance", "--no-start"] {
        assert!(body.contains(&format!("`{flag}`")), "{flag}");
        assert!(help.contains(flag), "{flag}");
    }
}

#[test]
fn the_command_table_has_verify_and_the_flags_of_approve() {
    let (_, body) = skill();
    assert!(body.contains(
        "| `invar acceptance verify <TICKET>` | yours, after the test-writer's `LOCKED`"
    ));
    let approve = body
        .lines()
        .find(|line| line.starts_with("| `/invar:approve` |"))
        .unwrap();
    assert!(approve.contains("`--acceptance`"), "{approve}");
    assert!(approve.contains("`--no-acceptance`"), "{approve}");
    assert!(approve.contains("`--no-start`"), "{approve}");
}

#[test]
fn the_main_session_never_writes_tests_or_locks_them() {
    let flat = one_line(&skill().1);
    let never = flat.split_once("## Never").unwrap().1;
    assert!(never.contains(
        "start the coder for a `required` ticket before `invar acceptance verify` passes"
    ));
    assert!(never.contains(
        "edit or write tests yourself, or run `invar acceptance lock`: that is the test-writer's"
    ));
    assert!(never.contains("run `invar acceptance unlock`"));
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
        frontmatter("agents/test-writer.md").1,
        frontmatter("agents/reviewer.md").1,
        skill().1,
        frontmatter("skills/approve/SKILL.md").1,
        frontmatter("skills/relock/SKILL.md").1,
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
fn the_coder_answers_in_one_of_three_shapes() {
    let (meta, body) = frontmatter("agents/coder.md");

    assert!(meta["description"].contains("ticket line and nothing else"));
    assert!(meta["description"].contains("BLOCKED-TEST"));
    assert!(body.contains("Exactly one of three shapes"));
    assert!(
        body.contains("```\nBLOCKED-TEST <file> <AC> <reason in one line>\n"),
        "the BLOCKED-TEST shape"
    );
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
fn the_coders_blocked_test_is_for_a_locked_test_and_changes_no_test() {
    let flat = one_line(&frontmatter("agents/coder.md").1);

    assert!(flat.contains("`BLOCKED-TEST` is only for a **locked** test"));
    assert!(flat.contains("It is not for a test that is merely hard to pass."));
    assert!(flat.contains("Leave your code in place, change no test"));
    assert!(flat.contains("The developer decides whether the test is wrong; you do not."));
}

#[test]
fn the_coder_writes_no_lock_line_and_leaves_locked_tests_alone() {
    let flat = one_line(&frontmatter("agents/coder.md").1);

    assert!(flat.contains("Note its `acceptance:` line"));
    assert!(flat.contains("**With `acceptance: required`**, the test-writer has already"));
    assert!(flat.contains("You never write a file with an `invar-lock:` line"));
    assert!(flat.contains("Your unit tests are your own, as always."));
    assert!(flat.contains("**With `acceptance: skipped`, each criterion gets a test from you"));
    assert!(flat.contains("with no `invar-lock:` line"));
    assert!(flat.contains("Never run `invar acceptance unlock`"));
}

#[test]
fn the_test_writer_does_what_the_parts_of_the_spec_say() {
    // Those parts are the spec's, not the prompt's (A-7).
    let flat = one_line(&frontmatter("agents/test-writer.md").1);
    assert!(flat.contains("called `Unlocked tests` or `Acceptance tests to change`"));
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

// --- the test-writer (AT-11) ------------------------------------------------------------------

#[test]
fn the_test_writer_has_exactly_its_tools_and_a_strong_model() {
    let (meta, _) = frontmatter("agents/test-writer.md");
    let tools: BTreeSet<&str> = meta["tools"].split(',').map(str::trim).collect();

    assert_eq!(meta["name"], "test-writer");
    assert_eq!(
        tools,
        BTreeSet::from(["Read", "Grep", "Glob", "Bash", "Write", "Edit"])
    );
    // What it writes is the fixed point the coder is held to (plugin/NOTES.md §14).
    assert_eq!(meta["model"], "opus");
    for (key, value) in &meta {
        assert!(!value.contains(": "), "`{key}` contains `: `");
        assert!(!value.contains(" #"), "`{key}` contains ` #`");
    }
}

#[test]
fn the_test_writers_description_tells_the_main_session_what_to_give_it_and_what_it_returns() {
    let (meta, _) = frontmatter("agents/test-writer.md");
    let description = &meta["description"];
    assert!(description.contains("ticket line and nothing else"));
    assert!(description.contains("Acceptance required"));
    assert!(description.contains("LOCKED"));
    assert!(description.contains("BLOCKED"));
}

#[test]
fn the_test_writer_answers_in_one_of_two_shapes_and_locked_is_tied_to_the_commands_line() {
    let (_, body) = frontmatter("agents/test-writer.md");
    let flat = body.replace('\n', " ");

    assert!(
        body.contains("```\nLOCKED\n<at most 8 lines"),
        "the LOCKED shape"
    );
    assert!(
        body.contains("```\nBLOCKED <the reason, in one line>\n"),
        "the BLOCKED shape"
    );
    assert!(flat.contains(
        "Answer `LOCKED` only after `invar acceptance lock <TICKET>` printed \
         `locked <TICKET> <N> file(s)`"
    ));
    // The line it keys on is the one the command prints.
    assert!(
        fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../contracts/formats/acceptance-lock.md")
        )
        .unwrap()
        .contains("locked <TICKET> <N> file(s)")
    );
    // Every reason the procedure stops for.
    for reason in [
        "cannot be tested from outside the code",
        "cannot honestly be made red",
        "missing test infrastructure",
    ] {
        assert!(flat.contains(reason), "{reason}");
    }
}

#[test]
fn the_test_writer_begins_with_plan_check_then_the_spec_and_then_locks() {
    let flat = frontmatter("agents/test-writer.md").1.replace('\n', " ");
    let check = flat
        .find("Run `invar plan check <TICKET>` **before anything else**.")
        .expect("plan check first");
    let spec = flat
        .find("Run `invar acceptance spec <TICKET>`.")
        .expect("then the spec");
    let lock = flat
        .find("Run `invar acceptance lock <TICKET>`")
        .expect("then lock");
    assert!(check < spec && spec < lock);
    assert!(flat.contains("`acceptance:` line says `skipped`"));
    assert!(flat.contains("answer `BLOCKED <what it said>` and stop"));
    // Its rounds are capped, and a refusal is never met by asserting less.
    assert!(flat.contains("at most five runs"));
    assert!(flat.contains("Never get past it by making a test weaker."));
    // `lock` runs a suite, and may take longer than a Bash call's default limit.
    assert!(flat.contains("the longest timeout your Bash tool allows"));
}

#[test]
fn the_test_writer_follows_the_spec_and_has_no_branch_by_style_or_framework() {
    // A new style is a change to the command's output, not to this prompt (A-7, §13b).
    let (_, body) = frontmatter("agents/test-writer.md");
    let flat = body.replace('\n', " ");
    assert!(flat.contains("Follow its conventions exactly"));
    let lower = body.to_lowercase();
    for branch in [
        "gherkin",
        "if the style",
        "integration",
        "pest",
        "phpunit",
        "laravel",
        "pytest",
    ] {
        assert!(!lower.contains(branch), "the prompt says `{branch}`");
    }
}

#[test]
fn the_test_writer_changes_only_its_own_tests() {
    let body = frontmatter("agents/test-writer.md").1;
    let flat = body.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(flat.contains("**Change a factory, a seeder, application code or the Plan**"));
    assert!(flat.contains("**Touch a file with a locked line**"));
    assert!(flat.contains("Only `invar acceptance lock` writes a digest."));
    assert!(flat.contains("**Commit, push, or switch branches.**"));
    assert!(flat.contains("never run `invar plan approve` or `invar done`"));
}

#[test]
fn the_test_writers_example_is_valid_for_the_token_and_the_line_it_shows() {
    // One criterion, negative, with its control probe and the pending line, in a domain
    // that is not the pilot's. No digest: only `invar acceptance lock` writes one.
    let (_, body) = frontmatter("agents/test-writer.md");
    let example = body
        .split_once("in made-up syntax:\n\n```\n")
        .unwrap()
        .1
        .split_once("\n```")
        .unwrap()
        .0;
    assert!(example.starts_with("# invar-lock: LIB-87\n"), "{example}");
    assert!(!example.contains("sha256:"), "{example}");
    assert!(example.contains("test \"AC2 "), "{example}");
    assert!(example.contains("control probe"), "{example}");
}

#[test]
fn the_test_writer_is_never_pointed_at_business_rules_or_the_log() {
    // DESIGN §5c, as for the coder.
    let (_, body) = frontmatter("agents/test-writer.md");
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
                "the test-writer is pointed at business rules or the log: {line}"
            );
        }
    }
    assert!(
        prohibitions >= 2,
        "the prohibitions themselves went missing"
    );
    assert!(body.contains("=== invar: conventions ==="));
    assert!(body.contains("`invar context show --conventions`"));
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
        "AC1: met —",
        "Evidence: <file:line>",
        "AC2: not met —",
        "AC3: unclear —",
        "Why:",
        // AT-14: the locked tests, then the code they depend on, then what
        // `skipped` leaves uncovered.
        "## Locked acceptance tests",
        "AC1: asserts —",
        "AC2: weaker —",
        "AC3: unrelated —",
        "AC4: no locked test",
        "Unlocked: <file> (round <R>)",
        "## Shared test code changed by the coder",
        "## Criteria no test covers",
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
fn the_reviewer_judges_each_locked_test_against_its_criterion() {
    // AT-14: run and red are proved by `acceptance lock`; meaning is judged here.
    let (meta, body) = frontmatter("agents/reviewer.md");
    let flat = body.split_whitespace().collect::<Vec<_>>().join(" ");

    assert!(flat.contains("### 4. Check each locked test against its criterion"));
    assert!(flat.contains("Only when `=== Acceptance tests ===` says `Acceptance: required`"));
    // The three verdicts, each with what it means.
    for verdict in [
        "**`asserts`** — the test fails if the criterion does not hold",
        "**`weaker`** — it asserts less than the criterion says",
        "**`unrelated`** — it does not test this criterion",
    ] {
        assert!(flat.contains(verdict), "no {verdict:?}");
    }
    assert!(flat.contains(
        "Give the `file:line` of the assertion for every `weaker` and every `unrelated`"
    ));
    assert!(flat.contains("Every `weaker` and every `unrelated` has its `file:line`"));
    // Files, not test names: the input has no names to give.
    assert!(flat.contains("read the tests in each file"));
    // Unlocked files, with extra care.
    assert!(flat.contains("**Read unlocked files with extra care.**"));
    assert!(flat.contains("say whether it still asserts its criterion"));
    // Shared test code is a judgement on the diff.
    assert!(flat.contains("### 5. Look for shared test code the coder changed"));
    assert!(flat.contains("Factories, seeders and test helpers outside `acceptance.dir`"));
    assert!(flat.contains("could make that test pass without the behavior it checks"));
    // `skipped`: information, not a defect.
    assert!(flat.contains("criteria **no test covers** — as information, not as a defect"));
    assert!(
        meta["description"].contains("whether each locked acceptance test asserts its criterion")
    );
}

#[test]
fn the_reviewer_treats_declared_test_changes_as_inside_scope() {
    let (_, body) = frontmatter("agents/reviewer.md");
    let flat = body.split_whitespace().collect::<Vec<_>>().join(" ");

    assert!(flat.contains("its `## Acceptance tests to change`, when it declares any"));
    assert!(flat.contains(
        "A change to a locked test that the Plan declares there — its `change` or `delete` — \
         is approved, and is **not** reported as outside Scope"
    ));
    assert!(flat.contains(
        "A change to another ticket's locked test that the Plan does **not** declare is \
         reported here, under Changes outside Scope"
    ));
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
fn review_resolves_the_ticket_in_its_block() {
    // It names the ticket, and the reviewer is started from that line.
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

// --- the developer's skills: one command, allowed by name ------------------------------------

/// The skills whose command records the developer's decision, and the command:
/// the model cannot run any of them (`plan guard` denies it), so each runs
/// before the model sees the skill, in its ```! block.
const DEVELOPER_SKILLS: [(&str, &str); 4] = [
    ("skills/approve/SKILL.md", "invar plan approve"),
    ("skills/log-approve/SKILL.md", "invar decision approve"),
    ("skills/done/SKILL.md", "invar done"),
    ("skills/relock/SKILL.md", "invar acceptance unlock"),
];

/// The ```! block of a skill.
fn shell_block(file: &str) -> String {
    let (_, body) = frontmatter(file);
    body.split_once("```!\n")
        .unwrap_or_else(|| panic!("{file}: the command runs before the model, in a ```! block"))
        .1
        .split_once("\n```")
        .unwrap()
        .0
        .to_owned()
}

#[test]
fn each_developer_skill_runs_one_command_that_its_frontmatter_allows() {
    // Claude Code checks a skill's command against the permission rules before
    // it runs it. A command no rule allows does not run: outside auto mode the
    // invocation aborts, in auto mode the model is asked to run it instead —
    // and the model may not, so the approval silently never happens (the
    // pilot session in plugin/NOTES.md §15). So the block is one simple command,
    // with no shell syntax a permission check could balk at, and the skill's
    // `allowed-tools` names exactly that command.
    for (file, command) in DEVELOPER_SKILLS {
        let (meta, body) = frontmatter(file);
        let block = shell_block(file);

        assert_eq!(
            block,
            format!("{command} --skill $ARGUMENTS"),
            "{file}: the block is the command and nothing else"
        );
        assert_eq!(
            meta["allowed-tools"],
            format!("Bash({command} *)"),
            "{file}: allowed-tools must name the block's command"
        );
        assert_eq!(meta["disable-model-invocation"], "true", "{file}");
        assert!(
            !meta.contains_key("user-invocable"),
            "{file}: it must stay in the / menu"
        );
        // A refusal comes back as `error: …`, and an empty result is said, not
        // worked around.
        assert!(body.contains("A line starting `error:`"), "{file}");
        assert!(body.contains("If there is no output at all"), "{file}");
        assert!(
            body.contains(&format!("Do not run `{command}` yourself")),
            "{file}"
        );
    }
}

#[cfg(unix)]
#[test]
fn each_developer_skills_block_reports_a_refusal_as_output_and_exits_0() {
    // The block run as Claude Code runs it, through `sh`, with the real binary:
    // a command that exits non-zero would abort the skill and drop the reason
    // with it.
    let binary = assert_cmd::cargo::cargo_bin("invar");
    let bin = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink(&binary, bin.path().join("invar")).unwrap();
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("invar.yaml"), "").unwrap();
    fs::create_dir_all(dir.path().join(".invar/plans")).unwrap();

    for (file, _) in DEVELOPER_SKILLS {
        let out = std::process::Command::new("sh")
            .arg("-c")
            .arg(shell_block(file).replace("$ARGUMENTS", "SHOP-999"))
            .current_dir(dir.path())
            .env(
                "PATH",
                format!(
                    "{}:{}",
                    bin.path().display(),
                    std::env::var("PATH").unwrap()
                ),
            )
            .output()
            .unwrap();
        let stdout = String::from_utf8(out.stdout).unwrap();
        assert!(out.status.success(), "{file}: exit {:?}", out.status);
        assert!(stdout.starts_with("error: "), "{file}: {stdout}");
        assert!(stdout.contains("SHOP-999"), "{file}: {stdout}");
    }
}

#[test]
fn done_is_the_developers_and_the_protocol_skill_points_at_it() {
    let (meta, body) = frontmatter("skills/done/SKILL.md");
    assert_eq!(meta["name"], "done");
    assert_eq!(
        meta["argument-hint"],
        "[ticket] [--commit <message> | --no-commit]"
    );
    // Both flags exist on the command the skill runs.
    let help = Command::cargo_bin("invar")
        .unwrap()
        .args(["done", "--help"])
        .assert()
        .success();
    let help = String::from_utf8(help.get_output().stdout.clone()).unwrap();
    assert!(help.contains("--commit <MESSAGE>"), "{help}");
    assert!(help.contains("--no-commit"), "{help}");
    assert!(body.contains("A line starting `removed:` means the cycle is closed"));
    let words = body.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(words.contains("never delete the Plan another way"));

    let flat = skill().1.replace('\n', " ");
    assert!(flat.contains("*\"Ready to close: run `/invar:done`.\"*"));
    assert!(flat.contains("| `/invar:done` | the developer's:"));
}

#[test]
fn no_skill_sends_the_developer_to_another_terminal() {
    // The whole cycle runs in Claude Code: every step that is the developer's
    // has a skill. A prompt that says "from their own terminal" sends them out
    // of the session for nothing.
    for skill in fs::read_dir(plugin_dir().join("skills")).unwrap() {
        let path = skill.unwrap().path().join("SKILL.md");
        if !path.is_file() {
            continue;
        }
        let text = fs::read_to_string(&path).unwrap().replace('\n', " ");
        assert!(
            !text.contains("own terminal"),
            "{} sends the developer to their own terminal",
            path.display()
        );
    }
}

#[test]
fn approve_starts_phase_2_only_on_the_recorded_line() {
    // Task 6: the approval is the decision to implement, so the skill starts
    // Phase 2 — but only when the command's own success line is there, never
    // on the model's reading of the output.
    let (meta, body) = frontmatter("skills/approve/SKILL.md");
    let flat = body.replace('\n', " ");

    assert_eq!(
        meta["argument-hint"],
        "[ticket] [--acceptance | --no-acceptance] [--no-start]"
    );
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
        .find("**Otherwise, stop.** Write no code and start no agent.")
        .expect("what to do without the line");
    assert!(condition < no_start && no_start < start && start < otherwise);
    // Phase 2 as the protocol describes it, and only there: a second copy of the
    // dispatch here could only disagree with it.
    assert!(flat.contains("as the \"Phase 2\" section of the protocol skill (`invar`)"));
    for dispatch in ["test-writer", "`acceptance:`", "invar acceptance verify"] {
        assert!(!flat.contains(dispatch), "the skill says {dispatch}");
    }
    assert!(flat.contains("never by your own reading"));
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
fn approve_passes_both_acceptance_flags_through_a_block_that_is_unchanged() {
    // The developer's decision reaches the CLI through `$ARGUMENTS`, so neither
    // the block nor what it is allowed to run has to name a flag.
    let (meta, _) = frontmatter("skills/approve/SKILL.md");
    assert_eq!(meta["allowed-tools"], "Bash(invar plan approve *)");
    assert_eq!(meta["disable-model-invocation"], "true");
    assert_eq!(
        shell_block("skills/approve/SKILL.md"),
        "invar plan approve --skill $ARGUMENTS"
    );

    let help = Command::cargo_bin("invar")
        .unwrap()
        .args(["plan", "approve", "--help"])
        .assert()
        .success();
    let help = String::from_utf8(help.get_output().stdout.clone()).unwrap();
    for flag in meta["argument-hint"]
        .split(|c: char| !(c.is_ascii_lowercase() || c == '-'))
        .filter(|word| word.starts_with("--"))
    {
        assert!(help.contains(flag), "`plan approve` has no {flag}");
    }
}

#[test]
fn approve_shows_a_flag_that_changed_nothing_and_starts_nothing() {
    // AT-3: on an approval that holds, a flag naming another value is not applied.
    let flat = frontmatter("skills/approve/SKILL.md").1.replace('\n', " ");
    let otherwise = flat.find("**Otherwise, stop.**").unwrap();
    let flag = flat
        .find("if a line under it says a flag changed nothing, make sure the developer sees")
        .expect("the line is shown");
    assert!(otherwise < flag);
}

// --- /invar:relock (AT-13) ----------------------------------------------------------------

#[test]
fn relock_cannot_be_invoked_by_the_model_and_allows_exactly_its_command() {
    // As for /invar:approve: a misspelt field is ignored without a word, so the exact
    // string, and the skill's other half is the command `plan guard` denies to the agent.
    let (meta, body) = frontmatter("skills/relock/SKILL.md");

    assert_eq!(meta["name"], "relock");
    assert_eq!(meta["disable-model-invocation"], "true");
    assert_eq!(meta["allowed-tools"], "Bash(invar acceptance unlock *)");
    assert_eq!(meta["argument-hint"], "<ticket> <file>...");
    assert!(
        !meta.contains_key("user-invocable"),
        "it must stay in the / menu"
    );
    assert_eq!(
        shell_block("skills/relock/SKILL.md"),
        "invar acceptance unlock --skill $ARGUMENTS"
    );
    for (key, value) in &meta {
        assert!(!value.contains(": "), "`{key}` contains `: `");
        assert!(!value.contains(" #"), "`{key}` contains ` #`");
    }
    assert!(body.contains("Do not run `invar acceptance unlock` yourself"));
}

#[test]
fn relock_starts_the_test_writer_only_on_the_success_line() {
    let flat = one_line(&frontmatter("skills/relock/SKILL.md").1);

    let condition = flat
        .find("**If the output contains the line `unlocked <TICKET> <N> file(s), round <R> of 2`**")
        .expect("the start is conditional on the line");
    let start = flat
        .find("start the test-writer for that ticket now")
        .expect("the start");
    let otherwise = flat
        .find("**Otherwise, stop.** Write no code and start no agent.")
        .expect("what to do without the line");
    let silent = flat
        .find("**If there is no output at all**")
        .expect("no output");
    assert!(condition < start && start < otherwise && otherwise < silent);
    assert!(flat.contains("never by your own reading"));
    // It points at the protocol skill, and keeps no copy of what follows.
    assert!(flat.contains("the section \"When the coder answers BLOCKED-TEST\" of the protocol"));
    assert!(!flat.contains("invar acceptance verify"));
    assert!(!flat.contains("start the coder"));
    assert!(flat.contains("the message says to end the cycle as `failed`"));
}

/// The protocol skill's section on `BLOCKED-TEST`, on one line.
fn blocked_test() -> String {
    one_line(&skill().1)
        .split_once("## When the coder answers BLOCKED-TEST")
        .expect("the section")
        .1
        .split_once("## Closing")
        .unwrap()
        .0
        .to_owned()
}

#[test]
fn the_protocol_skill_stops_on_blocked_test_and_puts_it_to_the_developer() {
    let flat = one_line(&skill().1);
    assert!(flat.contains("It answers with one of three things."));
    assert!(flat.contains("**`BLOCKED-TEST <file> <AC> <reason>`**"));

    let section = blocked_test();
    assert!(section.contains("**Put it to the developer, as it was given, and stop.**"));
    assert!(section.contains("Start no agent."));
    // What the developer can do, each theirs.
    assert!(section.contains("`/invar:relock <TICKET> <file>`"));
    assert!(section.contains("tell you to start the coder again, with their words"));
    assert!(section.contains("end the cycle as `failed`, with `/invar:log`"));
    assert!(section.contains("never unlock a test yourself"));
    assert!(section.contains("Never start the test-writer on your own reading"));
    assert!(section.contains("A third `/invar:relock` is refused"));
}

#[test]
fn the_test_writer_starts_only_after_relock_and_the_coder_only_after_verify() {
    let section = blocked_test();
    let stop = section.find("as it was given, and stop.").unwrap();
    let relock = section
        .find("**After `/invar:relock` succeeds**, and only then, start the test-writer")
        .expect("the test-writer waits for the skill");
    let locked = section
        .find("on `LOCKED`, run `invar acceptance verify <TICKET>`")
        .unwrap();
    let only_if = section.find("only if it passes").unwrap();
    let coder = section
        .find("start the **coder** again with the ticket line")
        .unwrap();
    assert!(stop < relock && relock < locked && locked < only_if && only_if < coder);
    assert!(section.contains("start the test-writer with the ticket line and nothing else"));
    // No start of either before the skill has succeeded.
    let before = &section[..relock];
    assert!(!before.contains("start the test-writer"), "{before}");
}

#[test]
fn the_line_relock_keys_on_is_the_one_the_contract_gives_the_cli() {
    let contract = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../contracts/formats/acceptance-lock.md"),
    )
    .unwrap();
    assert!(contract.contains("unlocked <TICKET> <N> file(s), round <R> of 2"));
    // tests/acceptance_unlock.rs holds that the binary prints it.
}

#[test]
fn the_command_table_has_relock() {
    let (_, body) = skill();
    let line = body
        .lines()
        .find(|line| line.starts_with("| `/invar:relock <TICKET> <file>...` |"))
        .expect("the table names it");
    assert!(line.contains("the developer's"), "{line}");
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
fn log_drafts_acceptance_from_plan_check_and_not_from_the_conversation() {
    // decision-entry.md §8.1, D-15: the value is the Plan's, which
    // `decision approve` checks the draft against.
    let (_, body) = frontmatter("skills/log/SKILL.md");
    let flat = body.replace('\n', " ");
    let flat = flat.split_whitespace().collect::<Vec<_>>().join(" ");

    assert!(
        flat.contains(
            "`acceptance` is the value of the `acceptance:` line that \
             `invar plan check <TICKET>` prints, never what the conversation said"
        ),
        "{flat}"
    );
    assert!(flat.contains("when `plan check` prints no such line, leave the key out"));
    // The example shows the key, and `decision validate` accepts it (below).
    assert!(body.contains("\ntype: confirmed\nacceptance: required\n---\n"));
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
    // Closing is the developer's, through `/invar:done` or CI; a draft entry
    // blocks it.
    assert!(flat.contains("**6. Closing is the developer's.**"));
    assert!(flat.contains("CI can run it instead"));
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
         `/invar:log-approve` (if an entry is wanted) → `/invar:done`**"
    ));
    // And the steps in that order.
    let steps: Vec<usize> = [
        "**1. Commit.**",
        "**2. Offer the review**",
        "**3. Render the PR description**",
        "**4. Push, and the PR is reviewed**",
        "**5. Offer to draft the decision entry**",
        "**6. Closing is the developer's.**",
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
            // `allowed-tools` names the command so the ```! block may run it.
            if in_fence
                || line.starts_with("allowed-tools: ")
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
