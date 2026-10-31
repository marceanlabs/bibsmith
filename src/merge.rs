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
    /// Set when the entry is written only so a `related` field can be resolved: biblatex
    /// stores it under this hash rather than under its citekey.
    pub data_key: Option<String>,
    pub key: String,
    pub kind: String,
    /// Non-name fields, values macro-expanded, in the entry's own order.
    pub values: Vec<(String, String)>,
    /// Name lists, already parsed.
    pub names: Vec<(String, Vec<Person>)>,
    /// The `entryset` link, if this is a `@set` or a member of one.
    pub entryset: Option<String>,
    /// A name that cannot be parsed, as biber reports it. The entry is then skipped:
    /// one bad author must not shift every label after it.
    pub malformed_name: Option<String>,
    /// The entry is in the bibliography only because of a wildcard `\nocite{*}`, never
    /// because the document cited it. biblatex prints it differently and needs to know.
    pub nocite: bool,
}

/// The entry type as biblatex spells it.
///
/// A non-standard type such as `@underreview` is an `Unknown` in the parser, and Debug
/// prints `Unknown("underreview")`, which biblatex cannot resolve: the entry is then
/// dropped from the list. biber writes the name the author used.
fn kind_of(entry: &Entry) -> String {
    let raw = match &entry.entry_type {
        biblatex::EntryType::Unknown(name) => name.to_ascii_lowercase(),
        other => format!("{other:?}").to_ascii_lowercase(),
    };
    match raw.as_str() {
        "mastersthesis" => "thesis".to_string(),
        "phdthesis" => "thesis".to_string(),
        "techreport" => "report".to_string(),
        "conference" => "inproceedings".to_string(),
        "electronic" | "www" => "online".to_string(),
        _ => raw,
    }
}

/// The `type` biblatex implies from a BibTeX-only entry type.
///
/// `@mastersthesis` is a `thesis` with `type = {mathesis}`: without it the driver prints
/// the title and year and nothing else, and the "MA thesis." line disappears.
fn implied_type(entry: &Entry) -> Option<&'static str> {
    let raw = match &entry.entry_type {
        biblatex::EntryType::Unknown(name) => name.to_ascii_lowercase(),
        other => format!("{other:?}").to_ascii_lowercase(),
    };
    match raw.as_str() {
        "mastersthesis" => Some("mathesis"),
        "phdthesis" => Some("phdthesis"),
        "techreport" => Some("techreport"),
        _ => None,
    }
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
        let written = raw.get(&entry.key).and_then(|fields| {
            fields
                .iter()
                .find(|(k, _)| k == field)
                .map(|(_, v)| v.as_str())
        });
        let people = crate::names::names_with(entry, field, written);
        if !people.is_empty() {
            names.push((field.to_string(), people));
        }
    }
    let entryset = entry
        .get("entryset")
        .map(crate::normalize::render_value);

    // Checked on the raw text: the parser quietly accepts `Werneck,, Renato`, and the
    // double comma is exactly what biber refuses. One bad author must not shift every
    // label after it, so the entry is skipped rather than written.
    let malformed_name = raw.get(&entry.key).and_then(|fields| {
        ["author", "editor", "translator"].iter().find_map(|field| {
            fields
                .iter()
                .find(|(k, _)| k == field)
                .and_then(|(_, v)| v.split(" and ").find(|name| name.contains(",,")))
                .map(|name| name.trim().to_string())
        })
    });

    // `xdata` is a list of entries whose fields are merged in. Measured against biber:
    // the inherited value wins over the entry's own, so this overwrites rather than
    // fills — an entry with `location = {Cambridge}` and an xdata `location = {Oxford}`
    // prints Oxford.
    let xdata_list = values
        .iter()
        .find(|(k, _)| k == "xdata")
        .map(|(_, v)| v.clone());
    if let Some(list) = xdata_list {
        for source in list.split(',').map(|k| k.trim()).filter(|k| !k.is_empty()) {
            let Some(shared) = raw.get(source) else { continue };
            for (name, value) in shared {
                if NO_INHERIT.contains(&name.as_str()) || NAME_FIELDS.contains(&name.as_str()) {
                    continue;
                }
                match values.iter_mut().find(|(k, _)| k == name) {
                    Some(slot) => slot.1 = value.clone(),
                    None => values.push((name.clone(), value.clone())),
                }
            }
        }
    }

    // A BibTeX-only type implies a `type` field, and an explicit one wins.
    if let Some(name) = implied_type(entry) {
        if !values.iter().any(|(k, _)| k == "type") {
            values.push(("type".to_string(), name.to_string()));
        }
    }

    let mut child_kind = kind_of(entry);
    let mut seen: Vec<String> = vec![entry.key.clone()];
    let mut parent_key = values
        .iter()
        .find(|(k, _)| k == "crossref" || k == "xref")
        .map(|(_, v)| v.trim().to_string());

    // biblatex's `\inherit{author}{bookauthor}` for a book parent: the book's author is
    // the child's *book* author, and it stays that way down a chain (a chapter of a
    // chapter still names the book's author after "In:"). The rule is applied after the
    // walk because the ancestor that carries it may be two hops up.
    let own_kind = kind_of(entry);
    let mut book_author: Option<Vec<Person>> = None;

    while let Some(key) = parent_key {
        if seen.contains(&key) {
            break; // a cycle would otherwise spin forever
        }
        let Some(parent) = db.get(&key) else { break };
        seen.push(key.clone());

        let parent_kind = kind_of(parent);
        if matches!(parent_kind.as_str(), "book" | "mvbook") && book_author.is_none() {
            let people = crate::names::names(parent, "author");
            if !people.is_empty() {
                book_author = Some(people);
            }
        }
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
            let people = crate::names::names(parent, field);
            if !people.is_empty() {
                names.push((field.to_string(), people));
            }
        }

        child_kind = parent_kind;
        parent_key = parent_values
            .iter()
            .find(|(k, _)| k == "crossref" || k == "xref")
            .map(|(_, v)| v.trim().to_string());
    }

    if let Some(people) = book_author {
        if matches!(own_kind.as_str(), "inbook" | "bookinbook" | "suppbook")
            && !names.iter().any(|(k, _)| k == "bookauthor")
        {
            names.push(("bookauthor".to_string(), people));
        }
    }

    Effective {
        // The key a `related` entry is written under: biblatex looks it up by this string,
        // not by the citekey, so the parent's `related` field carries it too.
        data_key: None,
        key: entry.key.clone(),
        kind: kind_of(entry),
        values,
        names,
        entryset,
        malformed_name,
        nocite: false,
    }
}
