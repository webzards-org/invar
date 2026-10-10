//! Adding one entry to `.gitattributes` and to `.gitignore`, and touching
//! nothing else (M1-15).
//!
//! Both are line-oriented files a real project already has, so the rule is the
//! same for both: **what is there stays byte for byte, and our entry goes at
//! the end**, after a blank line and a comment saying where it came from. An
//! entry that is already there — spelled the way we spell it, or any of the
//! few ways that mean the same — is not added again.

#[cfg(test)]
mod tests;

/// The line that pins a Plan's line endings. A Plan is checksummed over its
/// text (§4d); a checkout that turned LF into CRLF must not be able to change
/// what was approved. Canonicalization already makes the digest immune to it,
/// and this keeps the working tree and the index in agreement as well.
pub const GITATTRIBUTES_ENTRY: &str = ".invar/plans/**/*.md text eol=lf";

const GITATTRIBUTES_COMMENT: &str =
    "invar: Plan bodies are checksummed; keep their line endings the same on every platform";

/// What `.gitignore` gets: the working files `invar done` renders, such as the
/// PR description.
pub const GITIGNORE_ENTRY: &str = ".invar/out/";

const GITIGNORE_COMMENT: &str = "invar: working files (the rendered PR description)";

/// `.gitattributes` with the Plan line-ending rule added; `None` when it is
/// already there. `existing` is `None` for no file.
pub fn pin_plan_line_endings(existing: Option<&str>) -> Option<String> {
    ensure(
        existing,
        GITATTRIBUTES_COMMENT,
        GITATTRIBUTES_ENTRY,
        pins_plan_line_endings,
    )
}

/// `.gitignore` with `.invar/out/` added; `None` when it is already ignored.
pub fn ignore_out_dir(existing: Option<&str>) -> Option<String> {
    ensure(
        existing,
        GITIGNORE_COMMENT,
        GITIGNORE_ENTRY,
        ignores_out_dir,
    )
}

/// Does `.gitignore` ignore the whole of `.invar/`? That would keep the Plans
/// and the decision log out of git — and Plans live in git so that a reviewer
/// reads them (§12). Not something `init` may fix for the team, but something
/// it should say.
pub fn hides_invar_dir(gitignore: &str) -> bool {
    gitignore.lines().map(str::trim).any(|line| {
        matches!(
            line,
            ".invar"
                | ".invar/"
                | "/.invar"
                | "/.invar/"
                | ".invar/*"
                | "/.invar/*"
                | ".invar/**"
                | "/.invar/**"
        )
    })
}

/// `line` is a `.gitattributes` line that sets `text` and `eol=lf` on the
/// Plans, in either order and with or without a leading slash.
fn pins_plan_line_endings(line: &str) -> bool {
    let mut tokens = line.split_whitespace();
    let Some(pattern) = tokens.next() else {
        return false;
    };
    if pattern != ".invar/plans/**/*.md" && pattern != "/.invar/plans/**/*.md" {
        return false;
    }
    let attributes: Vec<&str> = tokens.collect();
    attributes.contains(&"text") && attributes.contains(&"eol=lf")
}

/// `line` is a `.gitignore` pattern that ignores `.invar/out/`.
fn ignores_out_dir(line: &str) -> bool {
    matches!(
        line,
        ".invar/out"
            | ".invar/out/"
            | "/.invar/out"
            | "/.invar/out/"
            | ".invar/out/*"
            | "/.invar/out/*"
            | ".invar/out/**"
            | "/.invar/out/**"
    )
}

fn ensure(
    existing: Option<&str>,
    comment: &str,
    entry: &str,
    already: impl Fn(&str) -> bool,
) -> Option<String> {
    let text = existing.unwrap_or("");
    let there = text
        .lines()
        .map(str::trim)
        .any(|line| !line.starts_with('#') && already(line));
    (!there).then(|| append(text, comment, entry))
}

/// `text` with a comment and the entry at the end: after a final newline if it
/// lacked one, and after a blank line unless one is there already. The line
/// ending is the file's own.
fn append(text: &str, comment: &str, entry: &str) -> String {
    let eol = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let mut out = text.to_owned();
    if !out.is_empty() {
        if !out.ends_with('\n') {
            out.push_str(eol);
        }
        let last_line = out[..out.len() - 1].rsplit('\n').next().unwrap_or("");
        if !last_line.trim().is_empty() {
            out.push_str(eol);
        }
    }
    out.push_str(&format!("# {comment}{eol}{entry}{eol}"));
    out
}
