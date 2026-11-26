//! The line diff of the audit report (`plan-audit.md` §7.1): the longest common
//! subsequence of two sections' lines, as hunks of removed and added lines. No context
//! lines, removals before additions within a hunk, and the same input always gives the
//! same hunks. Sections are short, so the table is fine.

/// One run of changed lines: what the baseline had there, and what is there now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hunk {
    pub removed: Vec<String>,
    pub added: Vec<String>,
}

/// The hunks from `before` to `after`, in order. None when they are the same lines.
pub fn hunks(before: &str, after: &str) -> Vec<Hunk> {
    let a: Vec<&str> = before.lines().collect();
    let b: Vec<&str> = after.lines().collect();
    // `lcs[i][j]`: the length of the longest common subsequence of a[i..] and b[j..].
    let mut lcs = vec![vec![0usize; b.len() + 1]; a.len() + 1];
    for i in (0..a.len()).rev() {
        for j in (0..b.len()).rev() {
            lcs[i][j] = if a[i] == b[j] {
                lcs[i + 1][j + 1] + 1
            } else {
                lcs[i + 1][j].max(lcs[i][j + 1])
            };
        }
    }

    let mut hunks = Vec::new();
    let mut current = Hunk {
        removed: Vec::new(),
        added: Vec::new(),
    };
    let flush = |current: &mut Hunk, hunks: &mut Vec<Hunk>| {
        if !current.removed.is_empty() || !current.added.is_empty() {
            hunks.push(std::mem::replace(
                current,
                Hunk {
                    removed: Vec::new(),
                    added: Vec::new(),
                },
            ));
        }
    };
    let (mut i, mut j) = (0, 0);
    while i < a.len() || j < b.len() {
        if i < a.len() && j < b.len() && a[i] == b[j] {
            flush(&mut current, &mut hunks);
            i += 1;
            j += 1;
        } else if j == b.len() || (i < a.len() && lcs[i + 1][j] >= lcs[i][j + 1]) {
            current.removed.push(a[i].to_owned());
            i += 1;
        } else {
            current.added.push(b[j].to_owned());
            j += 1;
        }
    }
    flush(&mut current, &mut hunks);
    hunks
}

#[cfg(test)]
mod tests {
    use super::{Hunk, hunks};

    fn hunk(removed: &[&str], added: &[&str]) -> Hunk {
        Hunk {
            removed: removed.iter().map(|s| (*s).to_owned()).collect(),
            added: added.iter().map(|s| (*s).to_owned()).collect(),
        }
    }

    #[test]
    fn the_same_lines_have_no_hunk() {
        assert!(hunks("a\nb\n", "a\nb\n").is_empty());
        assert!(hunks("", "").is_empty());
    }

    #[test]
    fn a_changed_line_is_one_hunk_with_the_removal_first() {
        assert_eq!(hunks("a\nb\nc\n", "a\nB\nc\n"), vec![hunk(&["b"], &["B"])]);
    }

    #[test]
    fn changes_apart_are_separate_hunks_in_order() {
        assert_eq!(
            hunks("a\nb\nc\nd\ne\n", "a\nB\nc\nd\nE\nF\n"),
            vec![hunk(&["b"], &["B"]), hunk(&["e"], &["E", "F"])]
        );
    }

    #[test]
    fn added_and_removed_lines_alone() {
        assert_eq!(hunks("a\n", "a\nb\n"), vec![hunk(&[], &["b"])]);
        assert_eq!(hunks("a\nb\n", "b\n"), vec![hunk(&["a"], &[])]);
    }

    #[test]
    fn crlf_and_lf_are_the_same_lines() {
        assert!(hunks("a\r\nb\r\n", "a\nb\n").is_empty());
    }
}
