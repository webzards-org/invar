//! Tests for `invar.yaml` (contract `contracts/formats/invar-yaml.md`).
//!
//! The first group runs the contract's own examples, read straight out of the
//! Markdown by their `<!-- example: … -->` markers, so the specification and
//! the implementation cannot drift apart unnoticed. The rest pin down each
//! rule of the contract on its own.

use std::fs;
use std::path::{Path, PathBuf};

use super::{
    Config, ConfigError, Context, DEFAULT_BINDING_LIMIT, FILE_NAME, Git, Loaded, Review, Warning,
    load, load_for_command, parse, parse_bytes,
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

// --- defaults ---------------------------------------------------------------

#[test]
fn defaults_are_the_documented_values() {
    // Pinned against contract §3, not against `Default`, so a change to the
    // defaults has to change this test and the contract together.
    let config = read("").config;
    assert!(!config.scenarios);
    assert!(!config.review.offer);
    assert_eq!(config.git.base_branch, None);
    assert_eq!(config.context.binding_limit, 9000);
    assert_eq!(DEFAULT_BINDING_LIMIT, 9000);
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
