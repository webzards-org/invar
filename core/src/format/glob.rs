//! The globs of `cycle.required_paths` (`contracts/formats/invar-yaml.md` §3.8).
//!
//! A small, exact subset of `.gitignore` patterns, matched by hand so that the rule is the
//! one the contract states and no crate's idea of a glob:
//!
//! - a pattern and a path are split at `/` into segments, and the pattern is anchored at
//!   the project root;
//! - `*` matches any run of characters within one segment, the empty one included, and
//!   never a `/`;
//! - `**` as a whole segment matches any number of segments, none included;
//! - every other character, `?` and `[` among them, matches itself;
//! - a pattern matches a path when it matches the whole path, or a directory the path is
//!   in, so `app/Models` covers `app/Models/Order.php`;
//! - a pattern ending in `/` matches only a directory the path is in.

#[cfg(test)]
mod tests;

/// Why a pattern is refused (§3.8).
pub fn problem(pattern: &str) -> Option<String> {
    if pattern.trim().is_empty() {
        return Some("a pattern is empty".to_owned());
    }
    if pattern.contains('\\') {
        return Some(format!(
            "`{pattern}` holds a `\\`; separate the segments with `/`"
        ));
    }
    let is_drive = matches!(pattern.as_bytes(), [letter, b':', ..] if letter.is_ascii_alphabetic());
    if pattern.starts_with('/') || is_drive {
        return Some(format!(
            "`{pattern}` is absolute; a pattern is relative to the project root"
        ));
    }
    let body = pattern.strip_suffix('/').unwrap_or(pattern);
    for segment in body.split('/') {
        match segment {
            "" => {
                return Some(format!(
                    "`{pattern}` has an empty segment; write one `/` between segments"
                ));
            }
            "." | ".." => {
                return Some(format!(
                    "`{pattern}` has a `{segment}` segment; a pattern names paths under the \
                     project root as they are"
                ));
            }
            _ => {}
        }
    }
    None
}

/// Whether `path`, relative to the project root with `/` separators, matches `pattern`.
/// A pattern [`problem`] refuses matches nothing.
pub fn matches(pattern: &str, path: &str) -> bool {
    if problem(pattern).is_some() {
        return false;
    }
    let (body, directory_only) = match pattern.strip_suffix('/') {
        Some(body) => (body, true),
        None => (pattern, false),
    };
    let pattern: Vec<&str> = body.split('/').collect();
    let path: Vec<&str> = path.split('/').filter(|part| !part.is_empty()).collect();
    // The whole path, then each directory it is in, deepest first.
    let whole = usize::from(directory_only);
    (1..=path.len().saturating_sub(whole))
        .rev()
        .any(|end| segments(&pattern, &path[..end]))
}

/// `pattern` against the whole of `path`, segment by segment.
fn segments(pattern: &[&str], path: &[&str]) -> bool {
    match pattern.split_first() {
        None => path.is_empty(),
        Some((&"**", rest)) => (0..=path.len()).any(|skip| segments(rest, &path[skip..])),
        Some((first, rest)) => path
            .split_first()
            .is_some_and(|(segment, more)| segment_matches(first, segment) && segments(rest, more)),
    }
}

/// One segment against one segment: `*` is any run of characters.
fn segment_matches(pattern: &str, segment: &str) -> bool {
    let mut parts = pattern.split('*');
    let first = parts.next().unwrap_or_default();
    let Some(mut rest) = segment.strip_prefix(first) else {
        return false;
    };
    let parts: Vec<&str> = parts.collect();
    let Some((last, middle)) = parts.split_last() else {
        // No `*`: the segment is the pattern exactly.
        return rest.is_empty();
    };
    for part in middle {
        match rest.find(part) {
            Some(at) => rest = &rest[at + part.len()..],
            None => return false,
        }
    }
    rest.len() >= last.len() && rest.ends_with(last)
}

/// The first of `patterns` that `path` matches.
pub fn first_match<'a>(patterns: &'a [String], path: &str) -> Option<&'a str> {
    patterns
        .iter()
        .find(|pattern| matches(pattern, path))
        .map(String::as_str)
}
