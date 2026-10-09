//! Tests for `context show` (M1-14): which files are loaded and which are not,
//! the limit on the binding output, and the order and budget of the decision
//! log.
//!
//! The command as a whole — what reaches stdout and stderr, and the exit code —
//! is tested in `tests/context_show.rs`.

use std::fs;

use tempfile::TempDir;

use super::{
    Binding, ContextError, Decisions, MARKER, MAX_CHARS, MAX_ENTRIES, State, classify,
    load_binding, load_decisions, planner_output,
};
use crate::format::decision::{Entry, FileName, Type};
use crate::fs::ProjectRoot;

// --- fixtures ------------------------------------------------------------------

fn project() -> (TempDir, ProjectRoot) {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("invar.yaml"), "").unwrap();
    let root = ProjectRoot::for_test(dir.path());
    (dir, root)
}

fn write(root: &ProjectRoot, name: &str, text: &str) {
    fs::write(root.path().join(name), text).unwrap();
}

/// `body` under the marker.
fn marked(body: &str) -> String {
    format!("{MARKER}\n{body}")
}

fn binding(root: &ProjectRoot) -> Binding {
    load_binding(root).unwrap()
}

/// A decision entry for `module`, written at 16:37:`second` UTC the way the
/// core writes one. Every entry in a test needs its own second. Returns the id.
fn add_entry(root: &ProjectRoot, module: &str, second: u32, body: &str) -> String {
    let id = format!("d-{:08x}", 0xabc0_0000 + second);
    let entry = Entry {
        id: id.clone(),
        date: format!("2026-10-06T18:37:{second:02}+02:00"),
        ticket: "SHOP-1".to_owned(),
        module: module.to_owned(),
        kind: Type::Note,
        scenario: None,
        body: body.to_owned(),
    };
    let name = FileName {
        stamp: format!("2026-10-06-1637{second:02}"),
        module: module.to_owned(),
        slug: "shop-1".to_owned(),
    };
    fs::create_dir_all(root.decisions_dir()).unwrap();
    fs::write(root.decisions_dir().join(name.render(1)), entry.render()).unwrap();
    id
}

fn decisions(root: &ProjectRoot, module: Option<&str>) -> Decisions {
    load_decisions(root, module).unwrap()
}

/// Where each needle first appears in `haystack`, which must be in order.
fn in_order(haystack: &str, needles: &[&str]) -> bool {
    let positions: Vec<usize> = needles
        .iter()
        .map(|needle| {
            haystack
                .find(needle)
                .unwrap_or_else(|| panic!("no {needle:?} in:\n{haystack}"))
        })
        .collect();
    positions.windows(2).all(|pair| pair[0] < pair[1])
}

// --- the marker --------------------------------------------------------------------

#[test]
fn a_marked_file_is_loaded_without_its_marker() {
    let (_dir, root) = project();
    write(
        &root,
        "AI_CONTEXT.md",
        &marked("# AI_CONTEXT.md\n\n## Layers\n\n- No SQLAlchemy in routers.\n"),
    );

    match &binding(&root).files[0].state {
        State::Loaded(text) => {
            assert!(text.starts_with("# AI_CONTEXT.md"), "{text}");
            assert!(text.ends_with("routers."), "{text:?}");
            assert!(!text.contains("invar-context"), "{text}");
        }
        other => panic!("expected Loaded, got {other:?}"),
    }
}

#[test]
fn the_marker_may_follow_a_bom_blank_lines_and_spaces_and_use_crlf() {
    for text in [
        "\u{FEFF}<!-- invar-context: v1 -->\nBody.\n",
        "<!-- invar-context: v1 -->\r\nBody.\r\n",
        "<!-- invar-context: v1 -->\rBody.\r",
        "\n\n  <!-- invar-context: v1 -->  \nBody.\n",
    ] {
        assert_eq!(
            classify(text),
            State::Loaded("Body.".to_owned()),
            "{text:?}"
        );
    }
}

#[test]
fn a_file_whose_first_non_blank_line_is_not_the_marker_is_unrecognized() {
    for text in [
        "Body.\n",
        // Further down is not the top: a document that only quotes the marker
        // must not be loaded as binding context.
        "# Title\n<!-- invar-context: v1 -->\nBody.\n",
        "```\n<!-- invar-context: v1 -->\n```\n",
        "> <!-- invar-context: v1 -->\nBody.\n",
        // Another version, a different spelling, something after it.
        "<!-- invar-context: v2 -->\nBody.\n",
        "<!--invar-context: v1-->\nBody.\n",
        "<!-- invar-context: v1 --> and more\nBody.\n",
        // Nothing at all.
        "",
        "   \n\n",
    ] {
        assert_eq!(classify(text), State::Unrecognized, "{text:?}");
    }
}

#[test]
fn only_blank_lines_headings_and_comments_after_the_marker_make_a_file_empty() {
    // What `init` writes (§5) is empty: switching nothing on that the team did
    // not ask for.
    for body in [
        "",
        "\n\n   \n",
        "# BUSINESS_CONTEXT.md\n\n## Invariants\n\n## Regional / parameterized facts\n",
        "<!-- TODO: fill in -->\n## Layers\n",
        "<!--\nmore than\none line\n-->\n# Heading\n",
        "#\n",
        "###### the sixth level\n",
        // Never closed: the rest is hidden, as it is when rendered.
        "<!-- never closed\nText that is hidden.\n",
    ] {
        assert_eq!(classify(&marked(body)), State::Empty, "{body:?}");
    }
}

#[test]
fn anything_else_after_the_marker_is_content() {
    for body in [
        "One sentence.\n",
        "## Invariants\n- Every query is scoped to the account.\n",
        "| Country | Provider |\n|---|---|\n",
        "#hashtag\n",
        "####### seven hashes is not a heading\n",
        "<!-- a note -->\nreal text\n",
        "```\ncode\n```\n",
    ] {
        assert!(
            matches!(classify(&marked(body)), State::Loaded(_)),
            "{body:?}"
        );
    }
}

// --- what is printed -----------------------------------------------------------------

#[test]
fn both_files_are_printed_with_their_origin_in_a_fixed_order() {
    let (_dir, root) = project();
    write(
        &root,
        "BUSINESS_CONTEXT.md",
        &marked("Invariant: scope by account.\n"),
    );
    write(
        &root,
        "AI_CONTEXT.md",
        &marked("Convention: thin controllers.\n"),
    );

    let out = binding(&root).render();

    assert!(out.starts_with("=== invar: binding context ===\n"), "{out}");
    assert!(
        in_order(
            &out,
            &[
                "=== AI_CONTEXT.md — this project's own conventions; binding ===",
                "Convention: thin controllers.",
                "=== end of AI_CONTEXT.md ===",
                "=== BUSINESS_CONTEXT.md — business rules the application must obey; binding ===",
                "Invariant: scope by account.",
                "=== end of BUSINESS_CONTEXT.md ===",
            ]
        ),
        "{out}"
    );
    assert!(!out.contains("invar-context"), "the marker is not content");
    // The first line of hook output must not look like JSON.
    assert!(!out.starts_with('{'));
    assert_eq!(out, binding(&root).render(), "not deterministic");
}

#[test]
fn a_file_that_is_not_loaded_prints_nothing_of_itself() {
    let (_dir, root) = project();
    write(&root, "AI_CONTEXT.md", "Unmarked: SECRET-RULE.\n");
    write(
        &root,
        "BUSINESS_CONTEXT.md",
        &marked("Invariant: scope by account.\n"),
    );

    let found = binding(&root);
    let out = found.render();

    assert!(!out.contains("SECRET-RULE"), "{out}");
    assert!(!out.contains("AI_CONTEXT.md"), "{out}");
    assert!(out.contains("Invariant: scope by account."), "{out}");
    assert_eq!(found.unrecognized(), ["AI_CONTEXT.md"]);
}

#[test]
fn an_empty_marked_file_is_indistinguishable_from_an_absent_one() {
    let (_absent_dir, absent) = project();
    let (_empty_dir, empty) = project();
    write(
        &empty,
        "AI_CONTEXT.md",
        &marked("# AI_CONTEXT.md\n\n## Layers\n"),
    );
    write(
        &empty,
        "BUSINESS_CONTEXT.md",
        &marked("# BUSINESS_CONTEXT.md\n\n## Invariants\n\n## Regional / parameterized facts\n"),
    );
    for root in [&absent, &empty] {
        add_entry(root, "global", 10, "Same entry in both.");
    }

    let (a, e) = (binding(&absent), binding(&empty));

    assert_eq!(a.render(), "");
    assert_eq!(a.render(), e.render());
    assert_eq!(a.unrecognized(), e.unrecognized());
    assert!(a.unrecognized().is_empty(), "nothing to report for either");
    assert_eq!(a.render_within(0).unwrap(), e.render_within(0).unwrap());
    assert_eq!(
        planner_output(&a, &decisions(&absent, None)),
        planner_output(&e, &decisions(&empty, None))
    );
}

#[test]
fn a_context_file_that_cannot_be_read_is_an_error_not_an_absence() {
    // "Cannot tell" must not be taken for "not there": that would drop the
    // invariants because of a permissions problem and say nothing.
    let (_dir, root) = project();
    fs::create_dir(root.path().join("AI_CONTEXT.md")).unwrap();

    let err = load_binding(&root).unwrap_err();

    assert!(matches!(err, ContextError::Read { .. }), "{err}");
    assert!(err.to_string().contains("AI_CONTEXT.md"), "{err}");
}

#[test]
fn a_context_file_that_is_not_utf8_is_an_error() {
    let (_dir, root) = project();
    fs::write(
        root.path().join("BUSINESS_CONTEXT.md"),
        b"\xFF\xFE not text",
    )
    .unwrap();

    let err = load_binding(&root).unwrap_err();

    assert!(
        matches!(err, ContextError::NotUtf8 { valid_up_to: 0, .. }),
        "{err}"
    );
    assert!(err.to_string().contains("UTF-8"), "{err}");
}

// --- the limit -----------------------------------------------------------------------

#[test]
fn the_limit_is_on_the_whole_output_exactly() {
    let (_dir, root) = project();
    write(&root, "AI_CONTEXT.md", &marked(&"a".repeat(300)));
    let found = binding(&root);
    let total = found.render().chars().count();

    assert_eq!(found.render_within(total).unwrap(), found.render());
    match found.render_within(total - 1) {
        Err(ContextError::OverLimit(over)) => {
            assert_eq!((over.total, over.limit), (total, total - 1));
        }
        other => panic!("expected OverLimit, got {other:?}"),
    }
}

#[test]
fn an_output_over_the_limit_names_both_files_their_sizes_and_the_limit() {
    let (_dir, root) = project();
    write(&root, "AI_CONTEXT.md", &marked(&"a".repeat(37)));
    write(&root, "BUSINESS_CONTEXT.md", &marked(&"b".repeat(53)));

    let err = binding(&root).render_within(100).unwrap_err();

    let ContextError::OverLimit(over) = &err else {
        panic!("expected OverLimit, got {err:?}");
    };
    assert_eq!(over.files.len(), 2);
    assert_eq!(
        (over.files[0].name, over.files[0].chars),
        ("AI_CONTEXT.md", 37)
    );
    assert_eq!(
        (over.files[1].name, over.files[1].chars),
        ("BUSINESS_CONTEXT.md", 53)
    );
    // What the labels and framing add is what is left of the total.
    assert_eq!(over.framing, over.total - 37 - 53);
    assert!(over.framing > 0);

    let message = err.to_string();
    assert!(
        message.contains(&format!(
            "binding context is {} characters, over the limit of 100",
            over.total
        )),
        "{message}"
    );
    assert!(
        message.contains("`context.binding_limit` in invar.yaml"),
        "{message}"
    );
    assert!(message.contains("AI_CONTEXT.md"), "{message}");
    assert!(message.contains("BUSINESS_CONTEXT.md"), "{message}");
    assert!(message.contains("labels and framing"), "{message}");
    assert!(message.contains("do not raise the limit"), "{message}");
}

#[test]
fn a_file_that_is_not_in_the_output_is_still_named_with_why() {
    let (_dir, root) = project();
    write(&root, "AI_CONTEXT.md", &marked(&"a".repeat(200)));
    // No BUSINESS_CONTEXT.md at all.

    let err = binding(&root).render_within(50).unwrap_err();

    let ContextError::OverLimit(over) = &err else {
        panic!("expected OverLimit, got {err:?}");
    };
    assert_eq!(
        (over.files[1].name, over.files[1].chars),
        ("BUSINESS_CONTEXT.md", 0)
    );
    assert!(err.to_string().contains("BUSINESS_CONTEXT.md"));
    assert!(err.to_string().contains("(absent)"), "{err}");
}

#[test]
fn the_limit_counts_characters_not_bytes() {
    // The hook's cap is in characters. 4,000 of these are 8,000 bytes.
    let (_dir, root) = project();
    write(&root, "AI_CONTEXT.md", &marked(&"ć".repeat(4000)));
    let found = binding(&root);

    assert!(found.render().len() > 5000, "more than 5000 bytes");
    assert!(
        found.render().chars().count() <= 5000,
        "within 5000 characters"
    );
    assert!(found.render_within(5000).is_ok());
}

#[test]
fn nothing_loaded_is_never_over_any_limit() {
    let (_dir, root) = project();
    assert_eq!(binding(&root).render_within(0).unwrap(), "");
}

// --- the decision log ---------------------------------------------------------------------

#[test]
fn the_modules_entries_come_first_then_global_each_newest_first() {
    let (_dir, root) = project();
    add_entry(&root, "orders", 10, "orders-old");
    add_entry(&root, "global", 20, "global-old");
    add_entry(&root, "orders", 30, "orders-new");
    add_entry(&root, "global", 40, "global-new");
    add_entry(&root, "payments", 50, "payments-entry");

    let found = decisions(&root, Some("orders"));
    let out = found.render();

    assert!(
        in_order(
            &out,
            &["orders-new", "orders-old", "global-new", "global-old"]
        ),
        "{out}"
    );
    assert!(
        !out.contains("payments-entry"),
        "another module's entry: {out}"
    );
    assert_eq!(found.shown.len(), 4);
    assert_eq!(found.other_modules, [("payments".to_owned(), 1)]);
    assert!(out.contains("Showing the entries for module `orders`, then `global`."));
    assert!(
        out.contains("Other modules have entries too: payments (1)."),
        "{out}"
    );
}

#[test]
fn without_a_module_only_global_is_shown_and_the_other_modules_are_listed() {
    let (_dir, root) = project();
    add_entry(&root, "orders", 10, "orders-a");
    add_entry(&root, "orders", 20, "orders-b");
    add_entry(&root, "payments", 30, "payments-a");
    add_entry(&root, "global", 40, "global-a");

    let found = decisions(&root, None);
    let out = found.render();

    assert_eq!(found.shown.len(), 1);
    assert!(out.contains("global-a"), "{out}");
    assert!(
        !out.contains("orders-a") && !out.contains("payments-a"),
        "{out}"
    );
    assert!(out.contains("Showing the `global` entries."), "{out}");
    assert!(
        out.contains("Other modules have entries too: orders (2), payments (1)."),
        "{out}"
    );
    assert!(
        out.contains("`invar context show --module <name>`"),
        "{out}"
    );
}

#[test]
fn asking_for_global_does_not_show_it_twice() {
    let (_dir, root) = project();
    add_entry(&root, "global", 10, "global-a");
    add_entry(&root, "global", 20, "global-b");

    let found = decisions(&root, Some("global"));

    assert_eq!(found.shown.len(), 2);
    assert_eq!(found.module, None);
}

#[test]
fn an_entry_is_a_header_line_and_its_text() {
    let (_dir, root) = project();
    let id = add_entry(&root, "orders", 12, "Reservation is released on cancel.");

    let found = decisions(&root, Some("orders"));

    assert_eq!(
        found.shown,
        [format!(
            "[2026-10-06T18:37:12+02:00] note · SHOP-1 · orders · {id}\n\
             Reservation is released on cancel.\n"
        )]
    );
}

#[test]
fn a_name_that_is_not_a_module_is_refused() {
    let (_dir, root) = project();
    for module in ["Orders", "", "or-ders", "orders/x"] {
        let err = load_decisions(&root, Some(module)).unwrap_err();
        assert!(
            matches!(err, ContextError::InvalidModule { .. }),
            "{module:?}"
        );
    }
}

#[test]
fn at_most_fifty_entries_are_shown_and_the_rest_are_counted() {
    let (_dir, root) = project();
    for second in 0..60 {
        add_entry(&root, "global", second, &format!("Entry {second}."));
    }

    let found = decisions(&root, None);
    let out = found.render();

    assert_eq!(found.shown.len(), MAX_ENTRIES);
    assert_eq!(found.omitted, 10);
    // The newest fifty: 59 down to 10.
    assert!(found.shown[0].contains("Entry 59."));
    assert!(found.shown[MAX_ENTRIES - 1].contains("Entry 10."));
    assert!(!out.contains("Entry 9."), "an older entry was shown");
    assert!(
        out.contains(
            "(10 older entries are not shown: the limit is 50 entries or about 4000 tokens.)"
        ),
        "{out}"
    );
}

#[test]
fn the_character_budget_stops_before_an_entry_that_would_not_fit() {
    let (_dir, root) = project();
    for second in 10..15 {
        add_entry(&root, "global", second, &"x".repeat(5000));
    }

    let found = decisions(&root, None);

    // 5,000 characters and a header each: three fit in 16,000, a fourth does
    // not — and an entry is never cut in the middle.
    assert_eq!(found.shown.len(), 3);
    assert_eq!(found.omitted, 2);
    let used: usize = found.shown.iter().map(|text| text.chars().count()).sum();
    assert!(used <= MAX_CHARS, "{used}");
    assert!(
        found
            .shown
            .iter()
            .all(|text| text.contains(&"x".repeat(5000)))
    );
    assert!(found.render().contains("(2 older entries are not shown"));
}

#[test]
fn the_budget_is_shared_so_the_modules_entries_use_it_first() {
    let (_dir, root) = project();
    for second in 0..50 {
        add_entry(&root, "orders", second, &format!("Orders {second}."));
    }
    for second in 50..55 {
        add_entry(&root, "global", second, &format!("Global {second}."));
    }

    let found = decisions(&root, Some("orders"));

    assert_eq!(found.shown.len(), 50);
    assert!(found.shown.iter().all(|text| text.contains("Orders")));
    assert_eq!(found.omitted, 5, "the global entries did not fit");
}

#[test]
fn an_entry_that_cannot_be_read_is_skipped_and_reported_and_the_rest_are_shown() {
    let (_dir, root) = project();
    add_entry(&root, "global", 10, "A good entry.");
    fs::write(
        root.decisions_dir()
            .join("2026-10-06-163799-global-broken.md"),
        "no front matter\n",
    )
    .unwrap();

    let found = decisions(&root, None);

    assert_eq!(found.shown.len(), 1);
    assert_eq!(found.total, 1, "the broken one is not an entry");
    assert_eq!(found.unreadable.len(), 1);
    assert!(
        found.unreadable[0]
            .0
            .ends_with("2026-10-06-163799-global-broken.md")
    );
}

#[test]
fn a_log_with_no_entries_renders_nothing() {
    let (_dir, root) = project();
    assert_eq!(decisions(&root, None).render(), "");

    fs::create_dir_all(root.decisions_dir()).unwrap();
    assert_eq!(decisions(&root, Some("orders")).render(), "");
}

#[test]
fn entries_only_in_other_modules_still_say_where_they_are() {
    let (_dir, root) = project();
    add_entry(&root, "orders", 10, "orders-a");
    add_entry(&root, "orders", 20, "orders-b");

    let out = decisions(&root, None).render();

    assert!(out.contains("(no entries for this selection)"), "{out}");
    assert!(
        out.contains("Other modules have entries too: orders (2)."),
        "{out}"
    );
    assert!(!out.contains("orders-a"), "{out}");
}

// --- both together ----------------------------------------------------------------------------

#[test]
fn the_planner_reads_the_binding_context_then_the_log() {
    let (_dir, root) = project();
    write(
        &root,
        "AI_CONTEXT.md",
        &marked("Convention: thin controllers.\n"),
    );
    add_entry(&root, "global", 10, "global-a");

    let found = binding(&root);
    let log = decisions(&root, None);
    let out = planner_output(&found, &log);

    assert_eq!(out, format!("{}\n{}", found.render(), log.render()));
    assert!(
        in_order(
            &out,
            &[
                "Convention: thin controllers.",
                "global-a",
                "=== end of decision log ==="
            ]
        ),
        "{out}"
    );
}

#[test]
fn the_planner_output_is_whichever_part_there_is() {
    let (_dir, root) = project();
    assert_eq!(planner_output(&binding(&root), &decisions(&root, None)), "");

    write(
        &root,
        "AI_CONTEXT.md",
        &marked("Convention: thin controllers.\n"),
    );
    let only_binding = planner_output(&binding(&root), &decisions(&root, None));
    assert_eq!(only_binding, binding(&root).render());

    let (_other_dir, other) = project();
    add_entry(&other, "global", 10, "global-a");
    let only_log = planner_output(&binding(&other), &decisions(&other, None));
    assert_eq!(only_log, decisions(&other, None).render());
}
