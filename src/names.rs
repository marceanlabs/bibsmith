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
    let Ok(parsed) = entry.get_as::<Vec<Person>>(field) else {
        return Vec::new();
    };
    // `author = {}` must mean "no author", not "an author with no name": a phantom
    // person with an empty family sorts to the front of the bibliography.
    let parsed: Vec<Person> = parsed
        .into_iter()
        .filter(|p| !(p.name.trim().is_empty() && p.given_name.trim().is_empty()))
        .collect();
    if parsed.iter().all(|p| !p.name.trim().is_empty()) {
        return parsed;
    }
    let Some(chunks) = entry.get(field) else { return parsed };
    let raw = chunks.format_verbatim();
    let repaired = from_plain(&raw);
    if repaired.is_empty() {
        parsed
    } else {
        repaired
    }
}

/// Split `Family, Given and Family, Given` the way BibTeX always has.
fn from_plain(raw: &str) -> Vec<Person> {
    raw.split(" and ")
        .map(|part| part.trim().trim_matches(|c| c == '{' || c == '}'))
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
