//! `invar context show` (M1-14): the context the agents read (§5, §5a, §5c,
//! §7).
//!
//! Three outputs from the same sources:
//!
//! - **Binding context** — `AI_CONTEXT.md` and `BUSINESS_CONTEXT.md`, each
//!   labelled with its origin. Pushed into the main session by a
//!   `SessionStart` hook (M1-17), which does not reach a sub-agent
//!   (`plugin/NOTES.md` §17).
//! - **Conventions** (M1-23) — `AI_CONTEXT.md` alone, labelled the same way.
//!   What the coder and the test-writer read (§5c, §13c): it describes how code
//!   is written, and they are the ones writing it. Never `BUSINESS_CONTEXT.md` and
//!   never a decision entry — they meet business rules only as the approved Plan
//!   states them.
//! - **Planning context** — the binding context plus the decision log's
//!   entries, newest first, within a budget. Only the planner receives it
//!   (§13a): the coder must not see the log (§7).
//!
//! The first two are the same rendering of a selection of the files
//! ([`Kind`]), so the marker, the empty-file rule and the limit below apply to
//! both identically, and neither can drift from the other.
//!
//! # The limit exists so that a failure is loud
//!
//! Hook output above 10,000 characters is moved into a file that Claude is not
//! asked to read (`plugin/NOTES.md` §5). Binding context that grew past that
//! would stop arriving while everything looked fine. So the binding output has
//! a ceiling (`context.binding_limit`), measured on **exactly what is
//! printed** — labels and framing included, counted in characters, which is
//! what Claude Code counts — and exceeding it is an error that names both
//! files and their sizes: the team trims or splits now, instead of finding out
//! months later that the invariants were never in context.
//!
//! # What a context file is
//!
//! | State | Meaning | Output | Reported |
//! |---|---|---|---|
//! | absent | no such file | nothing | no |
//! | unrecognized | the file exists, and its first non-blank line is not the marker | nothing — it is not loaded | **yes**, on stderr |
//! | empty | the marker, then only blank lines, headings and comments | nothing | no |
//! | loaded | the marker, then anything else | the text after the marker | no |
//!
//! **The marker is `<!-- invar-context: v1 -->`, and it has to be the first
//! non-blank line** (a BOM and CRLF are fine). A marker further down would let
//! any document that merely quotes it be loaded as binding context. An
//! unrecognized file is reported rather than loaded, and rather than ignored
//! quietly: a team that wrote `BUSINESS_CONTEXT.md` without the marker believes
//! its invariants are being delivered.
//!
//! **An empty file behaves exactly like an absent one** (§5): `init` scaffolds
//! the two files with the marker and their section headings, and writing them
//! must not switch anything on that the team never asked for. Absent and empty
//! produce the same output on both stdout and stderr.
//!
//! # The decision log
//!
//! **Every** entry is shown, newest first (decision-entry.md §9), whatever its
//! module: for the MVP the planner reads all the facts rather than a selection
//! (the module is only a label in each entry's header line). The budget is
//! **50 entries or about 4,000 tokens**, whichever comes first; an entry is
//! never cut in the middle, and older entries fall off the end (§7), with a
//! line saying so. Tokens are estimated
//! at four characters each: nothing here ships a tokenizer, and the budget is
//! "roughly" by its own definition.
//!
//! The log is **advisory** (§7): nothing in it binds, and nothing here fails
//! on what an entry says. An entry that cannot be parsed is skipped and
//! reported, because one hand-edited file must not hide the others.

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::decision::{self, RecordError};
use crate::diagnostics::Diagnostics;
use crate::format::decision::{Entry, normalize_body};
use crate::format::values::without_comments;
use crate::fs::ProjectRoot;

#[cfg(test)]
mod tests;

/// The first non-blank line of every context file `invar` reads.
pub const MARKER: &str = "<!-- invar-context: v1 -->";

/// The most decision entries one output carries (§7).
pub const MAX_ENTRIES: usize = 50;

/// The character budget for decision entries: about 4,000 tokens at four
/// characters each (§7).
pub const MAX_CHARS: usize = 16_000;

// --- binding context ---------------------------------------------------------------

/// A context file `invar` reads, in the order it is printed.
struct Spec {
    name: &'static str,
    /// What the file is and why it binds, for its label.
    label: &'static str,
}

const FILES: [Spec; 2] = [
    Spec {
        name: "AI_CONTEXT.md",
        label: "this project's own conventions; binding",
    },
    Spec {
        name: "BUSINESS_CONTEXT.md",
        label: "business rules the application must obey; binding",
    },
];

/// What was found at a context file's path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum State {
    Absent,
    /// There, but its first non-blank line is not the marker. Not loaded.
    Unrecognized,
    /// The marker and nothing worth delivering: behaves like [`State::Absent`].
    Empty,
    /// The text after the marker, trimmed of surrounding blank lines.
    Loaded(String),
}

#[derive(Debug)]
pub struct File {
    pub name: &'static str,
    pub label: &'static str,
    pub state: State,
}

/// Which of the two outputs built from the context files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Both files: `context show --binding`, what every session is pushed.
    Binding,
    /// `AI_CONTEXT.md` alone: `context show --conventions`, what the coder
    /// reads (M1-23).
    Conventions,
}

impl Kind {
    /// Whether the file called `name` belongs in this output. A selection by
    /// name, so a third context file added to [`FILES`] reaches the coder only
    /// if this says so.
    fn includes(self, name: &str) -> bool {
        match self {
            Self::Binding => true,
            Self::Conventions => name == "AI_CONTEXT.md",
        }
    }

    /// What the output is called in a message: "binding context is 9,412
    /// characters".
    pub fn noun(self) -> &'static str {
        match self {
            Self::Binding => "binding context",
            Self::Conventions => "the conventions output",
        }
    }

    /// The first lines of the output. The first line must not look like JSON:
    /// Claude Code examines the first line of hook output to decide how to read
    /// the rest (`plugin/NOTES.md` §9).
    fn header(self) -> &'static str {
        match self {
            Self::Binding => {
                "=== invar: binding context ===\n\
                 The team's own decisions for this project, delivered at the start of every \
                 session. Follow them: a Plan or a change that contradicts them is wrong.\n"
            }
            Self::Conventions => {
                "=== invar: conventions ===\n\
                 How code is written in this project. Follow it in everything you write. \
                 The rules of the business reach you through the approved Plan, not here.\n"
            }
        }
    }
}

/// Both context files, as found.
#[derive(Debug)]
pub struct Binding {
    pub files: Vec<File>,
}

/// Read both context files.
pub fn load_binding(root: &ProjectRoot) -> Result<Binding, ContextError> {
    let mut files = Vec::with_capacity(FILES.len());
    for spec in &FILES {
        files.push(File {
            name: spec.name,
            label: spec.label,
            state: read_state(&root.path().join(spec.name))?,
        });
    }
    Ok(Binding { files })
}

/// A file that is not there is [`State::Absent`]. Any other reason it cannot
/// be read is an error: "cannot tell" must not be taken for "not there", or a
/// permissions problem would quietly drop the invariants.
fn read_state(path: &Path) -> Result<State, ContextError> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(State::Absent),
        Err(source) => {
            return Err(ContextError::Read {
                path: path.to_owned(),
                source,
            });
        }
    };
    let text = std::str::from_utf8(&bytes).map_err(|err| ContextError::NotUtf8 {
        path: path.to_owned(),
        valid_up_to: err.valid_up_to(),
    })?;
    Ok(classify(text))
}

/// Which state a file's text is in.
fn classify(text: &str) -> State {
    let text = text.strip_prefix('\u{FEFF}').unwrap_or(text);
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    let lines: Vec<&str> = text.split('\n').collect();

    let Some(first) = lines.iter().position(|line| !line.trim().is_empty()) else {
        return State::Unrecognized;
    };
    if lines[first].trim() != MARKER {
        return State::Unrecognized;
    }

    let rest = lines[first + 1..].join("\n");
    if has_content(&rest) {
        State::Loaded(normalize_body(&rest))
    } else {
        State::Empty
    }
}

/// Is there anything after the marker that is not a blank line, a Markdown
/// heading or a comment? The three are what `init` writes (§5), so a freshly
/// scaffolded file is empty, and the first sentence a team adds makes it
/// loaded.
fn has_content(text: &str) -> bool {
    let mut in_comment = false;
    text.lines().any(|line| {
        let visible = without_comments(line, &mut in_comment);
        let visible = visible.trim();
        !visible.is_empty() && !is_heading(visible)
    })
}

/// An ATX heading: one to six `#`, then a space or the end of the line.
/// `#hashtag` is text.
fn is_heading(line: &str) -> bool {
    let hashes = line.bytes().take_while(|byte| *byte == b'#').count();
    (1..=6).contains(&hashes)
        && line[hashes..]
            .chars()
            .next()
            .is_none_or(char::is_whitespace)
}

impl Binding {
    /// The names of the files that exist but are not recognized, so the caller
    /// can say so (stderr, never stdout: stdout is what the agent reads).
    pub fn unrecognized(&self) -> Vec<&'static str> {
        self.unrecognized_in(Kind::Binding)
    }

    /// [`unrecognized`](Self::unrecognized), among the files `kind` carries:
    /// `--conventions` has nothing to say about `BUSINESS_CONTEXT.md`.
    pub fn unrecognized_in(&self, kind: Kind) -> Vec<&'static str> {
        self.files_in(kind)
            .filter(|file| file.state == State::Unrecognized)
            .map(|file| file.name)
            .collect()
    }

    fn files_in(&self, kind: Kind) -> impl Iterator<Item = &File> {
        self.files
            .iter()
            .filter(move |file| kind.includes(file.name))
    }

    /// The binding output: one block per loaded file, each labelled with where
    /// it comes from. Empty when nothing is loaded — absent, unrecognized and
    /// empty files all print nothing, so the first and the third cannot be told
    /// apart (§5).
    pub fn render(&self) -> String {
        self.render_as(Kind::Binding)
    }

    /// [`render`](Self::render) of the files `kind` carries, under its own
    /// header. Empty when none of them is loaded.
    pub fn render_as(&self, kind: Kind) -> String {
        let loaded: Vec<(&File, &str)> = self
            .files_in(kind)
            .filter_map(|file| match &file.state {
                State::Loaded(text) => Some((file, text.as_str())),
                _ => None,
            })
            .collect();
        if loaded.is_empty() {
            return String::new();
        }

        let mut out = String::from(kind.header());
        for (file, text) in loaded {
            out.push_str(&format!(
                "\n=== {} — {} ===\n\n{text}\n\n=== end of {} ===\n",
                file.name, file.label, file.name
            ));
        }
        out
    }

    /// [`render_as`](Self::render_as), refused when it is longer than `limit`
    /// characters, with the sizes of the files `kind` carries. The ceiling is
    /// on what is printed, labels included, because that is what the hook's
    /// 10,000-character cap counts.
    pub fn render_as_within(&self, kind: Kind, limit: usize) -> Result<String, ContextError> {
        let text = self.render_as(kind);
        let total = text.chars().count();
        if total <= limit {
            return Ok(text);
        }

        let files: Vec<FileSize> = self
            .files_in(kind)
            .map(|file| {
                let (chars, note) = match &file.state {
                    State::Loaded(text) => (text.chars().count(), ""),
                    State::Absent => (0, " (absent)"),
                    State::Empty => (0, " (empty)"),
                    State::Unrecognized => (0, " (not recognized, so not loaded)"),
                };
                FileSize {
                    name: file.name,
                    chars,
                    note,
                }
            })
            .collect();
        let in_files: usize = files.iter().map(|file| file.chars).sum();
        Err(ContextError::OverLimit(OverLimit {
            what: kind.noun(),
            total,
            limit,
            framing: total - in_files,
            files,
        }))
    }
}

/// One file's share of an output that is over the limit.
#[derive(Debug)]
pub struct FileSize {
    pub name: &'static str,
    pub chars: usize,
    pub note: &'static str,
}

#[derive(Debug)]
pub struct OverLimit {
    /// Which output: [`Kind::noun`].
    pub what: &'static str,
    pub total: usize,
    pub limit: usize,
    /// The labels and framing the output adds around the files.
    pub framing: usize,
    pub files: Vec<FileSize>,
}

// --- the decision log ------------------------------------------------------------------

/// The decision entries a planner is shown.
#[derive(Debug)]
pub struct Decisions {
    /// Entries that can be read.
    pub total: usize,
    /// The entries within the budget, each already rendered.
    pub shown: Vec<String>,
    /// Entries that did not fit.
    pub omitted: usize,
    /// Files in the log that could not be read as entries, with why.
    pub unreadable: Vec<(PathBuf, Diagnostics)>,
}

/// Read the whole log, newest first, within the budget.
pub fn load_decisions(root: &ProjectRoot) -> Result<Decisions, ContextError> {
    let mut entries = Vec::new();
    let mut unreadable = Vec::new();
    for item in decision::log(root)
        .map_err(ContextError::Log)?
        .unwrap_or_default()
    {
        match item.entry {
            Ok(entry) => entries.push(entry),
            Err(problems) => unreadable.push((item.path, problems)),
        }
    }

    // The log arrives newest first.
    let mut shown = Vec::new();
    let mut used = 0;
    for entry in &entries {
        let text = render_entry(entry);
        let size = text.chars().count();
        if shown.len() >= MAX_ENTRIES || used + size > MAX_CHARS {
            break;
        }
        used += size;
        shown.push(text);
    }
    let omitted = entries.len() - shown.len();

    Ok(Decisions {
        total: entries.len(),
        shown,
        omitted,
        unreadable,
    })
}

/// One entry as the planner reads it: a header line, then the text.
fn render_entry(entry: &Entry) -> String {
    format!(
        "[{}] {} · {} · {} · {}\n{}\n",
        entry.date, entry.kind, entry.ticket, entry.module, entry.id, entry.body
    )
}

impl Decisions {
    /// The decision log block. Empty when the log holds no readable entry at
    /// all.
    pub fn render(&self) -> String {
        if self.total == 0 {
            return String::new();
        }
        let mut out = String::from(
            "=== decision log — advisory, newest first ===\nWhat was built before, and \
             why. Nothing here binds: if this change goes against an entry, say so \
             (\"previously decided X — intentional?\") and carry on.\n",
        );
        for entry in &self.shown {
            out.push('\n');
            out.push_str(entry);
        }
        if self.omitted > 0 {
            out.push_str(&format!(
                "\n({} older {} not shown: the limit is {MAX_ENTRIES} entries or about 4000 \
                 tokens.)\n",
                self.omitted,
                if self.omitted == 1 {
                    "entry is"
                } else {
                    "entries are"
                }
            ));
        }
        out.push_str("\n=== end of decision log ===\n");
        out
    }
}

/// What the planner reads: the binding context, then the decision log, with a
/// blank line between them. Empty when there is neither.
pub fn planner_output(binding: &Binding, decisions: &Decisions) -> String {
    let binding = binding.render();
    let decisions = decisions.render();
    match (binding.is_empty(), decisions.is_empty()) {
        (true, true) => String::new(),
        (false, true) => binding,
        (true, false) => decisions,
        (false, false) => format!("{binding}\n{decisions}"),
    }
}

// --- errors ---------------------------------------------------------------------------------

#[derive(Debug)]
pub enum ContextError {
    /// A context file exists and cannot be read.
    Read {
        path: PathBuf,
        source: io::Error,
    },
    NotUtf8 {
        path: PathBuf,
        valid_up_to: usize,
    },
    /// The decision log could not be listed.
    Log(RecordError),
    /// The binding or conventions output is longer than
    /// `context.binding_limit`.
    OverLimit(OverLimit),
}

impl fmt::Display for ContextError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read { path, source } => write!(f, "cannot read {}: {source}", path.display()),
            Self::NotUtf8 { path, valid_up_to } => write!(
                f,
                "{} is not valid UTF-8 (first invalid byte at offset {valid_up_to}); a context \
                 file is read as text — save it as UTF-8",
                path.display()
            ),
            Self::Log(err) => write!(f, "{err}"),
            Self::OverLimit(over) => {
                write!(
                    f,
                    "{} is {} characters, over the limit of {} \
                     (`context.binding_limit` in invar.yaml):",
                    over.what, over.total, over.limit
                )?;
                for file in &over.files {
                    write!(
                        f,
                        "\n  {:<20} {} characters{}",
                        file.name, file.chars, file.note
                    )?;
                }
                write!(
                    f,
                    "\n  {:<20} {} characters\n\
                     Claude Code moves hook output above 10,000 characters into a file the agent \
                     is not asked to read, so binding context that is over the limit would stop \
                     arriving without a sign. Trim or split the files; do not raise the limit \
                     to fit them",
                    "labels and framing", over.framing
                )
            }
        }
    }
}

// No `source()`: every variant's message already carries its cause, and
// `main` prints the whole chain, so exposing it would print it twice.
impl std::error::Error for ContextError {}
