//! Adding one deny rule to `.claude/settings.json`, and touching nothing else.
//!
//! A real project's `.claude/settings.json` is the team's: permissions, hooks,
//! environment, a model choice. `init` adds one entry and must leave the rest
//! exactly as it was. Losing it would end the pilot on day one (M1-15).
//!
//! # Why this is text surgery and not "parse, edit, write"
//!
//! Round-tripping through a JSON value looks simpler and does damage:
//! `serde_json` without `preserve_order` re-sorts every object's keys
//! alphabetically, and any serializer re-lays the whole file out. The team's
//! `git diff` would show a rewritten file where the change is one line, and
//! its own ordering and indentation would be gone.
//!
//! So the document is **parsed only to be understood** — is it valid JSON, is
//! it an object, does `permissions.deny` exist and what is in it — and the
//! change is made in the text: one entry is inserted at the end of the right
//! container, with the file's own indentation and line endings, and every
//! other byte stays where it was. The inserted text is the only difference
//! between the old file and the new one.
//!
//! # What is refused
//!
//! A file that is not valid JSON is **refused, never repaired or replaced**
//! (comments and trailing commas included: Claude Code's settings are JSON).
//! So is one whose `permissions` is not an object or whose `permissions.deny`
//! is not an array. Guessing what the team meant is how a config gets lost.

use std::fmt;

use serde_json::Value;

#[cfg(test)]
mod tests;

/// The file with `rule` added to `permissions.deny`.
///
/// - `existing` is `None`, or empty, or only whitespace: a new file.
/// - `Ok(None)`: the rule is already there, and nothing needs to change.
/// - `Ok(Some(text))`: the new contents.
pub fn add_deny_rule(existing: Option<&str>, rule: &str) -> Result<Option<String>, SettingsError> {
    let rule_json = serde_json::to_string(rule).expect("a string serializes");
    let Some(text) = existing.filter(|text| !text.trim().is_empty()) else {
        return Ok(Some(fresh_file(&rule_json)));
    };

    // A BOM is not JSON, but a file may carry one; it is kept where it is.
    let bom = if text.starts_with('\u{FEFF}') {
        '\u{FEFF}'.len_utf8()
    } else {
        0
    };
    let value: Value =
        serde_json::from_str(&text[bom..]).map_err(|err| SettingsError::NotJson {
            line: err.line(),
            column: err.column(),
            message: err.to_string(),
        })?;
    let Value::Object(top) = &value else {
        return Err(SettingsError::NotAnObject);
    };

    // What is there, as JSON understands it. Where the text is then edited is
    // found with the scanner below, and the two agree because the scanner
    // takes the last of a repeated key, as `serde_json` does.
    let permissions = match top.get("permissions") {
        None => None,
        Some(Value::Object(permissions)) => Some(permissions),
        Some(_) => {
            return Err(SettingsError::WrongType {
                path: "permissions",
                expected: "an object",
            });
        }
    };
    let deny = match permissions.and_then(|permissions| permissions.get("deny")) {
        None => None,
        Some(Value::Array(deny)) => Some(deny),
        Some(_) => {
            return Err(SettingsError::WrongType {
                path: "permissions.deny",
                expected: "an array",
            });
        }
    };
    if deny.is_some_and(|deny| deny.iter().any(|entry| entry.as_str() == Some(rule))) {
        return Ok(None);
    }

    let scan = Scanner::new(text);
    let top = scan.object(scan.skip_ws(bom));
    let layout = Layout::of(text, &top);

    let edited = match (permissions, deny) {
        (None, _) => insert(text, &top, &layout, |indent, multiline| {
            layout.permissions_member(&rule_json, indent, multiline)
        }),
        (Some(_), None) => {
            let permissions = scan.object(last(&top, "permissions").value_start);
            insert(text, &permissions, &layout, |indent, multiline| {
                layout.deny_member(&rule_json, indent, multiline)
            })
        }
        (Some(_), Some(_)) => {
            let permissions = scan.object(last(&top, "permissions").value_start);
            let deny = scan.array(last(&permissions, "deny").value_start);
            insert(text, &deny, &layout, |_, _| rule_json.clone())
        }
    };
    Ok(Some(edited))
}

/// A new file, in the layout Claude Code's own examples use.
fn fresh_file(rule_json: &str) -> String {
    format!("{{\n  \"permissions\": {{\n    \"deny\": [\n      {rule_json}\n    ]\n  }}\n}}\n")
}

// --- inserting -------------------------------------------------------------------

/// How the file is laid out, so what is added looks like what is there.
struct Layout {
    /// `\r\n` when the file uses it anywhere, else `\n`.
    eol: &'static str,
    /// One level of indentation: the top level's own, or two spaces.
    unit: String,
}

impl Layout {
    fn of(text: &str, top: &Container) -> Self {
        let eol = if text.contains("\r\n") { "\r\n" } else { "\n" };
        let unit = top
            .entries
            .first()
            .filter(|first| text[top.open..first.start].contains('\n'))
            .map(|first| indent_before(text, first.start))
            .filter(|indent| !indent.is_empty())
            .unwrap_or_else(|| "  ".to_owned());
        Self { eol, unit }
    }

    /// `"deny": [ "<rule>" ]`, whose own line is indented by `indent`.
    fn deny_member(&self, rule_json: &str, indent: &str, multiline: bool) -> String {
        if multiline {
            let inner = format!("{indent}{}", self.unit);
            format!(
                "\"deny\": [{eol}{inner}{rule_json}{eol}{indent}]",
                eol = self.eol
            )
        } else {
            format!("\"deny\": [{rule_json}]")
        }
    }

    /// `"permissions": { "deny": [ "<rule>" ] }`.
    fn permissions_member(&self, rule_json: &str, indent: &str, multiline: bool) -> String {
        if multiline {
            let inner = format!("{indent}{}", self.unit);
            let deny = self.deny_member(rule_json, &inner, true);
            format!(
                "\"permissions\": {{{eol}{inner}{deny}{eol}{indent}}}",
                eol = self.eol
            )
        } else {
            format!("\"permissions\": {{\"deny\": [{rule_json}]}}")
        }
    }
}

/// Add one entry as the last of `container`. `make(indent, multiline)` renders
/// the entry for a container laid out over several lines (its own line
/// indented by `indent`) or on one.
fn insert(
    text: &str,
    container: &Container,
    layout: &Layout,
    make: impl Fn(&str, bool) -> String,
) -> String {
    let Some(last) = container.entries.last() else {
        // `{}` or `[ ]`: open it up, one entry to a line, closing on the
        // indentation of the line it opened on.
        let base = line_indent(text, container.open);
        let child = format!("{base}{}", layout.unit);
        let entry = make(&child, true);
        let eol = layout.eol;
        let replacement = format!("{eol}{child}{entry}{eol}{base}");
        return splice(text, container.open + 1, container.close, &replacement);
    };

    let first = &container.entries[0];
    if text[container.open..first.start].contains('\n') {
        let indent = indent_before(text, first.start);
        let entry = make(&indent, true);
        let eol = layout.eol;
        splice(text, last.end, last.end, &format!(",{eol}{indent}{entry}"))
    } else {
        let entry = make("", false);
        splice(text, last.end, last.end, &format!(", {entry}"))
    }
}

fn splice(text: &str, start: usize, end: usize, replacement: &str) -> String {
    format!("{}{replacement}{}", &text[..start], &text[end..])
}

/// The indentation of the line `pos` is on, when nothing but that indentation
/// stands in front of `pos`; otherwise the line's own indentation, one level
/// deeper being left to the caller's `unit`.
fn indent_before(text: &str, pos: usize) -> String {
    let line_start = text[..pos].rfind('\n').map_or(0, |at| at + 1);
    let before = &text[line_start..pos];
    if before.chars().all(|c| c == ' ' || c == '\t') {
        before.to_owned()
    } else {
        line_indent(text, pos)
    }
}

/// The leading spaces and tabs of the line `pos` is on.
fn line_indent(text: &str, pos: usize) -> String {
    let line_start = text[..pos].rfind('\n').map_or(0, |at| at + 1);
    text[line_start..]
        .chars()
        .take_while(|c| *c == ' ' || *c == '\t')
        .collect()
}

/// The last member called `key`, which is the one JSON readers keep.
fn last<'a>(container: &'a Container, key: &str) -> &'a Entry {
    container
        .entries
        .iter()
        .rev()
        .find(|entry| entry.key == key)
        .expect("the value was found by parsing the same text")
}

// --- a scanner for what is already known to be JSON -----------------------------------

/// One member of an object (with its key) or one element of an array.
struct Entry {
    key: String,
    /// Where it begins: the key's opening quote, or the element itself.
    start: usize,
    /// Where its value begins.
    value_start: usize,
    /// Just past the end of its value.
    end: usize,
}

struct Container {
    /// The `{` or `[`.
    open: usize,
    /// The matching `}` or `]`.
    close: usize,
    entries: Vec<Entry>,
}

/// Finds where things are in text `serde_json` has already accepted. It
/// understands strings (and their escapes), nesting and separators, and
/// nothing else; it is not a validator, and does not need to be.
struct Scanner<'a> {
    text: &'a str,
    bytes: &'a [u8],
}

impl<'a> Scanner<'a> {
    fn new(text: &'a str) -> Self {
        Self {
            text,
            bytes: text.as_bytes(),
        }
    }

    fn skip_ws(&self, mut at: usize) -> usize {
        while at < self.bytes.len() && matches!(self.bytes[at], b' ' | b'\t' | b'\n' | b'\r') {
            at += 1;
        }
        at
    }

    /// Just past the string that opens at `at`.
    fn string_end(&self, at: usize) -> usize {
        let mut i = at + 1;
        while i < self.bytes.len() {
            match self.bytes[i] {
                b'\\' => i += 2,
                b'"' => return i + 1,
                _ => i += 1,
            }
        }
        self.bytes.len()
    }

    /// Just past the value that begins at `at`.
    fn value_end(&self, at: usize) -> usize {
        match self.bytes[at] {
            b'"' => self.string_end(at),
            b'{' | b'[' => {
                let mut depth = 0usize;
                let mut i = at;
                while i < self.bytes.len() {
                    match self.bytes[i] {
                        b'"' => {
                            i = self.string_end(i);
                            continue;
                        }
                        b'{' | b'[' => depth += 1,
                        b'}' | b']' => {
                            depth -= 1;
                            if depth == 0 {
                                return i + 1;
                            }
                        }
                        _ => {}
                    }
                    i += 1;
                }
                self.bytes.len()
            }
            _ => {
                let mut i = at;
                while i < self.bytes.len()
                    && !matches!(
                        self.bytes[i],
                        b',' | b'}' | b']' | b' ' | b'\t' | b'\n' | b'\r'
                    )
                {
                    i += 1;
                }
                i
            }
        }
    }

    /// The object whose `{` is at `open`.
    fn object(&self, open: usize) -> Container {
        let mut entries = Vec::new();
        let mut i = self.skip_ws(open + 1);
        while i < self.bytes.len() && self.bytes[i] != b'}' {
            let key_end = self.string_end(i);
            let key: String = serde_json::from_str(&self.text[i..key_end]).unwrap_or_default();
            let colon = self.skip_ws(key_end);
            let value_start = self.skip_ws(colon + 1);
            let end = self.value_end(value_start);
            entries.push(Entry {
                key,
                start: i,
                value_start,
                end,
            });
            i = self.skip_ws(end);
            if i < self.bytes.len() && self.bytes[i] == b',' {
                i = self.skip_ws(i + 1);
            }
        }
        Container {
            open,
            close: i,
            entries,
        }
    }

    /// The array whose `[` is at `open`.
    fn array(&self, open: usize) -> Container {
        let mut entries = Vec::new();
        let mut i = self.skip_ws(open + 1);
        while i < self.bytes.len() && self.bytes[i] != b']' {
            let end = self.value_end(i);
            entries.push(Entry {
                key: String::new(),
                start: i,
                value_start: i,
                end,
            });
            i = self.skip_ws(end);
            if i < self.bytes.len() && self.bytes[i] == b',' {
                i = self.skip_ws(i + 1);
            }
        }
        Container {
            open,
            close: i,
            entries,
        }
    }
}

// --- errors -----------------------------------------------------------------------------

#[derive(Debug, PartialEq, Eq)]
pub enum SettingsError {
    NotJson {
        line: usize,
        column: usize,
        message: String,
    },
    NotAnObject,
    WrongType {
        path: &'static str,
        expected: &'static str,
    },
}

impl fmt::Display for SettingsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotJson {
                line,
                column,
                message,
            } => write!(
                f,
                "is not valid JSON ({message}; line {line}, column {column}). `init` does not \
                 edit a file it cannot read, and does not replace one: fix it, or add the rule \
                 by hand"
            ),
            Self::NotAnObject => write!(
                f,
                "is JSON, but not an object: a settings file is `{{ ... }}`. `init` does not \
                 replace it; add the rule by hand"
            ),
            Self::WrongType { path, expected } => write!(
                f,
                "has `{path}` that is not {expected}, so there is nowhere to add the rule. \
                 `init` does not rewrite it; add the rule by hand"
            ),
        }
    }
}

impl std::error::Error for SettingsError {}
