//! A git repository with SSH keys of its own, for the tests of signed trailers
//! (`approval-signature.md` §6.5): the real binary, real `git`, keys made by `ssh-keygen`
//! in each test, and nothing from the developer's own agent, keys or git config. The
//! keys and the git config sit beside the repository, so that no commit holds them.

#![allow(dead_code, reason = "each test file uses what it needs")]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command as Process, Stdio};

use assert_cmd::Command;
use tempfile::TempDir;

pub const BRANCH: &str = "feat/SHOP-214-cancel";

pub struct Project {
    pub dir: TempDir,
}

impl Project {
    /// `main` with `config` as `invar.yaml`, then a branch from it, checked out.
    pub fn new(config: &str) -> Self {
        let project = Self {
            dir: tempfile::tempdir().unwrap(),
        };
        fs::create_dir_all(project.keys()).unwrap();
        fs::write(
            project.gitconfig(),
            "[user]\n\tname = dev\n\temail = dev@example.com\n",
        )
        .unwrap();
        project.git(&["init", "-q", "-b", "main", "."]);
        project.write("invar.yaml", config);
        project.write("app/Models/Order.php", "<?php // v1\n");
        project.write("README.md", "Shop\n");
        project.commit_all("base");
        project.git(&["checkout", "-q", "-b", BRANCH]);
        project
    }

    /// As [`new`](Self::new), with `approver` listed on `main` as `dev@example.com` in
    /// every namespace, and git set to sign with it.
    pub fn signed(config: &str) -> (Self, PathBuf) {
        let project = Self::new(config);
        let key = project.key("approver");
        project.git(&["checkout", "-q", "main"]);
        project.write(
            ".invar/approvers",
            &format!("{}\n", project.approver_line("dev@example.com", &key, None)),
        );
        project.commit_all("chore: list the approvers");
        project.git(&["checkout", "-q", "-B", BRANCH]);
        project.sign_with(&key);
        (project, key)
    }

    pub fn root(&self) -> PathBuf {
        let root = self.dir.path().join("repo");
        fs::create_dir_all(&root).unwrap();
        root.canonicalize().unwrap()
    }

    fn keys(&self) -> PathBuf {
        self.dir.path().join("keys")
    }

    fn gitconfig(&self) -> PathBuf {
        self.dir.path().join("gitconfig")
    }

    pub fn write(&self, relative: &str, text: &str) {
        let path = self.root().join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    pub fn read(&self, relative: &str) -> Option<String> {
        fs::read_to_string(self.root().join(relative)).ok()
    }

    /// A new ed25519 key with no passphrase.
    pub fn key(&self, name: &str) -> PathBuf {
        let path = self.keys().join(name);
        let status = Process::new("ssh-keygen")
            .args(["-q", "-t", "ed25519", "-C", name, "-N", "", "-f"])
            .arg(&path)
            .stdin(Stdio::null())
            .status()
            .expect("ssh-keygen must be installed to run these tests");
        assert!(status.success());
        path
    }

    /// An allowed-signers line; `namespaces` as written, or none.
    pub fn approver_line(&self, principal: &str, key: &Path, namespaces: Option<&str>) -> String {
        let public = fs::read_to_string(key.with_extension("pub")).unwrap();
        match namespaces {
            Some(namespaces) => {
                format!("{principal} namespaces=\"{namespaces}\" {}", public.trim())
            }
            None => format!("{principal} {}", public.trim()),
        }
    }

    pub fn sign_with(&self, key: &Path) {
        self.git(&["config", "--global", "gpg.format", "ssh"]);
        self.git(&[
            "config",
            "--global",
            "user.signingkey",
            &key.display().to_string(),
        ]);
    }

    fn env(&self) -> Vec<(&'static str, String)> {
        vec![
            ("GIT_CONFIG_GLOBAL", self.gitconfig().display().to_string()),
            ("GIT_CONFIG_NOSYSTEM", "1".into()),
            ("HOME", self.root().display().to_string()),
            ("GIT_COMMITTER_DATE", "2026-10-05T10:00:00+02:00".into()),
            ("GIT_AUTHOR_DATE", "2026-10-05T10:00:00+02:00".into()),
        ]
    }

    pub fn git(&self, args: &[&str]) -> String {
        let output = Process::new("git")
            .args(args)
            .current_dir(self.root())
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .envs(self.env())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap()
    }

    pub fn invar(&self, args: &[&str]) -> assert_cmd::assert::Assert {
        Command::cargo_bin("invar")
            .unwrap()
            .current_dir(self.root())
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env_remove("XDG_CONFIG_HOME")
            .env_remove("SSH_AUTH_SOCK")
            .envs(self.env())
            .args(args)
            .write_stdin("")
            .assert()
    }

    pub fn commit_all(&self, message: &str) {
        self.git(&["add", "-A"]);
        self.git(&["commit", "-q", "--allow-empty", "-m", message]);
    }

    pub fn head(&self) -> String {
        self.git(&["rev-parse", "HEAD"]).trim().to_owned()
    }
}

pub fn fingerprint(key: &Path) -> String {
    let output = Process::new("ssh-keygen")
        .arg("-lf")
        .arg(key.with_extension("pub"))
        .output()
        .unwrap();
    String::from_utf8(output.stdout)
        .unwrap()
        .split_whitespace()
        .nth(1)
        .unwrap()
        .to_owned()
}

pub fn stdout(assert: &assert_cmd::assert::Assert) -> String {
    String::from_utf8(assert.get_output().stdout.clone()).unwrap()
}

pub fn last_line(assert: &assert_cmd::assert::Assert) -> String {
    stdout(assert).lines().last().unwrap_or_default().to_owned()
}
