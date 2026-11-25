//! Tests for the statement, the key as git reads it, and the deadline. Signing
//! with a real key is in `tests/plan_approve_signed.rs`.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use super::{
    Group, Key, SignError, Signer, isolate, new_approval_id, parse_key, run_with_deadline,
    statement,
};

#[test]
fn the_statement_is_four_lines_ending_in_lf() {
    assert_eq!(
        statement(
            "SHOP-214",
            "sha256:00ff",
            "0123456789abcdef0123456789abcdef"
        ),
        "invar-approval 1\nticket: SHOP-214\napproved-checksum: sha256:00ff\n\
         approval-id: 0123456789abcdef0123456789abcdef\n"
    );
}

#[test]
fn an_approval_id_is_32_lowercase_hex_digits_and_new_each_time() {
    let first = new_approval_id().unwrap();
    assert_eq!(first.len(), 32);
    assert!(
        first
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
        "{first}"
    );
    assert_ne!(first, new_approval_id().unwrap());
}

#[test]
fn a_signing_key_is_read_as_git_reads_it() {
    let home = Some(PathBuf::from("/home/dev"));
    assert_eq!(
        parse_key("~/.ssh/id_ed25519.pub", home.clone()),
        Key::File(PathBuf::from("/home/dev/.ssh/id_ed25519.pub"))
    );
    assert_eq!(
        parse_key("/keys/approver", home.clone()),
        Key::File(PathBuf::from("/keys/approver"))
    );
    assert_eq!(
        parse_key("key::ssh-ed25519 AAAAC3Nza dev", home.clone()),
        Key::Literal("ssh-ed25519 AAAAC3Nza dev".to_owned())
    );
    assert_eq!(
        parse_key("key::sk-ssh-ed25519@openssh.com AAAAGnNr", home.clone()),
        Key::Literal("sk-ssh-ed25519@openssh.com AAAAGnNr".to_owned())
    );
    // Without `key::`, only `ssh-` marks a key written in place, as in git.
    assert_eq!(
        parse_key("sk-ssh-ed25519@openssh.com AAAAGnNr", home),
        Key::File(PathBuf::from("sk-ssh-ed25519@openssh.com AAAAGnNr"))
    );
    // No home to expand against: the path as it is.
    assert_eq!(parse_key("~/k", None), Key::File(PathBuf::from("~/k")));
}

#[test]
fn a_program_past_the_deadline_is_killed() {
    let child = Command::new("sleep")
        .arg("5")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let started = Instant::now();
    let finished = run_with_deadline(child, Group::Ours, b"", Duration::from_millis(200)).unwrap();
    assert!(finished.is_none());
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "{:?}",
        started.elapsed()
    );
}

#[test]
fn a_program_within_the_deadline_gives_its_status_and_output() {
    let child = Command::new("cat")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let super::Finished {
        status,
        stdout,
        stderr,
    } = run_with_deadline(child, Group::Ours, b"statement\n", Duration::from_secs(5))
        .unwrap()
        .unwrap();
    assert!(status.success());
    assert_eq!(stdout, b"statement\n");
    assert!(stderr.is_empty());
}

fn signer(program: &str) -> Signer {
    Signer {
        program: program.to_owned(),
        key: Key::File(PathBuf::from("/no/such/key")),
    }
}

#[test]
fn a_program_that_fails_or_writes_no_signature_is_refused() {
    let cwd = Path::new("/");
    match signer("false").sign(cwd, "x\n", Duration::from_secs(5)) {
        Err(SignError::Failed {
            status: Some(1), ..
        }) => {}
        other => panic!("expected Failed, got {other:?}"),
    }
    match signer("true").sign(cwd, "x\n", Duration::from_secs(5)) {
        Err(err @ SignError::NoSignature { .. }) => {
            assert!(err.to_string().contains("wrote no SSH signature"), "{err}");
        }
        other => panic!("expected NoSignature, got {other:?}"),
    }
    match signer("invar-no-such-signing-program").sign(cwd, "x\n", Duration::from_secs(5)) {
        Err(err @ SignError::CannotStart { .. }) => {
            assert!(
                err.to_string()
                    .contains("cannot run `invar-no-such-signing-program`")
            );
        }
        other => panic!("expected CannotStart, got {other:?}"),
    }
}

#[test]
fn the_timeout_says_what_to_do() {
    let err = SignError::TimedOut {
        program: "ssh-keygen".into(),
        seconds: 90,
    };
    let message = err.to_string();
    assert!(message.contains("within 90 seconds"), "{message}");
    assert!(message.contains("then approve again"), "{message}");
}

// --- what a signing program starts (#162 review) ---------------------------------

/// The pid a test program wrote to `file`, once it is there.
#[cfg(unix)]
fn pid_in(file: &Path) -> u32 {
    let started = Instant::now();
    loop {
        if let Some(pid) = std::fs::read_to_string(file)
            .ok()
            .and_then(|text| text.trim().parse().ok())
        {
            return pid;
        }
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "no pid in {}",
            file.display()
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[cfg(unix)]
fn alive(pid: u32) -> bool {
    Command::new("kill")
        .args(["-0", &pid.to_string()])
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

/// Whether `pid` is gone within two seconds: a killed process may stay a
/// zombie for a moment, until whoever inherited it reaps it.
#[cfg(unix)]
fn gone(pid: u32) -> bool {
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(2) {
        if !alive(pid) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    false
}

#[cfg(unix)]
fn kill(pid: u32) {
    let _ = Command::new("kill")
        .args(["-KILL", &pid.to_string()])
        .stderr(Stdio::null())
        .status();
}

/// A shell that starts a background process holding its output pipes, writes
/// that process's pid to `pid_file`, and then waits far past any deadline.
#[cfg(unix)]
fn holds_the_pipes(pid_file: &Path) -> Command {
    let mut command = Command::new("sh");
    command
        .arg("-c")
        .arg(format!(
            "( sleep 30 ) & echo $! > '{}'; sleep 30",
            pid_file.display()
        ))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command
}

#[cfg(unix)]
#[test]
fn without_a_terminal_the_deadline_kills_everything_the_program_started() {
    let dir = tempfile::tempdir().unwrap();
    let pid_file = dir.path().join("grandchild");
    let mut command = holds_the_pipes(&pid_file);
    assert_eq!(isolate(&mut command, false), Group::Own);
    let child = command.spawn().unwrap();
    let started = Instant::now();
    let finished = run_with_deadline(child, Group::Own, b"", Duration::from_millis(300)).unwrap();
    assert!(finished.is_none());
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "{:?}",
        started.elapsed()
    );
    let grandchild = pid_in(&pid_file);
    let killed = gone(grandchild);
    kill(grandchild);
    assert!(
        killed,
        "the background process {grandchild} outlived the deadline"
    );
}

#[cfg(unix)]
#[test]
fn with_a_terminal_the_deadline_still_returns_at_once() {
    // In our own group, only the program itself is killed. What it started may
    // keep the pipes open; the deadline does not wait for them.
    let dir = tempfile::tempdir().unwrap();
    let pid_file = dir.path().join("grandchild");
    let mut command = holds_the_pipes(&pid_file);
    assert_eq!(isolate(&mut command, true), Group::Ours);
    let child = command.spawn().unwrap();
    let started = Instant::now();
    let finished = run_with_deadline(child, Group::Ours, b"", Duration::from_millis(300)).unwrap();
    assert!(finished.is_none());
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "{:?}",
        started.elapsed()
    );
    kill(pid_in(&pid_file));
}

/// A signing program written as a script in `dir`.
#[cfg(unix)]
fn script(dir: &Path, body: &str) -> String {
    use std::os::unix::fs::PermissionsExt as _;
    let path = dir.join("signer");
    std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path.display().to_string()
}

/// Sign with the script at `program`, trying again while Linux says it is busy. Another
/// test's child can inherit the script's write handle for the moment between its `fork`
/// and its `exec`, and `exec` of a file open for writing fails with `ETXTBSY`. Only the
/// tests write a program and run it at once.
#[cfg(unix)]
fn sign_script(program: &str, dir: &Path, deadline: Duration) -> Result<String, SignError> {
    for _ in 0..100 {
        match signer(program).sign(dir, "x\n", deadline) {
            Err(SignError::CannotStart { source, .. })
                if source.kind() == std::io::ErrorKind::ExecutableFileBusy =>
            {
                std::thread::sleep(Duration::from_millis(10));
            }
            other => return other,
        }
    }
    signer(program).sign(dir, "x\n", deadline)
}

#[cfg(unix)]
#[test]
fn a_signer_whose_helper_holds_the_pipes_times_out_within_the_deadline() {
    // ssh-keygen with an askpass dialog, or a password manager's signer.
    let dir = tempfile::tempdir().unwrap();
    let pid_file = dir.path().join("helper");
    let program = script(
        dir.path(),
        &format!(
            "( sleep 30 ) & echo $! > '{}'\nsleep 30",
            pid_file.display()
        ),
    );
    let started = Instant::now();
    match sign_script(&program, dir.path(), Duration::from_millis(300)) {
        Err(SignError::TimedOut { .. }) => {}
        other => panic!("expected TimedOut, got {other:?}"),
    }
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "{:?}",
        started.elapsed()
    );
    // Gone without a terminal; with one (a developer's `cargo test`), left to us.
    kill(pid_in(&pid_file));
}

#[cfg(unix)]
#[test]
fn a_signer_that_finishes_in_time_is_read_to_the_end() {
    let dir = tempfile::tempdir().unwrap();
    let program = script(
        dir.path(),
        "cat > /dev/null\necho '-----BEGIN SSH SIGNATURE-----'\necho 'U1NIU0lH'\necho '-----END SSH SIGNATURE-----'",
    );
    let signature = sign_script(&program, dir.path(), Duration::from_secs(5)).unwrap();
    assert_eq!(
        signature,
        "-----BEGIN SSH SIGNATURE-----\nU1NIU0lH\n-----END SSH SIGNATURE-----\n"
    );
}
