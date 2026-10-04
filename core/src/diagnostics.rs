//! Collecting problems instead of stopping at the first one.
//!
//! `plan validate` must report **every** problem in one run (§4a): a developer
//! — or the planner agent reading the output — should fix a Plan in one round
//! trip, not one error per invocation. So the parsers push what they find into
//! a [`Diagnostics`] and carry on, and the caller decides at the end whether
//! anything was found.
//!
//! Deliberately small: a line number where there is one, and a message. Every
//! message says what to change, not only what is wrong, because the output is
//! read by an agent as often as by a person (M1-8).

// `plan validate` (M1-8) prints it; `finish` has no caller in the binary
// yet. `expect` rather than `allow`, so the compiler asks for this line to go
// once it does.
#![cfg_attr(
    not(test),
    expect(dead_code, reason = "`finish` has no caller in the binary yet")
)]

use std::fmt;
use std::path::Path;

/// One problem, at a line of the file when it has one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    /// 1-based line number in the file, if the problem sits on a line.
    pub line: Option<usize>,
    pub message: String,
}

/// Every problem found in one pass.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Diagnostics {
    items: Vec<Diagnostic>,
}

impl Diagnostics {
    pub fn new() -> Self {
        Self::default()
    }

    /// A problem at a line (1-based).
    pub fn at(&mut self, line: usize, message: impl Into<String>) {
        self.items.push(Diagnostic {
            line: Some(line),
            message: message.into(),
        });
    }

    /// A problem that belongs to the file as a whole, such as a missing key.
    pub fn general(&mut self, message: impl Into<String>) {
        self.items.push(Diagnostic {
            line: None,
            message: message.into(),
        });
    }

    /// Take over every problem another pass found.
    pub fn extend(&mut self, other: Diagnostics) {
        self.items.extend(other.items);
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// In reading order: problems of the whole file first, then by line. The
    /// order of problems on the same line is the order they were found.
    pub fn iter(&self) -> impl Iterator<Item = &Diagnostic> {
        let mut sorted: Vec<&Diagnostic> = self.items.iter().collect();
        sorted.sort_by_key(|diagnostic| diagnostic.line);
        sorted.into_iter()
    }

    /// `Ok(value)` when nothing was found, the problems otherwise.
    pub fn finish<T>(self, value: T) -> Result<T, Diagnostics> {
        if self.is_empty() {
            Ok(value)
        } else {
            Err(self)
        }
    }

    /// One problem per line, compiler-style: `path:12: message`, or
    /// `path: message` for problems of the whole file. Editors and agents
    /// both know how to jump to that.
    pub fn render(&self, path: &Path) -> String {
        let mut out = String::new();
        for diagnostic in self.iter() {
            match diagnostic.line {
                Some(line) => out.push_str(&format!("{}:{line}: ", path.display())),
                None => out.push_str(&format!("{}: ", path.display())),
            }
            out.push_str(&diagnostic.message);
            out.push('\n');
        }
        out
    }
}

impl fmt::Display for Diagnostics {
    /// Without a path: `line 12: message`, one per line.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, diagnostic) in self.iter().enumerate() {
            if index > 0 {
                writeln!(f)?;
            }
            match diagnostic.line {
                Some(line) => write!(f, "line {line}: {}", diagnostic.message)?,
                None => write!(f, "{}", diagnostic.message)?,
            }
        }
        Ok(())
    }
}

impl std::error::Error for Diagnostics {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_finishes_with_the_value() {
        assert_eq!(Diagnostics::new().finish(7), Ok(7));
    }

    #[test]
    fn anything_found_finishes_with_every_problem() {
        let mut found = Diagnostics::new();
        found.at(3, "first");
        found.general("second");
        let err = found.finish(()).unwrap_err();
        assert_eq!(err.len(), 2);
    }

    #[test]
    fn problems_read_top_down_with_whole_file_problems_first() {
        let mut found = Diagnostics::new();
        found.at(9, "late");
        found.at(2, "early");
        found.general("whole file");
        found.at(2, "early, second");
        assert_eq!(
            found.to_string(),
            "whole file\nline 2: early\nline 2: early, second\nline 9: late"
        );
    }

    #[test]
    fn render_prefixes_the_path_like_a_compiler() {
        let mut found = Diagnostics::new();
        found.at(4, "fix this");
        found.general("and this");
        assert_eq!(
            found.render(Path::new("plans/spec.md")),
            "plans/spec.md: and this\nplans/spec.md:4: fix this\n"
        );
    }

    #[test]
    fn extend_keeps_both_passes() {
        let mut header = Diagnostics::new();
        header.at(2, "header");
        let mut sections = Diagnostics::new();
        sections.at(20, "sections");
        header.extend(sections);
        assert_eq!(header.len(), 2);
    }
}
