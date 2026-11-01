//! The subset of JUnit XML the core reads.
//!
//! Implements `contracts/formats/test-results.md` v1.1.0. The contract is the
//! authority; section numbers below (`§3`, `D-5`) point into it.
//!
//! Bytes and the project root come in, and a list of test cases comes out. No
//! file is read and no lock line is looked at: the ticket of a test case is in the
//! lock line of its file, and the caller reads that (§5). Nothing here refuses a
//! test for what it found, either: which outcomes and criteria are acceptable is
//! the caller's decision (AT-8).

use std::fmt;
use std::path::Path;

use quick_xml::escape::resolve_predefined_entity;
use quick_xml::events::{BytesRef, BytesStart, Event};
use quick_xml::{Reader, XmlVersion};

#[cfg(test)]
mod tests;

/// How much of a failure's message is kept, in characters (§2.2).
const MESSAGE_LIMIT: usize = 2000;

/// What a test case's direct children say (§2.1). In order of precedence: the
/// greatest one present decides (D-5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Outcome {
    Passed,
    Skipped,
    /// An assertion failed.
    Failure,
    /// An exception: a missing class, a missing route (D-2).
    Error,
}

/// The acceptance criterion a test case's name claims (§4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Criterion {
    None,
    One(String),
    /// In the order first found.
    Several(Vec<String>),
}

/// One `<testcase>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestCase {
    pub name: String,
    /// Relative to the project root, with `/` separators (§3).
    pub file: String,
    pub outcome: Outcome,
    pub criterion: Criterion,
    /// The `type` of the child that decided the outcome (§2.2).
    pub failure_type: Option<String>,
    /// What that child says, trimmed and bounded (§2.2).
    pub message: Option<String>,
}

/// Why a report cannot be read (§6). **Missing** is not here: this takes bytes, so
/// a report that was never written is for the caller to report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JunitError {
    Empty,
    NotXml { reason: String },
    WrongRoot { found: String },
    NoName { line: usize },
    NoFile { name: String, line: usize },
    OutsideRoot { name: String, file: String },
}

impl fmt::Display for JunitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(
                f,
                "the report is empty, so the run wrote no results; runners leave an empty \
                 report when a test file cannot load, so look at the command's own output"
            ),
            Self::NotXml { reason } => write!(f, "the report is not well-formed XML: {reason}"),
            Self::WrongRoot { found } => write!(
                f,
                "the report's root element is `<{found}>`, and a JUnit report has \
                 `<testsuites>` or `<testsuite>`"
            ),
            Self::NoName { line } => write!(f, "line {line}: a `<testcase>` has no `name`"),
            Self::NoFile { name, line } => write!(
                f,
                "line {line}: test case `{name}` has no `file`, so there is no lock line to take \
                 its ticket from; configure the runner to write it"
            ),
            Self::OutsideRoot { name, file } => write!(
                f,
                "test case `{name}` is in `{file}`, which is not a file inside the project root; \
                 a runner in a container or behind a symlink sees another path"
            ),
        }
    }
}

impl std::error::Error for JunitError {}

/// An open element, and what it means for what is inside it.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Open {
    Suite,
    /// The last of the cases found so far.
    Case,
    /// The child that decided the last case's outcome: its text is the message.
    Message,
    /// Not read, and neither is anything inside it (§2).
    Ignored,
}

/// Read the test cases of a report, in document order. The first problem in the
/// document is the one reported (§6).
// `invar acceptance lock` is the caller (AT-8); remove this with its arrival.
#[allow(dead_code)]
pub fn read(bytes: &[u8], root: &Path) -> Result<Vec<TestCase>, JunitError> {
    let text = std::str::from_utf8(bytes).map_err(|err| JunitError::NotXml {
        reason: format!(
            "not valid UTF-8 (first invalid byte at offset {})",
            err.valid_up_to()
        ),
    })?;
    let text = text.strip_prefix('\u{FEFF}').unwrap_or(text);
    if text.trim().is_empty() {
        return Err(JunitError::Empty);
    }

    let root = root.to_string_lossy().replace('\\', "/");
    let root = resolve(&root);
    let line_of = |position: u64| {
        1 + text.as_bytes()[..position as usize]
            .iter()
            .filter(|byte| **byte == b'\n')
            .count()
    };
    let not_xml = |position: u64, what: String| JunitError::NotXml {
        reason: format!("line {}: {what}", line_of(position)),
    };

    let mut reader = Reader::from_str(text);
    let mut cases: Vec<TestCase> = Vec::new();
    let mut open: Vec<Open> = Vec::new();
    let mut root_seen = false;
    // Of the `Open::Message` that is open.
    let mut text = String::new();
    loop {
        // Text never sits between this position and a tag's own `<`: it is an event
        // of its own, so this is where the next tag starts.
        let start = reader.buffer_position();
        let event = reader
            .read_event()
            .map_err(|err| not_xml(reader.error_position(), err.to_string()))?;
        let (element, closes_at_once) = match event {
            Event::Start(element) => (element, false),
            Event::Empty(element) => (element, true),
            Event::End(_) => {
                if open.pop() == Some(Open::Message) {
                    let said = text.trim();
                    if let (false, Some(case)) = (said.is_empty(), cases.last_mut()) {
                        case.message = Some(said.to_owned());
                    }
                    text.clear();
                }
                continue;
            }
            Event::Eof => {
                if !open.is_empty() {
                    return Err(not_xml(
                        start,
                        "the report ends with an element open".into(),
                    ));
                }
                if !root_seen {
                    return Err(not_xml(start, "there is no element in the report".into()));
                }
                return Ok(cases);
            }
            // A DOCTYPE is skipped and nothing in it is resolved (§6).
            Event::Decl(_) | Event::PI(_) | Event::Comment(_) | Event::DocType(_) => continue,
            Event::Text(ref content) if open.is_empty() => {
                if content.bytes().all(|byte| byte.is_ascii_whitespace()) {
                    continue;
                }
                return Err(not_xml(start, "text outside the root element".into()));
            }
            Event::CData(_) | Event::GeneralRef(_) if open.is_empty() => {
                return Err(not_xml(start, "text outside the root element".into()));
            }
            // No other element's text is ever read (§2.1).
            Event::Text(_) | Event::CData(_) | Event::GeneralRef(_)
                if open.last() != Some(&Open::Message) =>
            {
                continue;
            }
            Event::Text(content) => {
                append(&mut text, &content.xml_content(XmlVersion::Implicit1_0));
                continue;
            }
            Event::CData(content) => {
                append(&mut text, &content.xml_content(XmlVersion::Implicit1_0));
                continue;
            }
            Event::GeneralRef(reference) => {
                append(&mut text, &reference_text(&reference));
                continue;
            }
        };

        let attributes = attributes_of(&element).map_err(|reason| not_xml(start, reason))?;
        let name = element.name();
        let tag: &str = name.as_ref();
        let frame = match open.last() {
            None if root_seen => {
                return Err(not_xml(start, "a second root element".into()));
            }
            None => {
                root_seen = true;
                if tag != "testsuites" && tag != "testsuite" {
                    return Err(JunitError::WrongRoot {
                        found: tag.to_owned(),
                    });
                }
                Open::Suite
            }
            Some(Open::Suite) => match tag {
                "testsuites" | "testsuite" => Open::Suite,
                "testcase" => {
                    cases.push(test_case(&attributes, &root, line_of(start))?);
                    Open::Case
                }
                _ => Open::Ignored,
            },
            Some(Open::Case) => {
                let outcome = match tag {
                    "failure" => Some(Outcome::Failure),
                    "error" => Some(Outcome::Error),
                    "skipped" => Some(Outcome::Skipped),
                    _ => None,
                };
                match (outcome, cases.last_mut()) {
                    (Some(outcome), Some(case)) if outcome > case.outcome => {
                        case.outcome = outcome;
                        let attribute = |key: &str| {
                            attributes
                                .iter()
                                .find(|(name, value)| name == key && !value.trim().is_empty())
                                .map(|(_, value)| value.as_str())
                        };
                        case.failure_type = attribute("type").map(str::to_owned);
                        case.message = attribute("message").map(|said| {
                            let mut bounded = String::new();
                            append(&mut bounded, said.trim());
                            bounded
                        });
                        Open::Message
                    }
                    _ => Open::Ignored,
                }
            }
            Some(Open::Message | Open::Ignored) => Open::Ignored,
        };
        if !closes_at_once {
            open.push(frame);
        }
    }
}

/// Every attribute of `element`, decoded. Done for all elements, read or not: this
/// is where a malformed attribute or an unknown entity shows (§6).
fn attributes_of(element: &BytesStart<'_>) -> Result<Vec<(String, String)>, String> {
    element
        .attributes()
        .map(|attribute| {
            let attribute = attribute.map_err(|err| err.to_string())?;
            let value = attribute
                .normalized_value(XmlVersion::Implicit1_0)
                .map_err(|err| err.to_string())?;
            let key: &str = attribute.key.as_ref();
            Ok((key.to_owned(), value.into_owned()))
        })
        .collect()
}

/// A `<testcase>` from its attributes, with the outcome its children have yet to set.
fn test_case(
    attributes: &[(String, String)],
    root: &Option<Vec<&str>>,
    line: usize,
) -> Result<TestCase, JunitError> {
    let value = |key: &str| {
        attributes
            .iter()
            .find(|(name, value)| name == key && !value.is_empty())
            .map(|(_, value)| value.as_str())
    };
    let name = value("name").ok_or(JunitError::NoName { line })?;
    let no_file = || JunitError::NoFile {
        name: name.to_owned(),
        line,
    };
    let raw = value("file").ok_or_else(no_file)?;
    let file = without_suffix(raw, name);
    if file.is_empty() {
        return Err(no_file());
    }
    Ok(TestCase {
        name: name.to_owned(),
        file: normalise(file, root).ok_or_else(|| JunitError::OutsideRoot {
            name: name.to_owned(),
            file: raw.to_owned(),
        })?,
        outcome: Outcome::Passed,
        criterion: criterion(name),
        failure_type: None,
        message: None,
    })
}

/// `more` added to `text`, up to [`MESSAGE_LIMIT`] characters in all.
fn append(text: &mut String, more: &str) {
    let room = MESSAGE_LIMIT.saturating_sub(text.chars().count());
    text.extend(more.chars().take(room));
}

/// What a reference in a message stands for. Never an error, because a message is
/// only ever shown, and never looked up in a DTD: anything else stays as written (§6).
fn reference_text(reference: &BytesRef<'_>) -> String {
    let name = reference.xml_content(XmlVersion::Implicit1_0);
    match reference.resolve_char_ref() {
        Ok(Some(character)) => character.to_string(),
        _ => match resolve_predefined_entity(&name) {
            Some(resolved) => resolved.to_owned(),
            None => format!("&{name};"),
        },
    }
}

/// `file` without `::<name>` at its end, and otherwise as it is (§3 step 1, D-6).
fn without_suffix<'a>(file: &'a str, name: &str) -> &'a str {
    file.strip_suffix(name)
        .and_then(|rest| rest.strip_suffix("::"))
        .unwrap_or(file)
}

/// `file` relative to the root, or `None` if it is not inside it (§3 steps 2 to 4).
fn normalise(file: &str, root: &Option<Vec<&str>>) -> Option<String> {
    let file = file.replace('\\', "/");
    let segments = resolve(&file)?;
    let relative = if is_absolute(&file) {
        segments.strip_prefix(root.as_ref()?.as_slice())?
    } else {
        segments.as_slice()
    };
    if relative.is_empty() {
        return None;
    }
    Some(relative.join("/"))
}

/// Starts with `/`, or with a drive such as `C:/`.
fn is_absolute(path: &str) -> bool {
    path.starts_with('/')
        || matches!(path.as_bytes(), [letter, b':', b'/', ..] if letter.is_ascii_alphabetic())
}

/// The segments of a `/`-separated path, by its text alone. `None` if a `..` climbs
/// above where the path starts.
fn resolve(path: &str) -> Option<Vec<&str>> {
    let mut segments = Vec::new();
    for segment in path.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                segments.pop()?;
            }
            _ => segments.push(segment),
        }
    }
    Some(segments)
}

/// The criterion tokens of a test's name (§4).
fn criterion(name: &str) -> Criterion {
    let mut found: Vec<String> = Vec::new();
    let chars: Vec<char> = name.chars().collect();
    for start in 0..chars.len() {
        if let Some(number) = token_at(&chars, start)
            && !found.iter().any(|id| id[2..] == number)
        {
            found.push(format!("AC{number}"));
        }
    }
    match found.len() {
        0 => Criterion::None,
        1 => Criterion::One(found.remove(0)),
        _ => Criterion::Several(found),
    }
}

/// The number of the token starting at `start`, if there is one.
fn token_at(chars: &[char], start: usize) -> Option<String> {
    let boundary = |c: Option<&char>| c.is_none_or(|c| !c.is_alphanumeric());
    if !boundary(start.checked_sub(1).and_then(|before| chars.get(before))) {
        return None;
    }
    let rest = chars.get(start..)?;
    let [a, c, rest @ ..] = rest else {
        return None;
    };
    if !a.eq_ignore_ascii_case(&'a') || !c.eq_ignore_ascii_case(&'c') {
        return None;
    }
    let rest = match rest {
        ['_' | '-', rest @ ..] => rest,
        _ => rest,
    };
    let length = rest.iter().take_while(|c| c.is_ascii_digit()).count();
    let number = &rest[..length];
    if !matches!(number.first(), Some('1'..='9')) || !boundary(rest.get(length)) {
        return None;
    }
    Some(number.iter().collect())
}
