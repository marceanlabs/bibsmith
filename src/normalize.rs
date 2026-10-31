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

use std::collections::HashMap;

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
/// Fields whose value biber re-punctuates as a numeric range.
pub const RANGES: &[&str] = &["pages", "pageref", "pagetotal"];

/// What biber calls this field, if it renames it.
///
/// The `.bbl` writer needs the same table: an entry that says `journal` must reach
/// biblatex as `journaltitle`, or the style prints "In: 4.3" with no journal name.
pub fn renamed_field(name: &str) -> Option<&'static str> {
    RENAMES
        .iter()
        .find(|(from, _)| *from == name)
        .map(|(_, to)| *to)
}

/// One entry, normalised, ready to write.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawNormalised {
    pub kind: String,
    pub key: String,
    /// name, value, and whether the value was an abbreviation reference.
    pub fields: Vec<(String, String, bool)>,
    /// Fields whose name changes, as (what the author wrote, what biber writes).
    pub renamed: Vec<(String, String)>,
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
            let mut skipped = false;
            while chars.get(i + 1).is_some_and(|n| *n == ' ' || *n == '\n') {
                i += 1;
                skipped = true;
            }
            // The space after a comma is part of how the range prints: "431–456, 791–798".
            if skipped {
                out.push(' ');
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
    normalise_raw_with(entry, true)
}

/// `rename = false` keeps the author's own field names.
///
/// The default `sort` must not rename anything: `to_biblatex_string` escapes braces and
/// backslashes to make a *safe* file, which turns `\emph{y}` into `\\\\emph\\{y\\}` and
/// `100%` into `100\%`. biblatex reads those literally, so a database that has been
/// through that writer no longer means what it said.
pub fn normalise_raw_with(entry: &RawEntry<'_>, rename: bool) -> RawNormalised {
    normalise_raw_macros(entry, rename, &HashMap::new())
}

fn normalise_raw_macros(
    entry: &RawEntry<'_>,
    rename: bool,
    macros: &HashMap<String, String>,
) -> RawNormalised {
    let mut fields: Vec<(String, String, bool)> = Vec::new();
    let mut renamed: Vec<(String, String)> = Vec::new();
    let mut dropped: Vec<String> = Vec::new();

    for pair in &entry.fields {
        let original = pair.key.v.to_ascii_lowercase();
        let target = if rename {
            RENAMES
                .iter()
                .find(|(from, _)| *from == original)
                .map(|(_, to)| (*to).to_string())
                .unwrap_or_else(|| original.clone())
        } else {
            original.clone()
        };
        // A rename must not collide with a field already present under the modern
        // name; the modern one wins and the legacy one is reported.
        if fields.iter().any(|(k, _, _)| *k == target) {
            dropped.push(format!("{original} (duplicate of {target})"));
            continue;
        }
        if target != original {
            renamed.push((original.clone(), target.clone()));
        }
        let (raw_text, abbreviation) = render(&pair.value.v);
        // biber joins a value that was wrapped over several lines into one line
        // and collapses the indentation, so every value gets the same treatment.
        let mut text = collapse_whitespace(&raw_text);
        if rename && crate::merge::NAME_FIELDS.contains(&target.as_str()) && !abbreviation {
            text = family_first(&text);
        }
        // A macro that is not the whole value has to become its text: biber writes
        // `dtv and {Walter de Gruyter}` as `Deutscher Taschenbuch-Verlag and Walter de
        // Gruyter`. A value that is exactly one macro stays bare.
        if rename && !abbreviation && text.contains(char::is_alphabetic) {
            let mut expanded = String::with_capacity(text.len());
            let mut rest = text.as_str();
            while let Some(pos) = rest.find(|c: char| c.is_alphabetic()) {
                let (before, from) = rest.split_at(pos);
                expanded.push_str(before);
                let end = from
                    .find(|c: char| !(c.is_alphanumeric() || c == '-' || c == '_'))
                    .unwrap_or(from.len());
                let word = &from[..end];
                match macros.get(&word.to_ascii_lowercase()) {
                    Some(value) => expanded.push_str(value),
                    None => expanded.push_str(word),
                }
                rest = &from[end..];
            }
            expanded.push_str(rest);
            text = expanded;
        }
        if target == "options" && rename {
            // biber writes `useprefix=1`, not `useprefix`.
            text = text
                .split(',')
                .map(|o| {
                    let o = o.trim();
                    if o.is_empty() || o.contains('=') {
                        o.to_string()
                    } else {
                        format!("{o}=1")
                    }
                })
                .collect::<Vec<_>>()
                .join(",");
        }
        if rename {
            // `biber --tool` writes UTF-8: `{\"O}zge` is `Özge` in its output. Sorting
            // without --normalise leaves the file as the author wrote it.
            text = decode_accents(&text);
        }
        // biber --tool does rewrite a range: `55-65` becomes `55--65`, and an en dash
        // becomes `--` as well. Measured, not assumed.
        if RANGES.contains(&target.as_str()) && !abbreviation {
            // A `.bib` page range is written without the space after a comma: `biber --tool`
            // gives `431--456,791--823`. biblatex prints with a space, and the `.bbl` writer
            // puts it back where it belongs.
            text = normalise_range(&text).replace(", ", ",");
        }
        fields.push((target, text, abbreviation));
    }

    fields.sort_by(|a, b| a.0.cmp(&b.0));
    RawNormalised {
        kind: entry.kind.v.to_ascii_lowercase(),
        key: entry.key.v.to_string(),
        fields,
        renamed,
        dropped,
    }
}

/// One entry as text in biber's `--tool` layout: upper-case field names, sorted,
/// values braced unless the value is an abbreviation reference, which stays bare.
pub fn write_raw(entry: &RawNormalised) -> String {
    let mut out = String::new();
    out.push('@');
    // biber writes the type and the field names in capitals.
    out.push_str(&entry.kind.to_uppercase());
    out.push('{');
    out.push_str(&entry.key);
    out.push(',');
    out.push('\n');
    for (name, value, abbreviation) in &entry.fields {
        out.push_str("  ");
        out.push_str(&name.to_ascii_uppercase());
        out.push_str(" = ");
        if *abbreviation {
            // biber writes a macro name in capitals, whatever case the author used.
            out.push_str(&value.to_uppercase());
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

/// Every entry's fields exactly as the author wrote them, keyed by citation key.
///
/// The `.bbl` writer needs this rather than the resolved values: biblatex prints
/// what the author wrote, and `{\TeX}` decoded to `TeX` prints differently.
/// A citation key and the fields written for it.
pub type KeyedFields = (String, Vec<(String, String)>);

/// One entry's author-written fields, typed but not renamed.
///
/// The `.bbl` writer and `crossref` resolution both work from this: biblatex prints
/// the author's own field names and values, so neither wants the normalised form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawEntryView {
    pub key: String,
    pub kind: String,
    pub fields: Vec<(String, String)>,
}

/// Every entry, with its fields exactly as written.
pub fn raw_entries(src: &str) -> Result<Vec<RawEntryView>, biblatex::ParseError> {
    let raw = RawBibliography::parse(src)?;
    Ok(raw
        .entries
        .iter()
        .map(|e| RawEntryView {
            kind: e.v.kind.v.to_ascii_lowercase(),
            key: e.v.key.v.to_string(),
            fields: e
                .v
                .fields
                .iter()
                .map(|p| {
                    let (text, _) = render(&p.value.v);
                    (p.key.v.to_ascii_lowercase(), text)
                })
                .collect(),
        })
        .collect())
}

/// The whole database, normalised, in source order.
pub fn normalise_all(src: &str) -> Result<Vec<RawNormalised>, biblatex::ParseError> {
    normalise_all_with(src, true)
}

pub fn normalise_all_with(src: &str, rename: bool) -> Result<Vec<RawNormalised>, biblatex::ParseError> {
    let raw = RawBibliography::parse(src)?;
    let macros = if rename { macro_table(src) } else { Default::default() };
    Ok(raw
        .entries
        .iter()
        .map(|e| normalise_raw_macros(&e.v, rename, &macros))
        .collect())
}

/// Render a field value for the `.bbl`, keeping the braces the author wrote.
///
/// `format_verbatim` drops them, and that changes the printed page: `{\TeX}` prints
/// the E in small capitals ("TEX"), plain `\TeX` does not ("TeX"). biber keeps the
/// braces, so they are kept here too. Macros are already expanded by the time a value
/// reaches this function, so there is nothing to resolve.
pub fn render_value(chunks: &[biblatex::Spanned<biblatex::Chunk>]) -> String {
    let mut out = String::new();
    let mut prev_was_whitespace = false;
    for chunk in chunks {
        match &chunk.v {
            biblatex::Chunk::Normal(text) => {
                for mut c in text.chars() {
                    if c == '\n' || c == '\r' {
                        if prev_was_whitespace {
                            continue;
                        }
                        c = ' ';
                    }
                    out.push(c);
                    prev_was_whitespace = c.is_whitespace();
                }
            }
            biblatex::Chunk::Verbatim(text) => {
                out.push('{');
                out.push_str(text);
                out.push('}');
                prev_was_whitespace = false;
            }
            biblatex::Chunk::Math(text) => {
                out.push('$');
                out.push_str(text);
                out.push('$');
                prev_was_whitespace = false;
            }
        }
    }
    out.trim().to_string()
}

/// The `@string` table, keyed by lowercased abbreviation name.
///
/// The parser resolves abbreviations but the resolved `Entry` loses the braces and
/// backslashes the author wrote (`{\TeX}` becomes `TeX`, which prints differently), so
/// the `.bbl` is written from the raw text and expanded here instead.
pub fn macro_table(src: &str) -> HashMap<String, String> {
    let mut table: HashMap<String, String> = HashMap::new();
    let lower = src.to_ascii_lowercase();
    let mut from = 0usize;
    while let Some(pos) = lower[from..].find("@string") {
        let start = from + pos + "@string".len();
        let Some(open) = src[start..].find('{').map(|i| start + i) else {
            break;
        };
        let mut depth = 1usize;
        let mut end = open + 1;
        for (i, c) in src[open + 1..].char_indices() {
            match c {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        end = open + 1 + i;
                        break;
                    }
                }
                _ => {}
            }
        }
        let body = &src[open + 1..end];
        if let Some(eq) = body.find('=') {
            let name = body[..eq].trim().to_ascii_lowercase();
            let value = unquote_concatenation(body[eq + 1..].trim());
            if !name.is_empty() {
                table.insert(name, value);
            }
        }
        from = end.max(start);
        if from >= src.len() {
            break;
        }
    }
    table
}

/// A `@string` value: pieces joined by `#`, each either quoted, braced or a bare word.
fn unquote_concatenation(value: &str) -> String {
    let mut out = String::new();
    for piece in value.split('#') {
        let piece = piece.trim().trim_end_matches(',');
        let inner = piece.trim_matches('"');
        let inner = inner.trim();
        let inner = if inner.starts_with('{') && inner.ends_with('}') && inner.len() >= 2 {
            &inner[1..inner.len() - 1]
        } else {
            inner
        };
        out.push_str(inner);
    }
    out
}

/// Every entry's fields, as written, with abbreviations expanded.
///
/// Expansion is one level of lookup applied recursively: `@string{a = b # "x"}` where
/// `b` is another abbreviation resolves all the way down.
pub fn raw_expanded_entries(src: &str) -> Result<Vec<RawEntryView>, biblatex::ParseError> {
    let macros = macro_table(src);
    let raw = RawBibliography::parse(src)?;
    Ok(raw
        .entries
        .iter()
        .map(|e| RawEntryView {
            kind: e.v.kind.v.to_ascii_lowercase(),
            key: e.v.key.v.to_string(),
            fields: e
                .v
                .fields
                .iter()
                .map(|p| {
                    let value = expand_chunks(&p.value.v, &macros, 0);
                    (p.key.v.to_ascii_lowercase(), collapse_whitespace(&value))
                })
                .collect(),
        })
        .collect())
}

fn expand_chunks(
    chunks: &[Spanned<RawChunk<'_>>],
    macros: &HashMap<String, String>,
    depth: usize,
) -> String {
    let mut out = String::new();
    for chunk in chunks {
        match &chunk.v {
            RawChunk::Normal(text) => out.push_str(text),
            RawChunk::Abbreviation(name) => {
                let key = name.trim().to_ascii_lowercase();
                match macros.get(&key) {
                    Some(value) if depth < 8 => {
                        out.push_str(&expand_text(value, macros, depth + 1))
                    }
                    // An unknown abbreviation stays as written; dropping it would lose
                    // data, and biber treats an undefined macro as a warning, not a loss.
                    _ => out.push_str(name.trim()),
                }
            }
        }
    }
    out
}

/// Expand abbreviations inside a `@string` value, which may reference others.
fn expand_text(value: &str, macros: &HashMap<String, String>, depth: usize) -> String {
    let mut out = String::new();
    let mut word = String::new();
    for c in value.chars() {
        if c.is_alphanumeric() || c == '_' || c == '-' || c == '.' {
            word.push(c);
            continue;
        }
        if !word.is_empty() {
            match macros.get(&word.to_ascii_lowercase()) {
                Some(v) if depth < 8 => out.push_str(&expand_text(v, macros, depth + 1)),
                _ => out.push_str(&word),
            }
            word.clear();
        }
        out.push(c);
    }
    if !word.is_empty() {
        match macros.get(&word.to_ascii_lowercase()) {
            Some(v) if depth < 8 => out.push_str(&expand_text(v, macros, depth + 1)),
            _ => out.push_str(&word),
        }
    }
    out
}

/// Decode the accent commands a `.bib` written in the 1990s uses, the way `biber --tool`
/// does: `{\"O}zge` becomes `Özge`, `{\c{C}}etinkaya` becomes `Çetinkaya`.
///
/// Only the `.bib` writer does this. The `.bbl` writer must keep the TeX exactly as it was
/// written, because biblatex is the thing that is supposed to interpret it.
pub fn decode_accents(text: &str) -> String {
    use unicode_normalization::UnicodeNormalization;

    const MARKS: &[(char, char)] = &[
        ('"', '\u{0308}'), // diaeresis
        ('\'', '\u{0301}'), // acute
        ('`', '\u{0300}'), // grave
        ('^', '\u{0302}'), // circumflex
        ('~', '\u{0303}'), // tilde
        ('=', '\u{0304}'), // macron
        ('.', '\u{0307}'), // dot above
        ('u', '\u{0306}'), // breve
        ('v', '\u{030C}'), // caron
        ('H', '\u{030B}'), // double acute
        ('c', '\u{0327}'), // cedilla
        ('k', '\u{0328}'), // ogonek
    ];
    // Accents that stand alone rather than over a letter.
    const SINGLES: &[(&str, char)] = &[
        ("\\i", 'ı'),
        ("\\j", 'ȷ'),
        ("\\ss", 'ß'),
        ("\\o", 'ø'),
        ("\\O", 'Ø'),
        ("\\aa", 'å'),
        ("\\AA", 'Å'),
        ("\\ae", 'æ'),
        ("\\AE", 'Æ'),
        ("\\oe", 'œ'),
        ("\\OE", 'Œ'),
        ("\\l", 'ł'),
        ("\\L", 'Ł'),
        ("\\dh", 'ð'),
        ("\\th", 'þ'),
    ];

    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '{' && chars.get(i + 1) == Some(&'\\') {
            // `{\CMD x}` or `{\CMD{x}}`
            // `\"O` is a punctuation command; `\c{C}` and `\v{s}` are letter commands.
            let first = chars.get(i + 2).copied();
            let mut j = i + 2;
            let command: String = match first {
                Some(c) if MARKS.iter().any(|(k, _)| *k == c) && !c.is_alphabetic() => {
                    j += 1;
                    c.to_string()
                }
                _ => {
                    let mut c = String::new();
                    while j < chars.len() && chars[j].is_alphabetic() {
                        c.push(chars[j]);
                        j += 1;
                    }
                    c
                }
            };
            let mark = command.chars().next();
            let is_mark = command.len() == 1
                && MARKS.iter().any(|(k, _)| Some(*k) == mark);
            if is_mark {
                let mut k = j;
                if chars.get(k) == Some(&' ') {
                    k += 1;
                }
                let braced = chars.get(k) == Some(&'{');
                if braced {
                    k += 1;
                }
                if let Some(&base) = chars.get(k) {
                    let combining = MARKS.iter().find(|(m, _)| Some(*m) == mark).unwrap().1;
                    let close = if braced { k + 2 } else { k + 1 };
                    let closed = if braced {
                        chars.get(k + 1) == Some(&'}')
                    } else {
                        true
                    };
                    let outer = chars.get(close) == Some(&'}');
                    if closed && outer {
                        out.push(base);
                        out.push(combining);
                        i = close + 1;
                        continue;
                    }
                }
            }
            // A word-like command such as `\ss` or `\i`.
            let name = format!("\\{command}");
            if let Some((_, ch)) = SINGLES.iter().find(|(n, _)| *n == name) {
                if chars.get(j) == Some(&'}') {
                    // biber keeps the braces around a letter command: `Aks{\i}n` becomes
                    // `Aks{ı}n`, while an accent over a letter (`{\"O}zge`) loses them.
                    out.push('{');
                    out.push(*ch);
                    out.push('}');
                    i = j + 1;
                    continue;
                }
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out.nfc().collect()
}

/// Rewrite `Given Family` as `Family, Given`, the way `biber --tool` writes a name.
///
/// Only the unambiguous case is touched: a name already written `Family, Given`, a list
/// with braces (a corporate name can contain "and"), or anything with a command in it is
/// left exactly as the author wrote it. Guessing there would corrupt a name that is
/// already correct.
fn family_first(value: &str) -> String {
    if value.contains('{') || value.contains('\\') {
        return value.to_string();
    }
    // Each name is decided on its own: `Brigitte Goldstein and Smith, John` keeps Smith as
    // it is and rewrites the other, which is what biber does.
    let mut out = Vec::new();
    for part in value.split(" and ") {
        if part.contains(',') {
            out.push(part.to_string());
            continue;
        }
        let words: Vec<&str> = part.split_whitespace().collect();
        if words.len() < 2 {
            return value.to_string();
        }
        let (family, given) = words.split_last().unwrap();
        out.push(format!("{family}, {}", given.join(" ")));
    }
    out.join(" and ")
}
