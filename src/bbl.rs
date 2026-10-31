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

/// biblatex's `extraname`: the position of an entry among those whose label name is the
/// same, in bibliography order, counting from one.
///
/// Measured on the corpus: four Aristotle texts number 1 to 4, seven Knuth volumes 1 to 7,
/// and a `related` copy is not counted at all, because it is not in the list.
const LABEL_ORDER: &[&str] = &["author", "editor", "translator", "bookauthor"];

/// The letter biblatex files an entry under, one per entry.
///
/// The label name comes first and keeps its case — the corpus has `van Gennep`, and biber
/// writes `v`. `options = {useeditor=false}` has to be honoured, or an entry whose editor
/// is suppressed indexes under the editor instead of its title. A set has neither name nor
/// title, so it takes its first member's letter, which is where the reader finds it.
/// An option that is explicitly turned off: `useeditor=false`.
fn option_off(options: &str, name: &str) -> bool {
    options.split(',').any(|part| {
        part.trim()
            .split_once('=')
            .is_some_and(|(key, value)| key.trim() == name && value.trim() == "false")
    })
}

/// A biblatex entry option: bare means on, `name=false` means off.
fn option_on(options: &str, name: &str) -> bool {
    options.split(',').any(|part| {
        let part = part.trim();
        match part.split_once('=') {
            Some((key, value)) => key.trim() == name && value.trim() != "false",
            None => part == name,
        }
    })
}

fn sortinits(entries: &[&Effective]) -> std::collections::HashMap<String, String> {
    let mut out: std::collections::HashMap<String, String> = Default::default();
    for entry in entries {
        let options = entry
            .values
            .iter()
            .find(|(k, _)| k == "options")
            .map(|(_, v)| v.to_lowercase())
            .unwrap_or_default();
        let name = entry.names.iter().find(|(field, people)| {
            if people.is_empty() || !LABEL_ORDER.contains(&field.as_str()) {
                return false;
            }
            // `useauthor` and `useeditor` are on unless the entry turns them off;
            // `usetranslator` is off unless the entry turns it on.
            match field.as_str() {
                "author" | "editor" => !option_off(&options, &format!("use{field}")),
                "translator" => option_on(&options, "usetranslator"),
                _ => true,
            }
        });
        let source = name
            .and_then(|(_, people)| people.first())
            // The particle is only part of the label when the entry asks for it:
            // `options = {useprefix}` files `van Gennep` under `v`, while the default
            // drops it and files `von Brandt` under `B`.
            .map(|p| match p.prefix.is_empty() || !option_on(&options, "useprefix") {
                true => p.name.clone(),
                false => format!("{} {}", p.prefix, p.name),
            })
            .or_else(|| {
                entry
                    .values
                    .iter()
                    .find(|(k, _)| k == "key" || k == "sortkey")
                    .map(|(_, v)| v.clone())
                    .filter(|v| !v.trim().is_empty())
            })
            .or_else(|| {
                entry
                    .values
                    .iter()
                    .find(|(k, _)| k == "sorttitle")
                    .or_else(|| entry.values.iter().find(|(k, _)| k == "title"))
                    .map(|(_, v)| v.clone())
            })
            .unwrap_or_default();
        let initial = source
            .chars()
            .find(|c| c.is_alphanumeric())
            .map(|c| c.to_string())
            .unwrap_or_default();
        out.insert(entry.key.clone(), initial);
    }
    for entry in entries {
        if out.get(&entry.key).is_some_and(|s| !s.is_empty()) {
            continue;
        }
        let Some(members) = &entry.entryset else { continue };
        let Some(first) = members.split(',').next() else { continue };
        if let Some(initial) = out.get(first.trim()).cloned() {
            out.insert(entry.key.clone(), initial);
        }
    }
    out
}

fn name_ranks(entries: &[&Effective]) -> std::collections::HashMap<String, usize> {
    let mut groups: std::collections::HashMap<String, Vec<&str>> = Default::default();
    for entry in entries {
        if entry.data_key.is_some() {
            continue;
        }
        let Some(label) = entry
            .names
            .iter()
            .find(|(field, people)| !people.is_empty() && LABEL_ORDER.contains(&field.as_str()))
            .map(|(_, people)| people)
        else {
            continue;
        };
        let key: String = label
            .iter()
            .map(|p| format!("{}|{}", p.name.to_lowercase(), p.given_name.to_lowercase()))
            .collect::<Vec<_>>()
            .join(";");
        groups.entry(key).or_default().push(entry.key.as_str());
    }
    let mut ranks = std::collections::HashMap::new();
    for (_, keys) in groups {
        if keys.len() < 2 {
            continue;
        }
        for (i, key) in keys.iter().enumerate() {
            ranks.insert((*key).to_string(), i + 1);
        }
    }
    ranks
}

/// biblatex's `uniquename`, as a level per name.
///
/// 0 when the family name is enough, 1 when initials separate the names, 2 when the whole
/// given name is needed. Measured on a file with Smith/John, Smith/Jane and Smith/John A.:
/// biber gives the first two 2 and the third 1, because `J.` collides but `J. A.` does not.
fn uniqueness(
    entries: &[&Effective],
) -> std::collections::HashMap<(String, String, String), u8> {
    // Scoped to a year: `Other, Olivia` 1990 and `Other, Oscar` 2001 both print as "Other",
    // because the label already tells them apart. Two Smiths in the same year do not.
    let mut families: std::collections::HashMap<(String, String), std::collections::HashSet<String>> =
        Default::default();
    for entry in entries {
        let year = entry_year(entry);
        for (_, people) in &entry.names {
            for p in people {
                families
                    .entry((p.name.to_lowercase(), year.clone()))
                    .or_default()
                    .insert(p.given_name.to_lowercase());
            }
        }
    }
    let mut levels = std::collections::HashMap::new();
    for ((family, year), people) in &families {
        for given in people {
            let level = if people.len() == 1 {
                0
            } else {
                let mine = initials(given, "");
                let shared = people
                    .iter()
                    .filter(|other| initials(other, "") == mine)
                    .count();
                if shared == 1 {
                    1
                } else {
                    2
                }
            };
            levels.insert((family.clone(), given.clone(), year.clone()), level);
        }
    }
    levels
}

/// The year a label would show, for the uniqueness scopes.
fn entry_year(entry: &Effective) -> String {
    let value = |name: &str| {
        entry
            .values
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.clone())
    };
    let raw = value("date").or_else(|| value("year")).unwrap_or_default();
    // The same rule the extradate uses: a range is its two years, anything else is its
    // first four digits. A different rule here splits entries that biblatex keeps
    // together and invents suffixes.
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

/// biblatex's `uniquelist`, as a count per entry.
///
/// Where two entries share a year and the first name, the list has to be shown further
/// until it differs: biber writes `\name{author}{4}{ul=2}{...}`, and prints
/// "Akyildiz, I. F., W. Su, et al." instead of stopping at the first name. If the lists
/// agree all the way, the count is the whole list — biber writes `ul=4` there, and prints
/// every name. Where one name already tells the entries apart, nothing is written.
fn uniquelist(entries: &[&Effective], maxnames: usize) -> std::collections::HashMap<String, usize> {
    fn keys(people: &[biblatex::Person]) -> Vec<String> {
        people
            .iter()
            .map(|p| format!("{}\u{1}{}", p.name.to_lowercase(), p.given_name.to_lowercase()))
            .collect()
    }

    // The year plays no part: two entries by the same first author are compared even when
    // they are years apart, which is why Akyildiz 2002 and Akyildiz 2007 force a second
    // name on the 2002 one.
    let mut groups: std::collections::HashMap<(), Vec<&Effective>> = Default::default();
    for entry in entries {
        if entry.names.is_empty() {
            continue;
        }
        groups.entry(()).or_default().push(entry);
    }

    let mut out = std::collections::HashMap::new();
    for (_, members) in groups {
        if members.len() < 2 {
            continue;
        }
        for entry in &members {
            let Some((field, people)) = entry.names.first() else { continue };
            let mine = keys(people);
            // A list no longer than `maxnames` is printed in full anyway, so extending it
            // would change nothing.
            if mine.is_empty() || mine.len() <= maxnames {
                continue;
            }
            let others: Vec<Vec<String>> = members
                .iter()
                .filter(|o| o.key != entry.key)
                .filter_map(|o| o.names.iter().find(|(f, _)| f == field).map(|(_, p)| keys(p)))
                .collect();

            // Duplicates of my own list are not a clash: they are the same people, and
            // the extradate already separates them. Only a *different* list can be
            // disambiguated by showing more names.
            let mut distinct: Vec<Vec<String>> = Vec::new();
            for theirs in &others {
                if *theirs != mine && !distinct.contains(theirs) {
                    distinct.push(theirs.clone());
                }
            }
            let clashes_at = |n: usize| -> Vec<&Vec<String>> {
                distinct
                    .iter()
                    .filter(|theirs| theirs.len() >= n && theirs[..n] == mine[..n])
                    .collect()
            };

            // The shortest list that no other entry shares. If the whole list is shared,
            // there are two outcomes, and they were measured separately: against a
            // *longer* list that starts the same way, biber shows every name; against an
            // identical list it writes nothing, because showing them all gains nothing.
            let mut n = 1;
            while n <= mine.len() {
                if clashes_at(n).is_empty() {
                    break;
                }
                n += 1;
            }
            if n > mine.len() {
                let longer = distinct
                    .iter()
                    .any(|theirs| theirs.len() > mine.len() && theirs[..mine.len()] == mine[..]);
                if longer {
                    out.insert(entry.key.clone(), mine.len());
                }
                continue;
            }
            if n > 1 {
                out.insert(entry.key.clone(), n);
            }
        }
    }
    out
}

fn name_block(
    field_name: &str,
    people: &[Person],
    year: &str,
    levels: &std::collections::HashMap<(String, String, String), u8>,
    ul: Option<usize>,
) -> String {
    let mut out = String::new();
    let options = match ul {
        Some(n) => format!("ul={n}"),
        None => String::new(),
    };
    out.push_str(&format!(
        "      \\name{{{field_name}}}{{{}}}{{{options}}}{{%\n",
        people.len()
    ));
    for (i, p) in people.iter().enumerate() {
        let level = levels
            .get(&(
                p.name.to_lowercase(),
                p.given_name.to_lowercase(),
                year.to_string(),
            ))
            .copied()
            .unwrap_or(0);
        let part = if level == 0 { "base" } else { "given" };
        out.push_str(&format!(
            "        {{{{un={level},uniquepart={part},hash={}}}{{%\n",
            name_hash(std::slice::from_ref(p))
        ));
        if !p.prefix.is_empty() {
            out.push_str(&format!("           prefix={{{}}},\n", balanced(typeset(&p.prefix))));
        }
        out.push_str(&format!("           family={{{}}},\n", balanced(typeset(&p.name))));
        out.push_str(&format!(
            "           familyi={{{}}},\n",
            balanced(typeset(&initials(&p.name, "\\bibinitperiod")))
        ));
        if !p.given_name.is_empty() {
            // Parts of a given name are joined the way biber joins them: after an
            // initial (a part ending in a full stop) it uses the thin `\bibnamedelimi`,
            // after a word the wider `\bibnamedelima`. One space for both reflows the
            // page: "G. E. L. Owen" breaks in the wrong place.
            let given = join_given_parts(&p.given_name);
            out.push_str(&format!("           given={{{}}},\n", balanced(typeset(&given))));
            out.push_str(&format!(
                "           giveni={{{}}},\n",
                balanced(typeset(&initials(&p.given_name, "\\bibinitperiod")))
            ));
        }
        if !p.suffix.is_empty() {
            out.push_str(&format!("           suffix={{{}}},\n", balanced(typeset(&p.suffix))));
        }
        // No blank line here: `\name` scans its argument, and a blank line is a `\par`,
        // which ends the scan and spills the rest of the attributes onto the page.
        out.push_str("}}%");
        if i + 1 < people.len() {
            out.push('\n');
        }
    }
    out.push_str("\n      }\n");
    out
}

/// Characters that mean something to TeX and must not reach it raw.
///
/// `&` is a column separator, so an unescaped one silently disappears from the page.
/// `%` starts a comment, which is worse: it swallows the closing brace and the whole
/// document fails with "File ended while scanning use of \field". Both turn up in real
/// files — a URL with `%3A` in a `source` field was the first one found.
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
        } else if c == '%' {
            out.push_str("\\%");
        } else {
            out.push(c);
        }
    }
    out
}

/// Join the parts of a given name with the delimiter biblatex expects between them.
fn join_given_parts(given: &str) -> String {
    let parts: Vec<&str> = given.split_whitespace().collect();
    let mut out = String::new();
    for (i, part) in parts.iter().enumerate() {
        if i > 0 {
            let previous = parts[i - 1];
            if previous.ends_with('.') {
                out.push_str("\\bibnamedelimi ");
            } else {
                out.push_str("\\bibnamedelima ");
            }
        }
        out.push_str(part);
    }
    out
}

/// How many pages a normalised range covers: `3027--3036` is ten, `431--456, 791--823` is
/// fifty-nine. `None` when any part is not a number, because a wrong count is worse than no
/// count: the style would print "p." for a range.
fn page_span(value: &str) -> Option<u32> {
    let mut total = 0u32;
    for part in value.split("\\bibrangessep").flat_map(|p| p.split(',')) {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let numbers: Vec<&str> = part
            .split("\\bibrangedash")
            .map(|n| n.trim())
            .collect();
        let parse = |s: &str| -> Option<u32> {
            let digits: String = s.chars().take_while(|c| c.is_ascii_digit()).collect();
            digits.parse().ok()
        };
        match numbers.as_slice() {
            [single] => {
                parse(single)?;
                total += 1;
            }
            [first, last] => {
                let (a, b) = (parse(first)?, parse(last)?);
                total += b.saturating_sub(a) + 1;
            }
            _ => return None,
        }
    }
    (total > 0).then_some(total)
}

fn field(out: &mut String, name: &str, value: &str) {
    let value = balanced(typeset(value));
    out.push_str(&format!("      \\field{{{name}}}{{{value}}}\n"));
}

/// Remove the bytes TeX cannot hold.
///
/// A `.bib` exported from a database sometimes carries a control character — a NUL from a
/// truncated write, a stray BEL. biber gives up on such a file and writes no `.bbl` at all,
/// which leaves the user with no bibliography and no message; the character cannot be
/// typeset either way, so it is dropped and the rest of the entry is kept.
/// Escape the braces of a value whose braces do not pair up.
///
/// A `.bib` written by hand or damaged in transit can hold `{Proceeding{ of the {IEEE}`,
/// and writing that into `\field{…}{…}` leaves LaTeX scanning for a brace that never
/// comes: the error lands a hundred lines later and blames the wrong entry. A value whose
/// braces do pair up is left alone, because those braces are grouping that biblatex needs.
fn balanced(value: String) -> String {
    // A value ending in a single backslash escapes the closing brace that follows it, and
    // the field runs into the next one: `prefix={ineGreenwoo\}` swallows its own delimiter.
    let mut value = value;
    let trailing = value.chars().rev().take_while(|c| *c == '\\').count();
    if trailing % 2 == 1 {
        value.push('\\');
    }
    // A brace that is already escaped is a literal brace, not a group: `\{V\}` is how a
    // `.bib` protects a capital V, and counting it as a group would escape it a second time.
    let mut depth: i32 = 0;
    let mut chars = value.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                chars.next();
            }
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth < 0 {
                    return escape_bare_braces(&value);
                }
            }
            _ => {}
        }
    }
    if depth == 0 {
        return value;
    }
    escape_bare_braces(&value)
}

/// Escape the braces that group, leaving the ones the author already escaped alone.
fn escape_bare_braces(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 8);
    let mut chars = value.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                out.push(c);
                if let Some(next) = chars.next() {
                    out.push(next);
                }
            }
            '{' => out.push_str("\\{"),
            '}' => out.push_str("\\}"),
            other => out.push(other),
        }
    }
    out
}

fn typeset(value: &str) -> String {
    let mut out: String = value
        .chars()
        .filter(|c| !c.is_control() || *c == '\n' || *c == '\t')
        .collect();
    // Removing a control character from the end of `Knuth\x02` leaves the value ending in
    // a backslash, which then escapes whatever follows it — the closing brace of the field.
    // An odd run of backslashes at the end must be made even.
    let trailing = out.chars().rev().take_while(|c| *c == '\\').count();
    if trailing % 2 == 1 {
        out.push('\\');
    }
    out
}

/// A list-valued field: `location`, `publisher` and the rest of the `list` kind.
fn list(out: &mut String, name: &str, values: &[&str]) {
    out.push_str(&format!("      \\list{{{name}}}{{{}}}{{%\n", values.len()));
    for value in values {
        out.push_str(&format!("        {{{}}}%\n", balanced(typeset(value))));
    }
    out.push_str("      }\n");
}

fn verb(out: &mut String, name: &str, value: &str) {
    out.push_str(&format!("      \\verb{{{name}}}\n"));
    out.push_str(&format!("      \\verb {}\n", typeset(value)));
    out.push_str("      \\endverb\n");
}

/// A legacy `month = {January}` reaches biblatex as a name, and a name prints as "jan"
/// instead of "Jan.". biber converts it to the number; this is the same table.
fn month_number(month: &str) -> Option<u8> {
    const NAMES: [&str; 12] = [
        "january",
        "february",
        "march",
        "april",
        "may",
        "june",
        "july",
        "august",
        "september",
        "october",
        "november",
        "december",
    ];
    // `Jan.-March` is January: the range starts there, and the rest is noise.
    let head = month.split(['-', '/']).next().unwrap_or(month);
    let cleaned: String = head
        .chars()
        .filter(|c| c.is_alphabetic())
        .collect::<String>()
        .to_lowercase();
    if cleaned.len() < 3 {
        return None;
    }
    // Three letters identify every month in English, so `Oct.`, `October` and `oct` all
    // agree — and a full-name comparison misses the abbreviation.
    if !head.trim_start().starts_with(|c: char| c.is_alphabetic()) {
        // A year-like or junk value (`10~1`) is passed through exactly as written.
        return None;
    }
    let prefix: String = cleaned.chars().take(3).collect();
    NAMES
        .iter()
        .position(|name| name.starts_with(&prefix))
        .map(|i| i as u8 + 1)
}

/// The parts biblatex wants instead of a date: `2024-03-09` becomes year 2024, month 3,
/// day 9, under the field's own prefix (`urldate` gives urlyear and friends).
///
/// A range such as `1984/1986` is two dates: the start under the prefix and the end
/// under `end` + prefix, which is how biblatex prints "1984–1986".
fn date_parts(out: &mut String, prefix: &str, value: &str) -> bool {
    let (start, end) = match value.split_once('/') {
        Some((a, b)) => (a.trim(), Some(b.trim())),
        None => (value.trim(), None),
    };

    let wrote = write_one_date(out, prefix, start);
    if let Some(end) = end {
        // The end of a range takes the prefix, then `end`: `eventdate` gives
        // eventendyear, not endeventyear, and it needs its own era field or biblatex
        // prints both ends in full instead of collapsing "19-25 May".
        write_one_date(out, &format!("{prefix}end"), end);
        field(out, &format!("{prefix}enddateera"), "ce");
    }
    if !prefix.is_empty() && wrote {
        field(out, &format!("{prefix}dateera"), "ce");
    }
    wrote
}

fn write_one_date(out: &mut String, prefix: &str, value: &str) -> bool {
    // A date is all-or-nothing. `2019-06-01 00:00:00` is what a Zotero export writes,
    // and biber rejects the whole field with a warning rather than printing a month of
    // "100:00:00": the entry then reads "n.d.".
    let Some((year, month, day)) = parse_date(value) else {
        return false;
    };
    field(out, &format!("{prefix}year"), &year);
    if let Some(month) = month {
        field(out, &format!("{prefix}month"), &month);
    }
    if let Some(day) = day {
        field(out, &format!("{prefix}day"), &day);
    }
    true
}

/// Split and validate `YYYY[-MM[-DD]]`, returning nothing if any part is not a date.
fn parse_date(value: &str) -> Option<(String, Option<String>, Option<String>)> {
    let mut parts = value.split(['-', '/']).map(|s| s.trim());
    let year = parts.next()?;
    let year_digits: String = year.chars().take_while(|c| c.is_ascii_digit()).collect();
    if year_digits.len() != 4 {
        return None;
    }
    let mut month_out = None;
    if let Some(month) = parts.next() {
        let parsed = if month.chars().all(|c| c.is_ascii_digit()) {
            match month.parse::<u32>() {
                Ok(n) if (1..=12).contains(&n) => n.to_string(),
                _ => return None,
            }
        } else {
            // A trailing time (`00:00:00`) or any other junk invalidates the date.
            month_number(month)?.to_string()
        };
        month_out = Some(parsed);
    }
    let mut day_out = None;
    if let Some(day) = parts.next() {
        if !day.chars().all(|c| c.is_ascii_digit()) {
            return None;
        }
        match day.parse::<u32>() {
            Ok(n) if (1..=31).contains(&n) => day_out = Some(n.to_string()),
            _ => return None,
        }
    }
    Some((year_digits, month_out, day_out))
}

/// Write the whole file.
pub fn write_bbl(
    ctl: &ControlFile,
    ordered: &[&Effective],
    extras: &HashMap<String, u8>,
    labels: &HashMap<String, (String, Option<u8>)>,
) -> String {
    write_bbl_sections(ctl, &[("0", ordered)], extras, labels)
}

/// The same file for a document with several `\refsection` blocks: one `\refsection`
/// per section, each with its own datalist.
pub fn write_bbl_sections(
    ctl: &ControlFile,
    sections: &[(&str, &[&Effective])],
    extras: &HashMap<String, u8>,
    labels: &HashMap<String, (String, Option<u8>)>,
) -> String {
    let per_section: Vec<HashMap<String, u8>> = vec![extras.clone(); sections.len()];
    write_bbl_scoped(ctl, sections, &per_section, labels)
}

/// The same, with the extradate worked out per section.
///
/// Each `\refsection` is its own namespace: two `Alpha 2001` entries in different sections
/// both print 2001, and numbering them globally invents a 2001a/2001b that biber never
/// writes.
pub fn write_bbl_scoped(
    ctl: &ControlFile,
    sections: &[(&str, &[&Effective])],
    extras_by_section: &[HashMap<String, u8>],
    labels: &HashMap<String, (String, Option<u8>)>,
) -> String {
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

    for (index, (section_no, ordered)) in sections.iter().map(|(n, o)| (n, *o)).enumerate() {
        let extras = extras_by_section
            .get(index)
            .or_else(|| extras_by_section.first())
            .cloned()
            .unwrap_or_default();
        let extras = &extras;
        out.push_str(&format!("\\refsection{{{section_no}}}\n"));
        out.push_str(&format!("  \\datalist[entry]{{{}}}\n", ctl.datalist_name));

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

        // A member records which set it belongs to, because the member's own record does
        // not: only the `@set` carries `entryset`. Without `\\inset`, biblatex has no way to
        // give a cited member the set's number and prints "[]".
        let mut member_of: HashMap<&str, &str> = HashMap::new();
        for entry in ordered {
            if let Some(set) = &entry.entryset {
                for key in set.split(',') {
                    member_of.insert(key.trim(), entry.key.as_str());
                }
            }
        }

        // Uniqueness is a property of the whole bibliography: a name is only shortened or
        // expanded by comparison with the others.
        let levels = uniqueness(ordered);
        let rank = name_ranks(ordered);
        let ranks_init = sortinits(ordered);
        // The style decides whether a name list is ever extended: `numeric` turns
        // `uniquelist` off, and biber writes no `ul` there at all.
        let lists = if ctl.option("uniquelist") == Some("false") {
            Default::default()
        } else {
            uniquelist(
                ordered,
                ctl.option("maxbibnames")
                    .or_else(|| ctl.option("maxnames"))
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(3),
            )
        };

        // Set members first, with the options that keep them out of the list: writing them
        // after the set leaves biblatex without a label number for a cited member.
        for entry in ordered {
            if is_member.contains(&entry.key.as_str()) {
                write_entry(
                    &mut out,
                    ctl,
                    entry,
                    extras,
                    labels,
                    &levels,
                    &rank,
                    &ranks_init,
                    &lists,
                    Some(
                        member_of
                            .get(entry.key.as_str())
                            .copied()
                            .unwrap_or_default(),
                    ),
                    "skipbib=true,skipbiblist=true,skiplab=true",
                );
            }
        }

        for entry in ordered {
            if is_member.contains(&entry.key.as_str()) {
                continue; // written above
            }
            let inset = if entry.entryset.is_some() { Some("") } else { None };
            write_entry(&mut out, ctl, entry, extras, labels, &levels, &rank, &ranks_init, &lists, inset, "");
        }

        out.push_str("  \\enddatalist\n");
        out.push_str("\\endrefsection\n");
    }
    out.push_str("\\endinput\n");
    // One pass over the finished file: a control character can arrive through a key, a name
    // or a field, and none of them can be typeset.
    typeset(&out)
}

#[allow(clippy::too_many_arguments)]
fn write_entry(
    out: &mut String,
    ctl: &ControlFile,
    entry: &Effective,
    extras: &HashMap<String, u8>,
    labels: &HashMap<String, (String, Option<u8>)>,
    levels: &std::collections::HashMap<(String, String, String), u8>,
    rank: &std::collections::HashMap<String, usize>,
    ranks_init: &std::collections::HashMap<String, String>,
    lists: &std::collections::HashMap<String, usize>,
    inset: Option<&str>,
    options: &str,
) {
    // A `related` entry is stored under its hash, and stays out of the list.
    let (entry_key, options) = match &entry.data_key {
        Some(hash) => (
            hash.as_str(),
            "skipbib=true,skipbiblist=true,skiplab=true",
        ),
        None => (entry.key.as_str(), options),
    };
    out.push_str(&format!(
        "    \\entry{{{}}}{{{}}}{{{options}}}{{}}\n",
        balanced(entry_key.to_string()),
        entry.kind
    ));

    match inset {
        // A `@set` lists its members.
        Some("") => {
            if let Some(set) = &entry.entryset {
                out.push_str(&format!("      \\set{{{set}}}\n"));
            }
        }
        // A member points back at its set, which is how biblatex finds the set's label.
        Some(set) if !set.is_empty() => out.push_str(&format!("      \\inset{{{set}}}\n")),
        _ => {}
    }

    let year = entry_year(entry);
    let mut label_source = String::new();
    for (field_name, people) in &entry.names {
        let ul = if label_source.is_empty() { lists.get(&entry.key).copied() } else { None };
        out.push_str(&name_block(field_name, people, &year, levels, ul));
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

    let sortinit = ranks_init.get(entry.key.as_str()).cloned().unwrap_or_default();
    // A `related` copy says which entry it is a copy of; biblatex uses that to print the
    // copy's own citation without treating it as a separate work.
    if entry.data_key.is_some() {
        field(out, "clonesourcekey", &entry.key);
    }
    // `uniquename`: each entry whose label name is shared with another is numbered in
    // bibliography order, and biblatex uses the number to decide whether to add initials.
    // Entries whose name is their own are left without the field.
    match rank.get(entry.key.as_str()) {
        // A `related` copy is written under its hash and is not in the list, so it carries
        // no rank: biber leaves the field off the copies too.
        Some(n) if *n > 0 && entry.data_key.is_none() => field(out, "extraname", &n.to_string()),
        _ => {}
    }
    if !sortinit.is_empty() {
        field(out, "sortinit", &sortinit);
        // biber writes the hash of the sortinit as well; biblatex uses it to tell one
        // initial group from another without comparing the letters.
        let digest = format!("{:x}", md5::compute(sortinit.as_bytes()));
        field(out, "sortinithash", &digest);
    }

    // Values are the macro-expanded ones: a corpus entry that says
    // `journaltitle = JOMCH` must print "J. Organomet. Chem.", and expanding is not the
    // same as decoding, so `{\TeX}` still prints as TeX.
    // A field written under both its old and new name keeps the new one: `location`
    // beats `address`, and writing both leaves the last one printed, which is the old
    // value.
    let spelled: std::collections::HashSet<&str> =
        entry.values.iter().map(|(k, _)| k.as_str()).collect();

    for (key, value) in &entry.values {
        if SKIP_FIELDS.contains(&key.as_str()) || NAME_FIELDS.contains(&key.as_str()) {
            continue;
        }
        // `origyear` is a field in its own right, not a stray spelling of `origdate`:
        // biblatex reads it, and biber writes both it and `year`.
        let legacy_part = matches!(key.as_str(), "origyear" | "origmonth" | "origday");
        if !legacy_part
            && crate::normalize::renamed_field(key).is_some_and(|target| spelled.contains(target))
        {
            continue;
        }
        let target = match legacy_part {
            true => key.as_str(),
            false => crate::normalize::renamed_field(key).unwrap_or(key.as_str()),
        };
        // `key` is renamed to `sortkey`, which is a sort control: biblatex never reads it.
        if SKIP_FIELDS.contains(&target) {
            continue;
        }
        let is_verbatim = matches!(
            ctl.fieldtypes.get(target).map(|s| s.as_str()),
            Some("verbatim") | Some("uri")
        );
        let value = if is_verbatim {
            // Inside `\verb ... \endverb` nothing is special: an escaped `&` prints a
            // literal backslash. biber writes these values raw.
            value.clone()
        } else if crate::normalize::RANGES.contains(&key.as_str()) {
            // biblatex prints ranges with its own macros, not a literal dash: `431--456,
            // 791--823` is two ranges and the separator between them is `\bibrangessep`,
            // which lets a style change it without touching the data.
            crate::normalize::normalise_range(value)
                .replace("--", "\\bibrangedash ")
                // `431--456,791--823` and `431--456, 791--823` are the same two ranges, and
                // biblatex wants the separator between them either way.
                .replace(", ", "\\bibrangessep ")
                .replace(',', "\\bibrangessep ")
        } else if target == "keywords" {
            // A keyword list is `\keyw`, not a field: biblatex reads it as its own list and
            // `\field{keywords}` prints nothing.
            let words: Vec<&str> = value
                .split(',')
                .map(|w| w.trim())
                .filter(|w| !w.is_empty())
                .collect();
            out.push_str(&format!("      \\keyw{{{}}}\n", words.join(",")));
            continue;
        } else {
            escape_ampersands(value)
        };
        // `\range{pages}{10}` says how many pages the entry covers, which a style uses to
        // decide between "pp." and "p." without reading the field itself.
        let page_count = if target == "pages" {
            page_span(&value)
        } else {
            None
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
            if let Some(count) = page_count {
                out.push_str(&format!("      \\range{{pages}}{{{count}}}\n"));
            }
        }
    }

    // biblatex counts date parts, never the date itself. An entry may carry a `date`
    // or the older `year`/`month`/`day`, and both have to reach it as parts — but a
    // legacy month is not a date: `Jan.-March` is one month, and splitting it on the
    // dash hands biblatex a day of "March".
    let has_date = match entry.values.iter().find(|(k, _)| k == "date") {
        Some((_, date)) => date_parts(out, "", date),
        None => {
            let mut wrote_year = false;
            if let Some((_, year)) = entry.values.iter().find(|(k, _)| k == "year") {
                let digits: Vec<char> = year.chars().filter(|c| c.is_ascii_digit()).collect();
                if digits.len() == 4 {
                    field(out, "year", &digits.iter().collect::<String>());
                    wrote_year = true;
                }
            }
            if let Some((_, month)) = entry.values.iter().find(|(k, _)| k == "month") {
                let month = month_number(month)
                    .map(|n| n.to_string())
                    .unwrap_or_else(|| month.trim().to_string());
                field(out, "month", &month);
            }
            // A legacy `day` is never written. biber emits a day only from a `date`, and
            // on a `year`/`month`/`day` entry it prints "Mar. 2008" — the 5 is dropped.
            wrote_year
        }
    };

    // The other date fields keep their prefix: `urldate` gives urlyear/urlmonth/urlday,
    // `eventdate` gives eventyear… and the end of a range gives eventendyear.
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
    // `nodate` is what makes biblatex print "(n.d.)" instead of "()" for an entry whose
    // date it could not read — a Zotero export writes `2019-06-01 00:00:00`.
    field(
        out,
        "labeldatesource",
        if has_date { "" } else { "nodate" },
    );
    if has_date {
        field(out, "dateera", "ce");
    }
    if !label_source.is_empty() {
        field(out, "labelnamesource", &label_source);
    }
    // The label prints the short title when there is one, and biber records that
    // choice in the .bbl: with `title` hardcoded, an authortitle citation prints the
    // whole title instead of the short one.
    let title_source = if entry.values.iter().any(|(k, _)| k == "shorttitle") {
        "shorttitle"
    } else {
        "title"
    };
    field(out, "labeltitlesource", title_source);

    // The alphabetic label, and the number that disambiguates a repeated one:
    // biblatex prints the letter itself from the number.
    if let Some((label, extra)) = labels.get(&entry.key) {
        if !label.is_empty() {
            field(out, "labelalpha", label);
        }
        if let Some(n) = extra {
            field(out, "extraalpha", &n.to_string());
        }
    }

    // A `related` entry is data, not a bibliography item: it carries no label of its own.
    if let Some(n) = entry.data_key.is_none().then(|| extras.get(&entry.key)).flatten() {
        field(out, "extradate", &n.to_string());
        // `extraname` counts the occurrences of one name list in a year. biblatex uses it
        // to decide which of them prints the name and which get the dash: with ten copies
        // the first and the last are named and the eight between are dashed, and without
        // this field it dashes all but the first.
        field(out, "extraname", &n.to_string());
        field(out, "extradatescope", "labelyear");
    }

    // An entry that only a wildcard `\nocite{*}` pulled in is marked as such: biblatex
    // prints it without a number in the numeric styles and the style can tell the two apart.
    if entry.nocite {
        out.push_str("      \\true{nocite}\n");
    }
    out.push_str("    \\endentry\n");
}
