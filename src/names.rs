//! Reading a name list, with a fallback for the strings the parser refuses.
//!
//! The `biblatex` crate handles the BibTeX and extended name grammars, and for ordinary
//! names it is right. For a family name that starts with words and a hyphen
//! (`General Chair-Czerwinski, Mary`) it returns one person with an empty family, which
//! prints as an entry with no author and sorts to the front of the bibliography.
//!
//! biblatex itself has no trouble with that string, so falling back to the plain
//! `Family, Given` reading is what keeps the output honest: one entry, in the right place,
//! with the name the author wrote.

use biblatex::{ChunksExt, Entry, Person};

/// A name list for `field`, repaired if the parser produced an empty family name.
pub fn names(entry: &Entry, field: &str) -> Vec<Person> {
    names_with(entry, field, None)
}

/// As `names`, but told what the author actually wrote. The parser answers in Unicode
/// (`{\~N}et` comes back `Ñet`) and LaTeX wants the TeX: a combining accent is not a
/// character pdflatex can set, so a converted name stops the document building.
pub fn names_with(entry: &Entry, field: &str, written: Option<&str>) -> Vec<Person> {
    let Ok(parsed) = entry.get_as::<Vec<Person>>(field) else {
        return Vec::new();
    };
    // `author = {}` must mean "no author", not "an author with no name": a phantom
    // person with an empty family sorts to the front of the bibliography.
    let parsed: Vec<Person> = parsed
        .into_iter()
        .filter(|p| !(p.name.trim().is_empty() && p.given_name.trim().is_empty()))
        .collect();
    let raw = match written {
        Some(text) => text.to_string(),
        None => match entry.get(field) {
            Some(chunks) => chunks.format_verbatim(),
            None => return parsed,
        },
    };
    if parsed.iter().all(|p| !p.name.trim().is_empty()) {
        return parsed;
    }
    let repaired = from_plain(&raw);
    if repaired.is_empty() {
        parsed
    } else {
        repaired
    }
}

fn braces_balanced(text: &str) -> bool {
    let mut depth = 0i32;
    let mut escaped = false;
    for c in text.chars() {
        if escaped {
            escaped = false;
            continue;
        }
        match c {
            '\\' => escaped = true,
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth < 0 {
                    return false;
                }
            }
            _ => {}
        }
    }
    depth == 0
}

/// Split `Family, Given and Family, Given` the way BibTeX always has.
fn from_plain(raw: &str) -> Vec<Person> {
    raw.split(" and ")
        // Only a part wrapped as a whole comes off: `{\'E}douard Masterly` starts with the
        // brace of its own accent, and trimming it leaves a name whose braces no longer
        // balance, which the writer then escapes and the page prints as `\{E\}`.
        .map(|part| {
            let part = part.trim();
            match part.strip_prefix('{').and_then(|p| p.strip_suffix('}')) {
                Some(inner) if braces_balanced(inner) => inner.trim(),
                _ => part,
            }
        })
        .filter(|part| !part.is_empty())
        .map(|part| {
            let (family, given) = match part.split_once(',') {
                Some((family, given)) => (family.trim().to_string(), given.trim().to_string()),
                None => (part.to_string(), String::new()),
            };
            Person {
                name: family,
                given_name: given,
                prefix: String::new(),
                suffix: String::new(),
                id: None,
                prefix_initials: None,
                given_initials: None,
                use_prefix: None,
            }
        })
        .collect()
}
