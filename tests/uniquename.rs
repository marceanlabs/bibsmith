//! `uniquename` and `uniquelist`, measured against a document built to need them.
//!
//! `oracle/uniq/` is a real biblatex run over nine entries chosen to produce every
//! decision biber makes about names:
//!
//! - three `Smith`s in one year, two sharing a given name's first letter (John, Jane) and
//!   one that does not (John A.), so the levels are 2 and 1 rather than 0;
//! - two three-name lists that agree on the first and last author but not the middle, so
//!   `uniquelist` has to widen the visible list to two;
//! - a repeated author across two entries, which is what the dash compares;
//! - two editors, one alone and one with a co-editor.
//!
//! The expected `.bbl` is biber 2.22's. This checks the `\name` blocks, which carry the
//! fields biblatex reads for disambiguation: `un`, `uniquepart`, `givenun` and the
//! per-name `hash`, plus `ul` in the list header.

use std::collections::HashMap;
use std::path::Path;

use bibsmith::ControlFile;

fn oracle(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("oracle")
        .join(name)
}

/// The `\name{...}` blocks of each entry, whitespace removed so only tokens are compared.
fn name_blocks(bbl: &str) -> HashMap<String, Vec<String>> {
    let mut out = HashMap::new();
    for chunk in bbl.split("\\entry{").skip(1) {
        let key = chunk.split('}').next().unwrap_or_default().to_string();
        let seg = chunk.split("\\endentry").next().unwrap_or_default();
        let mut blocks = Vec::new();
        let mut rest = seg;
        while let Some(i) = rest.find("\\name{") {
            rest = &rest[i..];
            // The block ends at the first `}` that closes the options group after the two
            // numeric arguments, then runs to the matching line of closing braces.
            let Some(body_start) = rest.find("{%") else {
                break;
            };
            let body = &rest[body_start + 2..];
            let Some(end) = body.find("\n      }") else {
                break;
            };
            blocks.push(body[..end].split_whitespace().collect::<Vec<_>>().join(""));
            rest = &body[end + 8..];
        }
        out.insert(key, blocks);
    }
    out
}

fn render() -> String {
    let dir = oracle("uniq");
    let src = std::fs::read_to_string(dir.join("refs.bib")).expect("fixture");
    let xml = std::fs::read_to_string(dir.join("uniq.bcf")).expect("control file");
    let ctl = ControlFile::parse(&xml).expect("bcf parses");
    let db = biblatex::Bibliography::parse(&src).expect("parse bib");
    let raw: HashMap<String, Vec<(String, String)>> =
        bibsmith::normalize::raw_expanded_entries(&src)
            .expect("raw")
            .into_iter()
            .map(|e| (e.key, e.fields))
            .collect();
    let keys: Vec<String> = raw.keys().cloned().collect();
    let entries: Vec<&biblatex::Entry> = keys.iter().filter_map(|k| db.get(k)).collect();
    let effective: Vec<_> = entries
        .into_iter()
        .map(|e| bibsmith::merge::effective(&db, e, &raw))
        .collect();
    let refs: Vec<&bibsmith::merge::Effective> = effective.iter().collect();
    bibsmith::write_bbl(&ctl, &refs, &HashMap::new(), &HashMap::new())
}

#[test]
fn every_name_block_matches_biber() {
    let ours = name_blocks(&render());
    let want = name_blocks(
        &std::fs::read_to_string(oracle("uniq/uniq.expected.bbl")).expect("expected bbl"),
    );

    let mut bad = Vec::new();
    for (key, expected) in &want {
        match ours.get(key) {
            None => bad.push(format!("{key}: missing")),
            Some(got) if got != expected => {
                bad.push(format!("{key}:\n  biber {expected:?}\n  ours  {got:?}"))
            }
            Some(_) => {}
        }
    }
    assert!(bad.is_empty(), "differ from biber:\n{}", bad.join("\n"));
}

#[test]
fn the_levels_are_the_ones_biber_computes() {
    // Jane and John Smith share an initial, so both need the full given name (2); John A.
    // does not, because `J. A.` is not `J.` (1). Getting this backwards prints initials
    // that do not disambiguate anything.
    let blocks = name_blocks(&render());
    let a2 = blocks.get("a2").expect("a2");
    assert!(a2[0].contains("un=2,uniquepart=given"), "{:?}", a2[0]);
    assert!(a2[0].contains("givenun=2"), "{:?}", a2[0]);
    let a3 = blocks.get("a3").expect("a3");
    assert!(a3[0].contains("un=1,uniquepart=given"), "{:?}", a3[0]);
    // A repeated author alone: no ambiguity, so the base part, and the level is zero.
    let d1 = blocks.get("d1").expect("d1");
    assert!(d1[0].contains("un=0,uniquepart=base"), "{:?}", d1[0]);
    assert!(d1[0].contains("givenun=0"), "{:?}", d1[0]);
}

#[test]
fn a_truncated_list_widens_for_uniquelist() {
    // b1 and b2 share their first and last author, so one visible name would make them
    // look identical; biber widens the list to two and says so in `ul`.
    let bbl = render();
    let b1 = bbl
        .split("\\entry{b1}")
        .nth(1)
        .and_then(|s| s.split("\\endentry").next())
        .expect("b1");
    assert!(b1.contains("\\name{author}{3}{ul=2}"), "{b1}");
}
