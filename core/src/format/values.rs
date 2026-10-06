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

/// `^[a-z][a-z0-9_]*$` — which `global` satisfies too.
pub fn is_module(value: &str) -> bool {
    let mut chars = value.chars();
    chars.next().is_some_and(|c| c.is_ascii_lowercase())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
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
