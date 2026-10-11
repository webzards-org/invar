//! Value shapes shared by more than one on-disk format.
//!
//! A ticket id and a module name mean the same thing in a Plan header
//! (plan-header.md §5) and in a decision entry (decision-entry.md §4.1);
//! checking them in one place keeps the two contracts from drifting.

/// `^[A-Z][A-Z0-9]*-[0-9]+$`: `PROJ-412`.
pub fn is_ticket(value: &str) -> bool {
    let Some((prefix, number)) = value.split_once('-') else {
        return false;
    };
    let mut prefix_chars = prefix.chars();
    prefix_chars.next().is_some_and(|c| c.is_ascii_uppercase())
        && prefix_chars.all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
        && !number.is_empty()
        && number.chars().all(|c| c.is_ascii_digit())
}

/// `^[A-Za-z][A-Za-z0-9_]*$` — which `global` satisfies too. Case is kept as
/// written: Laravel projects name modules `Billing`, and nothing compares
/// modules case-insensitively.
pub fn is_module(value: &str) -> bool {
    let mut chars = value.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// `^[a-z0-9]+(-[a-z0-9]+)*$`: `reserve-stock`.
pub fn is_slug(value: &str) -> bool {
    !value.is_empty()
        && value.split('-').all(|group| {
            !group.is_empty()
                && group
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
        })
}

/// `line` without HTML comments (`<!-- … -->`).
///
/// A text rule rather than a value shape, but shared for the same reason as
/// the shapes above: what counts as a comment decides what counts as written
/// content, in a Plan's sections (plan-header.md D-14) and in a context file
/// (`context`), and the two must not disagree.
///
/// `in_comment` carries an unclosed `<!--` over to the next line, so a comment
/// that spans lines hides all of them; one that is never closed hides the rest
/// of the text, as it does when the Markdown is rendered.
pub fn without_comments(line: &str, in_comment: &mut bool) -> String {
    let mut visible = String::new();
    let mut rest = line;
    loop {
        if *in_comment {
            let Some(end) = rest.find("-->") else {
                return visible;
            };
            rest = &rest[end + 3..];
            *in_comment = false;
        } else {
            let Some(start) = rest.find("<!--") else {
                visible.push_str(rest);
                return visible;
            };
            visible.push_str(&rest[..start]);
            rest = &rest[start + 4..];
            *in_comment = true;
        }
    }
}
