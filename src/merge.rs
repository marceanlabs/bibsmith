//! Resolving `crossref` the way biblatex expects.
//!
//! A child entry such as an `@incollection` inherits the parent's fields, and the
//! inheritance is declared, not guessed: `biblatex.def` says everything is inherited
//! except an explicit list (`crossref`, `options`, `related*`, `sortkey`, ...), and a
//! handful of type pairs rename a field on the way across (`collection` title becomes
//! the child's `booktitle`).
//!
//! This matters beyond tidiness: an `@incollection` whose `booktitle` and `editor` are
//! only on the parent prints as "In: pp. 55-65" with no book at all.
//!
//! Resolution runs on parsed entries, not on text, because the values have to be the
//! macro-expanded ones: `journaltitle = JOMCH` prints as "J. Organomet. Chem.", and
//! copying raw text instead would turn the abbreviation into an unknown literal.

use std::collections::HashMap;

use biblatex::{Bibliography, Entry, Person};

/// Fields that never cross a `crossref`.
const NO_INHERIT: &[&str] = &[
    "ids",
    "crossref",
    "xref",
    "entryset",
    "entrysubtype",
    "execute",
    "label",
    "options",
    "presort",
    "related",
    "relatedoptions",
    "relatedstring",
    "relatedtype",
    "shorthand",
    "shorthandintro",
    "sortkey",
    "sorttitle",
    "sortname",
    "sortyear",
];

/// A rename applied when a child of one type inherits from a parent of another.
struct Rename {
    parent: &'static [&'static str],
    child: &'static [&'static str],
    from: &'static str,
    to: &'static str,
}

const RENAMES: &[Rename] = &[
    Rename {
        parent: &["book", "mvbook"],
        child: &["inbook", "bookinbook", "suppbook"],
        from: "title",
        to: "booktitle",
    },
    Rename {
        parent: &["book", "mvbook"],
        child: &["inbook", "bookinbook", "suppbook"],
        from: "subtitle",
        to: "booksubtitle",
    },
    Rename {
        parent: &["collection", "reference", "mvcollection", "mvreference"],
        child: &["incollection", "inreference", "suppcollection"],
        from: "title",
        to: "booktitle",
    },
    Rename {
        parent: &["collection", "reference", "mvcollection", "mvreference"],
        child: &["incollection", "inreference", "suppcollection"],
        from: "subtitle",
        to: "booksubtitle",
    },
    Rename {
        parent: &["proceedings", "mvproceedings"],
        child: &["inproceedings"],
        from: "title",
        to: "booktitle",
    },
    Rename {
        parent: &["proceedings", "mvproceedings"],
        child: &["inproceedings"],
        from: "subtitle",
        to: "booksubtitle",
    },
    Rename {
        parent: &["periodical"],
        child: &["article", "suppperiodical"],
        from: "title",
        to: "journaltitle",
    },
    Rename {
        parent: &["periodical"],
        child: &["article", "suppperiodical"],
        from: "subtitle",
        to: "journalsubtitle",
    },
    Rename {
        parent: &["mvbook", "mvcollection", "mvreference", "mvproceedings"],
        child: &[
            "book",
            "inbook",
            "bookinbook",
            "suppbook",
            "collection",
            "reference",
            "incollection",
            "inreference",
            "suppcollection",
            "proceedings",
            "inproceedings",
        ],
        from: "title",
        to: "maintitle",
    },
];

/// Name lists biblatex prints from a `\name` block.
pub const NAME_FIELDS: &[&str] = &[
    "author",
    "editor",
    "translator",
    "bookauthor",
    "commentator",
    "annotator",
    "introduction",
    "foreword",
    "afterword",
    "holder",
    // The a/b/c variants are separate lists that print as their own "Ed. by" runs:
    // `editora` carries a second set of editors with a different role.
    "editora",
    "editorb",
    "editorc",
    "translatora",
    "translatorb",
    "translatorc",
    "commentatora",
    "commentatorb",
    "commentatorc",
    "annotatora",
    "annotatorb",
    "annotatorc",
    "introductiona",
    "introductionb",
    "introductionc",
    "foreworda",
    "forewordb",
    "forewordc",
    "afterworda",
    "afterwordb",
    "afterwordc",
    "holdera",
    "holderb",
    "holderc",
];

/// One entry with its `crossref` parents folded in.
#[derive(Debug, Clone)]
pub struct Effective {
    pub key: String,
    pub kind: String,
    /// Non-name fields, values macro-expanded, in the entry's own order.
    pub values: Vec<(String, String)>,
    /// Name lists, already parsed.
    pub names: Vec<(String, Vec<Person>)>,
    /// The `entryset` link, if this is a `@set` or a member of one.
    pub entryset: Option<String>,
}

fn kind_of(entry: &Entry) -> String {
    format!("{:?}", entry.entry_type).to_ascii_lowercase()
}

/// Expand one entry into the fields biblatex will see.
pub fn effective(
    db: &Bibliography,
    entry: &Entry,
    raw: &HashMap<String, Vec<(String, String)>>,
) -> Effective {
    // Values come from the raw text so braces and backslashes survive: the parser's
    // `{\TeX}` becomes plain `TeX`, which prints without the small-capital E.
    let mut values: Vec<(String, String)> = raw
        .get(&entry.key)
        .cloned()
        .unwrap_or_else(|| {
            entry
                .fields
                .iter()
                .map(|(k, v)| (k.to_ascii_lowercase(), crate::normalize::render_value(v)))
                .collect()
        })
        .into_iter()
        .filter(|(k, _)| !NAME_FIELDS.contains(&k.as_str()))
        .collect();
    let mut names: Vec<(String, Vec<Person>)> = Vec::new();
    for field in NAME_FIELDS {
        if let Ok(people) = entry.get_as::<Vec<Person>>(*field) {
            if !people.is_empty() {
                names.push((field.to_string(), people));
            }
        }
    }
    let entryset = entry
        .get("entryset")
        .map(|c| crate::normalize::render_value(c));

    let mut child_kind = kind_of(entry);
    let mut seen: Vec<String> = vec![entry.key.clone()];
    let mut parent_key = values
        .iter()
        .find(|(k, _)| k == "crossref" || k == "xref")
        .map(|(_, v)| v.trim().to_string());

    while let Some(key) = parent_key {
        if seen.contains(&key) {
            break; // a cycle would otherwise spin forever
        }
        let Some(parent) = db.get(&key) else { break };
        seen.push(key.clone());

        let parent_kind = kind_of(parent);
        let parent_values = raw.get(&key).cloned().unwrap_or_else(|| {
            parent
                .fields
                .iter()
                .map(|(k, v)| (k.to_ascii_lowercase(), crate::normalize::render_value(v)))
                .collect()
        });
        for (name, value) in parent_values.iter() {
            let lower = name.to_ascii_lowercase();
            if NO_INHERIT.contains(&lower.as_str()) || NAME_FIELDS.contains(&lower.as_str()) {
                continue;
            }
            let renamed = RENAMES
                .iter()
                .find(|r| {
                    r.parent.contains(&parent_kind.as_str())
                        && r.child.contains(&child_kind.as_str())
                        && r.from == lower
                })
                .map(|r| r.to.to_string());
            if lower == "title" && renamed.is_none() {
                continue; // a parent title must not overwrite a child's
            }
            let target = renamed.unwrap_or(lower);
            if values.iter().any(|(k, _)| *k == target) {
                continue;
            }
            values.push((target, value.clone()));
        }

        for field in NAME_FIELDS {
            if names.iter().any(|(k, _)| k == field) {
                continue;
            }
            if let Ok(people) = parent.get_as::<Vec<Person>>(*field) {
                if !people.is_empty() {
                    names.push((field.to_string(), people));
                }
            }
        }

        child_kind = parent_kind;
        parent_key = parent_values
            .iter()
            .find(|(k, _)| k == "crossref" || k == "xref")
            .map(|(_, v)| v.trim().to_string());
    }

    Effective {
        key: entry.key.clone(),
        kind: kind_of(entry),
        values,
        names,
        entryset,
    }
}
