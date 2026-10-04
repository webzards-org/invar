//! `invar.yaml` — the project config (§6).
//!
//! Implements `contracts/formats/invar-yaml.md` v1.0.0. The contract is the
//! authority; section numbers below (`§3.4`, `D-2`) point into it.
//!
//! Light mode reads four keys: `scenarios`, `review.offer`,
//! `git.base_branch` and `context.binding_limit`. Everything else in a config
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
use serde::de::IgnoredAny;

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
}

/// The `review` section (§3.2).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Review {
    /// `review.offer` — whether the protocol skill offers `/invar:review`.
    pub offer: bool,
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

impl Default for Config {
    /// What an empty `invar.yaml` means (§2, example 10.2).
    fn default() -> Self {
        Self {
            scenarios: false,
            review: Review::default(),
            git: Git::default(),
            context: Context::default(),
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

    // Every value problem in one message, so one edit fixes the file (§7).
    if !problems.is_empty() {
        return Err(invalid(problems.join("; ")));
    }

    Ok(Loaded {
        path: path.to_owned(),
        config: Config {
            scenarios: raw.scenarios.unwrap_or(false),
            review: Review {
                offer: review.offer.unwrap_or(false),
            },
            git: Git {
                base_branch: git.base_branch,
            },
            context: Context { binding_limit },
        },
        warnings,
    })
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
