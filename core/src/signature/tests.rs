//! Tests for the statement, the key as git reads it, and the deadline. Signing
//! with a real key is in `tests/plan_approve_signed.rs`.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use super::{Key, SignError, Signer, parse_key, run_with_deadline, statement};

#[test]
fn the_statement_is_three_lines_ending_in_lf() {
    assert_eq!(
        statement("SHOP-214", "sha256:00ff"),
        "invar-approval 1\nticket: SHOP-214\napproved-checksum: sha256:00ff\n"
    );
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
    let finished = run_with_deadline(child, b"", Duration::from_millis(200)).unwrap();
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
    } = run_with_deadline(child, b"statement\n", Duration::from_secs(5))
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
