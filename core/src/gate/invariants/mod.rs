//! Invariant ids at the gate (#158): which invariants of `BUSINESS_CONTEXT.md` a
//! Plan names, which criteria cite them, and which it leaves out.
//!
//! The id is the join key (plan-header.md §6.3, §6.4; `business-context.md` §5).
//! Nothing here reads what an invariant or a criterion says: a Plan that names
//! `INV-2` and cites it from `AC3` is consistent, whatever the words, and whether
//! `AC3` really catches a broken `INV-2` stays the developer's judgement at the gate.
//! What the core adds is that an absence becomes visible: every invariant the Plan
//! does not name is listed, so approving is a conscious "none of these applies".
//!
//! An approved Plan is not held to the file again: its body is frozen, and the file
//! may move on after it. The references are checked when a Plan is written and when
//! it is approved.

use std::fmt::Write as _;

use crate::context::ContextError;
use crate::context::invariants::{self, FILE, Invariant, Invariants};
use crate::diagnostics::Diagnostics;
use crate::format::plan::InvariantRefs;
use crate::fs::ProjectRoot;

#[cfg(test)]
mod tests;

/// The widest an invariant's first line is shown at the gate, in characters.
pub const TEXT_WIDTH: usize = 60;

/// Read the invariants, as every command here does.
pub fn load(root: &ProjectRoot) -> Result<Invariants, ContextError> {
    invariants::load(root)
}

/// What `plan validate` refuses of a Plan's references, each at its line of the Plan,
/// and the problems of the file, each at its line of the file.
pub fn check(invariants: &Invariants, refs: &InvariantRefs) -> Diagnostics {
    let mut found = Diagnostics::new();

    let defined: Option<&[Invariant]> = match invariants {
        Invariants::Invalid(problems) => {
            for problem in problems.iter() {
                found.general(match problem.line {
                    Some(line) => format!("{FILE}:{line}: {}", problem.message),
                    None => format!("{FILE}: {}", problem.message),
                });
            }
            None
        }
        Invariants::Ids(list) => Some(list),
        Invariants::None | Invariants::WithoutIds { .. } => Some(&[]),
    };
    if let Some(defined) = defined {
        let known = |number: u64| defined.iter().any(|item| item.number == number);
        let why = if defined.is_empty() {
            format!("{FILE} gives no invariant an id")
        } else {
            format!(
                "{FILE} defines {}",
                defined
                    .iter()
                    .map(Invariant::id)
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        };
        for &(number, line) in &refs.named {
            if !known(number) {
                found.at(line, undefined(number, &why));
            }
        }
        for (_, numbers, line) in &refs.cited {
            for &number in numbers {
                if !known(number) {
                    found.at(*line, undefined(number, &why));
                }
            }
        }
        // With ids, the old form names an invariant the core cannot see: the gate
        // would call it NOT NAMED, and no criterion would have to cite it.
        if !defined.is_empty() {
            for &line in &refs.without_id {
                found.at(
                    line,
                    format!(
                        "name the invariant by its id: `- BUSINESS_CONTEXT INV-<n>: \
                         <guidance>`; {why} (plan-header.md §6.3)"
                    ),
                );
            }
        }
    }

    for &(number, line) in &refs.named {
        if refs.citing(number).is_empty() {
            found.at(
                line,
                format!(
                    "`{}` is named here and no criterion cites it; give it a criterion that \
                     fails if the invariant is broken, and cite it after the criterion's id: \
                     `- AC<n> ({}): Given …` (plan-header.md §6.4)",
                    invariants::id(number),
                    invariants::id(number)
                ),
            );
        }
    }
    for (criterion, numbers, line) in &refs.cited {
        for &number in numbers {
            if !refs.names(number) {
                found.at(
                    *line,
                    format!(
                        "`{criterion}` cites `{id}`, which `## Implementation context` does not \
                         name: the coder reads only the Plan, so name it there as \
                         `- BUSINESS_CONTEXT {id}: <guidance>` (plan-header.md §6.3)",
                        id = invariants::id(number)
                    ),
                );
            }
        }
    }
    found
}

fn undefined(number: u64, why: &str) -> String {
    format!(
        "`{}` is not an invariant of {FILE} ({why}); use the id the file gives the \
         invariant, or ask the developer to add it there (business-context.md §5)",
        invariants::id(number)
    )
}

/// One invariant, against a Plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Covered<'a> {
    pub invariant: &'a Invariant,
    /// Named in `## Implementation context`.
    pub named: bool,
    /// The criteria that cite it, in order.
    pub by: Vec<&'a str>,
}

/// Every invariant of the file, in file order, against `refs`.
pub fn coverage<'a>(list: &'a [Invariant], refs: &'a InvariantRefs) -> Vec<Covered<'a>> {
    list.iter()
        .map(|invariant| Covered {
            invariant,
            named: refs.names(invariant.number),
            by: refs.citing(invariant.number),
        })
        .collect()
}

/// What `plan approve` prints after the criteria: every invariant, named or not, and
/// a line saying what approving means for those not named. Empty when the file has
/// no invariants; one warning when they have no ids. Deterministic: the same files
/// print the same bytes.
pub fn gate_text(invariants: &Invariants, refs: &InvariantRefs) -> String {
    let mut out = String::new();
    let list = match invariants {
        Invariants::Ids(list) => list,
        Invariants::WithoutIds { count } => {
            let which = match count {
                1 => "the invariant in BUSINESS_CONTEXT.md has".to_owned(),
                count => format!("the {count} invariants in BUSINESS_CONTEXT.md have"),
            };
            let _ = writeln!(
                out,
                "warning: {which} no id, so which of them this Plan leaves out cannot be shown. \
                 Start each item under `## Invariants` with an id, `- INV-1: …`, to have them \
                 listed here (business-context.md §5)"
            );
            return out;
        }
        Invariants::None | Invariants::Invalid(_) => return out,
    };

    let covered = coverage(list, refs);
    let rows: Vec<(String, String, String)> = covered
        .iter()
        .map(|item| {
            let state = match (item.named, item.by.as_slice()) {
                (true, []) => "named, covered by no criterion".to_owned(),
                (true, by) => format!("named, covered by {}", by.join(", ")),
                (false, []) => "NOT NAMED in this Plan".to_owned(),
                (false, by) => format!("NOT NAMED, cited by {}", by.join(", ")),
            };
            (item.invariant.id(), state, cut(&item.invariant.text))
        })
        .collect();
    let id_width = rows.iter().map(|row| row.0.len()).max().unwrap_or(0);
    let state_width = rows
        .iter()
        .map(|row| row.1.chars().count())
        .max()
        .unwrap_or(0);

    let _ = writeln!(out, "Invariants in {FILE}:");
    for (id, state, text) in &rows {
        let line = format!("  {id:<id_width$}  {state:<state_width$}  - {text}");
        let _ = writeln!(out, "{}", line.trim_end());
    }
    let unnamed = covered.iter().filter(|item| !item.named).count();
    if unnamed > 0 {
        let _ = writeln!(out);
        let _ = writeln!(
            out,
            "{} not named: approving says none of them applies to this ticket.",
            plural(unnamed, "invariant is", "invariants are")
        );
    }
    out
}

/// `text` cut to [`TEXT_WIDTH`] characters, with `…` where it was cut.
pub fn cut(text: &str) -> String {
    if text.chars().count() <= TEXT_WIDTH {
        return text.to_owned();
    }
    let kept: String = text.chars().take(TEXT_WIDTH - 1).collect();
    format!("{}…", kept.trim_end())
}

fn plural(count: usize, one: &str, many: &str) -> String {
    format!("{count} {}", if count == 1 { one } else { many })
}
