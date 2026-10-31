//! `invar.yaml` — the project config (§6).
//!
//! Implements `contracts/formats/invar-yaml.md` v2.2.0. The contract is the
//! authority; section numbers below (`§3.4`, `D-2`) point into it.
//!
//! Light mode reads `scenarios`, `review.offer`, `git.base_branch`,
//! `context.binding_limit`, `done.commit`, `done.commit_message` and the
//! `acceptance` section. Everything else in a config
//! is either a key of a later milestone, ignored silently (contract §5), or an
//! unknown key, which produces a [`Warning`] and never a failure (§6) — a
//! config written by a newer `invar` must not break an older one.
//!
//! Reading and refusing are separate on purpose (D-4). [`load`] accepts
//! `scenarios: true`, because the file is valid; [`load_for_command`] refuses
//! it, because no command in this version supports `.feature` files. **Every
//! command that reads the config goes through [`load_for_command`].**
//!
//! The file is read, never written back, which is why a YAML library is
//! acceptable here and not for the Plan header (D-1).

use std::collections::BTreeMap;
use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde::de::{self, IgnoredAny};

use crate::fs::ProjectRoot;

#[cfg(test)]
mod tests;

/// The config file's name. Its directory is the project root, which
/// [`crate::fs`] finds and builds every path from (M1-4).
pub const FILE_NAME: &str = "invar.yaml";

/// Default for `context.binding_limit` (§3.4).
pub const DEFAULT_BINDING_LIMIT: usize = 9_000;

/// Upper bound for `context.binding_limit`: Claude Code's hook output cap
/// (§3.4, D-5).
pub const MAX_BINDING_LIMIT: usize = 10_000;

/// Default for `done.commit_message` (§3.5). `{ticket}` is replaced by the
/// Plan's ticket id.
pub const DEFAULT_DONE_COMMIT_MESSAGE: &str = "Remove plan {ticket}";

/// The placeholder `done.commit_message` may contain (§3.5).
pub const TICKET_PLACEHOLDER: &str = "{ticket}";

/// The placeholder in `acceptance.run` for the report's path (§3.6).
pub const REPORT_PLACEHOLDER: &str = "{report}";

/// The placeholder in `acceptance.run` for the test files (§3.6).
pub const FILES_PLACEHOLDER: &str = "{files}";

/// The smallest `acceptance` section, for a message that says what to add (§3.6).
pub const ACCEPTANCE_SECTION_EXAMPLE: &str = "acceptance:\n  \
     dir: tests/Acceptance\n  \
     run: [\"vendor/bin/pest\", \"--log-junit\", \"{report}\", \"{files}\"]";

/// Top-level keys of §6 that belong to later milestones (§5). Ignored
/// silently, together with everything under them.
const IGNORED_TOP_LEVEL: &[&str] = &[
    "language",
    "framework",
    "infrastructure",
    "pipeline",
    "lint",
    "exclude",
];

/// Keys under `context` that belong to later milestones (§5).
const IGNORED_IN_CONTEXT: &[&str] = &["sync_on_plan", "business_context_ref"];

// --- the config -------------------------------------------------------------

/// The config as M1 reads it, with every default applied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    /// `scenarios` — the Plan shape (§3.1). `false` is light mode.
    pub scenarios: bool,
    pub review: Review,
    pub git: Git,
    pub context: Context,
    pub done: Done,
    /// The `acceptance` section (§3.6). `None`: absent, so no acceptance tests.
    pub acceptance: Option<AcceptanceConfig>,
}

/// The `review` section (§3.2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Review {
    /// `review.offer` — whether the protocol skill offers `/invar:review`.
    /// `true` unless the project says otherwise, since 2.0.0 (D-8).
    pub offer: bool,
}

impl Default for Review {
    fn default() -> Self {
        Self { offer: true }
    }
}

/// The `git` section (§3.3).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Git {
    /// `git.base_branch`. `None` means detect it: `origin/HEAD`, else `main`.
    pub base_branch: Option<String>,
}

/// The `context` section (§3.4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Context {
    /// `context.binding_limit` — a character count, 1 to 10,000.
    pub binding_limit: usize,
}

impl Default for Context {
    fn default() -> Self {
        Self {
            binding_limit: DEFAULT_BINDING_LIMIT,
        }
    }
}

/// The `done` section (§3.5): what `invar done` does with the deletion when
/// the command line does not say.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Done {
    /// `done.commit` — commit the deletion, alone, as `--commit` would.
    /// `false` unless the project says otherwise: invar makes no commit unless
    /// asked, and this key is how a project asks once for every cycle.
    pub commit: bool,
    /// `done.commit_message` — the message of that commit, with `{ticket}`
    /// replaced by the Plan's ticket id. One non-empty line.
    pub commit_message: String,
}

impl Done {
    /// The commit message for the Plan of `ticket`.
    pub fn message_for(&self, ticket: &str) -> String {
        self.commit_message.replace(TICKET_PLACEHOLDER, ticket)
    }
}

impl Default for Done {
    fn default() -> Self {
        Self {
            commit: false,
            commit_message: DEFAULT_DONE_COMMIT_MESSAGE.to_owned(),
        }
    }
}

/// The `acceptance` section (§3.6): how this project runs its acceptance tests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcceptanceConfig {
    /// `acceptance.required` — the default for `plan approve`. `true` unless the
    /// project says otherwise (D-10).
    pub required: bool,
    pub style: AcceptanceStyle,
    /// `acceptance.dir` — relative to the project root. Whether it exists is not
    /// checked here.
    pub dir: String,
    /// `acceptance.run` — element 0 is the program, the rest its arguments, with
    /// no shell (D-11). Holds each placeholder exactly once.
    pub run: Vec<String>,
    /// `acceptance.fakes_allowed` — not interpreted by the core.
    pub fakes_allowed: Vec<String>,
}

/// `acceptance.style` (§3.6). `gherkin` is reserved, and an error for now.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum AcceptanceStyle {
    #[default]
    Integration,
}

impl AcceptanceStyle {
    /// As it is written in `invar.yaml`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Integration => "integration",
        }
    }
}

impl Default for Config {
    /// What an empty `invar.yaml` means (§2, example 10.2).
    fn default() -> Self {
        Self {
            scenarios: false,
            review: Review::default(),
            git: Git::default(),
            context: Context::default(),
            done: Done::default(),
            acceptance: None,
        }
    }
}

/// A config that was read successfully, with what the caller should show.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Loaded {
    /// The file it came from, for messages.
    pub path: PathBuf,
    pub config: Config,
    /// Unknown keys (§6). Callers print them to stderr as
    /// `warning: <path>: <warning>` and carry on.
    pub warnings: Vec<Warning>,
}

impl Loaded {
    /// Refuse a config this version cannot act on (§3.1, D-4).
    ///
    /// The reader accepts `scenarios: true` because the file is valid; a
    /// command must not run on it, because `.feature` files do not exist yet.
    pub fn require_light_mode(self) -> Result<Self, ConfigError> {
        if self.config.scenarios {
            return Err(ConfigError::ScenariosNotSupported { path: self.path });
        }
        Ok(self)
    }
}

/// An unknown key: neither read by M1 nor a §6 key of a later milestone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Warning {
    /// The full dotted path: `senarios`, `review.ofer`.
    pub key: String,
}

impl fmt::Display for Warning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "unknown key `{}` is ignored — check it for a typo; a key added by \
             a newer version of invar is also reported this way",
            self.key
        )
    }
}

// --- errors -----------------------------------------------------------------

/// Why the config could not be used. Each failure of contract §7 is its own
/// variant, so a caller can tell them apart.
#[derive(Debug)]
pub enum ConfigError {
    /// No `invar.yaml` where one was expected.
    Missing { path: PathBuf },
    /// The file exists but cannot be read.
    Unreadable { path: PathBuf, source: io::Error },
    /// The file was read and is not a valid config.
    Invalid { path: PathBuf, reason: String },
    /// `scenarios: true`, refused at command time (§3.1).
    ScenariosNotSupported { path: PathBuf },
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Missing { path } => write!(
                f,
                "{} does not exist; run `invar init` in the project root to create it",
                path.display()
            ),
            Self::Unreadable { path, source } => {
                write!(f, "cannot read {}: {source}", path.display())
            }
            Self::Invalid { path, reason } => write!(f, "{}: {reason}", path.display()),
            Self::ScenariosNotSupported { path } => write!(
                f,
                "{}: `scenarios: true` selects the .feature workflow, and .feature \
                 files arrive in a later milestone of invar. This version supports \
                 light mode only: set `scenarios: false`, or remove the key",
                path.display()
            ),
        }
    }
}

impl std::error::Error for ConfigError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Unreadable { source, .. } => Some(source),
            _ => None,
        }
    }
}

// --- reading ----------------------------------------------------------------

/// Read `invar.yaml` from the project root and refuse `scenarios: true`.
///
/// The entry point for every command (§3.1). Use [`load`] only where the
/// mode does not matter.
pub fn load_for_command(root: &ProjectRoot) -> Result<Loaded, ConfigError> {
    load(root)?.require_light_mode()
}

/// Read `invar.yaml` from the project root. Accepts `scenarios: true`.
///
/// The root was found *because* the file is there, so `Missing` means it was
/// removed in between — rare, but reported as what it is.
pub fn load(root: &ProjectRoot) -> Result<Loaded, ConfigError> {
    let path = root.config_file();
    let bytes = std::fs::read(&path).map_err(|source| match source.kind() {
        io::ErrorKind::NotFound => ConfigError::Missing { path: path.clone() },
        _ => ConfigError::Unreadable {
            path: path.clone(),
            source,
        },
    })?;
    parse_bytes(&bytes, &path)
}

/// Parse the bytes of a config file. `path` is used for messages only.
pub fn parse_bytes(bytes: &[u8], path: &Path) -> Result<Loaded, ConfigError> {
    let text = std::str::from_utf8(bytes).map_err(|err| ConfigError::Invalid {
        path: path.to_owned(),
        reason: format!(
            "not valid UTF-8 (first invalid byte at offset {})",
            err.valid_up_to()
        ),
    })?;
    parse(text, path)
}

/// Parse the text of a config file. `path` is used for messages only.
pub fn parse(text: &str, path: &Path) -> Result<Loaded, ConfigError> {
    let invalid = |reason: String| ConfigError::Invalid {
        path: path.to_owned(),
        reason,
    };

    // The parser handles a leading BOM itself; stripping it here anyway keeps
    // §2 true no matter which parser sits underneath.
    let text = text.strip_prefix('\u{FEFF}').unwrap_or(text);

    let options = serde_saphyr::options! {
        // `no`, `off`, `y`, `1` are errors, never booleans (§4, D-6).
        strict_booleans: true,
        // A plain value that YAML resolves to a number, bool or null is not a
        // string, whatever the target type (§4, D-7).
        no_schema: true,
        // The location goes into our own message; the multi-line source
        // excerpt would not survive the CLI's one-line error output.
        with_snippet: false,
    };

    // `None`: an empty document, or one holding only comments (§2).
    let raw: Option<RawConfig> = serde_saphyr::from_str_with_options(text, options)
        .map_err(|err| invalid(yaml_message(&err)))?;
    let raw = raw.unwrap_or_default();

    let mut warnings = Vec::new();
    let mut problems = Vec::new();

    unknown_keys(&raw.other, "", IGNORED_TOP_LEVEL, &mut warnings);

    let review = raw.review.unwrap_or_default();
    unknown_keys(&review.other, "review.", &[], &mut warnings);

    let git = raw.git.unwrap_or_default();
    unknown_keys(&git.other, "git.", &[], &mut warnings);
    if let Some(branch) = &git.base_branch
        && (branch.is_empty() || branch.chars().any(char::is_whitespace))
    {
        problems.push(format!(
            "`git.base_branch` must be a branch name without whitespace, found {branch:?}; \
             remove the key to detect the base branch automatically"
        ));
    }

    let context = raw.context.unwrap_or_default();
    unknown_keys(
        &context.other,
        "context.",
        IGNORED_IN_CONTEXT,
        &mut warnings,
    );
    let binding_limit = match context.binding_limit {
        None => DEFAULT_BINDING_LIMIT,
        Some(limit) => match usize::try_from(limit) {
            Ok(limit @ 1..=MAX_BINDING_LIMIT) => limit,
            _ => {
                problems.push(format!(
                    "`context.binding_limit` must be between 1 and {MAX_BINDING_LIMIT}, \
                     found {limit}: Claude Code moves hook output above \
                     {MAX_BINDING_LIMIT} characters to a file the agent is not asked \
                     to read, so binding context would silently stop arriving. Trim \
                     or split the context files instead of raising the limit"
                ));
                DEFAULT_BINDING_LIMIT
            }
        },
    };

    let done = raw.done.unwrap_or_default();
    unknown_keys(&done.other, "done.", &[], &mut warnings);
    let commit_message = match done.commit_message {
        None => DEFAULT_DONE_COMMIT_MESSAGE.to_owned(),
        Some(message) => {
            let trimmed = message.trim();
            if trimmed.is_empty() || trimmed.contains(['\n', '\r']) {
                problems.push(format!(
                    "`done.commit_message` must be one non-empty line, found {message:?}; \
                     `{TICKET_PLACEHOLDER}` stands for the ticket id. Remove the key for \
                     the default, {DEFAULT_DONE_COMMIT_MESSAGE:?}"
                ));
                DEFAULT_DONE_COMMIT_MESSAGE.to_owned()
            } else {
                trimmed.to_owned()
            }
        }
    };

    let acceptance = raw
        .acceptance
        .filter(|section| !section.is_empty())
        .and_then(|section| {
            unknown_keys(&section.other, "acceptance.", &[], &mut warnings);
            acceptance_section(section, &mut problems)
        });

    // Every value problem in one message, so one edit fixes the file (§7).
    if !problems.is_empty() {
        return Err(invalid(problems.join("; ")));
    }

    Ok(Loaded {
        path: path.to_owned(),
        config: Config {
            scenarios: raw.scenarios.unwrap_or(false),
            review: Review {
                offer: review.offer.unwrap_or(Review::default().offer),
            },
            git: Git {
                base_branch: git.base_branch,
            },
            context: Context { binding_limit },
            done: Done {
                commit: done.commit.unwrap_or(false),
                commit_message,
            },
            acceptance,
        },
        warnings,
    })
}

/// The `acceptance` section with its defaults applied, or `None` with every
/// problem in it added to `problems` (§3.6).
fn acceptance_section(
    section: RawAcceptance,
    problems: &mut Vec<String>,
) -> Option<AcceptanceConfig> {
    let before = problems.len();

    match section.style.as_deref() {
        None | Some("integration") => {}
        Some("gherkin") => problems.push(
            "`acceptance.style: gherkin` is reserved, and arrives in a later version of \
             invar; use `integration`, or remove the key"
                .to_owned(),
        ),
        Some(other) => problems.push(format!(
            "`acceptance.style` must be `integration` (`gherkin` is reserved), found {other:?}"
        )),
    }

    match &section.dir {
        None => problems.push(
            "`acceptance.dir` is required: the directory of the acceptance tests, relative \
             to the project root"
                .to_owned(),
        ),
        Some(dir) => {
            if let Some(reason) = dir_problem(dir) {
                problems.push(format!(
                    "`acceptance.dir` must be a path relative to the project root, found \
                     {dir:?}: {reason}"
                ));
            }
        }
    }

    match &section.run {
        None => problems.push(
            "`acceptance.run` is required: a list with the program first, such as \
             [\"vendor/bin/pest\", \"--log-junit\", \"{report}\", \"{files}\"]"
                .to_owned(),
        ),
        Some(RawRun::Text) => problems.push(
            "`acceptance.run` is a list, not a string: write the program and each argument \
             as its own element, such as \
             [\"vendor/bin/pest\", \"--log-junit\", \"{report}\", \"{files}\"] \
             (invar-yaml.md §3.6)"
                .to_owned(),
        ),
        Some(RawRun::List(run)) => run_problems(run, problems),
    }

    let fakes_allowed = section.fakes_allowed.unwrap_or_default();
    if fakes_allowed.iter().any(|fake| fake.trim().is_empty()) {
        problems.push("`acceptance.fakes_allowed` must hold non-empty strings".to_owned());
    }

    if problems.len() > before {
        return None;
    }
    Some(AcceptanceConfig {
        required: section.required.unwrap_or(true),
        style: AcceptanceStyle::Integration,
        dir: section.dir?,
        run: match section.run? {
            RawRun::List(run) => run,
            RawRun::Text => return None,
        },
        fakes_allowed,
    })
}

/// What is wrong with `dir`, if anything. A backslash counts as a separator, so the
/// rule does not depend on the platform the config is read on.
fn dir_problem(dir: &str) -> Option<&'static str> {
    let drive = matches!(dir.as_bytes(), [letter, b':', ..] if letter.is_ascii_alphabetic());
    if dir.trim().is_empty() {
        Some("it is empty")
    } else if dir.starts_with(['/', '\\']) || drive {
        Some("it is absolute")
    } else if dir.ends_with(['/', '\\']) {
        Some("remove the trailing separator")
    } else if dir.split(['/', '\\']).any(|segment| segment == "..") {
        Some("it has a `..` segment")
    } else {
        None
    }
}

/// Add what is wrong with `acceptance.run` to `problems` (§3.6, D-11).
fn run_problems(run: &[String], problems: &mut Vec<String>) {
    let Some(program) = run.first() else {
        problems.push("`acceptance.run` must have at least one element: the program".to_owned());
        return;
    };
    if program.trim().is_empty() || program == REPORT_PLACEHOLDER || program == FILES_PLACEHOLDER {
        problems.push(format!(
            "`acceptance.run` starts with the program to run, found {program:?}"
        ));
    }

    let mut unknown = Vec::new();
    for placeholder in run.iter().flat_map(|element| placeholders(element)) {
        if placeholder != REPORT_PLACEHOLDER
            && placeholder != FILES_PLACEHOLDER
            && !unknown.contains(&placeholder)
        {
            unknown.push(placeholder);
        }
    }
    for placeholder in unknown {
        problems.push(format!(
            "`acceptance.run` has the unknown placeholder `{placeholder}`; the placeholders \
             are `{REPORT_PLACEHOLDER}` and `{FILES_PLACEHOLDER}`"
        ));
    }

    for placeholder in [REPORT_PLACEHOLDER, FILES_PLACEHOLDER] {
        let whole = run.iter().filter(|element| *element == placeholder).count();
        let inside = run
            .iter()
            .any(|element| element != placeholder && element.contains(placeholder));
        if whole != 1 || inside {
            let found = if inside {
                "it is part of a longer element".to_owned()
            } else {
                format!("found it {whole} times")
            };
            problems.push(format!(
                "`acceptance.run` must have `{placeholder}` exactly once, as an element of \
                 its own; {found}"
            ));
        }
    }
}

/// Every placeholder in `element`: `{`, then letters, digits, `_` or `-`, then
/// `}`. Any other brace is a literal one (§3.6).
fn placeholders(element: &str) -> Vec<&str> {
    let mut found = Vec::new();
    for (start, _) in element.match_indices('{') {
        let rest = &element[start + 1..];
        if let Some(end) = rest.find('}')
            && end > 0
            && rest[..end]
                .chars()
                .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
        {
            found.push(&element[start..=start + 1 + end]);
        }
    }
    found
}

/// Record a warning for every key in `other` that is not a known key of a
/// later milestone. `prefix` turns `ofer` into `review.ofer`.
fn unknown_keys(
    other: &BTreeMap<String, IgnoredAny>,
    prefix: &str,
    ignored: &[&str],
    warnings: &mut Vec<Warning>,
) {
    for key in other.keys() {
        if !ignored.contains(&key.as_str()) {
            warnings.push(Warning {
                key: format!("{prefix}{key}"),
            });
        }
    }
}

/// The parser's message, without the hints it addresses to programmers
/// rather than to whoever edits `invar.yaml`.
fn yaml_message(err: &serde_saphyr::Error) -> String {
    const PROGRAMMER_HINTS: &[&str] = &[
        ", set DuplicateKeyPolicy in Options if acceptable",
        "; use from_multiple or from_multiple_with_options",
    ];
    let mut message = err.to_string();
    for hint in PROGRAMMER_HINTS {
        message = message.replace(hint, "");
    }
    message
}

// --- the shape on disk ------------------------------------------------------
//
// Every field is optional: absent and empty (null) both mean "use the
// default" (§4, D-3). `other` collects every key the struct does not name, so
// it can be reported or ignored — serde would otherwise drop it silently, and
// a typo would go unnoticed (§6).

#[derive(Debug, Default, Deserialize)]
struct RawConfig {
    scenarios: Option<bool>,
    review: Option<RawReview>,
    git: Option<RawGit>,
    context: Option<RawContext>,
    done: Option<RawDone>,
    acceptance: Option<RawAcceptance>,
    #[serde(flatten)]
    other: BTreeMap<String, IgnoredAny>,
}

#[derive(Debug, Default, Deserialize)]
struct RawAcceptance {
    required: Option<bool>,
    style: Option<String>,
    dir: Option<String>,
    run: Option<RawRun>,
    fakes_allowed: Option<Vec<String>>,
    #[serde(flatten)]
    other: BTreeMap<String, IgnoredAny>,
}

/// `acceptance.run` as written. A string is only recognised, so that it can be
/// refused by name; it is never split (D-11).
#[derive(Debug)]
enum RawRun {
    Text,
    List(Vec<String>),
}

impl<'de> Deserialize<'de> for RawRun {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Run;

        impl<'de> de::Visitor<'de> for Run {
            type Value = RawRun;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a list of strings")
            }

            fn visit_str<E: de::Error>(self, _: &str) -> Result<RawRun, E> {
                Ok(RawRun::Text)
            }

            // Element by element, so that each one keeps the parser's own error
            // and position.
            fn visit_seq<A: de::SeqAccess<'de>>(self, mut seq: A) -> Result<RawRun, A::Error> {
                let mut run = Vec::new();
                while let Some(element) = seq.next_element()? {
                    run.push(element);
                }
                Ok(RawRun::List(run))
            }
        }

        deserializer.deserialize_any(Run)
    }
}

impl RawAcceptance {
    /// Nothing set: the same as an absent section (§3, D-3).
    fn is_empty(&self) -> bool {
        self.required.is_none()
            && self.style.is_none()
            && self.dir.is_none()
            && self.run.is_none()
            && self.fakes_allowed.is_none()
            && self.other.is_empty()
    }
}

#[derive(Debug, Default, Deserialize)]
struct RawDone {
    commit: Option<bool>,
    commit_message: Option<String>,
    #[serde(flatten)]
    other: BTreeMap<String, IgnoredAny>,
}

#[derive(Debug, Default, Deserialize)]
struct RawReview {
    offer: Option<bool>,
    #[serde(flatten)]
    other: BTreeMap<String, IgnoredAny>,
}

#[derive(Debug, Default, Deserialize)]
struct RawGit {
    base_branch: Option<String>,
    #[serde(flatten)]
    other: BTreeMap<String, IgnoredAny>,
}

#[derive(Debug, Default, Deserialize)]
struct RawContext {
    binding_limit: Option<i64>,
    #[serde(flatten)]
    other: BTreeMap<String, IgnoredAny>,
}
