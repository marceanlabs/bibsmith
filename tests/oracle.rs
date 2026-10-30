//! Oracle tests: our order against the order biber actually produced.
//!
//! The fixtures in `oracle/order-*.txt` are key order extracted from `.bbl` files
//! written by biber 2.22 (biblatex 3.22a) over the canonical 100-block
//! `biblatex-examples.bib`, one file per sorting template, locale en-US. The crate
//! that parses the input must never be the only thing that reads the output, so
//! the expected order comes from a different implementation.
//!
//! Regenerate with `scripts/make_oracle.sh`.

use bibsmith::{Database, Scheme, sort_entries};

const BIB: &str = "oracle/refs.bib";

fn load() -> Database {
    let src = std::fs::read_to_string(BIB).expect("oracle corpus");
    Database::parse(&src).expect("parse oracle corpus")
}

fn expected_order(scheme: &str, db: &Database) -> Vec<String> {
    let path = format!("oracle/order-{scheme}.txt");
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{path}: {e}"))
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        // The .bbl carries hash-keyed entries for set members; keep only real keys.
        .filter(|k| db.get(k).is_some())
        .collect()
}

fn check(scheme: Scheme, name: &str) {
    let db = load();
    let expected = expected_order(name, &db);
    let entries: Vec<&biblatex::Entry> = db.iter().collect();
    let ours: Vec<String> = sort_entries(&entries, scheme)
        .iter()
        .map(|e| e.key.to_string())
        .collect();

    assert_eq!(
        ours.len(),
        expected.len(),
        "{name}: entry count differs, ours {} vs biber {}",
        ours.len(),
        expected.len()
    );

    let mut diffs = Vec::new();
    for (i, (a, b)) in ours.iter().zip(expected.iter()).enumerate() {
        if a != b {
            diffs.push(format!("  #{i:3} ours={a:28} biber={b}"));
        }
    }
    assert!(
        diffs.is_empty(),
        "{name}: {} of {} positions differ from biber\n{}",
        diffs.len(),
        ours.len(),
        diffs.join("\n")
    );
}

#[test]
fn nyt_matches_biber() {
    check(Scheme::Nyt, "nyt");
}

#[test]
fn nty_matches_biber() {
    check(Scheme::Nty, "nty");
}

#[test]
fn ynt_matches_biber() {
    check(Scheme::Ynt, "ynt");
}
