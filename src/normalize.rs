//! Field normalisation.
//!
//! Measured with `biber --tool` on `biblatex-examples.bib` on 30 Oct 2026: of the
//! 92 entries, 42 come back different from the input because biber rewrites
//! legacy BibTeX field names and recomputes `sorttitle`. A tool that copies
//! fields through verbatim therefore produces a file biber never would, which is
//! the first place a "replacement" gets noticed as not one.

use biblatex::{Chunks, ChunksExt, Entry};

/// A rename biber performs silently. Left side is what an old `.bib` says, right
/// side is what biber writes.
const RENAMES: &[(&str, &str)] = &[
    ("journal", "journaltitle"),
    ("year", "date"),
    ("address", "location"),
    ("annote", "annotation"),
    ("archiveprefix", "eprinttype"),
    ("journaltitle", "journaltitle"),
    ("key", "sortkey"),
    ("pdf", "file"),
    ("primaryclass", "eprintclass"),
    ("school", "institution"),
];

/// Fields biber computes rather than reads: carrying the input value through
/// would state something biblatex then disagrees with.
const COMPUTED: &[&str] = &[
    "sorttitle",
    "sortname",
    "sortyear",
    "presort",
    "label",
    "labelyear",
];

/// Normalised view of one entry: the fields to write, in a stable order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Normalised {
    pub fields: Vec<(String, String)>,
    pub dropped: Vec<String>,
}

impl Normalised {
    pub fn get(&self, field: &str) -> Option<&str> {
        self.fields
            .iter()
            .find(|(k, _)| k == field)
            .map(|(_, v)| v.as_str())
    }
}

fn value(chunks: &Chunks) -> String {
    chunks.format_verbatim()
}

/// Rename legacy fields, drop the computed ones, and report both so the CLI can
/// say what it changed instead of doing it invisibly.
pub fn normalise_entry(entry: &Entry) -> Normalised {
    let mut out: Vec<(String, String)> = Vec::new();
    let mut dropped: Vec<String> = Vec::new();

    for (key, chunks) in entry.fields.iter() {
        let lower = key.to_ascii_lowercase();
        if COMPUTED.contains(&lower.as_str()) {
            dropped.push(lower);
            continue;
        }
        let target = RENAMES
            .iter()
            .find(|(from, _)| *from == lower)
            .map(|(_, to)| (*to).to_string())
            .unwrap_or_else(|| lower.clone());
        // A rename must not collide with a field that is already there under the
        // modern name; the modern one wins and the legacy one is reported.
        if out.iter().any(|(k, _)| *k == target) {
            dropped.push(format!("{lower} (duplicate of {target})"));
            continue;
        }
        out.push((target, value(chunks)));
    }

    out.sort_by(|a, b| a.0.cmp(&b.0));
    Normalised {
        fields: out,
        dropped,
    }
}
