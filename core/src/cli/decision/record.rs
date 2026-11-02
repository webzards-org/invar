//! `invar decision record --ticket <T> --type <TYPE> [--module <M>] [--acceptance <A>]
//! [--message <TEXT>]`.

use anyhow::Result;
use clap::builder::{PossibleValuesParser, TypedValueParser as _};

use crate::cli::shown;
use crate::decision::{self, Request};
use crate::format::decision::Type;
use crate::format::plan::Acceptance;
use crate::fs;
use crate::git;

#[derive(Debug, clap::Args)]
pub struct Args {
    /// The ticket the entry belongs to, such as `PROJ-412`.
    #[arg(long)]
    pub ticket: String,

    /// `correction`, `failed`, `note` — or `confirmed`, which `invar done`
    /// normally writes when it closes a cycle.
    #[arg(long = "type", value_name = "TYPE", value_parser = type_parser())]
    pub kind: Type,

    /// The module the entry is about, or `global`. Read from the ticket's Plan
    /// when it has one; required when it has none.
    #[arg(long)]
    pub module: Option<String>,

    /// `required` or `skipped`: whether the ticket was approved with acceptance
    /// tests. Refused when the ticket's approved Plan says otherwise. Without
    /// it, the entry says nothing about them — unknown, not `skipped`.
    #[arg(long, value_name = "ACCEPTANCE", value_parser = acceptance_parser())]
    pub acceptance: Option<Acceptance>,

    /// The entry's text. Without it, the text is read from stdin.
    #[arg(long, short)]
    pub message: Option<String>,
}

/// The four types, taken from [`Type::ALL`] instead of being listed a second
/// time here: a type added there is accepted here, and nothing else is.
fn type_parser() -> impl clap::builder::TypedValueParser<Value = Type> {
    PossibleValuesParser::new(Type::ALL.map(Type::as_str))
        .map(|name| Type::parse(&name).expect("the parser allows only the names of Type::ALL"))
}

/// `required` and `skipped`, from [`Acceptance::ALL`], as for the types.
fn acceptance_parser() -> impl clap::builder::TypedValueParser<Value = Acceptance> {
    PossibleValuesParser::new(Acceptance::ALL.map(Acceptance::as_str)).map(|name| {
        Acceptance::parse(&name).expect("the parser allows only the names of Acceptance::ALL")
    })
}

pub fn run(args: Args) -> Result<()> {
    // Everything that does not need the entry's text comes first. A person who
    // has typed a message should not learn only afterwards that this is not an
    // invar project, and a command that is given no text must say what is
    // wrong before it waits for some from a stdin that may never end.
    let root = fs::project_root()?;
    let mode = crate::cli::mode(&root)?;
    let now = git::now(root.path())?;

    let body = match args.message {
        Some(message) => message,
        None => crate::cli::read_stdin()?,
    };
    let request = Request {
        ticket: args.ticket,
        kind: args.kind,
        module: args.module,
        acceptance: args.acceptance,
        body,
    };
    let recorded = decision::record(&root, request, now, mode)?;

    let cwd = std::env::current_dir().unwrap_or_default();
    println!(
        "recorded: {} ({}, {}, {})",
        shown(&recorded.path, &cwd),
        recorded.entry.id,
        recorded.entry.kind,
        recorded.entry.module
    );
    if let Some(spec) = recorded.reset {
        println!(
            "reset: {} is `draft` again — `Status`, `Approved-by`, `Approved-at` and \
             `Approved-checksum` cleared, the body unchanged. Back to Phase 1: revise the Plan, \
             then the developer approves it again",
            shown(&spec, &cwd)
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use clap::Parser as _;

    use super::{Acceptance, Args, Type};

    /// `Args` is not a command by itself; this is the smallest one around it.
    #[derive(Debug, clap::Parser)]
    struct Wrapper {
        #[command(flatten)]
        args: Args,
    }

    fn parse(kind: &str) -> Result<Type, clap::Error> {
        Wrapper::try_parse_from(["invar", "--ticket", "PROJ-1", "--type", kind])
            .map(|wrapper| wrapper.args.kind)
    }

    #[test]
    fn every_type_is_accepted_and_parses_to_itself() {
        // If this list were written out a second time, a type added to
        // `Type::ALL` would be refused here and this test would say which.
        for kind in Type::ALL {
            assert_eq!(parse(kind.as_str()).unwrap(), kind, "{kind}");
        }
    }

    #[test]
    fn nothing_else_is_accepted() {
        for name in ["fixed", "Failed", "NOTE", "", "note ", "confirm"] {
            assert!(parse(name).is_err(), "{name:?}");
        }
    }

    fn parse_acceptance(value: &str) -> Result<Option<Acceptance>, clap::Error> {
        Wrapper::try_parse_from([
            "invar",
            "--ticket",
            "PROJ-1",
            "--type",
            "note",
            "--acceptance",
            value,
        ])
        .map(|wrapper| wrapper.args.acceptance)
    }

    #[test]
    fn acceptance_is_required_or_skipped_and_absent_by_default() {
        for value in Acceptance::ALL {
            assert_eq!(parse_acceptance(value.as_str()).unwrap(), Some(value));
        }
        for name in ["none", "Required", "", "yes"] {
            assert!(parse_acceptance(name).is_err(), "{name:?}");
        }
        let absent = Wrapper::try_parse_from(["invar", "--ticket", "PROJ-1", "--type", "note"]);
        assert_eq!(absent.unwrap().args.acceptance, None);
    }

    #[test]
    fn the_definition_of_the_arguments_is_sound() {
        use clap::CommandFactory as _;
        Wrapper::command().debug_assert();
    }
}
