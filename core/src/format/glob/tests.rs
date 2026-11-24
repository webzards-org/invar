//! Tests for the globs of `cycle.required_paths` (`invar-yaml.md` §3.8), a rule each.

use super::{first_match, matches, problem};

fn yes(pattern: &str, paths: &[&str]) {
    for path in paths {
        assert!(matches(pattern, path), "`{pattern}` should match `{path}`");
    }
}

fn no(pattern: &str, paths: &[&str]) {
    for path in paths {
        assert!(
            !matches(pattern, path),
            "`{pattern}` should not match `{path}`"
        );
    }
}

#[test]
fn a_plain_pattern_is_the_path_or_a_directory_it_is_in() {
    yes("routes/web.php", &["routes/web.php"]);
    no(
        "routes/web.php",
        &["routes/web.phpx", "routes/api.php", "x/routes/web.php"],
    );
    yes(
        "app/Models",
        &["app/Models", "app/Models/Order.php", "app/Models/Sub/A.php"],
    );
    no(
        "app/Models",
        &["app/ModelsX/A.php", "app/Model/A.php", "app"],
    );
}

#[test]
fn a_trailing_slash_matches_only_a_directory() {
    yes(
        "app/Models/",
        &["app/Models/Order.php", "app/Models/Sub/A.php"],
    );
    no("app/Models/", &["app/Models", "app/ModelsX/A.php"]);
}

#[test]
fn a_star_stays_within_a_segment() {
    yes("app/*.php", &["app/User.php", "app/.php"]);
    no("app/*.php", &["app/Models/User.php", "app/User.phpx"]);
    yes(
        "database/migrations/*_orders_*",
        &["database/migrations/2026_01_01_create_orders_table.php"],
    );
    yes("a*b*c", &["abc", "aXbYc", "abbc"]);
    no("a*b*c", &["ab", "acb", "abcd"]);
    // Anchored at the root, unlike `.gitignore`.
    no("*.php", &["app/User.php"]);
    yes("*.php", &["index.php"]);
    // A directory whose name matches covers what is in it.
    yes("app/*", &["app/Models/Order.php"]);
}

#[test]
fn a_double_star_spans_any_number_of_segments() {
    yes(
        "app/Models/**",
        &["app/Models/Order.php", "app/Models/a/b/c.php"],
    );
    yes(
        "**/Policies/*.php",
        &[
            "app/Policies/OrderPolicy.php",
            "Policies/A.php",
            "x/y/Policies/A.php",
        ],
    );
    no(
        "**/Policies/*.php",
        &["app/Policies/Sub/A.php", "app/PoliciesX/A.php"],
    );
    yes(
        "app/**/Order.php",
        &["app/Order.php", "app/Models/Order.php", "app/a/b/Order.php"],
    );
    yes("**", &["anything/at/all.php", "top.php"]);
    // Not a whole segment: the same as one `*`.
    yes("app/x**y.php", &["app/xy.php", "app/xAy.php"]);
    no("app/x**y.php", &["app/x/y.php"]);
}

#[test]
fn every_other_character_is_itself() {
    yes("app/a?b[1].php", &["app/a?b[1].php"]);
    no("app/a?b[1].php", &["app/aXb1.php"]);
    // Case counts.
    no("app/models", &["app/Models/A.php"]);
}

#[test]
fn a_refused_pattern_says_why_and_matches_nothing() {
    for (pattern, says) in [
        ("", "empty"),
        ("   ", "empty"),
        ("/app/Models", "absolute"),
        ("C:/app", "absolute"),
        ("app\\Models", "`\\`"),
        ("app//Models", "empty segment"),
        ("../app", "`..`"),
        ("app/./Models", "`.`"),
        ("app/../x", "`..`"),
    ] {
        let found = problem(pattern).unwrap_or_else(|| panic!("`{pattern}` should be refused"));
        assert!(found.contains(says), "`{pattern}`: {found}");
        assert!(!matches(pattern, "app/Models/A.php"), "{pattern}");
    }
    for pattern in ["app/Models/**", "app/Models/", "*.php", "**"] {
        assert_eq!(problem(pattern), None, "{pattern}");
    }
}

#[test]
fn the_first_matching_pattern_is_the_one_named() {
    let patterns = vec!["app/Policies/**".to_owned(), "app/**".to_owned()];
    assert_eq!(
        first_match(&patterns, "app/Policies/A.php"),
        Some("app/Policies/**")
    );
    assert_eq!(first_match(&patterns, "app/Models/A.php"), Some("app/**"));
    assert_eq!(first_match(&patterns, "routes/web.php"), None);
}
