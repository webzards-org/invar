//! `invar context show` (M1-14): the context the agents read (§5, §5a, §5c,
//! §7).
//!
//! Two outputs from the same sources:
//!
//! - **Binding context** — `AI_CONTEXT.md` and `BUSINESS_CONTEXT.md`, each
//!   labelled with its origin. Pushed into every session by a `SessionStart`
//!   hook (M1-17), so both agents receive it without deciding to.
//! - **Planning context** — the binding context plus the decision log's
//!   entries, newest first, within a budget. Only the planner receives it
//!   (§13a): the coder must not see the log (§7).
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
//! Entries are read newest first (decision-entry.md §9): those of the requested
//! module, then those of `global`. Without a module, the `global` entries only —
//! the planner learns which other modules have entries from one line at the end
//! and asks for one. The budget is **50 entries or about 4,000 tokens**,
//! whichever comes first, and is shared by both groups; an entry is never cut
//! in the middle, and older entries fall off the end (§7). Tokens are estimated
//! at four characters each: nothing here ships a tokenizer, and the budget is
//! "roughly" by its own definition.
//!
//! The log is **advisory** (§7): nothing in it binds, and nothing here fails
//! on what an entry says. An entry that cannot be parsed is skipped and
//! reported, because one hand-edited file must not hide the others.

use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::decision::{self, RecordError};
use crate::diagnostics::Diagnostics;
use crate::format::decision::{Entry, normalize_body};
use crate::format::values::{is_module, without_comments};
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
        self.files
            .iter()
            .filter(|file| file.state == State::Unrecognized)
            .map(|file| file.name)
            .collect()
    }

    /// The binding output: one block per loaded file, each labelled with where
    /// it comes from. Empty when nothing is loaded — absent, unrecognized and
    /// empty files all print nothing, so the first and the third cannot be told
    /// apart (§5).
    pub fn render(&self) -> String {
        let loaded: Vec<(&File, &str)> = self
            .files
            .iter()
            .filter_map(|file| match &file.state {
                State::Loaded(text) => Some((file, text.as_str())),
                _ => None,
            })
            .collect();
        if loaded.is_empty() {
            return String::new();
        }

        // The first line must not look like JSON: Claude Code examines the
        // first line of hook output to decide how to read the rest
        // (`plugin/NOTES.md` §9).
        let mut out = String::from(
            "=== invar: binding context ===\n\
             The team's own decisions for this project, delivered at the start of every \
             session. Follow them: a Plan or a change that contradicts them is wrong.\n",
        );
        for (file, text) in loaded {
            out.push_str(&format!(
                "\n=== {} — {} ===\n\n{text}\n\n=== end of {} ===\n",
                file.name, file.label, file.name
            ));
        }
        out
    }

    /// [`render`](Self::render), refused when it is longer than `limit`
    /// characters. The ceiling is on what is printed, labels included, because
    /// that is what the hook's 10,000-character cap counts.
    pub fn render_within(&self, limit: usize) -> Result<String, ContextError> {
        let text = self.render();
        let total = text.chars().count();
        if total <= limit {
            return Ok(text);
        }

        let files: Vec<FileSize> = self
            .files
            .iter()
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
    /// The module asked for, when it was one other than `global`.
    pub module: Option<String>,
    /// Entries that can be read, whatever was selected.
    pub total: usize,
    /// The selected entries within the budget, each already rendered.
    pub shown: Vec<String>,
    /// Selected entries that did not fit.
    pub omitted: usize,
    /// Other modules that have entries, with how many: what the planner could
    /// ask for next.
    pub other_modules: Vec<(String, usize)>,
    /// Files in the log that could not be read as entries, with why.
    pub unreadable: Vec<(PathBuf, Diagnostics)>,
}

/// Read the log for `module` (and always `global`), newest first, within the
/// budget.
pub fn load_decisions(root: &ProjectRoot, module: Option<&str>) -> Result<Decisions, ContextError> {
    if let Some(module) = module
        && !is_module(module)
    {
        return Err(ContextError::InvalidModule {
            module: module.to_owned(),
        });
    }
    // `global` is always shown after the module's entries; asking for it by
    // name must not show it twice.
    let wanted = module.filter(|module| *module != "global");

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

    // The log arrives newest first, and each filter keeps that order.
    let mut selected: Vec<&Entry> = Vec::new();
    if let Some(wanted) = wanted {
        selected.extend(entries.iter().filter(|entry| entry.module == wanted));
    }
    selected.extend(entries.iter().filter(|entry| entry.module == "global"));

    let mut shown = Vec::new();
    let mut used = 0;
    for entry in &selected {
        let text = render_entry(entry);
        let size = text.chars().count();
        if shown.len() >= MAX_ENTRIES || used + size > MAX_CHARS {
            break;
        }
        used += size;
        shown.push(text);
    }
    let omitted = selected.len() - shown.len();

    let mut others: BTreeMap<&str, usize> = BTreeMap::new();
    for entry in &entries {
        if entry.module != "global" && Some(entry.module.as_str()) != wanted {
            *others.entry(entry.module.as_str()).or_default() += 1;
        }
    }
    let other_modules = others
        .into_iter()
        .map(|(module, count)| (module.to_owned(), count))
        .collect();

    Ok(Decisions {
        module: wanted.map(str::to_owned),
        total: entries.len(),
        shown,
        omitted,
        other_modules,
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
    /// all; otherwise printed even when the selection is empty, so the planner
    /// learns where the entries are.
    pub fn render(&self) -> String {
        if self.total == 0 {
            return String::new();
        }
        let selection = match &self.module {
            Some(module) => format!("Showing the entries for module `{module}`, then `global`."),
            None => "Showing the `global` entries.".to_owned(),
        };
        let mut out = format!(
            "=== decision log — advisory, newest first ===\n{selection} What was built \
             before, and why. Nothing here binds: if this change goes against an entry, \
             say so (\"previously decided X — intentional?\") and carry on.\n"
        );
        if self.shown.is_empty() {
            out.push_str("\n(no entries for this selection)\n");
        }
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
        if !self.other_modules.is_empty() {
            let modules: Vec<String> = self
                .other_modules
                .iter()
                .map(|(module, count)| format!("{module} ({count})"))
                .collect();
            out.push_str(&format!(
                "\n(Other modules have entries too: {}. Ask for one with `invar context \
                 show --module <name>`.)\n",
                modules.join(", ")
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
    /// `--module` is not a module name.
    InvalidModule {
        module: String,
    },
    /// The binding output is longer than `context.binding_limit`.
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
            Self::InvalidModule { module } => write!(
                f,
                "`{module}` is not a module name; give a lowercase name such as `orders`, or \
                 `global`"
            ),
            Self::OverLimit(over) => {
                write!(
                    f,
                    "binding context is {} characters, over the limit of {} \
                     (`context.binding_limit` in invar.yaml):",
                    over.total, over.limit
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

impl std::error::Error for ContextError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Read { source, .. } => Some(source),
            Self::Log(err) => Some(err),
            Self::NotUtf8 { .. } | Self::InvalidModule { .. } | Self::OverLimit(_) => None,
        }
    }
}
