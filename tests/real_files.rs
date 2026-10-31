//! Real files from other producers.
//!
//! `biblatex-examples.bib` is one producer's idea of a bibliography, and one
//! producer's habits hide bugs. These two are from elsewhere:
//!
//! - `acm-sample-base.bib`: the ACM sig-alternate sample, 265 blocks, 163 of them
//!   `@String` definitions, entries in mixed case (`@Inproceedings`).
//! - `ieee-abrv.bib`: IEEEtran's abbreviations file, not valid UTF-8, 263 blocks.
//!
//! What is checked: every block in survives, exactly once, and the output is stable
//! under a second pass. Malformed input is passed through, not repaired — the ACM
//! file contains `Werneck,, Renato`, which biber itself rejects, and a tool that
//! silently "fixes" someone's names is worse than one that copies them.

use std::collections::BTreeSet;

use bibsmith::{Database, Scheme, normalise_all, sort_entries};

fn read_bytes(path: &str) -> String {
    let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("{path}: {e}"));
    String::from_utf8(bytes).unwrap_or_else(|err| {
        // Latin-1 fallback, matching what the CLI does.
        err.into_bytes().iter().map(|b| *b as char).collect()
    })
}

fn block_keys(src: &str) -> BTreeSet<String> {
    let raw = biblatex::RawBibliography::parse(src).expect("raw parse");
    raw.entries.iter().map(|e| e.v.key.v.to_string()).collect()
}

fn check_file(path: &str, entries: usize, strings: usize) {
    let src = read_bytes(path);

    let db = Database::parse(&src).unwrap_or_else(|e| panic!("{path}: {e}"));
    assert_eq!(db.len(), entries, "{path}: entry count");
    assert_eq!(
        db.string_blocks().len(),
        strings,
        "{path}: @string blocks must all be found"
    );

    let ordered: Vec<&biblatex::Entry> = db.iter().collect();
    let sorted = sort_entries(&ordered, Scheme::Nyt);
    assert_eq!(
        sorted.len(),
        db.len(),
        "{path}: sorting must not lose an entry"
    );

    let before: BTreeSet<String> = ordered.iter().map(|e| e.key.to_string()).collect();
    let after: BTreeSet<String> = sorted.iter().map(|e| e.key.to_string()).collect();
    assert_eq!(before, after, "{path}: sorting must not change the key set");

    // Sorting is idempotent: feeding the order back through gives the same order.
    let again = sort_entries(&sorted, Scheme::Nyt);
    let first: Vec<&str> = sorted.iter().map(|e| e.key.as_str()).collect();
    let second: Vec<&str> = again.iter().map(|e| e.key.as_str()).collect();
    assert_eq!(
        first, second,
        "{path}: sort must be stable under a second pass"
    );

    // The raw parse and the resolved parse must agree on the key set.
    assert_eq!(
        block_keys(&src),
        before,
        "{path}: raw and resolved parses differ"
    );

    // Normalising must keep every entry and every field name valid.
    let normalised = normalise_all(&src).unwrap_or_else(|e| panic!("{path}: {e}"));
    assert_eq!(
        normalised.len(),
        db.len(),
        "{path}: normalise loses entries"
    );
    for entry in &normalised {
        for (name, _, _) in &entry.fields {
            assert!(
                !name.is_empty() && !name.contains(char::is_whitespace),
                "{path}: bad field name {name:?}"
            );
        }
    }
}

#[test]
fn acm_sample_round_trips() {
    check_file("oracle/corpus/acm-sample-base.bib", 102, 163);
}

#[test]
fn ieee_abbreviations_round_trip() {
    check_file("oracle/corpus/ieee-abrv.bib", 0, 263);
}

#[test]
fn latin1_input_is_read_not_refused() {
    // The IEEE file is not valid UTF-8; reading it must succeed and keep the blocks.
    let bytes = std::fs::read("oracle/corpus/ieee-abrv.bib").expect("fixture");
    assert!(
        std::str::from_utf8(&bytes).is_err(),
        "fixture should be the non-UTF-8 original"
    );
    let src = read_bytes("oracle/corpus/ieee-abrv.bib");
    assert_eq!(bibsmith::raw_string_blocks(&src).len(), 263);
}

/// On a real-world file, `biber --tool` deletes every field biblatex does not model:
/// measured on the ACM sample, `month` on 32 entries, `numpages` on 20, `acmid` on 19,
/// `lastaccessed` on 8, plus `articleno`, `source`, `issue_date`, `sortkey` and
/// `reviewer`-style fields. It also skips one entry outright (`384253`, whose author
/// string `Werneck,, Renato` it calls malformed).
///
/// What must hold: we never lose a field biber kept. Losing one is a data bug; keeping
/// one biber discarded is a design choice, and the choice is to keep the author's data.
#[test]
fn no_field_biber_keeps_is_ever_lost() {
    use std::collections::{BTreeMap, BTreeSet};

    let src = read_bytes("oracle/corpus/acm-sample-base.bib");
    let normalised = normalise_all(&src).expect("parse");
    let our_src: String = normalised.iter().map(bibsmith::write_raw).collect();

    fn index(text: &str) -> BTreeMap<String, BTreeSet<String>> {
        let mut out: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        let mut current: Option<String> = None;
        for line in text.lines() {
            let t = line.trim();
            if let Some(rest) = t.strip_prefix('@') {
                let kind: String = rest
                    .chars()
                    .take_while(|c| c.is_alphanumeric())
                    .collect::<String>()
                    .to_ascii_lowercase();
                if kind == "string" {
                    current = None;
                    continue;
                }
                if let Some(open) = rest.find('{') {
                    let key = rest[open + 1..]
                        .split(',')
                        .next()
                        .unwrap_or("")
                        .trim()
                        .to_string();
                    current = Some(key.clone());
                    out.entry(key).or_default();
                }
                continue;
            }
            if t == "}" {
                current = None;
                continue;
            }
            if let Some(key) = &current {
                if let Some((name, _)) = t.split_once('=') {
                    out.entry(key.clone())
                        .or_default()
                        .insert(name.trim().to_ascii_lowercase());
                }
            }
        }
        out
    }

    let ours = index(&our_src);
    let biber_src = std::fs::read_to_string("oracle/corpus/biber-tool-acm.bib").expect("oracle");
    let theirs = index(&biber_src);

    // `type` is the one field biber synthesises rather than reads: it writes a
    // default for report-like entries. A field the author never wrote is not ours to
    // invent, so its absence is expected rather than a loss.
    const COMPUTED_BY_BIBER: &[&str] = &["type"];

    let mut lost: Vec<String> = Vec::new();
    for (key, their_fields) in &theirs {
        let our_fields = ours
            .get(key)
            .unwrap_or_else(|| panic!("{key} missing from our output"));
        for field in their_fields {
            if !our_fields.contains(field) && !COMPUTED_BY_BIBER.contains(&field.as_str()) {
                lost.push(format!("{key}: {field}"));
            }
        }
    }
    assert!(
        lost.is_empty(),
        "fields biber kept but we dropped: {lost:?}"
    );
}
