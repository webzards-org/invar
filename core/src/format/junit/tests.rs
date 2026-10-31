//! Tests for JUnit reports (`contracts/formats/test-results.md`).
//!
//! The first group runs every report in `contracts/fixtures/junit/`, real and
//! synthetic, against `results.json`, the hand-written expectation shared with any
//! other implementation. The rest pin down single rules, so a failure names the
//! rule that broke.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

use super::{Criterion, JunitError, Outcome, TestCase, criterion, read};

const ROOT: &str = "/project";

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../contracts/fixtures/junit")
}

fn results() -> Value {
    let raw = fs::read_to_string(fixtures_dir().join("results.json")).unwrap();
    serde_json::from_str(&raw).unwrap()
}

fn cases(xml: &str) -> Vec<TestCase> {
    read(xml.as_bytes(), Path::new(ROOT)).unwrap()
}

fn error(xml: &str) -> JunitError {
    read(xml.as_bytes(), Path::new(ROOT)).unwrap_err()
}

fn one(name: &str) -> Criterion {
    Criterion::One(name.to_owned())
}

// --- fixtures ---------------------------------------------------------------

fn expected_case(case: &Value) -> TestCase {
    TestCase {
        name: case["name"].as_str().unwrap().to_owned(),
        file: case["file"].as_str().unwrap().to_owned(),
        outcome: match case["outcome"].as_str().unwrap() {
            "passed" => Outcome::Passed,
            "failure" => Outcome::Failure,
            "error" => Outcome::Error,
            "skipped" => Outcome::Skipped,
            other => panic!("unknown outcome {other}"),
        },
        criterion: match &case["criterion"] {
            Value::Null => Criterion::None,
            Value::String(id) => Criterion::One(id.clone()),
            Value::Array(ids) => Criterion::Several(
                ids.iter()
                    .map(|id| id.as_str().unwrap().to_owned())
                    .collect(),
            ),
            other => panic!("unknown criterion {other}"),
        },
    }
}

fn assert_error_is(report: &str, err: &JunitError, expected: &Value) {
    let kind = expected["kind"].as_str().unwrap();
    let line = expected["line"].as_u64().map(|line| line as usize);
    let name = expected["name"].as_str();
    match err {
        JunitError::Empty => assert_eq!(kind, "empty", "{report}: {err}"),
        JunitError::NotXml { .. } => assert_eq!(kind, "not-xml", "{report}: {err}"),
        JunitError::WrongRoot { found } => {
            assert_eq!(kind, "wrong-root", "{report}: {err}");
            assert_eq!(Some(found.as_str()), name, "{report}");
        }
        JunitError::NoName { line: found } => {
            assert_eq!(kind, "no-name", "{report}: {err}");
            assert_eq!(Some(*found), line, "{report}");
        }
        JunitError::NoFile {
            name: found,
            line: at,
        } => {
            assert_eq!(kind, "no-file", "{report}: {err}");
            assert_eq!(Some(found.as_str()), name, "{report}");
            assert_eq!(Some(*at), line, "{report}");
        }
        JunitError::OutsideRoot { name: found, .. } => {
            assert_eq!(kind, "outside-root", "{report}: {err}");
            assert_eq!(Some(found.as_str()), name, "{report}");
        }
    }
}

#[test]
fn every_report_reads_as_results_json_says() {
    let results = results();
    let reports = results["reports"].as_array().unwrap();
    assert!(!reports.is_empty());

    let cases_of = |report: &str| {
        reports
            .iter()
            .find(|entry| entry["report"] == report)
            .and_then(|entry| entry["cases"].as_array())
            .unwrap_or_else(|| panic!("no cases for {report}"))
    };

    for entry in reports {
        let report = entry["report"].as_str().unwrap();
        // Bytes, never a line-oriented reader.
        let bytes = fs::read(fixtures_dir().join(report)).unwrap();
        let root = Path::new(entry["root"].as_str().unwrap());
        let got = read(&bytes, root);

        if let Some(expected) = entry.get("error") {
            match got {
                Err(err) => assert_error_is(report, &err, expected),
                Ok(found) => panic!("{report}: expected an error, read {found:?}"),
            }
            continue;
        }
        let expected = match entry.get("same_cases_as") {
            Some(other) => cases_of(other.as_str().unwrap()),
            None => cases_of(report),
        };
        let expected: Vec<TestCase> = expected.iter().map(expected_case).collect();
        let got = got.unwrap_or_else(|err| panic!("{report}: {err}"));
        assert_eq!(got, expected, "{report}");
    }
}

#[test]
fn every_report_on_disk_is_in_the_table() {
    let results = results();
    let listed: Vec<&str> = results["reports"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["report"].as_str().unwrap())
        .collect();
    for directory in ["real", "synthetic"] {
        for file in fs::read_dir(fixtures_dir().join(directory)).unwrap() {
            let name = file.unwrap().file_name();
            let report = format!("{directory}/{}", name.to_str().unwrap());
            assert!(listed.contains(&report.as_str()), "{report}");
        }
    }
}

#[test]
fn the_fixtures_hold_real_reports_from_both_runners_and_every_outcome() {
    let results = results();
    let reports = results["reports"].as_array().unwrap();
    for runner in ["real/pest-", "real/phpunit-"] {
        let real: Vec<&Value> = reports
            .iter()
            .filter(|entry| entry["report"].as_str().unwrap().starts_with(runner))
            .collect();
        assert!(!real.is_empty(), "{runner}");
        let outcomes: Vec<&str> = real
            .iter()
            .filter_map(|entry| entry["cases"].as_array())
            .flatten()
            .map(|case| case["outcome"].as_str().unwrap())
            .collect();
        for outcome in ["passed", "failure", "error", "skipped"] {
            assert!(outcomes.contains(&outcome), "{runner}: {outcome}");
        }
    }
}

// --- outcomes -----------------------------------------------------------------

#[test]
fn a_failure_is_not_an_error() {
    let read = cases(
        "<testsuite>\
         <testcase name=\"a\" file=\"t.php\"><failure>x</failure></testcase>\
         <testcase name=\"b\" file=\"t.php\"><error>x</error></testcase>\
         </testsuite>",
    );
    assert_eq!(read[0].outcome, Outcome::Failure);
    assert_eq!(read[1].outcome, Outcome::Error);
}

#[test]
fn an_outcome_applies_only_to_its_own_test_case() {
    let read = cases(
        "<testsuite>\
         <testcase name=\"a\" file=\"t.php\"><error/></testcase>\
         <testcase name=\"b\" file=\"t.php\"/>\
         <testcase name=\"c\" file=\"t.php\"></testcase>\
         </testsuite>",
    );
    let outcomes: Vec<Outcome> = read.iter().map(|case| case.outcome).collect();
    assert_eq!(outcomes, [Outcome::Error, Outcome::Passed, Outcome::Passed]);
}

// --- what is read ---------------------------------------------------------------

#[test]
fn an_empty_report_is_empty_and_a_report_with_no_tests_is_not() {
    for empty in ["", " \n\t", "\u{FEFF}", "\u{FEFF}\n"] {
        assert_eq!(error(empty), JunitError::Empty, "{empty:?}");
    }
    assert!(cases("<testsuites/>").is_empty());
    assert!(cases("<testsuite></testsuite>").is_empty());
}

#[test]
fn what_is_not_well_formed_is_refused_with_its_line() {
    for (xml, line) in [
        ("hello\n", "line 1"),
        (
            "<testsuite>\n<testcase name=\"a\" file=\"b\"></testsuite>\n",
            "line 2",
        ),
        (
            "<testsuite>\n<testcase name=\"a\" file=\"b\" name=\"c\"/>\n</testsuite>",
            "line 2",
        ),
        ("<testsuite/>\n\n<testsuite/>", "line 3"),
        ("<testsuite/>\nstray", "line 1"),
        ("<testsuite>\n", "line 2"),
        ("</testsuite>", "line 1"),
    ] {
        match error(xml) {
            JunitError::NotXml { reason } => assert!(reason.contains(line), "{xml:?}: {reason}"),
            other => panic!("{xml:?}: {other:?}"),
        }
    }
    assert!(matches!(
        read(b"<testsuite>\xE9</testsuite>", Path::new(ROOT)),
        Err(JunitError::NotXml { .. })
    ));
}

#[test]
fn no_entity_is_resolved_from_a_dtd() {
    let doctype = "<!DOCTYPE t [<!ENTITY xxe SYSTEM \"file:///etc/passwd\">]>\n";
    // In an attribute, it is refused.
    assert!(matches!(
        error(&format!(
            "{doctype}<testsuite><testcase name=\"&xxe;\" file=\"a\"/></testsuite>"
        )),
        JunitError::NotXml { .. }
    ));
    // In an element's text, it is in what is not read, and is not read.
    let read = cases(&format!(
        "{doctype}<testsuite><testcase name=\"AC1\" file=\"a\"><failure>&xxe;</failure></testcase>\
         </testsuite>"
    ));
    assert_eq!(read[0].outcome, Outcome::Failure);
    assert_eq!(read[0].name, "AC1");
}

#[test]
fn a_wrong_root_names_what_it_found() {
    assert_eq!(
        error("<html/>"),
        JunitError::WrongRoot {
            found: "html".to_owned()
        }
    );
}

// --- paths ----------------------------------------------------------------------

fn file_of(file: &str, root: &str) -> Result<String, JunitError> {
    let xml = format!("<testsuite><testcase name=\"t\" file=\"{file}\"/></testsuite>");
    read(xml.as_bytes(), Path::new(root)).map(|cases| cases[0].file.clone())
}

#[test]
fn a_path_is_relative_to_the_root_however_the_root_is_written() {
    for root in ["/project", "/project/", "/project/./", "/other/../project"] {
        assert_eq!(
            file_of("/project/tests/A.php", root).unwrap(),
            "tests/A.php",
            "{root}"
        );
    }
    assert_eq!(file_of("/a/b/c/t.php", "/a/b").unwrap(), "c/t.php");
    assert_eq!(
        file_of("C:\\proj\\tests\\A.php", "C:\\proj").unwrap(),
        "tests/A.php"
    );
    // A root of `/` holds every absolute path.
    assert_eq!(file_of("/tests/A.php", "/").unwrap(), "tests/A.php");
}

#[test]
fn a_path_outside_the_root_is_an_error_naming_the_test_case_and_the_value() {
    for file in [
        "/other/A.php",
        "/projectile/A.php",
        "/project/../other/A.php",
        "../A.php",
        "tests/../../A.php",
        "/project",
        "/project/",
        ".",
        "C:/project/A.php",
    ] {
        assert_eq!(
            file_of(file, ROOT),
            Err(JunitError::OutsideRoot {
                name: "t".to_owned(),
                file: file.to_owned(),
            }),
            "{file}"
        );
    }
    // A drive is compared exactly.
    assert!(file_of("D:/proj/A.php", "C:/proj").is_err());
}

#[test]
fn only_a_suffix_of_exactly_the_name_is_removed() {
    let of = |name: &str, file: &str| {
        let xml = format!("<testsuite><testcase name=\"{name}\" file=\"{file}\"/></testsuite>");
        cases(&xml).remove(0).file
    };
    assert_eq!(of("n", "a.php::n"), "a.php");
    assert_eq!(of("a::b", "a.php::a::b"), "a.php");
    assert_eq!(of("n", "a.php::other"), "a.php::other");
    assert_eq!(of("n", "a.phpn"), "a.phpn");
    assert_eq!(of("n", "a.php::n::n"), "a.php::n");
    assert_eq!(of("&lt;x&gt;", "a.php::&lt;x&gt;"), "a.php");
}

// --- the criterion ----------------------------------------------------------------

#[test]
fn the_token_rule_holds() {
    for (name, id) in [
        ("AC1", "AC1"),
        ("ac1", "AC1"),
        ("aC_1", "AC1"),
        ("Ac-1", "AC1"),
        ("AC123", "AC123"),
        ("test_ac3_rejects", "AC3"),
        ("it('AC3: rejects')", "AC3"),
        ("rejects (ac-3)", "AC3"),
        ("AC3\u{2192}x", "AC3"),
        ("\u{2192}AC3", "AC3"),
    ] {
        assert_eq!(criterion(name), one(id), "{name}");
    }
    for name in [
        "", "A", "C", "AC", "ACME", "vac3", "vAC3", "AC3x", "xAC3", "9AC3", "AC39x", "AC03", "AC0",
        "AC_0", "AC_", "AC-", "AC__3", "AC_-3", "AC--3", "AC 3", "éAC3", "AC3é", "AC٣", "ac1ac2",
        "ACME3",
    ] {
        assert_eq!(criterion(name), Criterion::None, "{name}");
    }
}

#[test]
fn the_same_token_twice_is_one_and_different_ones_are_all_reported() {
    assert_eq!(criterion("AC3 ac_3 Ac-3"), one("AC3"));
    assert_eq!(
        criterion("AC3 AC1 ac-3 AC2"),
        Criterion::Several(vec!["AC3".into(), "AC1".into(), "AC2".into()])
    );
    // AC1 is not a prefix of AC10.
    assert_eq!(
        criterion("AC1 AC10"),
        Criterion::Several(vec!["AC1".into(), "AC10".into()])
    );
}

#[test]
fn the_name_of_a_data_set_counts_as_part_of_the_name() {
    // Reported rather than guessed: what the runner adds is in the name (§4).
    assert_eq!(
        criterion("test_ac1 with data set \"ac2\""),
        Criterion::Several(vec!["AC1".into(), "AC2".into()])
    );
}

#[test]
fn a_number_too_long_for_any_integer_is_still_a_token() {
    let long = format!("AC{}", "9".repeat(40));
    assert_eq!(criterion(&long), one(&long));
}

// --- messages -----------------------------------------------------------------

#[test]
fn every_error_says_what_is_wrong_and_where() {
    for (error, expected) in [
        (JunitError::Empty, &["empty", "cannot load"][..]),
        (
            JunitError::NotXml {
                reason: "line 2: x".to_owned(),
            },
            &["not well-formed", "line 2"],
        ),
        (
            JunitError::WrongRoot {
                found: "html".to_owned(),
            },
            &["`<html>`", "`<testsuites>`"],
        ),
        (JunitError::NoName { line: 4 }, &["line 4", "`name`"]),
        (
            JunitError::NoFile {
                name: "AC1 x".to_owned(),
                line: 5,
            },
            &["line 5", "`AC1 x`", "`file`", "lock line"],
        ),
        (
            JunitError::OutsideRoot {
                name: "AC1 x".to_owned(),
                file: "/other/a.php".to_owned(),
            },
            &["`AC1 x`", "`/other/a.php`", "project root"],
        ),
    ] {
        let message = error.to_string();
        for part in expected {
            assert!(message.contains(part), "{part}: {message}");
        }
    }
}
