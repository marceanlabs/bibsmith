//! Building the short label an `alphabetic` bibliography prints.
//!
//! biblatex's alphabetic style labels an entry with a few letters and the last two
//! digits of the year: `GMS94` for three authors, `Alm+98` for more than three,
//! `Gla61` for one. The recipe is not guessed: it arrives in the `.bcf` as a
//! `labelalphatemplate`, and this module follows that template.
//!
//! The template's first element is a list of alternatives, first non-empty wins:
//! `shorthand`, then `label`, then the label name with a substring width. The second
//! element takes two characters from the right of the year.
//!
//! `extraalpha` is what turns two entries labelled `Gao01` into `Gao01a` and `Gao01b`;
//! biblatex needs the number, not the letter.

use crate::merge::Effective;

/// The label name list: author, else editor, else translator.
fn label_name_field(entry: &Effective) -> Option<String> {
    ["author", "editor", "translator"]
        .iter()
        .find(|f| entry.names.iter().any(|(name, _)| name == *f))
        .map(|f| (*f).to_string())
}

/// Characters of a string that belong in a label: letters and digits, no TeX markup.
fn plain(text: &str) -> Vec<char> {
    text.chars().filter(|c| c.is_alphanumeric()).collect()
}

/// Build the label for one entry.
///
/// `substring_width` and `substring_side` come from the control file; the multi-name
/// behaviour is biblatex's: one name takes three characters, two or three names take
/// one character each, and more than three names take three characters from the first
/// and a plus sign.
pub fn label_alpha(entry: &Effective, year_width: usize) -> String {
    label_alpha_with(entry, year_width, None)
}

/// Build the label, following a `@set` to its first member: biblatex labels a set the
/// way it labels the item that opens it.
pub fn label_alpha_with(
    entry: &Effective,
    year_width: usize,
    first_member: Option<&Effective>,
) -> String {
    if let Some(member) = first_member {
        return label_alpha_with(member, year_width, None);
    }
    let get = |name: &str| {
        entry
            .values
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.clone())
    };

    let mut label = String::new();
    let mut final_part = false;
    if let Some(shorthand) = get("shorthand") {
        label.push_str(shorthand.trim());
        final_part = true;
    } else if let Some(explicit) = get("label") {
        // `label` is not final in the template: "CTAN" plus a year is `CTAN06`.
        label.push_str(explicit.trim());
    } else if let Some(field) = label_name_field(entry) {
        let people: Vec<&biblatex::Person> = entry
            .names
            .iter()
            .find(|(name, _)| *name == field)
            .map(|(_, people)| people.iter().collect())
            .unwrap_or_default();
        match people.len() {
            0 => {}
            1 => {
                // With a name prefix the label keeps it: "van Gennep" is `vGen`, not
                // `Gen`.
                let keeps_prefix = get("options")
                    .map(|o| !o.to_lowercase().contains("useprefix=false"))
                    .unwrap_or(true);
                if keeps_prefix {
                    if let Some(prefix) = plain(&people[0].prefix).first() {
                        label.push(*prefix);
                    }
                }
                let chars = plain(&people[0].name);
                label.extend(chars.iter().take(3));
            }
            2 | 3 => {
                for person in people.iter().take(3) {
                    if let Some(c) = plain(&person.name).first() {
                        label.push(*c);
                    }
                }
            }
            _ => {
                let chars = plain(&people[0].name);
                label.extend(chars.iter().take(3));
                label.push('+');
            }
        }
    }

    if year_width > 0 && !final_part {
        if let Some(year) = get("year").or_else(|| get("date")) {
            // The first four digits are the year: `2004-10-27` and `1996/2010` both
            // start with the year that labels the entry, and taking the last four would
            // pick up the day or the end of a range.
            let digits: Vec<char> = year.chars().filter(|c| c.is_ascii_digit()).collect();
            if digits.len() >= 4 {
                label.extend(&digits[2..4]);
            } else {
                label.extend(digits.iter());
            }
        }
    }

    label
}

trait Counting<'a> {
    fn entering(&mut self, key: &'a str) -> u8;
}

impl<'a> Counting<'a> for std::collections::HashMap<&'a str, u8> {
    fn entering(&mut self, key: &'a str) -> u8 {
        let slot = self.entry(key).or_insert(0);
        *slot += 1;
        *slot
    }
}

/// The `extraalpha` biblatex prints after a repeated label.
///
/// Entries with the same label are numbered in bibliography order, and biblatex adds
/// the matching letter itself: the first gets no number.
pub fn extra_alpha(labels: &[String]) -> Vec<Option<u8>> {
    // A label used once gets no number at all; a repeated one is numbered from 1 in
    // bibliography order, and biblatex prints 1 as "a".
    let mut totals: std::collections::HashMap<&str, u8> = std::collections::HashMap::new();
    for label in labels {
        *totals.entry(label.as_str()).or_insert(0) += 1;
    }
    let mut seen: std::collections::HashMap<&str, u8> = std::collections::HashMap::new();
    labels
        .iter()
        .map(|label| {
            let total = totals.get(label.as_str()).copied().unwrap_or(1);
            let count = seen.entering(label.as_str());
            if total > 1 { Some(count) } else { None }
        })
        .collect()
}
