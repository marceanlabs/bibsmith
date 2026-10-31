//! Disambiguation: the `a`, `b`, `c` suffixes biblatex adds when two works would
//! otherwise print the same label.
//!
//! biblatex calls the number behind the letter `extradate`. It is assigned per
//! group of entries that share a label name and a label year, in the order the
//! bibliography ends up, starting at 1 — so the first `Baez 2004` is 1 (`a`) and the
//! second is 2 (`b`).
//!
//! The oracle is the `\field{extradate}` biber writes into the `.bbl`: 21 entries in
//! the canonical corpus carry one, in six groups.

use std::collections::HashMap;

use biblatex::{ChunksExt, Entry, Person};

use crate::sort::{Scheme, sort_names};

/// letter for an extradate number: 1 -> "a", 26 -> "z", 27 -> "aa".
pub fn suffix_for(n: u8) -> String {
    let mut out = String::new();
    let mut n = n as u32;
    while n > 0 {
        let rem = ((n - 1) % 26) as u8;
        out.insert(0, (b'a' + rem) as char);
        n = (n - 1) / 26;
    }
    out
}

/// The label name list: what biblatex prints as the author position of the label.
///
/// For `authoryear` this is the `labelname` list, which falls back author, editor,
/// translator, then the title for a work with none of them.
fn label_names(entry: &Entry) -> String {
    let names: Vec<Person> = sort_names(entry);
    if names.is_empty() {
        return entry
            .get("sorttitle")
            .or_else(|| entry.get("title"))
            .map(|c| c.format_verbatim())
            .unwrap_or_default()
            .to_lowercase();
    }
    // The given name is part of the label once biblatex has expanded it to tell two
    // people apart, so two entries by `Smith, John` and `Smith, Jane` do not collide and
    // need no 2020a/2020b. Two entries by the same `Smith, John` still do.
    names
        .iter()
        .map(|p| {
            format!(
                "{}\u{1}{}",
                p.name.to_lowercase(),
                p.given_name.to_lowercase()
            )
        })
        .collect::<Vec<_>>()
        .join("\u{2}")
}

/// The label year.
///
/// Two measurements decide this rule, and each of the obvious versions fails one of
/// them:
///
/// - `baez/article` (`date = 2004`) and `baez/online` (`date = {2004-10-27}`) share a
///   label year in biber's output (extradate 1 and 2), so the extra precision must be
///   dropped.
/// - `knuth:ct` (`date = {1984/1986}`, a range) does not share a label year with the
///   1984 volume `knuth:ct:a`, so a range must not collapse to its first year.
fn label_year(entry: &Entry) -> String {
    let raw = entry
        .get("sortyear")
        .or_else(|| entry.get("date"))
        .or_else(|| entry.get("year"))
        .map(|c| c.format_verbatim())
        .unwrap_or_default();
    // A range's label is its two *years*: `2001/2003` and `2001-05/2003-06` both number as
    // 2001–2003, so biber prints 2001–2003a and then 2001–2003b. Leaving the raw string in
    // the key would keep those two apart, and collapsing the range to its start would
    // collide a range with a plain year — the corpus has `knuth:ct` at 1984 next to a
    // 1984–1985 range, and biber prints them both unsuffixed.
    if raw.contains('/') {
        let years: Vec<String> = raw
            .split('/')
            .filter_map(|side| {
                let digits: String = side.chars().filter(|c| c.is_ascii_digit()).collect();
                (digits.len() >= 4).then(|| digits[..4].to_string())
            })
            .collect();
        if years.len() == 2 {
            return format!("{}–{}", years[0], years[1]);
        }
    }
    let digits: String = raw.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.len() >= 4 {
        digits[..4].to_string()
    } else {
        raw
    }
}

/// extradate for every entry that needs one, given entries already in sort order.
///
/// A group of one gets no number: biblatex only prints a suffix when the label
/// would otherwise be ambiguous.
pub fn disambiguate(ordered: &[&Entry], _scheme: Scheme) -> HashMap<String, u8> {
    let mut groups: HashMap<(String, String), Vec<&Entry>> = HashMap::new();
    for entry in ordered {
        // biblatex applies no extradate to an entry with no name to disambiguate: ten
        // copies of one nameless record print "(1984)" ten times, not 1984a…1984j.
        if sort_names(entry).is_empty() {
            continue;
        }
        groups
            .entry((label_names(entry), label_year(entry)))
            .or_default()
            .push(entry);
    }

    let mut out = HashMap::new();
    for ((names, year), members) in groups {
        // Two entries with neither a name nor a title would collide on an empty
        // label. biblatex gives them no suffix (`set` and `stdmodel` in the
        // corpus), because a set's label comes from its members, not itself.
        if names.is_empty() && year.is_empty() {
            continue;
        }
        if members.len() < 2 {
            continue;
        }
        // `ordered` is already in sort order, so the group is too.
        for (i, entry) in members.iter().enumerate() {
            out.insert(entry.key.to_string(), (i + 1) as u8);
        }
    }
    out
}
