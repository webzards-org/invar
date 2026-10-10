//! Tests for merging into `.gitattributes` and `.gitignore` (M1-15).
//!
//! The property every one of them rests on: what is already in the file is
//! still there, byte for byte, in front of what was added.

use super::{
    GITATTRIBUTES_ENTRY, GITIGNORE_ENTRY, hides_invar_dir, ignore_out_dir, pin_plan_line_endings,
};

/// What a Laravel project's `.gitignore` looks like, more or less.
const LARAVEL_GITIGNORE: &str = "/vendor\n/node_modules\n/public/build\n/public/hot\n\
                                 /storage/*.key\n.env\n.env.backup\n.phpunit.result.cache\n\
                                 npm-debug.log\n";

const LARAVEL_GITATTRIBUTES: &str = "* text=auto eol=lf\n\n*.blade.php diff=html\n\
                                     *.css diff=css\n*.md diff=markdown\n*.php diff=php\n\n\
                                     /.github export-ignore\n";

// --- a file that is not there ---------------------------------------------------------

#[test]
fn no_gitattributes_gets_a_new_one_with_only_ours() {
    assert_eq!(
        pin_plan_line_endings(None).as_deref(),
        Some(
            "# invar: Plan bodies are checksummed; keep their line endings the same on every \
             platform\n.invar/plans/**/*.md text eol=lf\n"
        )
    );
}

#[test]
fn no_gitignore_gets_a_new_one_with_only_ours() {
    assert_eq!(
        ignore_out_dir(None).as_deref(),
        Some("# invar: working files (the rendered PR description)\n.invar/out/\n")
    );
}

#[test]
fn an_empty_file_is_treated_like_no_file() {
    assert_eq!(pin_plan_line_endings(Some("")), pin_plan_line_endings(None));
    assert_eq!(ignore_out_dir(Some("")), ignore_out_dir(None));
}

// --- a file the team already has ---------------------------------------------------------

#[test]
fn the_teams_lines_stay_exactly_and_ours_follow_a_blank_line() {
    let new = ignore_out_dir(Some(LARAVEL_GITIGNORE)).unwrap();

    assert!(
        new.starts_with(LARAVEL_GITIGNORE),
        "the team's lines moved:\n{new}"
    );
    assert_eq!(
        &new[LARAVEL_GITIGNORE.len()..],
        "\n# invar: working files (the rendered PR description)\n.invar/out/\n"
    );

    let new = pin_plan_line_endings(Some(LARAVEL_GITATTRIBUTES)).unwrap();
    assert!(new.starts_with(LARAVEL_GITATTRIBUTES), "{new}");
    assert!(new.ends_with(".invar/plans/**/*.md text eol=lf\n"));
}

#[test]
fn a_file_without_a_final_newline_gets_one_before_ours() {
    let old = "/vendor\n.env";

    let new = ignore_out_dir(Some(old)).unwrap();

    assert!(new.starts_with("/vendor\n.env\n"), "{new:?}");
    assert_eq!(
        new,
        "/vendor\n.env\n\n# invar: working files (the rendered PR description)\n.invar/out/\n"
    );
}

#[test]
fn a_blank_line_that_is_already_there_is_not_doubled() {
    let old = "/vendor\n\n";

    let new = ignore_out_dir(Some(old)).unwrap();

    assert_eq!(
        new,
        "/vendor\n\n# invar: working files (the rendered PR description)\n.invar/out/\n"
    );
}

#[test]
fn a_crlf_file_gets_crlf() {
    let old = "/vendor\r\n.env\r\n";

    let new = ignore_out_dir(Some(old)).unwrap();

    assert!(new.starts_with(old));
    assert_eq!(
        new.matches('\n').count(),
        new.matches("\r\n").count(),
        "{new:?}"
    );
}

#[test]
fn a_file_of_only_blank_lines_and_comments_is_still_added_to() {
    let old = "# nothing here yet\n\n";

    let new = pin_plan_line_endings(Some(old)).unwrap();

    assert!(new.starts_with(old));
    assert!(new.contains(GITATTRIBUTES_ENTRY));
}

// --- already there ---------------------------------------------------------------------------

#[test]
fn an_entry_that_is_already_there_is_not_added_again() {
    assert_eq!(
        pin_plan_line_endings(Some(&format!("* text=auto\n{GITATTRIBUTES_ENTRY}\n"))),
        None
    );
    assert_eq!(
        ignore_out_dir(Some(&format!("/vendor\n{GITIGNORE_ENTRY}\n.env\n"))),
        None
    );
}

#[test]
fn the_same_rule_spelled_another_way_counts() {
    for old in [
        ".invar/plans/**/*.md eol=lf text\n",
        "/.invar/plans/**/*.md text eol=lf\n",
        ".invar/plans/**/*.md   text   eol=lf  \n",
        ".invar/plans/**/*.md text eol=lf diff=markdown\n",
        "\t.invar/plans/**/*.md text eol=lf\r\n",
    ] {
        assert_eq!(pin_plan_line_endings(Some(old)), None, "{old:?}");
    }
    for old in [
        ".invar/out\n",
        "/.invar/out/\n",
        "/.invar/out\n",
        ".invar/out/*\n",
        ".invar/out/**\n",
        "  .invar/out/  \n",
        ".invar/out/\r\n",
    ] {
        assert_eq!(ignore_out_dir(Some(old)), None, "{old:?}");
    }
}

#[test]
fn a_commented_out_entry_does_not_count() {
    let old = "# .invar/out/\n";

    assert!(ignore_out_dir(Some(old)).is_some());
    assert!(pin_plan_line_endings(Some("# .invar/plans/**/*.md text eol=lf\n")).is_some());
}

#[test]
fn a_rule_for_something_else_does_not_count() {
    // Different files, or the attributes that matter missing.
    for old in [
        ".invar/plans/**/*.md text\n",
        ".invar/plans/**/*.md eol=lf\n",
        ".invar/plans/**/*.md text eol=crlf\n",
        "*.md text eol=lf\n",
        ".invar/decisions/**/*.md text eol=lf\n",
    ] {
        assert!(pin_plan_line_endings(Some(old)).is_some(), "{old:?}");
    }
    for old in [
        ".invar/outside/\n",
        ".invar/out-of-date/\n",
        "invar/out/\n",
        "out/\n",
    ] {
        assert!(ignore_out_dir(Some(old)).is_some(), "{old:?}");
    }
}

#[test]
fn adding_twice_adds_once() {
    for old in [
        None,
        Some(LARAVEL_GITIGNORE),
        Some("/vendor"),
        Some("a\r\nb\r\n"),
    ] {
        let once = ignore_out_dir(old).unwrap();
        assert_eq!(ignore_out_dir(Some(&once)), None, "{once:?}");

        let once = pin_plan_line_endings(old).unwrap();
        assert_eq!(pin_plan_line_endings(Some(&once)), None, "{once:?}");
    }
}

// --- a .gitignore that hides .invar -------------------------------------------------------------

#[test]
fn ignoring_the_whole_invar_directory_is_noticed() {
    for text in [
        ".invar\n",
        ".invar/\n",
        "/.invar/\n",
        "/vendor\n.invar/*\n",
        "  /.invar/**  \n",
    ] {
        assert!(hides_invar_dir(text), "{text:?}");
    }
}

#[test]
fn ignoring_only_what_invar_writes_is_not_hiding_anything() {
    for text in [
        LARAVEL_GITIGNORE,
        ".invar/out/\n",
        ".invar/out\n",
        "# .invar/\n",
        ".invariants/\n",
        "",
    ] {
        assert!(!hides_invar_dir(text), "{text:?}");
    }
}
