//! Running `acceptance.run` (`acceptance-lock.md` §9.2).
//!
//! The core runs a command it does not understand and reads what it wrote (A-2).
//! Its output goes to files and not to pipes: a suite that leaves a process behind
//! holding a pipe open would otherwise hang the read, past any limit.

use std::fmt;
use std::fs::{self, File};
use std::io::{self, Read as _, Seek as _, SeekFrom};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use crate::decision::random_u32;
use crate::fs::ProjectRoot;

/// How long a run may take (D-11).
pub const TIME_LIMIT: Duration = Duration::from_secs(10 * 60);

/// How often the run is looked at.
const POLL: Duration = Duration::from_millis(50);

/// How many lines of each stream a message shows (§9.2).
const TAIL_LINES: usize = 40;

/// How much of the end of a stream is read to find them.
const TAIL_BYTES: u64 = 64 * 1024;

/// What is left of a run that ended by itself.
#[derive(Debug)]
pub struct Finished {
    /// As printed: `exit status: 1`, or the signal. For messages only (D-8).
    pub status: String,
    /// `None` when no report was written.
    pub report: Option<Vec<u8>>,
    pub output: Output,
}

/// The last lines of what a run printed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Output {
    pub stderr: String,
    pub stdout: String,
}

impl fmt::Display for Output {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (name, tail) in [("stderr", &self.stderr), ("stdout", &self.stdout)] {
            if tail.is_empty() {
                write!(f, "\n--- {name}: nothing ---")?;
            } else {
                write!(f, "\n--- the last lines of {name} ---\n{tail}")?;
            }
        }
        Ok(())
    }
}

/// `argv` with `{report}` and `{files}` filled in. The config guarantees each is
/// exactly one whole element (`invar-yaml.md` §3.6).
pub fn argv(run: &[String], report: &Path, files: &[String]) -> Vec<String> {
    let mut argv = Vec::with_capacity(run.len() + files.len());
    for element in run {
        match element.as_str() {
            "{report}" => argv.push(report.to_string_lossy().into_owned()),
            "{files}" => argv.extend(files.iter().cloned()),
            _ => argv.push(element.clone()),
        }
    }
    argv
}

/// Run `run` on `files`, for at most `limit`. The report and the output are read
/// before this returns, and their directory is gone by then, whatever happened.
pub fn run(
    root: &ProjectRoot,
    run: &[String],
    files: &[String],
    limit: Duration,
) -> Result<Finished, RunError> {
    let scratch = Scratch::create(root)?;
    let report = scratch.path.join("report.xml");
    let stdout = scratch.path.join("stdout");
    let stderr = scratch.path.join("stderr");

    let argv = argv(run, &report, files);
    let stream = |path: &Path| -> Result<Stdio, RunError> {
        Ok(File::create(path).map_err(RunError::Scratch)?.into())
    };
    let mut child = Command::new(&argv[0])
        .args(&argv[1..])
        .current_dir(root.path())
        .stdin(Stdio::null())
        .stdout(stream(&stdout)?)
        .stderr(stream(&stderr)?)
        .spawn()
        .map_err(|source| RunError::CannotStart {
            program: argv[0].clone(),
            source,
        })?;

    let output = || Output {
        stderr: tail(&stderr),
        stdout: tail(&stdout),
    };
    let status = wait(&mut child, limit).map_err(|source| RunError::Wait { source })?;
    let Some(status) = status else {
        return Err(RunError::TimedOut {
            limit,
            output: output(),
        });
    };
    Ok(Finished {
        status: status.to_string(),
        report: match fs::read(&report) {
            Ok(bytes) => Some(bytes),
            Err(err) if err.kind() == io::ErrorKind::NotFound => None,
            Err(err) => return Err(RunError::Scratch(err)),
        },
        output: output(),
    })
}

/// The status, or `None` if the limit came first, in which case the program was
/// killed. What it started itself is not tracked (D-11).
fn wait(child: &mut std::process::Child, limit: Duration) -> io::Result<Option<ExitStatus>> {
    let started = Instant::now();
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(Some(status));
        }
        if started.elapsed() >= limit {
            // It may have ended just now; either way it is reaped by the `wait`.
            let _ = child.kill();
            child.wait()?;
            return Ok(None);
        }
        thread::sleep(POLL.min(limit));
    }
}

/// The last [`TAIL_LINES`] lines of a file, read from its end only. An unreadable
/// stream is an empty one: this is only ever part of a message.
fn tail(path: &Path) -> String {
    let read = || -> io::Result<Vec<u8>> {
        let mut file = File::open(path)?;
        let length = file.metadata()?.len();
        file.seek(SeekFrom::Start(length.saturating_sub(TAIL_BYTES)))?;
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)?;
        Ok(bytes)
    };
    let bytes = read().unwrap_or_default();
    let text = String::from_utf8_lossy(&bytes);
    let lines: Vec<&str> = text.trim_end().lines().collect();
    let first = lines.len().saturating_sub(TAIL_LINES);
    lines[first..].join("\n")
}

/// A fresh directory in the OS temp directory, removed when this goes out of scope,
/// on every path out of [`run`] (D-9). A directory rather than a file: the report
/// must not exist until the runner writes it, or a run that wrote none could not be
/// told from one that did.
struct Scratch {
    path: PathBuf,
}

impl Scratch {
    fn create(root: &ProjectRoot) -> Result<Self, RunError> {
        let temp = std::env::temp_dir();
        let resolved = temp.canonicalize().map_err(RunError::Scratch)?;
        if resolved.starts_with(root.path()) {
            return Err(RunError::TempInProject { temp: resolved });
        }
        let mut builder = fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt as _;
            builder.mode(0o700);
        }
        let mut attempts = 0;
        loop {
            let path = resolved.join(format!(
                "invar-acceptance-{}-{:08x}",
                std::process::id(),
                random_u32()
            ));
            match builder.create(&path) {
                Ok(()) => return Ok(Self { path }),
                Err(err) if err.kind() == io::ErrorKind::AlreadyExists && attempts < 16 => {
                    attempts += 1;
                }
                Err(err) => return Err(RunError::Scratch(err)),
            }
        }
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

/// Why a run has no result.
#[derive(Debug)]
pub enum RunError {
    /// Not found, not executable.
    CannotStart {
        program: String,
        source: io::Error,
    },
    TimedOut {
        limit: Duration,
        output: Output,
    },
    Wait {
        source: io::Error,
    },
    /// The temp directory is inside the project (D-9).
    TempInProject {
        temp: PathBuf,
    },
    /// The directory the run works in.
    Scratch(io::Error),
}

impl fmt::Display for RunError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CannotStart { program, source } => write!(
                f,
                "cannot start `{program}`, the program in `acceptance.run` in invar.yaml: {source}"
            ),
            Self::TimedOut { limit, output } => write!(
                f,
                "the tests did not finish within {} minutes, so the run was killed and nothing \
                 was locked; look for a test that hangs{output}",
                limit.as_secs() / 60
            ),
            Self::Wait { source } => write!(f, "cannot wait for the tests to finish: {source}"),
            Self::TempInProject { temp } => write!(
                f,
                "the temp directory {} is inside the project, and the report must stay out of \
                 the repository; point TMPDIR somewhere else",
                temp.display()
            ),
            Self::Scratch(source) => {
                write!(f, "cannot use the temp directory for the run: {source}")
            }
        }
    }
}

impl std::error::Error for RunError {}
