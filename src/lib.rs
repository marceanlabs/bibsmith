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

pub mod normalize;
pub mod sort;

pub use normalize::normalise_entry;
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
}

impl Database {
    pub fn parse(src: &str) -> Result<Self, biblatex::ParseError> {
        Ok(Self {
            inner: biblatex::Bibliography::parse(src)?,
        })
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
