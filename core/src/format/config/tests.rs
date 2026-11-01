//! Tests for `invar.yaml` (contract `contracts/formats/invar-yaml.md`).
//!
//! The first group runs the contract's own examples, read straight out of the
//! Markdown by their `<!-- example: … -->` markers, so the specification and
//! the implementation cannot drift apart unnoticed. The rest pin down each
//! rule of the contract on its own.

use std::fs;
use std::path::{Path, PathBuf};

use super::{
    AcceptanceConfig, AcceptanceStyle, Config, ConfigError, Context, DEFAULT_BINDING_LIMIT,
    DEFAULT_DONE_COMMIT_MESSAGE, Done, FILE_NAME, Git, Loaded, Review, Warning, load,
    load_for_command, parse, parse_bytes,
};
use crate::fs::ProjectRoot;

fn path() -> PathBuf {
    PathBuf::from("/project").join(FILE_NAME)
}

fn read(text: &str) -> Loaded {
    parse(text, &path()).unwrap_or_else(|err| panic!("expected a valid config: {err}"))
}

fn reject(text: &str) -> String {
    match parse(text, &path()) {
        Ok(loaded) => panic!("expected an invalid config, got {:?}", loaded.config),
        Err(err @ ConfigError::Invalid { .. }) => err.to_string(),
        Err(other) => panic!("expected ConfigError::Invalid, got {other:?}"),
    }
}

fn warned_keys(loaded: &Loaded) -> Vec<&str> {
    loaded.warnings.iter().map(|w| w.key.as_str()).collect()
}

// --- the contract's examples -----------------------------------------------

/// The fenced block that follows `<!-- example: <name> -->` in the contract.
fn contract_example(name: &str) -> String {
    let contract = Path::new(env!("CARGO_MANIFEST_DIR")).join("../contracts/formats/invar-yaml.md");
    let text = fs::read_to_string(&contract)
        .unwrap_or_else(|err| panic!("reading {}: {err}", contract.display()));

    let marker = format!("<!-- example: {name} -->");
    let after_marker = text
        .split_once(&marker)
        .unwrap_or_else(|| panic!("the contract has no example `{name}`"))
        .1;
    let body = after_marker
        .split_once("```yaml\n")
        .unwrap_or_else(|| panic!("example `{name}` is not followed by a yaml block"))
        .1;
    body.split_once("\n```")
        .unwrap_or_else(|| panic!("example `{name}` has no closing fence"))
        .0
        .to_owned()
        + "\n"
}

#[test]
fn contract_10_1_light_mode_project() {
    let loaded = read(&contract_example("valid-light-mode"));
    assert_eq!(
        loaded.config,
        Config {
            scenarios: false,
            review: Review { offer: true },
            git: Git {
                base_branch: Some("develop".to_owned())
            },
            context: Context {
                binding_limit: 8000
            },
            done: Done {
                commit: true,
                commit_message: "chore: remove the Plan of {ticket}".to_owned(),
            },
            acceptance: None,
        }
    );
    assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
}

#[test]
fn contract_10_2_empty_file_means_every_default() {
    let loaded = read(&contract_example("valid-empty"));
    assert_eq!(loaded.config, Config::default());
    assert!(loaded.warnings.is_empty());
}

#[test]
fn contract_10_3_config_for_a_later_milestone_reads_cleanly() {
    let loaded = read(&contract_example("valid-later-milestone"));
    assert_eq!(loaded.config, Config::default());
    // §5 keys are ignored *silently*: a warning on every command, including
    // the SessionStart hook, would teach the team to ignore warnings (D-2).
    assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
}

#[test]
fn contract_10_4_typo_reads_with_warnings() {
    let loaded = read(&contract_example("valid-typo-warns"));
    assert_eq!(loaded.config, Config::default());
    assert_eq!(warned_keys(&loaded), ["senarios", "review.ofer"]);
}

#[test]
fn contract_10_5_yaml11_boolean_is_invalid() {
    let message = reject(&contract_example("invalid-yaml11-boolean"));
    assert!(message.contains("invalid boolean"), "{message}");
    assert!(message.contains("line 1, column 12"), "{message}");
}

#[test]
fn contract_10_6_binding_limit_above_hook_cap_is_invalid() {
    let message = reject(&contract_example("invalid-binding-limit"));
    assert!(message.contains("`context.binding_limit`"), "{message}");
    assert!(message.contains("found 12000"), "{message}");
}

#[test]
fn contract_10_7_a_full_acceptance_section() {
    let loaded = read(&contract_example("valid-acceptance-full"));
    assert_eq!(
        loaded.config.acceptance,
        Some(AcceptanceConfig {
            required: true,
            style: AcceptanceStyle::Integration,
            dir: "tests/Acceptance".to_owned(),
            run: strings(&[
                "vendor/bin/pest",
                "--colors=never",
                "--log-junit",
                "{report}",
                "{files}"
            ]),
            fakes_allowed: strings(&["http", "mail", "queue", "notification"]),
        })
    );
    assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
}

#[test]
fn contract_10_8_a_minimal_acceptance_section_takes_the_defaults() {
    let loaded = read(&contract_example("valid-acceptance-minimal"));
    let section = loaded.config.acceptance.unwrap();
    // A team that configures acceptance tests gets them by default (D-10).
    assert!(section.required);
    assert_eq!(section.style, AcceptanceStyle::Integration);
    assert!(section.fakes_allowed.is_empty());
    assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
}

#[test]
fn contract_10_9_acceptance_without_run_is_invalid() {
    let message = reject(&contract_example("invalid-acceptance-no-run"));
    assert!(
        message.contains("`acceptance.run` is required"),
        "{message}"
    );
}

#[test]
fn contract_10_10_acceptance_without_report_is_invalid() {
    let message = reject(&contract_example("invalid-acceptance-no-report"));
    assert!(message.contains("`{report}` exactly once"), "{message}");
}

#[test]
fn contract_10_11_gherkin_is_reserved() {
    let message = reject(&contract_example("invalid-acceptance-gherkin"));
    assert!(message.contains("gherkin"), "{message}");
    assert!(message.contains("later version"), "{message}");
}

#[test]
fn contract_10_12_a_dotdot_in_dir_is_invalid() {
    let message = reject(&contract_example("invalid-acceptance-dir"));
    assert!(message.contains("`acceptance.dir`"), "{message}");
    assert!(message.contains("`..`"), "{message}");
}

// --- defaults ---------------------------------------------------------------

#[test]
fn defaults_are_the_documented_values() {
    // Pinned against contract §3, not against `Default`, so a change to the
    // defaults has to change this test and the contract together.
    let config = read("").config;
    assert!(!config.scenarios);
    // Offered by default since 2.0.0 (D-8): the review is the safety net for
    // an invariant the planner missed, now that the coder does not read them.
    assert!(config.review.offer);
    assert_eq!(config.git.base_branch, None);
    assert_eq!(config.context.binding_limit, 9000);
    assert_eq!(DEFAULT_BINDING_LIMIT, 9000);
    // invar makes no commit unless asked (§3.5).
    assert!(!config.done.commit);
    assert_eq!(config.done.commit_message, "Remove plan {ticket}");
    assert_eq!(DEFAULT_DONE_COMMIT_MESSAGE, "Remove plan {ticket}");
    // No section, no acceptance tests (§3.6).
    assert_eq!(config.acceptance, None);
}

#[test]
fn comments_only_and_a_lone_document_marker_mean_every_default() {
    for text in [
        "# nothing here\n",
        "---\n",
        "\n\n",
        "---\n# still nothing\n",
    ] {
        assert_eq!(read(text).config, Config::default(), "{text:?}");
    }
}

#[test]
fn an_empty_value_means_absent() {
    // D-3: `key:` is YAML null, and null means "use the default".
    let loaded = read("scenarios:\nreview:\ngit:\n  base_branch: ~\ncontext:\n");
    assert_eq!(loaded.config, Config::default());
    assert!(loaded.warnings.is_empty());
}

#[test]
fn each_key_is_read_on_its_own() {
    assert!(read("scenarios: true\n").config.scenarios);
    assert!(read("review:\n  offer: true\n").config.review.offer);
    // The one way to turn the offer off.
    assert!(!read("review:\n  offer: false\n").config.review.offer);
    assert_eq!(
        read("git:\n  base_branch: origin/main\n")
            .config
            .git
            .base_branch,
        Some("origin/main".to_owned())
    );
    assert_eq!(
        read("context:\n  binding_limit: 1\n")
            .config
            .context
            .binding_limit,
        1
    );
}

// --- done ---------------------------------------------------------------------

#[test]
fn done_commit_and_its_message_are_read() {
    let done = read("done:\n  commit: true\n").config.done;
    assert!(done.commit);
    assert_eq!(done.message_for("SHOP-214"), "Remove plan SHOP-214");

    let done =
        read("done:\n  commit: true\n  commit_message: \"chore({ticket}): remove the Plan\"\n")
            .config
            .done;
    assert_eq!(
        done.message_for("SHOP-214"),
        "chore(SHOP-214): remove the Plan"
    );

    // A message without the placeholder is taken as it is.
    let done = read("done:\n  commit_message: Close the cycle\n")
        .config
        .done;
    assert!(!done.commit);
    assert_eq!(done.message_for("SHOP-214"), "Close the cycle");
}

#[test]
fn an_empty_done_section_means_absent() {
    let loaded = read("done:\n");
    assert_eq!(loaded.config, Config::default());
    let loaded = read("done:\n  commit:\n  commit_message: ~\n");
    assert_eq!(loaded.config, Config::default());
}

#[test]
fn done_commit_is_a_strict_boolean() {
    for rejected in ["yes", "on", "1"] {
        let message = reject(&format!("done:\n  commit: {rejected}\n"));
        assert!(message.contains("invalid boolean"), "{rejected}: {message}");
    }
}

#[test]
fn done_commit_message_is_one_non_empty_line() {
    for value in [
        "\"\"",
        "\"   \"",
        "\"first\\nsecond\"",
        "|\n    first\n    second",
    ] {
        let message = reject(&format!("done:\n  commit_message: {value}\n"));
        assert!(
            message.contains("`done.commit_message`"),
            "{value}: {message}"
        );
        assert!(message.contains("one non-empty line"), "{value}: {message}");
    }
    // Surrounding whitespace is not part of the message.
    assert_eq!(
        read("done:\n  commit_message: \"  Remove {ticket}  \"\n")
            .config
            .done
            .commit_message,
        "Remove {ticket}"
    );
}

#[test]
fn an_unknown_key_under_done_warns() {
    let loaded = read("done:\n  comit: true\n");
    assert!(!loaded.config.done.commit);
    assert_eq!(warned_keys(&loaded), ["done.comit"]);
}

// --- acceptance ---------------------------------------------------------------

fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_owned()).collect()
}

/// A section with `run` as given, and everything else valid.
fn with_run(run: &str) -> String {
    format!("acceptance:\n  dir: tests/Acceptance\n  run: {run}\n")
}

/// A section with `dir` as given, and everything else valid.
fn with_dir(dir: &str) -> String {
    format!("acceptance:\n  dir: {dir}\n  run: [pest, \"{{report}}\", \"{{files}}\"]\n")
}

#[test]
fn an_empty_acceptance_section_means_absent() {
    for text in [
        "acceptance:\n",
        "acceptance: ~\n",
        "acceptance: {}\n",
        "acceptance:\n  # nothing yet\n",
        "acceptance:\n  dir:\n  run: ~\n",
    ] {
        let loaded = read(text);
        assert_eq!(loaded.config, Config::default(), "{text:?}");
        assert!(loaded.warnings.is_empty(), "{text:?}");
    }
}

#[test]
fn an_acceptance_section_must_be_a_mapping() {
    for text in [
        "acceptance: tests\n",
        "acceptance: [a, b]\n",
        "acceptance: true\n",
    ] {
        let message = reject(text);
        assert!(message.contains("line 1"), "{text:?}: {message}");
    }
}

#[test]
fn acceptance_required_is_read_and_is_a_strict_boolean() {
    let section = |required: &str| {
        format!(
            "acceptance:\n  required: {required}\n  dir: t\n  \
             run: [pest, \"{{report}}\", \"{{files}}\"]\n"
        )
    };
    let acceptance = |text: &str| read(text).config.acceptance.unwrap();
    assert!(acceptance(&section("true")).required);
    assert!(!acceptance(&section("false")).required);
    // An empty value means absent, and absent is `true` here.
    assert!(acceptance(&section("~")).required);
    for rejected in ["yes", "no", "on", "off", "1"] {
        let message = reject(&section(rejected));
        assert!(message.contains("invalid boolean"), "{rejected}: {message}");
    }
}

#[test]
fn acceptance_style_lists_what_is_allowed() {
    let message = reject(
        "acceptance:\n  style: cucumber\n  dir: t\n  \
         run: [pest, \"{report}\", \"{files}\"]\n",
    );
    assert!(message.contains("\"cucumber\""), "{message}");
    assert!(message.contains("`integration`"), "{message}");
    assert!(message.contains("`gherkin` is reserved"), "{message}");
}

#[test]
fn acceptance_dir_is_required_and_relative() {
    let message = reject("acceptance:\n  run: [pest, \"{report}\", \"{files}\"]\n");
    assert!(
        message.contains("`acceptance.dir` is required"),
        "{message}"
    );

    for (dir, reason) in [
        ("\"\"", "empty"),
        ("/srv/tests", "absolute"),
        ("\"C:\\\\tests\"", "absolute"),
        ("\"\\\\tests\"", "absolute"),
        ("tests/", "trailing"),
        ("\"tests\\\\\"", "trailing"),
        ("..", "`..`"),
        ("../tests", "`..`"),
        ("tests/../../other", "`..`"),
        ("\"tests\\\\..\\\\other\"", "`..`"),
    ] {
        let message = reject(&with_dir(dir));
        assert!(message.contains("`acceptance.dir`"), "{dir}: {message}");
        assert!(message.contains(reason), "{dir}: {message}");
    }

    // `..` is a segment, not a substring, and whether the directory exists is
    // not this reader's question.
    for dir in ["tests/Acceptance", "a..b/c", "..tests", "does/not/exist"] {
        assert_eq!(read(&with_dir(dir)).config.acceptance.unwrap().dir, dir);
    }
}

#[test]
fn acceptance_run_is_a_list_of_strings() {
    for run in [
        "{a: b}",
        "7",
        "true",
        "[pest, 7, \"{report}\", \"{files}\"]",
    ] {
        let message = reject(&with_run(run));
        assert!(message.contains("line 3"), "{run}: {message}");
    }
    let message = reject(&with_run("[]"));
    assert!(message.contains("at least one element"), "{message}");
    let message = reject("acceptance:\n  dir: t\n");
    assert!(
        message.contains("`acceptance.run` is required"),
        "{message}"
    );
}

#[test]
fn a_string_run_is_refused_by_name_and_never_split() {
    // The core defines no splitting (D-11), even of a string that would work.
    for run in [
        "\"vendor/bin/pest --log-junit {report} {files}\"",
        "'pest {report} {files}'",
        "pest",
        "|\n    pest {report} {files}",
    ] {
        let message = reject(&with_run(run));
        assert!(
            message.contains(
                "`acceptance.run` is a list, not a string: write the program and each \
                 argument as its own element, such as \
                 [\"vendor/bin/pest\", \"--colors=never\", \"--log-junit\", \"{report}\", \"{files}\"] \
                 (invar-yaml.md §3.6)"
            ),
            "{run}: {message}"
        );
    }

    // It takes its place in the combined message of the section.
    let message = reject("acceptance:\n  style: gherkin\n  dir: ../t\n  run: pest\n");
    for expected in ["gherkin", "`acceptance.dir`", "is a list, not a string"] {
        assert!(message.contains(expected), "{expected}: {message}");
    }
}

#[test]
fn acceptance_run_starts_with_a_program() {
    for run in [
        "[\"{report}\", \"{files}\"]",
        "[\"{files}\", \"{report}\"]",
        "[\"\", \"{report}\", \"{files}\"]",
    ] {
        let message = reject(&with_run(run));
        assert!(
            message.contains("starts with the program"),
            "{run}: {message}"
        );
    }
}

#[test]
fn each_acceptance_placeholder_is_one_whole_element() {
    for (run, placeholder, found) in [
        ("[pest, \"{files}\"]", "{report}", "found it 0 times"),
        ("[pest, \"{report}\"]", "{files}", "found it 0 times"),
        (
            "[pest, \"{report}\", \"{report}\", \"{files}\"]",
            "{report}",
            "found it 2 times",
        ),
        (
            "[pest, \"{report}\", \"{files}\", \"{files}\"]",
            "{files}",
            "found it 2 times",
        ),
        (
            "[pest, \"--log-junit={report}\", \"{files}\"]",
            "{report}",
            "part of a longer element",
        ),
        (
            "[\"pest --log-junit {report} {files}\"]",
            "{files}",
            "part of a longer element",
        ),
        (
            "[pest, \"{report}\", \"{report}.xml\", \"{files}\"]",
            "{report}",
            "part of a longer element",
        ),
    ] {
        let message = reject(&with_run(run));
        assert!(
            message.contains(&format!("`{placeholder}` exactly once")),
            "{run}: {message}"
        );
        assert!(message.contains(found), "{run}: {message}");
    }
}

#[test]
fn an_unknown_placeholder_in_run_is_caught_and_a_literal_brace_is_fine() {
    for (run, placeholder) in [
        (
            "[pest, \"{reports}\", \"{report}\", \"{files}\"]",
            "{reports}",
        ),
        (
            "[pest, \"{report}\", \"{files}\", \"--x={Files}\"]",
            "{Files}",
        ),
        ("[pest, \"{report}\", \"{files}\", \"${HOME}\"]", "{HOME}"),
    ] {
        let message = reject(&with_run(run));
        assert!(
            message.contains(&format!("unknown placeholder `{placeholder}`")),
            "{run}: {message}"
        );
    }

    let run =
        "[pest, \"{report}\", \"{files}\", \"{}\", \"{a,b}\", \"{\", \"}\", \"{\\\"k\\\": 1}\"]";
    let section = read(&with_run(run)).config.acceptance.unwrap();
    assert_eq!(section.run.len(), 8);
    assert_eq!(section.run[3], "{}");
    assert_eq!(section.run[7], "{\"k\": 1}");
}

#[test]
fn acceptance_fakes_allowed_is_free_form_and_non_empty() {
    let text = "acceptance:\n  dir: t\n  run: [pest, \"{report}\", \"{files}\"]\n  \
                fakes_allowed: [http, made-up]\n";
    assert_eq!(
        read(text).config.acceptance.unwrap().fakes_allowed,
        strings(&["http", "made-up"])
    );
    let empty = text.replace("made-up", "\"  \"");
    let message = reject(&empty);
    assert!(message.contains("`acceptance.fakes_allowed`"), "{message}");
    let not_a_list = text.replace("[http, made-up]", "http");
    assert!(reject(&not_a_list).contains("line 4"));
}

#[test]
fn an_unknown_key_under_acceptance_warns() {
    let loaded = read(&format!(
        "{}  timeout: 30\n",
        with_run("[pest, \"{report}\", \"{files}\"]")
    ));
    assert!(loaded.config.acceptance.is_some());
    assert_eq!(warned_keys(&loaded), ["acceptance.timeout"]);
}

#[test]
fn every_problem_in_the_acceptance_section_is_reported_in_one_message() {
    let message = reject(
        "git:\n  base_branch: \"\"\nacceptance:\n  style: gherkin\n  dir: ../t\n  \
         run: [pest, \"{reports}\"]\n  fakes_allowed: [\"\"]\n",
    );
    for expected in [
        "`git.base_branch`",
        "gherkin",
        "`acceptance.dir`",
        "unknown placeholder `{reports}`",
        "`{report}` exactly once",
        "`{files}` exactly once",
        "`acceptance.fakes_allowed`",
    ] {
        assert!(message.contains(expected), "{expected}: {message}");
    }
}

// --- unknown and ignored keys ----------------------------------------------

#[test]
fn unknown_keys_warn_and_never_fail() {
    let loaded = read(
        "scenarios: false\n\
         added_in_v9: {anything: [1, 2, 3]}\n\
         review:\n  offer: true\n  model: fast\n\
         git:\n  remote: upstream\n\
         context:\n  max_tokens: 4000\n",
    );
    assert!(loaded.config.review.offer);
    assert_eq!(
        warned_keys(&loaded),
        [
            "added_in_v9",
            "review.model",
            "git.remote",
            "context.max_tokens"
        ]
    );
}

#[test]
fn ignored_keys_are_silent_only_where_the_design_puts_them() {
    // `sync_on_plan` is a §6 key under `context`, not at the top level.
    let loaded = read("context:\n  sync_on_plan: true\nsync_on_plan: true\n");
    assert_eq!(warned_keys(&loaded), ["sync_on_plan"]);
}

#[test]
fn nothing_inside_an_ignored_subtree_is_validated_or_reported() {
    let loaded = read(
        "lint:\n  rules:\n    made-up-rule: {enabled: yes, severity: 7}\n\
         pipeline: not even a mapping\n\
         exclude: [\"a\", \"b\"]\n",
    );
    assert_eq!(loaded.config, Config::default());
    assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
}

#[test]
fn a_block_scalar_that_looks_like_a_key_is_not_a_key() {
    // The case that rules out a hand-written flat reader (D-1).
    let loaded = read("notes: |\n  scenarios: true\nscenarios: false\n");
    assert!(!loaded.config.scenarios);
    assert_eq!(warned_keys(&loaded), ["notes"]);
}

#[test]
fn warning_names_the_key_and_says_what_to_do() {
    let warning = Warning {
        key: "review.ofer".to_owned(),
    };
    let message = warning.to_string();
    assert!(message.contains("`review.ofer`"), "{message}");
    assert!(message.contains("typo"), "{message}");
}

// --- value rules ------------------------------------------------------------

#[test]
fn booleans_are_true_or_false_only() {
    // D-6. YAML 1.2 spellings are accepted; YAML 1.1 ones are errors.
    for accepted in ["true", "false", "True", "FALSE"] {
        read(&format!("scenarios: {accepted}\n"));
    }
    for rejected in ["yes", "no", "on", "off", "y", "n", "1", "0"] {
        let message = reject(&format!("scenarios: {rejected}\n"));
        assert!(message.contains("invalid boolean"), "{rejected}: {message}");
        let message = reject(&format!("review:\n  offer: {rejected}\n"));
        assert!(message.contains("invalid boolean"), "{rejected}: {message}");
    }
}

#[test]
fn a_branch_name_yaml_reads_as_another_type_must_be_quoted() {
    // D-7: unquoted, `1.10` is the number 1.1 to other YAML readers.
    for unquoted in ["1.10", "2026", "true", "0755"] {
        let message = reject(&format!("git:\n  base_branch: {unquoted}\n"));
        assert!(message.contains("must be quoted"), "{unquoted}: {message}");
    }
    assert_eq!(
        read("git:\n  base_branch: \"1.10\"\n")
            .config
            .git
            .base_branch,
        Some("1.10".to_owned())
    );
}

#[test]
fn a_branch_name_must_be_non_empty_without_whitespace() {
    for value in ["\"\"", "\" main\"", "\"release 2\""] {
        let message = reject(&format!("git:\n  base_branch: {value}\n"));
        assert!(message.contains("`git.base_branch`"), "{value}: {message}");
        assert!(message.contains("remove the key"), "{value}: {message}");
    }
}

#[test]
fn binding_limit_is_between_1_and_10000() {
    assert_eq!(
        read("context:\n  binding_limit: 10000\n")
            .config
            .context
            .binding_limit,
        10_000
    );
    for value in ["0", "10001", "-5"] {
        let message = reject(&format!("context:\n  binding_limit: {value}\n"));
        assert!(
            message.contains("between 1 and 10000"),
            "{value}: {message}"
        );
    }
    for value in ["9000.5", "lots"] {
        reject(&format!("context:\n  binding_limit: {value}\n"));
    }
}

#[test]
fn every_value_problem_is_reported_in_one_message() {
    let message = reject("git:\n  base_branch: \"\"\ncontext:\n  binding_limit: 0\n");
    assert!(message.contains("`git.base_branch`"), "{message}");
    assert!(message.contains("`context.binding_limit`"), "{message}");
}

// --- document shape ---------------------------------------------------------

#[test]
fn the_document_must_be_a_mapping() {
    reject("- scenarios\n- review\n");
    reject("just a sentence\n");
}

#[test]
fn a_section_must_be_a_mapping() {
    let message = reject("git: main\n");
    assert!(message.contains("line 1"), "{message}");
}

#[test]
fn a_duplicated_key_is_an_error_without_programmer_hints() {
    let message = reject("scenarios: false\nscenarios: true\n");
    assert!(message.contains("duplicate mapping key"), "{message}");
    assert!(message.contains("line 2"), "{message}");
    assert!(!message.contains("DuplicateKeyPolicy"), "{message}");
}

#[test]
fn a_second_document_is_an_error_without_programmer_hints() {
    let message = reject("scenarios: false\n---\nscenarios: true\n");
    assert!(message.contains("multiple YAML documents"), "{message}");
    assert!(!message.contains("from_multiple"), "{message}");
}

#[test]
fn a_syntax_error_names_the_file_and_the_position() {
    let message = reject("scenarios: [false\n");
    assert!(message.starts_with("/project/invar.yaml: "), "{message}");
    assert!(message.contains("line 1"), "{message}");
}

// --- encoding ---------------------------------------------------------------

#[test]
fn bom_and_every_line_ending_are_accepted() {
    let expected = read("scenarios: true\nreview:\n  offer: true\n").config;
    for text in [
        "\u{FEFF}scenarios: true\nreview:\n  offer: true\n",
        "scenarios: true\r\nreview:\r\n  offer: true\r\n",
        "scenarios: true\rreview:\r  offer: true\r",
    ] {
        assert_eq!(read(text).config, expected, "{text:?}");
    }
}

#[test]
fn invalid_utf8_is_an_error_with_its_offset() {
    let err = parse_bytes(b"git:\n  base_branch: r\xE9lease\n", &path()).unwrap_err();
    assert!(matches!(err, ConfigError::Invalid { .. }), "{err:?}");
    assert!(err.to_string().contains("offset 21"), "{err}");
}

// --- reading from disk ------------------------------------------------------

#[test]
fn a_missing_file_is_its_own_error_naming_invar_init() {
    let root = tempfile::tempdir().unwrap();
    let err = load(&ProjectRoot::for_test(root.path())).unwrap_err();
    assert!(matches!(err, ConfigError::Missing { .. }), "{err:?}");
    let message = err.to_string();
    assert!(message.contains("invar init"), "{message}");
    assert!(message.contains(FILE_NAME), "{message}");
}

#[test]
fn an_unreadable_file_is_not_reported_as_missing() {
    // A directory where the file should be: present, but not readable as one.
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join(FILE_NAME)).unwrap();
    let err = load(&ProjectRoot::for_test(root.path())).unwrap_err();
    assert!(matches!(err, ConfigError::Unreadable { .. }), "{err:?}");
    assert!(!err.to_string().contains("invar init"), "{err}");
}

#[test]
fn load_reads_the_file_in_the_project_root() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join(FILE_NAME), "review:\n  offer: true\n").unwrap();
    let loaded = load(&ProjectRoot::for_test(root.path())).unwrap();
    assert!(loaded.config.review.offer);
    assert_eq!(
        loaded.path,
        root.path().canonicalize().unwrap().join(FILE_NAME)
    );
}

// --- scenarios: true --------------------------------------------------------

#[test]
fn scenarios_true_is_read_but_refused_by_commands() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join(FILE_NAME), "scenarios: true\n").unwrap();

    // The reader accepts it: the file is valid (D-4).
    assert!(
        load(&ProjectRoot::for_test(root.path()))
            .unwrap()
            .config
            .scenarios
    );

    // A command refuses it, saying why and what to do.
    let err = load_for_command(&ProjectRoot::for_test(root.path())).unwrap_err();
    assert!(
        matches!(err, ConfigError::ScenariosNotSupported { .. }),
        "{err:?}"
    );
    let message = err.to_string();
    assert!(message.contains("later milestone"), "{message}");
    assert!(message.contains("scenarios: false"), "{message}");
}

#[test]
fn light_mode_passes_the_command_check() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join(FILE_NAME), "scenarios: false\n").unwrap();
    assert!(
        !load_for_command(&ProjectRoot::for_test(root.path()))
            .unwrap()
            .config
            .scenarios
    );

    // Absent means false, so an empty file passes too.
    fs::write(root.path().join(FILE_NAME), "").unwrap();
    assert!(load_for_command(&ProjectRoot::for_test(root.path())).is_ok());
}

#[test]
fn load_for_command_still_reports_a_missing_file_as_missing() {
    let root = tempfile::tempdir().unwrap();
    let err = load_for_command(&ProjectRoot::for_test(root.path())).unwrap_err();
    assert!(matches!(err, ConfigError::Missing { .. }), "{err:?}");
}
