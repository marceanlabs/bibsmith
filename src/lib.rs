//! bibsmith: sort and normalise `.bib` files the way biblatex does.
//!
//! The library half exists because the crates.io download counts that justify
//! this tool are library downloads (`biblatex`, 4.5M): the people who wanted a
//! dependency outnumber the people who want a binary, so the binary is a thin
//! caller over a public API.
//!
//! Nothing here renders citations. Scope is the four things biber does to a
//! database before it renders: field normalisation, name parsing, ordering, and
//! disambiguation.

pub mod bbl;
pub mod bcf;
pub mod disambiguate;
pub mod fetch;
pub mod label;
pub mod merge;
pub mod names;
pub mod normalize;
pub mod sort;

pub use bbl::{write_bbl, write_bbl_full, write_bbl_scoped, write_bbl_sections};
pub use bcf::ControlFile;
pub use disambiguate::{disambiguate, suffix_for};
pub use normalize::{RawNormalised, normalise_all, normalise_all_with, normalise_raw, write_raw};
pub use sort::{Parts, Scheme, parts as sort_parts, sort_entries};

/// Parse a scheme name, or explain which ones exist. The CLI calls this so a typo
/// is a refusal rather than a silent fallback to a different order.
pub fn scheme_or_default(s: &str) -> Result<Scheme, String> {
    Scheme::parse(s).ok_or_else(|| {
        format!("unknown sorting scheme {s:?}; this build implements nty, nyt and ynt")
    })
}

/// Parse a database, defining any abbreviation the file uses but never declares.
///
/// The `biblatex` crate refuses such a file outright (`unknown abbreviation "JACM"`), and
/// real files do it: a `.bib` written against a house style assumes the macro is shared.
/// biber warns and prints the name as written, so the definition it is missing is the
/// name itself, and the retry is what biber's behaviour costs.
pub fn parse_lenient(src: &str) -> Result<biblatex::Bibliography, biblatex::ParseError> {
    // A `crossref` loop makes the parser recurse until the stack gives out, which aborts
    // the process: `@book{a, crossref={b}}` and `@book{b, crossref={a}}` is enough. biber
    // reads that file without complaint and writes both entries back unchanged, so the
    // fields are hidden from the parser and kept for everything downstream, which reads
    // them from the raw text.
    // The check only says "no loop" when the file reads cleanly. A file the raw reader
    // cannot make sense of is exactly the one where a loop can hide, so it is hidden from
    // the parser too: the fields are still in the raw text, which is where the inheritance
    // is read from.
    let text;
    let guarded = match crate::normalize::raw_entries(src) {
        Ok(entries) => has_crossref_cycle(&entries),
        Err(_) => true,
    };
    let src = match guarded {
        true => {
            text = hide_crossref_fields(src);
            text.as_str()
        }
        false => src,
    };
    let text;
    let src = {
        text = strip_preamble(src);
        text.as_str()
    };
    let mut prelude = String::new();
    for _ in 0..64 {
        let mut text = String::with_capacity(prelude.len() + src.len());
        text.push_str(&prelude);
        text.push_str(src);
        match biblatex::Bibliography::parse(&text) {
            Err(e) if matches!(e.kind, biblatex::ParseErrorKind::UnknownAbbreviation(_)) => {
                let biblatex::ParseErrorKind::UnknownAbbreviation(name) = e.kind else {
                    unreachable!()
                };
                prelude.push_str(&format!("@string{{{name} = \"{name}\"}}\n"));
            }
            Err(e) => {
                // A bare abbreviation or a `#` join is legal BibTeX and a parser error
                // here. Repair the one line the error points at and read the file again.
                let line = src[..e.span.start.min(src.len())]
                    .bytes()
                    .filter(|b| *b == b'\n')
                    .count()
                    + 1;
                match crate::normalize::repair_value_line(src, line) {
                    Some(fixed) => {
                        return parse_lenient(&fixed);
                    }
                    None => return Err(e),
                }
            }
            other => return other,
        }
    }
    biblatex::Bibliography::parse(src)
}

/// Remove `@PREAMBLE{...}` blocks, which are BibTeX's way of putting raw TeX in the
/// `.bbl` header. biblatex has no use for them and biber drops them with a warning, so the
/// only thing keeping them would do is stop the file being read at all.
pub(crate) fn strip_preamble(src: &str) -> String {
    let lower = src.to_ascii_lowercase();
    let mut out = String::with_capacity(src.len());
    let mut from = 0usize;
    while let Some(pos) = lower[from..].find("@preamble") {
        let start = from + pos;
        out.push_str(&src[from..start]);
        let Some(open) = src[start..].find('{').map(|i| start + i) else {
            from = src.len();
            break;
        };
        let mut depth = 0usize;
        let mut end = src.len();
        let mut quoted = false;
        let mut escaped = false;
        for (i, c) in src[open..].char_indices() {
            if escaped {
                escaped = false;
                continue;
            }
            match c {
                '\\' => escaped = true,
                '"' if depth == 1 => quoted = !quoted,
                '{' if !quoted => depth += 1,
                '}' if !quoted => {
                    depth -= 1;
                    if depth == 0 {
                        end = open + i + 1;
                        break;
                    }
                }
                _ => {}
            }
        }
        from = end;
    }
    out.push_str(&src[from.min(src.len())..]);
    out
}

/// Whether the `crossref` graph contains a loop.
fn has_crossref_cycle(entries: &[crate::normalize::RawEntryView]) -> bool {
    let edges: std::collections::HashMap<&str, &str> = entries
        .iter()
        .filter_map(|e| {
            let target = e
                .fields
                .iter()
                .find(|(k, _)| k == "crossref")
                .map(|(_, v)| v.trim())?;
            Some((e.key.as_str(), target))
        })
        .collect();
    for start in edges.keys() {
        let mut seen: Vec<&str> = Vec::new();
        let mut at = *start;
        while let Some(next) = edges.get(at) {
            if seen.contains(&at) {
                return true;
            }
            seen.push(at);
            at = next;
        }
    }
    false
}

/// Rename every `crossref` field name so the parser does not follow it.
///
/// Only a field name is touched: a `crossref` that appears inside a value is left alone,
/// because that is text the author wrote.
pub fn hide_crossref_fields(src: &str) -> String {
    // A field name sits after `{` or `,` and before `=`, at any depth: a damaged file can
    // nest one entry inside another, or leave a brace open, and the parser follows the
    // field wherever it is. Text that merely mentions the word is left alone, because a
    // field is followed by `=` and nothing else is.
    let chars: Vec<char> = src.chars().collect();
    let mut out = String::with_capacity(src.len() + 16);
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == 'c' || chars[i] == 'C' {
            let word: String = chars[i..chars.len().min(i + 8)].iter().collect();
            if word.eq_ignore_ascii_case("crossref") {
                let before = out.chars().rev().find(|n| !n.is_whitespace());
                let after = chars[i + 8..].iter().find(|n| !n.is_whitespace());
                if matches!(before, Some(',') | Some('{')) && after == Some(&'=') {
                    out.push_str(&word);
                    out.push('_');
                    i += 8;
                    continue;
                }
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

/// One bibliography database, as parsed.
pub struct Database {
    inner: biblatex::Bibliography,
    strings: Vec<String>,
}

impl Database {
    pub fn parse(src: &str) -> Result<Self, biblatex::ParseError> {
        Ok(Self {
            inner: parse_lenient(src)?,
            strings: raw_string_blocks(src),
        })
    }

    /// The `@string` blocks, verbatim.
    ///
    /// The parser resolves macros but does not expose the definitions, and biber
    /// writes them back out, so they are lifted from the source text here. Dropping
    /// them would change files other tools read: 8 of the 100 blocks in the
    /// canonical corpus are definitions.
    pub fn string_blocks(&self) -> &[String] {
        &self.strings
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.len() == 0
    }

    pub fn iter(&self) -> impl Iterator<Item = &biblatex::Entry> {
        self.inner.iter()
    }

    pub fn get(&self, key: &str) -> Option<&biblatex::Entry> {
        self.inner.get(key)
    }
}

/// Lift every `@string{...}` block out of the source, byte for byte.
///
/// A hand-rolled scanner rather than a parse, because the goal is to preserve what
/// the user wrote, not to reinterpret it. Braces are counted so a definition
/// containing `{The \TeX book}` is not cut short, and an unterminated block ends at
/// end of file rather than panicking.
pub fn raw_string_blocks(src: &str) -> Vec<String> {
    let bytes = src.as_bytes();
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] != b'@' {
            i += 1;
            continue;
        }
        let rest = &src[i + 1..];
        let name_len = rest.chars().take_while(|c| c.is_ascii_alphabetic()).count();
        if !rest[..name_len].eq_ignore_ascii_case("string") {
            i += 1;
            continue;
        }
        // Walk to the opening brace, then to its match.
        let mut j = i + 1 + name_len;
        while j < bytes.len() && bytes[j] != b'{' {
            j += 1;
        }
        if j >= bytes.len() {
            break;
        }
        let start = j;
        let mut depth = 0usize;
        while j < bytes.len() {
            match bytes[j] {
                b'{' => depth += 1,
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        j += 1;
                        break;
                    }
                }
                _ => {}
            }
            j += 1;
        }
        out.push(format!("@string{}", &src[start..j.min(bytes.len())]));
        i = j;
    }
    out
}
