//! Tests for canonicalization and digests (§4d), M1-1 and M1-2.
//!
//! Two halves. The fixture tests run every file in
//! `contracts/fixtures/digest/` through `canonicalize_bytes` and `digest` and
//! check them against `digests.json` — the shared contract that any other
//! implementation is verified against too. The unit tests below pin down each
//! rule on its own, so a failure names the rule that broke rather than a
//! digest.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

use super::{InvalidUtf8, canonicalize, canonicalize_bytes, digest};

/// SHA-256 of zero bytes: the digest of the empty canonical form (D-1).
const EMPTY_DIGEST: &str =
    "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

// --- fixtures ---------------------------------------------------------------

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../contracts/fixtures/digest")
}

struct Group {
    name: String,
    expected_digest: String,
    canonical_byte_length: usize,
    files: Vec<(String, usize)>,
}

fn load_groups() -> Vec<Group> {
    let manifest = fixtures_dir().join("digests.json");
    let raw = fs::read_to_string(&manifest)
        .unwrap_or_else(|err| panic!("reading {}: {err}", manifest.display()));
    let json: Value = serde_json::from_str(&raw).expect("digests.json is valid JSON");

    let as_usize = |value: &Value| value.as_u64().expect("a byte count") as usize;
    let as_string = |value: &Value| value.as_str().expect("a string").to_owned();

    json["groups"]
        .as_array()
        .expect("digests.json has a `groups` array")
        .iter()
        .map(|group| Group {
            name: as_string(&group["name"]),
            expected_digest: as_string(&group["expected_digest"]),
            canonical_byte_length: as_usize(&group["canonical_byte_length"]),
            files: group["files"]
                .as_array()
                .expect("a group has a `files` array")
                .iter()
                .map(|file| (as_string(&file["path"]), as_usize(&file["bytes"])))
                .collect(),
        })
        .collect()
}

/// Canonicalize every file of every group; return `(group name, canonical
/// text)` per group after checking each file against the contract.
fn canonicalize_all_fixtures() -> Vec<(String, String)> {
    let groups = load_groups();
    assert!(!groups.is_empty(), "digests.json lists no groups");

    groups
        .into_iter()
        .map(|group| {
            let mut canonical_of_group: Option<String> = None;

            for (path, recorded_bytes) in &group.files {
                // Bytes, never a line-oriented reader: one that normalizes
                // terminators for us would make crlf.txt and lf.txt identical
                // and the test would pass without testing anything.
                let bytes = fs::read(fixtures_dir().join(path))
                    .unwrap_or_else(|err| panic!("reading fixture {path}: {err}"));

                assert_eq!(
                    bytes.len(),
                    *recorded_bytes,
                    "{path}: on disk {} bytes, digests.json says {recorded_bytes} — \
                     something rewrote this fixture (check .gitattributes and your editor)",
                    bytes.len(),
                );

                let canonical =
                    canonicalize_bytes(&bytes).unwrap_or_else(|err| panic!("{path}: {err}"));

                assert_eq!(
                    canonical.len(),
                    group.canonical_byte_length,
                    "{path}: canonical form is {} bytes, group `{}` expects {}: {canonical:?}",
                    canonical.len(),
                    group.name,
                    group.canonical_byte_length,
                );
                assert_eq!(
                    digest(&canonical),
                    group.expected_digest,
                    "{path}: canonical form does not match group `{}`: {canonical:?}",
                    group.name,
                );

                match &canonical_of_group {
                    None => canonical_of_group = Some(canonical),
                    Some(first) => assert_eq!(
                        &canonical, first,
                        "{path}: canonicalizes differently from the rest of group `{}`",
                        group.name,
                    ),
                }
            }

            let canonical = canonical_of_group
                .unwrap_or_else(|| panic!("group `{}` lists no files", group.name));
            (group.name, canonical)
        })
        .collect()
}

#[test]
fn every_fixture_canonicalizes_to_its_groups_content() {
    canonicalize_all_fixtures();
}

#[test]
fn every_fixture_digests_to_its_groups_expected_digest() {
    // The contract test of M1-2, in one place: walk every group and every
    // file, hash the bytes, compare against `expected_digest`, then check
    // that no two groups share one.
    let groups = load_groups();

    // Pinned counts, so a digests.json that silently lost a group or a file
    // fails here instead of quietly testing less. Adding a fixture is a
    // contract change (fixtures README), and updating these numbers is part
    // of it.
    let file_count: usize = groups.iter().map(|group| group.files.len()).sum();
    assert_eq!(groups.len(), 7, "digests.json should list 7 groups");
    assert_eq!(file_count, 28, "digests.json should list 28 files");

    let mut walked = 0;
    for group in &groups {
        for (path, _) in &group.files {
            // Bytes, never a line-oriented reader (see above).
            let bytes = fs::read(fixtures_dir().join(path))
                .unwrap_or_else(|err| panic!("reading fixture {path}: {err}"));
            let canonical =
                canonicalize_bytes(&bytes).unwrap_or_else(|err| panic!("{path}: {err}"));

            assert_eq!(
                digest(&canonical),
                group.expected_digest,
                "{path} does not hash to group `{}`'s expected digest",
                group.name,
            );
            walked += 1;
        }
    }
    assert_eq!(walked, 28, "every listed file should have been hashed");

    // Distinctness: equivalence alone is satisfied by an implementation that
    // collapses different content into one digest.
    let mut seen = HashSet::new();
    for group in &groups {
        assert!(
            seen.insert(group.expected_digest.as_str()),
            "group `{}` shares its digest with another group",
            group.name,
        );
    }
}

#[test]
fn empty_group_yields_the_sha256_of_zero_bytes() {
    let group = load_groups()
        .into_iter()
        .find(|group| group.name == "empty")
        .expect("digests.json has an `empty` group");
    assert_eq!(group.expected_digest, EMPTY_DIGEST);

    for (path, _) in &group.files {
        let bytes = fs::read(fixtures_dir().join(path))
            .unwrap_or_else(|err| panic!("reading fixture {path}: {err}"));
        let canonical = canonicalize_bytes(&bytes).unwrap_or_else(|err| panic!("{path}: {err}"));
        assert_eq!(digest(&canonical), EMPTY_DIGEST, "{path}");
    }
}

#[test]
fn fixture_groups_stay_distinct() {
    // The half of the contract that is easy to forget: an implementation that
    // strips *all* blank lines, or leading whitespace too, would collapse
    // `simple`, `interior-blank-lines` and `leading-blank-lines` into one.
    let groups = canonicalize_all_fixtures();
    for (i, (name_a, text_a)) in groups.iter().enumerate() {
        for (name_b, text_b) in &groups[i + 1..] {
            assert_ne!(
                text_a, text_b,
                "groups `{name_a}` and `{name_b}` collapsed into one"
            );
        }
    }
}

#[test]
fn canonicalizing_a_fixture_twice_changes_nothing() {
    for (name, canonical) in canonicalize_all_fixtures() {
        assert_eq!(
            canonicalize(&canonical),
            canonical,
            "group `{name}` is not idempotent"
        );
    }
}

// --- the rules, one at a time ----------------------------------------------

#[test]
fn already_canonical_text_is_unchanged() {
    for text in [
        "",
        "alpha\n",
        "alpha\nbeta\n",
        "\nalpha\n\n  beta\n",
        "Београд ☕\n",
    ] {
        assert_eq!(canonicalize(text), text);
    }
}

#[test]
fn idempotent_on_messy_input() {
    let messy = "\u{FEFF}  \r\nalpha \t\r\rbeta\r\n\n\t\n";
    let once = canonicalize(messy);
    assert_eq!(canonicalize(&once), once);
}

#[test]
fn empty_result_is_the_empty_string_not_a_newline() {
    // D-1.
    for input in [
        "",
        "\n",
        "\r\n\r\n",
        "   \n\t\n  ",
        "\u{FEFF}",
        "\u{FEFF}\r\n  \r\n",
    ] {
        assert_eq!(canonicalize(input), "", "input {input:?}");
    }
}

#[test]
fn every_line_ending_becomes_lf() {
    assert_eq!(canonicalize("alpha\r\nbeta\r\n"), "alpha\nbeta\n");
    assert_eq!(canonicalize("alpha\rbeta\r"), "alpha\nbeta\n");
    assert_eq!(
        canonicalize("alpha\r\nbeta\ngamma\r"),
        "alpha\nbeta\ngamma\n"
    );
    // A lone CR followed by a CRLF is two line breaks, not one.
    assert_eq!(canonicalize("alpha\r\r\nbeta"), "alpha\n\nbeta\n");
}

#[test]
fn ends_with_exactly_one_newline() {
    assert_eq!(canonicalize("alpha"), "alpha\n");
    assert_eq!(canonicalize("alpha\n"), "alpha\n");
    assert_eq!(canonicalize("alpha\n\n\n"), "alpha\n");
    assert_eq!(canonicalize("alpha\n  \n\t\n"), "alpha\n");
}

#[test]
fn interior_and_leading_blank_lines_are_preserved() {
    assert_eq!(canonicalize("alpha\n\nbeta\n"), "alpha\n\nbeta\n");
    assert_eq!(canonicalize("\n\nalpha\n"), "\n\nalpha\n");
    // A blank line made of spaces stays a blank line — an empty one.
    assert_eq!(canonicalize("alpha\n   \nbeta\n\n"), "alpha\n\nbeta\n");
    assert_eq!(canonicalize("  \nalpha\n"), "\nalpha\n");
}

#[test]
fn leading_whitespace_is_kept_trailing_is_stripped() {
    assert_eq!(
        canonicalize("  Scenario: x  \n\tGiven y\t\n    Then z \n"),
        "  Scenario: x\n\tGiven y\n    Then z\n",
    );
}

#[test]
fn bom_is_stripped_only_at_the_start() {
    assert_eq!(canonicalize("\u{FEFF}alpha\n"), "alpha\n");
    // Anywhere else U+FEFF is content, and it is not whitespace either.
    assert_eq!(canonicalize("al\u{FEFF}pha\n"), "al\u{FEFF}pha\n");
    // Only one BOM is an encoding mark; a second one is content.
    assert_eq!(canonicalize("\u{FEFF}\u{FEFF}alpha\n"), "\u{FEFF}alpha\n");
}

#[test]
fn multi_byte_characters_survive_whitespace_stripping() {
    assert_eq!(
        canonicalize("Ćirilica: Београд \t\r\nkafa: ☕  \r\n"),
        "Ćirilica: Београд\nkafa: ☕\n",
    );
}

// --- step 1: decoding -------------------------------------------------------

#[test]
fn canonicalize_bytes_decodes_and_canonicalizes() {
    let bytes = b"\xEF\xBB\xBFalpha  \r\nbeta\r\n\r\n";
    assert_eq!(canonicalize_bytes(bytes), Ok("alpha\nbeta\n".to_owned()));
}

#[test]
fn invalid_utf8_is_an_error_with_its_offset() {
    // A lone continuation byte after five valid ones.
    assert_eq!(
        canonicalize_bytes(b"alpha\x80beta\n"),
        Err(InvalidUtf8 { offset: 5 })
    );
    // 0xFF never appears in UTF-8.
    assert_eq!(canonicalize_bytes(b"\xFF"), Err(InvalidUtf8 { offset: 0 }));
    // A multi-byte sequence cut short at the end of the input.
    assert_eq!(
        canonicalize_bytes(b"kafa \xE2\x98"),
        Err(InvalidUtf8 { offset: 5 })
    );
}

#[test]
fn invalid_utf8_is_never_replaced_lossily() {
    // Lossy decoding would turn both into "alpha\u{FFFD}\n" — two different
    // files, one digest. Both must be refused instead.
    assert!(canonicalize_bytes(b"alpha\x80\n").is_err());
    assert!(canonicalize_bytes(b"alpha\xFF\n").is_err());
}

#[test]
fn invalid_utf8_message_is_clear() {
    let message = canonicalize_bytes(b"alpha\x80").unwrap_err().to_string();
    assert!(message.contains("not valid UTF-8"), "{message}");
    assert!(message.contains("offset 5"), "{message}");
}

// --- digest -----------------------------------------------------------------

#[test]
fn digest_of_empty_text_is_the_sha256_of_zero_bytes() {
    assert_eq!(digest(""), EMPTY_DIGEST);
}

#[test]
fn digest_matches_a_known_sha256_vector() {
    // FIPS 180-2, the standard "abc" test vector.
    assert_eq!(
        digest("abc"),
        "sha256:ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}

#[test]
fn digest_is_prefixed_lowercase_hex_of_fixed_length() {
    for text in ["", "alpha\n", "Београд ☕\n"] {
        let rendered = digest(text);
        let hex = rendered
            .strip_prefix("sha256:")
            .unwrap_or_else(|| panic!("no `sha256:` prefix: {rendered}"));
        assert_eq!(hex.len(), 64, "{rendered}");
        assert!(
            hex.chars().all(|c| matches!(c, '0'..='9' | 'a'..='f')),
            "not lowercase hex: {rendered}"
        );
    }
}

#[test]
fn digest_does_not_canonicalize_on_its_own() {
    // What is hashed is exactly what was passed in; canonicalizing is the
    // caller's job. Otherwise `invar hash` could not show what it hashed.
    assert_ne!(digest("alpha\r\n"), digest("alpha\n"));
}
