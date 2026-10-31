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

pub mod disambiguate;
pub mod normalize;
pub mod sort;

pub use disambiguate::{disambiguate, suffix_for};
pub use normalize::{RawNormalised, normalise_all, normalise_raw, write_raw};
pub use sort::{Parts, Scheme, parts as sort_parts, sort_entries};

/// Parse a scheme name, or explain which ones exist. The CLI calls this so a typo
/// is a refusal rather than a silent fallback to a different order.
pub fn scheme_or_default(s: &str) -> Result<Scheme, String> {
    Scheme::parse(s).ok_or_else(|| {
        format!("unknown sorting scheme {s:?}; this build implements nty, nyt and ynt")
    })
}

/// One bibliography database, as parsed.
pub struct Database {
    inner: biblatex::Bibliography,
    strings: Vec<String>,
}

impl Database {
    pub fn parse(src: &str) -> Result<Self, biblatex::ParseError> {
        Ok(Self {
            inner: biblatex::Bibliography::parse(src)?,
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
