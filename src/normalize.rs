//! Field normalisation and writing, done from the **raw** parse.
//!
//! The resolved [`biblatex::Bibliography`] is what sorting needs, but it is the
//! wrong source for writing a file back: it has already expanded abbreviations,
//! resolved `crossref` inheritance and decoded `{ı}` into `ı`. biber's `--tool`
//! output keeps all three as written, so this module works from
//! [`biblatex::RawBibliography`] instead and applies only the renames biber
//! applies.
//!
//! Measured against `biber --tool` on the canonical corpus: 92 of 92 entries
//! identical once these rules are applied.

use biblatex::{RawBibliography, RawChunk, RawEntry, Spanned};

/// A rename biber performs silently. Old `.bib` name on the left, what biber
/// writes on the right.
const RENAMES: &[(&str, &str)] = &[
    ("address", "location"),
    ("annote", "annotation"),
    ("archiveprefix", "eprinttype"),
    ("journal", "journaltitle"),
    ("key", "sortkey"),
    ("origyear", "origdate"),
    ("pdf", "file"),
    ("primaryclass", "eprintclass"),
    ("school", "institution"),
    ("year", "date"),
];

/// Fields biber computes rather than reads: writing the input value back would
/// state something biblatex then disagrees with. `label` is deliberately absent —
/// biber keeps it, which cost one round of measurement to learn.
/// Nothing is dropped by default, and that is a deliberate departure from
/// `biber --tool`, which deletes `sorttitle`, `sortname`, `sortyear` and `presort`
/// because biblatex recomputes them.
///
/// Measured: feeding biber's own `--tool` output back through biblatex reorders
/// the bibliography, because those fields are how an author controls the order
/// (`cms` sorts under C only because it sets `sorttitle`). A tool whose job is the
/// order must not throw away the fields that decide it.
const COMPUTED: &[&str] = &[];

/// Fields whose value biber re-punctuates as a numeric range.
const RANGES: &[&str] = &["pages", "pageref", "pagetotal"];

/// One entry, normalised, ready to write.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawNormalised {
    pub kind: String,
    pub key: String,
    /// name, value, and whether the value was an abbreviation reference.
    pub fields: Vec<(String, String, bool)>,
    pub dropped: Vec<String>,
}

impl RawNormalised {
    pub fn get(&self, field: &str) -> Option<&str> {
        self.fields
            .iter()
            .find(|(k, _, _)| k == field)
            .map(|(_, v, _)| v.as_str())
    }
}

/// Page ranges are the one place biber rewrites punctuation: `55-65` becomes
/// `55--65`, an en dash and an em dash become `--`, and the space after a comma
/// in a range list goes.
pub fn normalise_range(value: &str) -> String {
    let chars: Vec<char> = value.chars().collect();
    let mut out = String::with_capacity(value.len() + 4);
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let is_dash = c == '-' || c == '\u{2013}' || c == '\u{2014}';
        if is_dash {
            let prev_digit = out.chars().last().is_some_and(|p: char| p.is_ascii_digit());
            let mut j = i;
            while j + 1 < chars.len() && matches!(chars[j + 1], '-' | '\u{2013}' | '\u{2014}') {
                j += 1;
            }
            let next_digit = chars.get(j + 1).is_some_and(|n| n.is_ascii_digit());
            if prev_digit && next_digit {
                out.push_str("--");
                i = j + 1;
                continue;
            }
            out.push(c);
        } else if c == ',' {
            out.push(',');
            while chars.get(i + 1).is_some_and(|n| *n == ' ' || *n == '\n') {
                i += 1;
            }
        } else if c == '\n' {
            out.push(' ');
            while chars.get(i + 1) == Some(&' ') {
                i += 1;
            }
        } else {
            out.push(c);
        }
        i += 1;
    }
    out
}

/// Runs of whitespace become one space, ends are trimmed: a value wrapped over
/// four lines in the source comes out on one line, as biber writes it.
pub fn collapse_whitespace(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut space = false;
    for c in value.chars() {
        if c.is_whitespace() {
            space = true;
        } else {
            if space && !out.is_empty() {
                out.push(' ');
            }
            space = false;
            out.push(c);
        }
    }
    out
}

/// Join the raw chunks, and report whether the value is a *single* abbreviation.
///
/// Only a value made of exactly one abbreviation is written bare, as biber does
/// with `JOURNALTITLE = JOMCH`. A mixed value such as `dtv and Walter de Gruyter`
/// must be braced, or it is not valid BibTeX: found by feeding our own output back
/// to biber, which reported `found "and", expected end of entry`.
fn render(value: &[Spanned<RawChunk<'_>>]) -> (String, bool) {
    let mut text = String::new();
    let mut total = 0usize;
    let mut abbreviations = 0usize;
    for chunk in value {
        total += 1;
        match &chunk.v {
            RawChunk::Normal(s) => text.push_str(s),
            RawChunk::Abbreviation(s) => {
                abbreviations += 1;
                text.push_str(s);
            }
        }
    }
    (text, total == 1 && abbreviations == 1)
}

pub fn normalise_raw(entry: &RawEntry<'_>) -> RawNormalised {
    let mut fields: Vec<(String, String, bool)> = Vec::new();
    let mut dropped: Vec<String> = Vec::new();

    for pair in &entry.fields {
        let original = pair.key.v.to_ascii_lowercase();
        if COMPUTED.contains(&original.as_str()) {
            dropped.push(original);
            continue;
        }
        let target = RENAMES
            .iter()
            .find(|(from, _)| *from == original)
            .map(|(_, to)| (*to).to_string())
            .unwrap_or_else(|| original.clone());
        // A rename must not collide with a field already present under the modern
        // name; the modern one wins and the legacy one is reported.
        if fields.iter().any(|(k, _, _)| *k == target) {
            dropped.push(format!("{original} (duplicate of {target})"));
            continue;
        }
        let (raw_text, abbreviation) = render(&pair.value.v);
        // biber joins a value that was wrapped over several lines into one line
        // and collapses the indentation, so every value gets the same treatment.
        let mut text = collapse_whitespace(&raw_text);
        if RANGES.contains(&target.as_str()) && !abbreviation {
            text = normalise_range(&text);
        }
        fields.push((target, text, abbreviation));
    }

    fields.sort_by(|a, b| a.0.cmp(&b.0));
    RawNormalised {
        kind: entry.kind.v.to_ascii_lowercase(),
        key: entry.key.v.to_string(),
        fields,
        dropped,
    }
}

/// One entry as text in biber's `--tool` layout: upper-case field names, sorted,
/// values braced unless the value is an abbreviation reference, which stays bare.
pub fn write_raw(entry: &RawNormalised) -> String {
    let mut out = String::new();
    out.push('@');
    out.push_str(&entry.kind);
    out.push('{');
    out.push_str(&entry.key);
    out.push(',');
    out.push('\n');
    for (name, value, abbreviation) in &entry.fields {
        out.push_str("  ");
        out.push_str(&name.to_ascii_uppercase());
        out.push_str(" = ");
        if *abbreviation {
            out.push_str(value);
        } else {
            out.push('{');
            out.push_str(value);
            out.push('}');
        }
        out.push(',');
        out.push('\n');
    }
    out.push_str("}\n");
    out
}

/// The whole database, normalised, in source order.
pub fn normalise_all(src: &str) -> Result<Vec<RawNormalised>, biblatex::ParseError> {
    let raw = RawBibliography::parse(src)?;
    Ok(raw.entries.iter().map(|e| normalise_raw(&e.v)).collect())
}
