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

/// Disambiguation: the `a`/`b` suffix biblatex prints, taken from the `extradate`
/// field biber writes into the `.bbl` for the same corpus and scheme.
#[test]
fn extradates_match_biber() {
    use bibsmith::disambiguate;

    let db = load();
    let expected: std::collections::HashMap<String, u8> =
        std::fs::read_to_string("oracle/extradate-nyt.tsv")
            .expect("extradate fixture")
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| {
                let (k, v) = l.split_once('\t').expect("tab separated");
                (k.to_string(), v.trim().parse::<u8>().expect("number"))
            })
            .collect();

    let entries: Vec<&biblatex::Entry> = db.iter().collect();
    let ordered = sort_entries(&entries, Scheme::Nyt);
    let ours = disambiguate(&ordered, Scheme::Nyt);

    let mut problems = Vec::new();
    for (key, want) in &expected {
        match ours.get(key) {
            Some(got) if got == want => {}
            Some(got) => problems.push(format!("{key}: ours={got} biber={want}")),
            None => problems.push(format!("{key}: ours=none biber={want}")),
        }
    }
    for (key, got) in &ours {
        if !expected.contains_key(key) {
            problems.push(format!("{key}: ours={got} biber=none"));
        }
    }
    problems.sort();
    for line in &problems {
        println!("  {line}");
    }
    assert!(
        problems.is_empty(),
        "{} of {} extradates differ: {problems:?}",
        problems.len(),
        expected.len()
    );

    // The suffix is the letter, not the number.
    assert_eq!(bibsmith::suffix_for(1), "a");
    assert_eq!(bibsmith::suffix_for(2), "b");
    assert_eq!(bibsmith::suffix_for(26), "z");
    assert_eq!(bibsmith::suffix_for(27), "aa");
}
