//! `invar init` (M1-15).
//!
//! Checks that the current directory is inside a git work tree, scaffolds the
//! project in it, and says per file what happened — created, updated, or
//! unchanged. What a person needs to act on goes to stderr as a warning.
//! Everything else is `init::apply`'s.

use std::path::Path;

use anyhow::{Context as _, Result, bail};

use crate::git::{self, WorkTree};
use crate::init::{self, Change, Report};

pub fn run() -> Result<()> {
    let cwd = std::env::current_dir().context("cannot read the current directory")?;

    let top = match git::work_tree(&cwd)? {
        WorkTree::Inside(top) => top,
        WorkTree::Outside(said) => {
            let said = if said.is_empty() {
                String::new()
            } else {
                format!(" git said: {said}")
            };
            bail!(
                "`invar init` must run inside a git repository: Plans and the decision log live \
                 in git (§12), and `.gitattributes` and `.gitignore` are git's own files. Run \
                 `git init` first, or change to your project's directory.{said}"
            );
        }
    };

    let report = init::apply(&cwd)?;
    print_report(&report);

    // A project in a subdirectory of a repository is legitimate (a monorepo),
    // and an accident in `app/Http/` looks the same. Say which this is.
    if !same_directory(&cwd, &top) {
        eprintln!(
            "note: this directory is not the repository root ({}). It is the project root now: \
             `invar.yaml` is here, and every command finds it from here and below",
            top.display()
        );
    }
    Ok(())
}

fn print_report(report: &Report) {
    for item in &report.items {
        let label = match item.change {
            Change::Created => "created",
            Change::Updated => "updated",
            Change::Unchanged => "unchanged",
        };
        match &item.note {
            Some(note) => println!("{label:<10} {} — {note}", item.path),
            None => println!("{label:<10} {}", item.path),
        }
    }
    for warning in &report.warnings {
        eprintln!("warning: {warning}");
    }

    if !report.changed() {
        println!("\nnothing to do: everything is already in place");
        return;
    }
    let made_context = report.items.iter().any(|item| {
        item.change == Change::Created
            && (item.path == "AI_CONTEXT.md" || item.path == "BUSINESS_CONTEXT.md")
    });
    if made_context {
        println!(
            "\nNext: fill in AI_CONTEXT.md and BUSINESS_CONTEXT.md under their headings. Only \
             your team knows what goes there, and `invar init` never writes it. Until they \
             hold something besides the headings, they are treated as if they were not there."
        );
    }
}

/// The same directory, however each was spelled: symlinks and `..` resolved.
fn same_directory(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        // Cannot tell; saying nothing is better than a wrong note.
        _ => true,
    }
}
