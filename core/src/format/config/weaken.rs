//! What weakens the checks (`contracts/formats/invar-yaml.md` §3.9): a closed list,
//! read once here by the guard and by `invar config check`.
//!
//! Two configs that were read come in, the old and the new, and every weakening of the
//! old by the new comes out. Values are compared as read, defaults applied: removing
//! `review.offer: true` changes nothing, since `true` is its default. Adding or
//! strengthening is never a weakening, and every key not in the list is free to change.

use std::fmt;

use super::Config;

/// One weakening: the key, and its value before and after, as a person reads them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Weakening {
    pub key: &'static str,
    pub before: String,
    pub after: String,
}

impl fmt::Display for Weakening {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {} -> {}", self.key, self.before, self.after)
    }
}

/// Every way `new` weakens `old`, in the order of the contract's list.
pub fn weakenings(old: &Config, new: &Config) -> Vec<Weakening> {
    let mut found = Vec::new();
    let mut push = |key, before: String, after: String| {
        found.push(Weakening { key, before, after });
    };

    match (&old.acceptance, &new.acceptance) {
        (Some(_), None) => push("acceptance", "present".into(), "absent".into()),
        (Some(old), Some(new)) => {
            if old.dir != new.dir {
                push("acceptance.dir", old.dir.clone(), new.dir.clone());
            }
            if old.run != new.run {
                push("acceptance.run", list(&old.run), list(&new.run));
            }
            if old.required && !new.required {
                push("acceptance.required", "true".into(), "false".into());
            }
        }
        (None, _) => {}
    }

    if old.approval.signed && !new.approval.signed {
        push("approval.signed", "true".into(), "false".into());
    }

    for pattern in &old.cycle.required_paths {
        if !new.cycle.required_paths.contains(pattern) {
            push("cycle.required_paths", pattern.clone(), "removed".into());
        }
    }

    if old.review.offer && !new.review.offer {
        push("review.offer", "true".into(), "false".into());
    }
    found
}

/// `["vendor/bin/pest", "{report}"]`, as `invar.yaml` writes a list.
fn list(items: &[String]) -> String {
    let quoted: Vec<String> = items.iter().map(|item| format!("{item:?}")).collect();
    format!("[{}]", quoted.join(", "))
}

#[cfg(test)]
mod tests {
    use super::super::parse;
    use super::*;
    use std::path::Path;

    fn config(text: &str) -> Config {
        parse(text, Path::new("invar.yaml")).unwrap().config
    }

    fn keys(old: &str, new: &str) -> Vec<String> {
        weakenings(&config(old), &config(new))
            .iter()
            .map(ToString::to_string)
            .collect()
    }

    const FULL: &str = "review:\n  offer: true\napproval:\n  signed: true\n\
        acceptance:\n  required: true\n  dir: tests/Acceptance\n  run: [\"pest\", \"{report}\", \"{files}\"]\n\
        cycle:\n  required_paths: [\"app/Models/**\", \"database/migrations/\"]\n";

    #[test]
    fn each_weakening_on_the_list_is_found_with_both_values() {
        let cases = [
            ("acceptance:", "cycle:", "acceptance: present -> absent"),
            (
                "dir: tests/Acceptance",
                "dir: tests/Other",
                "acceptance.dir: tests/Acceptance -> tests/Other",
            ),
            (
                "run: [\"pest\", \"{report}\", \"{files}\"]",
                "run: [\"true\", \"{report}\", \"{files}\"]",
                "acceptance.run: [\"pest\", \"{report}\", \"{files}\"] -> [\"true\", \"{report}\", \"{files}\"]",
            ),
            (
                "required: true",
                "required: false",
                "acceptance.required: true -> false",
            ),
            (
                "signed: true",
                "signed: false",
                "approval.signed: true -> false",
            ),
            (
                "\"app/Models/**\", ",
                "",
                "cycle.required_paths: app/Models/** -> removed",
            ),
            ("offer: true", "offer: false", "review.offer: true -> false"),
        ];
        for (from, to, says) in cases {
            let new = if from == "acceptance:" {
                // The whole section, and nothing else.
                FULL.replace(
                    "acceptance:\n  required: true\n  dir: tests/Acceptance\n  run: [\"pest\", \"{report}\", \"{files}\"]\n",
                    "",
                )
            } else {
                FULL.replace(from, to)
            };
            assert_eq!(keys(FULL, &new), vec![says.to_owned()], "{from} -> {to}");
        }
    }

    #[test]
    fn removing_a_key_weakens_only_when_its_default_is_weaker() {
        // `acceptance.required` and `review.offer` default to true: removing them is nothing.
        assert!(keys(FULL, &FULL.replace("  required: true\n", "")).is_empty());
        assert!(keys(FULL, &FULL.replace("review:\n  offer: true\n", "")).is_empty());
        // `approval.signed` defaults to false, and the patterns to none.
        assert_eq!(
            keys(FULL, &FULL.replace("approval:\n  signed: true\n", "")),
            vec!["approval.signed: true -> false"]
        );
        assert_eq!(
            keys(
                FULL,
                &FULL.replace(
                    "cycle:\n  required_paths: [\"app/Models/**\", \"database/migrations/\"]\n",
                    ""
                )
            ),
            vec![
                "cycle.required_paths: app/Models/** -> removed",
                "cycle.required_paths: database/migrations/ -> removed"
            ]
        );
    }

    #[test]
    fn adding_strengthening_and_other_keys_are_never_weakenings() {
        let weak = "review:\n  offer: false\nacceptance:\n  required: false\n  dir: t\n  run: [\"x\", \"{report}\", \"{files}\"]\n";
        assert!(keys("", FULL).is_empty());
        assert!(
            keys(
                weak,
                &weak
                    .replace("offer: false", "offer: true")
                    .replace("required: false", "required: true")
            )
            .is_empty()
        );
        assert!(
            keys(
                FULL,
                &FULL.replace(
                    "\"database/migrations/\"]",
                    "\"database/migrations/\", \"routes/*.php\"]"
                )
            )
            .is_empty()
        );
        let others = format!(
            "{FULL}git:\n  base_branch: develop\ndone:\n  commit: true\ncontext:\n  binding_limit: 100\n"
        );
        assert!(keys(FULL, &others).is_empty());
        assert!(keys(&others, FULL).is_empty());
        // Reordering the patterns is not removing one.
        assert!(
            keys(
                FULL,
                &FULL.replace(
                    "[\"app/Models/**\", \"database/migrations/\"]",
                    "[\"database/migrations/\", \"app/Models/**\"]"
                )
            )
            .is_empty()
        );
    }
}
