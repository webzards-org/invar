//! Tests for adding the deny rule to `.claude/settings.json` (M1-15).
//!
//! Two properties are checked on every input that has content, by
//! [`merged`]: the result **means** the old file plus one rule, and it **is**
//! the old file with one piece of text inserted — nothing else moved.

use serde_json::{Value, json};

use super::{SettingsError, add_deny_rule};

const RULE: &str = "Read(/.invar/decisions/**)";

/// The file after the rule is added; `None` when it already was.
fn add(old: &str) -> Option<String> {
    add_deny_rule(Some(old), RULE).unwrap()
}

fn parse(text: &str) -> Value {
    serde_json::from_str(text.strip_prefix('\u{FEFF}').unwrap_or(text))
        .unwrap_or_else(|err| panic!("not JSON ({err}):\n{text}"))
}

/// What the old file means once the rule is in `permissions.deny`.
fn meaning_with_rule(old: &str) -> Value {
    let mut value = if old.trim().is_empty() {
        json!({})
    } else {
        parse(old)
    };
    let permissions = value
        .as_object_mut()
        .unwrap()
        .entry("permissions")
        .or_insert_with(|| json!({}));
    let deny = permissions
        .as_object_mut()
        .unwrap()
        .entry("deny")
        .or_insert_with(|| json!([]));
    deny.as_array_mut().unwrap().push(json!(RULE));
    value
}

/// `new` is `old` with one piece of text inserted somewhere: what comes
/// before the insertion and what comes after it cover all of `old`.
fn only_inserted(old: &str, new: &str) {
    let prefix = old
        .bytes()
        .zip(new.bytes())
        .take_while(|(a, b)| a == b)
        .count();
    let suffix = old
        .bytes()
        .rev()
        .zip(new.bytes().rev())
        .take_while(|(a, b)| a == b)
        .count();
    assert!(
        prefix + suffix >= old.len(),
        "more than an insertion changed.\n--- old\n{old}\n--- new\n{new}"
    );
}

/// Add the rule to a file that has content, and check both properties.
fn merged(old: &str) -> String {
    let new = add(old).unwrap_or_else(|| panic!("the rule was taken to be there already:\n{old}"));
    assert_eq!(
        parse(&new),
        meaning_with_rule(old),
        "the meaning changed.\n--- old\n{old}\n--- new\n{new}"
    );
    only_inserted(old, &new);
    new
}

// --- a file that is not there -----------------------------------------------------

const FRESH: &str = "{\n  \"permissions\": {\n    \"deny\": [\n      \"Read(/.invar/decisions/**)\"\n    ]\n  }\n}\n";

#[test]
fn no_file_gets_a_new_one_in_the_usual_layout() {
    assert_eq!(add_deny_rule(None, RULE).unwrap().as_deref(), Some(FRESH));
}

#[test]
fn an_empty_or_blank_file_is_treated_as_no_file() {
    for old in ["", "\n", "  \r\n\t "] {
        assert_eq!(add(old).as_deref(), Some(FRESH), "{old:?}");
    }
}

#[test]
fn an_empty_object_is_opened_up_and_its_closing_text_kept() {
    assert_eq!(
        add("{}").as_deref(),
        Some(
            "{\n  \"permissions\": {\n    \"deny\": [\n      \"Read(/.invar/decisions/**)\"\n    ]\n  }\n}"
        )
    );
    assert_eq!(add("{}\n").as_deref(), Some(FRESH));
    assert_eq!(add("{\n}\n").as_deref(), Some(FRESH));
    assert_eq!(
        add("{ }\r\n").map(|text| parse(&text)),
        Some(meaning_with_rule("{}"))
    );
}

// --- adding at each depth -----------------------------------------------------------

#[test]
fn a_file_without_permissions_gets_the_section_after_its_last_key() {
    let old = "{\n  \"model\": \"opus\",\n  \"env\": {\n    \"APP_ENV\": \"local\"\n  }\n}\n";

    assert_eq!(
        merged(old),
        "{\n  \"model\": \"opus\",\n  \"env\": {\n    \"APP_ENV\": \"local\"\n  },\n  \
         \"permissions\": {\n    \"deny\": [\n      \"Read(/.invar/decisions/**)\"\n    ]\n  }\n}\n"
    );
}

#[test]
fn permissions_without_deny_gets_the_deny_list_and_keeps_allow() {
    let old = "{\n  \"permissions\": {\n    \"allow\": [\"Bash(ls)\"]\n  }\n}\n";

    assert_eq!(
        merged(old),
        "{\n  \"permissions\": {\n    \"allow\": [\"Bash(ls)\"],\n    \"deny\": [\n      \
         \"Read(/.invar/decisions/**)\"\n    ]\n  }\n}\n"
    );
}

#[test]
fn an_existing_deny_list_gets_the_rule_last_and_its_entries_stay_in_order() {
    let old = "{\n  \"permissions\": {\n    \"deny\": [\n      \"Read(./.env)\",\n      \
               \"Bash(rm -rf:*)\"\n    ],\n    \"allow\": [\"Bash(ls)\"]\n  }\n}\n";

    assert_eq!(
        merged(old),
        "{\n  \"permissions\": {\n    \"deny\": [\n      \"Read(./.env)\",\n      \
         \"Bash(rm -rf:*)\",\n      \"Read(/.invar/decisions/**)\"\n    ],\n    \
         \"allow\": [\"Bash(ls)\"]\n  }\n}\n"
    );
}

#[test]
fn an_inline_deny_list_stays_inline() {
    let old = "{\n  \"permissions\": {\n    \"deny\": [\"Read(./.env)\"]\n  }\n}\n";

    assert_eq!(
        merged(old),
        "{\n  \"permissions\": {\n    \"deny\": [\"Read(./.env)\", \"Read(/.invar/decisions/**)\"]\n  }\n}\n"
    );
}

#[test]
fn an_empty_deny_list_is_opened_up() {
    let old = "{\n  \"permissions\": {\n    \"deny\": []\n  }\n}\n";

    assert_eq!(
        merged(old),
        "{\n  \"permissions\": {\n    \"deny\": [\n      \"Read(/.invar/decisions/**)\"\n    ]\n  }\n}\n"
    );
}

#[test]
fn a_compact_file_stays_compact() {
    assert_eq!(
        merged("{\"a\":1}"),
        "{\"a\":1, \"permissions\": {\"deny\": [\"Read(/.invar/decisions/**)\"]}}"
    );
    assert_eq!(
        merged("{\"permissions\":{\"allow\":[\"Bash(ls)\"]}}"),
        "{\"permissions\":{\"allow\":[\"Bash(ls)\"], \"deny\": [\"Read(/.invar/decisions/**)\"]}}"
    );
}

// --- the team's own layout ---------------------------------------------------------------

#[test]
fn tabs_and_wider_indentation_are_followed() {
    assert_eq!(
        merged("{\n\t\"a\": 1\n}\n"),
        "{\n\t\"a\": 1,\n\t\"permissions\": {\n\t\t\"deny\": [\n\t\t\t\"Read(/.invar/decisions/**)\"\n\t\t]\n\t}\n}\n"
    );
    assert_eq!(
        merged("{\n    \"a\": 1\n}\n"),
        "{\n    \"a\": 1,\n    \"permissions\": {\n        \"deny\": [\n            \
         \"Read(/.invar/decisions/**)\"\n        ]\n    }\n}\n"
    );
}

#[test]
fn crlf_files_get_crlf_and_nothing_else() {
    let old = "{\r\n  \"a\": 1\r\n}\r\n";

    let new = merged(old);

    assert_eq!(
        new.matches('\n').count(),
        new.matches("\r\n").count(),
        "a bare LF was written into a CRLF file: {new:?}"
    );
}

#[test]
fn a_bom_stays_where_it_is() {
    let new = merged("\u{FEFF}{\n  \"a\": 1\n}\n");

    assert!(new.starts_with('\u{FEFF}'));
}

#[test]
fn a_file_without_a_final_newline_does_not_get_one() {
    let new = merged("{\n  \"a\": 1\n}");

    assert!(!new.ends_with('\n'));
}

// --- the rule is already there -----------------------------------------------------------------

#[test]
fn a_rule_that_is_already_there_changes_nothing() {
    for old in [
        "{\"permissions\":{\"deny\":[\"Read(/.invar/decisions/**)\"]}}",
        "{\n  \"permissions\": {\n    \"deny\": [\n      \"Read(./.env)\",\n      \
         \"Read(/.invar/decisions/**)\",\n      \"Bash(rm:*)\"\n    ]\n  }\n}\n",
    ] {
        assert_eq!(add(old), None, "{old}");
    }
}

#[test]
fn adding_twice_adds_once() {
    for old in [
        "{}",
        "{\n  \"a\": 1\n}\n",
        "{\n  \"permissions\": {\"allow\": []}\n}\n",
        "{\"permissions\":{\"deny\":[\"x\"]}}",
    ] {
        let once = merged(old);
        assert_eq!(add(&once), None, "{once}");
    }
}

// --- what has to survive scanning ----------------------------------------------------------------

#[test]
fn braces_brackets_commas_and_escaped_quotes_inside_strings_are_only_text() {
    let old = "{\n  \"note\": \"a } , \\\" ] { [ \\\\\",\n  \"permissions\": {\n    \"allow\": \
               [\"Bash(echo \\\"}\\\":*)\"]\n  }\n}\n";

    merged(old);
}

#[test]
fn non_ascii_text_is_not_a_problem() {
    merged(
        "{\n  \"name\": \"Ćirilica — Београд ☕\",\n  \"permissions\": {\n    \"allow\": [\"Ж\"]\n  }\n}\n",
    );
}

#[test]
fn only_the_top_levels_permissions_counts() {
    // A `permissions` or `deny` deeper down is somebody else's key.
    merged("{\n  \"hooks\": {\"permissions\": \"x\", \"deny\": 1},\n  \"a\": 2\n}\n");
}

#[test]
fn a_repeated_key_is_edited_where_json_readers_look() {
    // JSON readers keep the last of two equal keys, so that is the one that
    // has to be edited.
    merged("{\n  \"permissions\": {\"allow\": []},\n  \"permissions\": {\"deny\": []}\n}\n");
}

#[test]
fn a_deny_list_with_other_kinds_of_values_is_added_to_all_the_same() {
    merged("{\n  \"permissions\": {\n    \"deny\": [1, null, \"x\", {\"a\": [2]}]\n  }\n}\n");
}

#[test]
fn a_realistic_settings_file_is_changed_by_one_insertion() {
    let old = r#"{
  "model": "claude-opus",
  "env": {
    "APP_ENV": "local",
    "DB_HOST": "127.0.0.1"
  },
  "permissions": {
    "allow": [
      "Bash(php artisan test:*)",
      "Bash(composer install)",
      "Bash(npm run build)"
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

    let new = merged(old);

    // The team's own keys are where they were, in the order they were in.
    for key in ["\"model\"", "\"env\"", "\"permissions\"", "\"hooks\""] {
        assert!(new.contains(key), "{key} is gone");
    }
    assert!(
        new.find("\"model\"").unwrap() < new.find("\"env\"").unwrap()
            && new.find("\"env\"").unwrap() < new.find("\"permissions\"").unwrap()
            && new.find("\"permissions\"").unwrap() < new.find("\"hooks\"").unwrap(),
        "keys were reordered:\n{new}"
    );
}

// --- what is refused ------------------------------------------------------------------------------

#[test]
fn a_file_that_is_not_json_is_refused_with_its_position_and_never_repaired() {
    for old in [
        "{\"a\": 1,}",
        "// the team's settings\n{}",
        "{'a': 1}",
        "{\"a\": ",
        "not json",
    ] {
        match add_deny_rule(Some(old), RULE) {
            Err(SettingsError::NotJson { line, column, .. }) => {
                assert!(line >= 1 && column >= 1, "{old:?}");
            }
            other => panic!("expected NotJson for {old:?}, got {other:?}"),
        }
    }
}

#[test]
fn json_that_is_not_an_object_is_refused() {
    for old in ["[]", "null", "\"settings\"", "42"] {
        assert_eq!(
            add_deny_rule(Some(old), RULE),
            Err(SettingsError::NotAnObject),
            "{old}"
        );
    }
}

#[test]
fn a_permissions_or_deny_of_the_wrong_type_is_refused() {
    assert_eq!(
        add_deny_rule(Some("{\"permissions\": \"all\"}"), RULE),
        Err(SettingsError::WrongType {
            path: "permissions",
            expected: "an object"
        })
    );
    assert_eq!(
        add_deny_rule(Some("{\"permissions\": []}"), RULE),
        Err(SettingsError::WrongType {
            path: "permissions",
            expected: "an object"
        })
    );
    assert_eq!(
        add_deny_rule(Some("{\"permissions\": {\"deny\": {}}}"), RULE),
        Err(SettingsError::WrongType {
            path: "permissions.deny",
            expected: "an array"
        })
    );
    assert_eq!(
        add_deny_rule(Some("{\"permissions\": {\"deny\": \"Read(x)\"}}"), RULE),
        Err(SettingsError::WrongType {
            path: "permissions.deny",
            expected: "an array"
        })
    );
}

#[test]
fn every_refusal_says_what_to_do_instead() {
    for error in [
        SettingsError::NotJson {
            line: 3,
            column: 7,
            message: "expected value".to_owned(),
        },
        SettingsError::NotAnObject,
        SettingsError::WrongType {
            path: "permissions",
            expected: "an object",
        },
    ] {
        let message = error.to_string();
        assert!(message.contains("by hand"), "{message}");
        assert!(
            message.contains("does not replace") || message.contains("does not rewrite"),
            "{message}"
        );
    }
}
