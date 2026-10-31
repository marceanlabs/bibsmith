//! Writing the `.bbl` that biblatex reads.
//!
//! This is the file the document actually typesets, so the bar is not "parses" but
//! "prints the same bibliography". The format is biblatex's, version 3.3: a header it
//! checks, a `\refsection`, one `\datalist` named after the sorting template, and one
//! `\entry` per item.
//!
//! Three details are not cosmetic, and each one was measured against biber before it
//! was written down:
//!
//! * `location` and `publisher` are *list* fields. Written as `\field` they are
//!   silently not printed.
//! * A date is never written as a date. biblatex wants the parts, named after the
//!   field: `date` gives year/month/day, `urldate` gives urlyear/urlmonth/urlday. With
//!   no parts there is no year in the label and the citation prints "()".
//! * The dash that replaces a repeated author comes from comparing name hashes with
//!   the previous entry, so identical authors need identical hashes.

use std::collections::HashMap;

use biblatex::Person;

use crate::bcf::ControlFile;
use crate::merge::{Effective, NAME_FIELDS};

/// Fields that never reach the `.bbl` as ordinary fields: sort controls, name lists
/// (written as `\name`), the links that become `\set`/`\inset`, and dates.
const SKIP_FIELDS: &[&str] = &[
    "sorttitle",
    "sortname",
    "sortyear",
    "presort",
    "sortkey",
    "sortinit",
    "entryset",
    "crossref",
    "xref",
    "date",
    "year",
    "month",
    "day",
    "urldate",
    "eventdate",
    "origdate",
];

/// Fields whose value biblatex prints as a range, with the dash re-punctuated.
const RANGES: &[&str] = &["pages", "pageref", "pagetotal"];

/// Initials in biblatex's notation: one letter per word, separated by
/// `\bibinitdelim`, each followed by `\bibinitperiod`.
fn initials(text: &str, period: &str) -> String {
    let parts: Vec<&str> = text
        .split(|c: char| c.is_whitespace() || c == '-')
        .filter(|s| !s.is_empty())
        .collect();
    let mut out = String::new();
    for (i, part) in parts.iter().enumerate() {
        if i > 0 {
            out.push_str("\\bibinitdelim ");
        }
        // Skip a leading brace or LaTeX command, take the first letter of the word.
        let letter = part
            .chars()
            .find(|c| c.is_alphabetic())
            .unwrap_or(part.chars().next().unwrap_or('?'));
        out.push(letter);
        out.push_str(period);
    }
    out
}

/// A stable hash for a name list.
///
/// biblatex only ever compares these for equality, to decide whether to print a dash
/// instead of a repeated author, so the values must be consistent rather than
/// identical to biber's.
fn name_hash(people: &[Person]) -> String {
    let mut canonical = String::new();
    for p in people {
        canonical.push_str(&p.name);
        canonical.push('|');
        canonical.push_str(&p.given_name);
        canonical.push('|');
        canonical.push_str(&p.prefix);
        canonical.push('|');
        canonical.push_str(&p.suffix);
        canonical.push(';');
    }
    format!("{:x}", md5::compute(canonical.as_bytes()))
}

fn name_block(field_name: &str, people: &[Person]) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "      \\name{{{field_name}}}{{{}}}{{}}{{%\n",
        people.len()
    ));
    for (i, p) in people.iter().enumerate() {
        out.push_str(&format!(
            "        {{{{un=0,uniquepart=base,hash={}}}{{%\n",
            name_hash(std::slice::from_ref(p))
        ));
        if !p.prefix.is_empty() {
            out.push_str(&format!("           prefix={{{}}},\n", p.prefix));
        }
        out.push_str(&format!("           family={{{}}},\n", p.name));
        out.push_str(&format!(
            "           familyi={{{}}},\n",
            initials(&p.name, "\\bibinitperiod")
        ));
        if !p.given_name.is_empty() {
            out.push_str(&format!("           given={{{}}},\n", p.given_name));
            out.push_str(&format!(
                "           giveni={{{}}},\n",
                initials(&p.given_name, "\\bibinitperiod")
            ));
        }
        if !p.suffix.is_empty() {
            out.push_str(&format!("           suffix={{{}}},\n", p.suffix));
        }
        out.push_str("           givenun=0}}%");
        if i + 1 < people.len() {
            out.push('\n');
        }
    }
    out.push_str("\n      }\n");
    out
}

/// An `&` in a field value is a column separator to TeX and disappears from the page
/// unless it is escaped: "Computers & Typesetting" prints as "Computers Typesetting".
fn escape_ampersands(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            out.push(c);
            if let Some(next) = chars.next() {
                out.push(next);
            }
            continue;
        }
        if c == '&' {
            out.push_str("\\&");
        } else {
            out.push(c);
        }
    }
    out
}

fn field(out: &mut String, name: &str, value: &str) {
    out.push_str(&format!("      \\field{{{name}}}{{{value}}}\n"));
}

/// A list-valued field: `location`, `publisher` and the rest of the `list` kind.
fn list(out: &mut String, name: &str, values: &[&str]) {
    out.push_str(&format!("      \\list{{{name}}}{{{}}}{{%\n", values.len()));
    for value in values {
        out.push_str(&format!("        {{{value}}}%\n"));
    }
    out.push_str("      }\n");
}

fn verb(out: &mut String, name: &str, value: &str) {
    out.push_str(&format!("      \\verb{{{name}}}\n"));
    out.push_str(&format!("      \\verb {value}\n"));
    out.push_str("      \\endverb\n");
}

fn strip_leading_zero(s: &str) -> String {
    match s.parse::<u32>() {
        Ok(n) => n.to_string(),
        Err(_) => s.to_string(),
    }
}

/// The parts biblatex wants instead of a date: `2024-03-09` becomes year 2024, month 3,
/// day 9, under the field's own prefix (`urldate` gives urlyear and friends).
///
/// A range such as `1984/1986` is two dates: the start under the prefix and the end
/// under `end` + prefix, which is how biblatex prints "1984–1986".
fn date_parts(out: &mut String, prefix: &str, value: &str) {
    let (start, end) = match value.split_once('/') {
        Some((a, b)) => (a.trim(), Some(b.trim())),
        None => (value.trim(), None),
    };

    write_one_date(out, prefix, start);
    if let Some(end) = end {
        // The end of a range takes the prefix, then `end`: `eventdate` gives
        // eventendyear, not endeventyear, and it needs its own era field or biblatex
        // prints both ends in full instead of collapsing "19-25 May".
        write_one_date(out, &format!("{prefix}end"), end);
        field(out, &format!("{prefix}enddateera"), "ce");
    }
    if !prefix.is_empty() {
        field(out, &format!("{prefix}dateera"), "ce");
    }
}

fn write_one_date(out: &mut String, prefix: &str, value: &str) {
    let mut parts = value.split('-').map(|s| s.trim());
    if let Some(year) = parts.next() {
        if !year.is_empty() {
            field(out, &format!("{prefix}year"), &year[..year.len().min(4)]);
        }
    }
    if let Some(month) = parts.next() {
        if !month.is_empty() {
            field(out, &format!("{prefix}month"), &strip_leading_zero(month));
        }
    }
    if let Some(day) = parts.next() {
        if !day.is_empty() {
            field(out, &format!("{prefix}day"), &strip_leading_zero(day));
        }
    }
}

/// Write the whole file.
pub fn write_bbl(
    ctl: &ControlFile,
    ordered: &[&Effective],
    extras: &HashMap<String, u8>,
) -> String {
    // A `@set` prints its members inline after itself, so they leave the main flow.
    let mut members: HashMap<&str, Vec<&str>> = HashMap::new();
    for entry in ordered {
        if let Some(set) = &entry.entryset {
            for key in set.split(',') {
                members.entry(set.as_str()).or_default().push(key.trim());
            }
        }
    }
    let is_member: Vec<&str> = members
        .values()
        .flatten()
        .copied()
        .filter(|k| ordered.iter().any(|e| e.key == *k))
        .collect();

    let mut out = String::new();
    out.push_str("% $ biblatex auxiliary file $\n");
    out.push_str("% $ biblatex bbl format version 3.3 $\n");
    out.push_str("% Do not modify the above lines!\n");
    out.push_str("%\n");
    out.push_str("% This is an auxiliary file used by the 'biblatex' package.\n");
    out.push_str("% This file may safely be deleted. It will be recreated\n");
    out.push_str("% by bibsmith as required.\n");
    out.push_str("%\n");
    out.push_str("\\begingroup\n\\makeatletter\n");
    out.push_str("\\@ifundefined{ver@biblatex.sty}\n");
    out.push_str("  {\\@latex@error\n");
    out.push_str("     {Missing 'biblatex' package}\n");
    out.push_str("     {The bibliography requires the 'biblatex' package.}\n");
    out.push_str("      \\aftergroup\\endinput}\n");
    out.push_str("  {}\n\\endgroup\n\n\n");
    out.push_str("\\refsection{0}\n");
    out.push_str(&format!("  \\datalist[entry]{{{}}}\n", ctl.datalist_name));

    for entry in ordered {
        if is_member.contains(&entry.key.as_str()) {
            continue; // printed with its set
        }
        write_entry(&mut out, ctl, entry, extras, "");

        if let Some(set) = &entry.entryset {
            for key in members.get(set.as_str()).cloned().unwrap_or_default() {
                if let Some(child) = ordered.iter().find(|e| e.key == key) {
                    // Members are skipped from the list and printed inside the set.
                    write_entry(
                        &mut out,
                        ctl,
                        child,
                        extras,
                        "skipbib=true,skipbiblist=true,skiplab=true",
                    );
                }
            }
        }
    }

    out.push_str("  \\enddatalist\n");
    out.push_str("\\endrefsection\n");
    out.push_str("\\endinput\n");
    out
}

fn write_entry(
    out: &mut String,
    ctl: &ControlFile,
    entry: &Effective,
    extras: &HashMap<String, u8>,
    options: &str,
) {
    out.push_str(&format!(
        "    \\entry{{{}}}{{{}}}{{{options}}}{{}}\n",
        entry.key, entry.kind
    ));

    if let Some(set) = &entry.entryset {
        let command = if options.is_empty() { "set" } else { "inset" };
        out.push_str(&format!("      \\{command}{{{set}}}\n"));
    }

    let mut label_source = String::new();
    for (field_name, people) in &entry.names {
        out.push_str(&name_block(field_name, people));
        if label_source.is_empty()
            && matches!(field_name.as_str(), "author" | "editor" | "translator")
        {
            label_source = field_name.clone();
        }
    }

    // The dash for a repeated author is decided by comparing these with the previous
    // entry's, so identical people must produce identical hashes.
    if let Some((_, people)) = entry.names.iter().find(|(f, _)| *f == label_source) {
        let hash = name_hash(people);
        for key in [
            "namehash",
            "fullhash",
            "fullhashraw",
            "bibnamehash",
            "authorbibnamehash",
            "authornamehash",
            "authorfullhash",
            "authorfullhashraw",
        ] {
            out.push_str(&format!("      \\strng{{{key}}}{{{hash}}}\n"));
        }
    }

    // A first-letter index, which some styles print.
    let sortinit = entry
        .values
        .iter()
        .find(|(k, _)| k == "sorttitle")
        .or_else(|| entry.values.iter().find(|(k, _)| k == "title"))
        .and_then(|(_, v)| v.chars().find(|c| c.is_alphabetic()))
        .map(|c| c.to_uppercase().to_string())
        .unwrap_or_default();
    if !sortinit.is_empty() {
        field(out, "sortinit", &sortinit);
    }

    // Values are the macro-expanded ones: a corpus entry that says
    // `journaltitle = JOMCH` must print "J. Organomet. Chem.", and expanding is not the
    // same as decoding, so `{\TeX}` still prints as TeX.
    for (key, value) in &entry.values {
        if SKIP_FIELDS.contains(&key.as_str()) || NAME_FIELDS.contains(&key.as_str()) {
            continue;
        }
        let target = crate::normalize::renamed_field(key).unwrap_or(key.as_str());
        let value = if RANGES.contains(&key.as_str()) {
            crate::normalize::normalise_range(value)
        } else {
            escape_ampersands(value)
        };
        if ctl.fieldkinds.get(target).is_some_and(|k| k == "list") {
            let values: Vec<&str> = value
                .split(" and ")
                .map(|v| v.trim())
                .filter(|v| !v.is_empty())
                .collect();
            list(out, target, &values);
        } else if matches!(
            ctl.fieldtypes.get(target).map(|s| s.as_str()),
            Some("verbatim") | Some("uri")
        ) {
            verb(out, target, &value);
        } else {
            field(out, target, &value);
        }
    }

    // biblatex counts date parts, never the date itself. An entry may carry a `date`
    // or the older `year`/`month`, and both have to reach it as parts.
    match entry.values.iter().find(|(k, _)| k == "date") {
        Some((_, date)) => date_parts(out, "", date),
        None => {
            if let Some((_, year)) = entry.values.iter().find(|(k, _)| k == "year") {
                let mut combined = year.clone();
                for part in ["month", "day"] {
                    if let Some((_, v)) = entry.values.iter().find(|(k, _)| k == part) {
                        combined.push('-');
                        combined.push_str(v);
                    }
                }
                date_parts(out, "", &combined);
            }
        }
    }
    for (name, prefix) in [
        ("urldate", "url"),
        ("eventdate", "event"),
        ("origdate", "orig"),
    ] {
        if let Some((_, date)) = entry.values.iter().find(|(k, _)| k == name) {
            date_parts(out, prefix, date);
        }
    }

    // biblatex prints the year in the label only when this field is present, even when
    // it is empty: absent means "no label date" and the year moves to the end.
    field(out, "labeldatesource", "");
    if !label_source.is_empty() {
        field(out, "labelnamesource", &label_source);
    }
    field(out, "labeltitlesource", "title");
    field(out, "dateera", "ce");

    if let Some(n) = extras.get(&entry.key) {
        field(out, "extradate", &n.to_string());
        field(out, "extradatescope", "labelyear");
    }

    out.push_str("    \\endentry\n");
}
