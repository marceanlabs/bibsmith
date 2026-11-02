//! What biber's `\strng{}` values are made of, measured against a real biber run.
//!
//! The `.bbl` carries eight name hashes per name list and they are not the same number:
//! `namehash` covers the names a citation shows, `bibnamehash` the names the bibliography
//! shows, `fullhash` every name, and `fullhashraw` every name without the hash template.
//! biblatex compares them to decide whether two names need initials, so a wrong value here
//! is an invisible wrong bibliography later.
//!
//! The expected values are biber 2.22's, taken from `oracle/nyt.bbl.biber`, and every
//! entry in the corpus is checked, not a sample.

use std::collections::HashMap;
use std::path::Path;

use bibsmith::bbl::{hash_canonical, list_hash};
use bibsmith::merge::effective;
use bibsmith::{ControlFile, write_bbl};

fn oracle(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("oracle")
        .join(name)
}

/// Every `\strng{key}{value}` pair per entry, in the order the file writes them.
///
/// `crossref` is a `\strng` too, but it is a field, not a name hash, and bibsmith resolves
/// inheritance instead of writing it back, so it is left out of the comparison.
fn hashes(bbl: &str) -> HashMap<String, Vec<(String, String)>> {
    let mut out = HashMap::new();
    for chunk in bbl.split("\\entry{").skip(1) {
        let key = chunk.split('}').next().unwrap_or_default().to_string();
        let seg = chunk.split("\\endentry").next().unwrap_or_default();
        let mut pairs = Vec::new();
        let mut rest = seg;
        while let Some(i) = rest.find("\\strng{") {
            rest = &rest[i + "\\strng{".len()..];
            let Some(close) = rest.find("}{") else { break };
            let name = rest[..close].to_string();
            rest = &rest[close + 2..];
            let Some(end) = rest.find('}') else { break };
            let value = rest[..end].to_string();
            rest = &rest[end..];
            if name != "crossref" {
                pairs.push((name, value));
            }
        }
        out.insert(key, pairs);
    }
    out
}

#[test]
fn name_hashes_match_biber() {
    let src = std::fs::read_to_string(oracle("refs.bib")).expect("fixture");
    let ctl = ControlFile::parse(&std::fs::read_to_string(oracle("nyt.bcf")).expect("bcf"))
        .expect("parse bcf");
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
        .map(|e| effective(&db, e, &raw))
        .collect();
    let refs: Vec<&bibsmith::merge::Effective> = effective.iter().collect();
    let ours = write_bbl(&ctl, &refs, &HashMap::new(), &HashMap::new());

    let biber = hashes(&std::fs::read_to_string(oracle("nyt.bbl.biber")).expect("biber bbl"));
    let mine = hashes(&ours);

    let mut bad = Vec::new();
    // Only entries this call writes are comparable: the `related` copies are hash-keyed
    // entries the CLI builds when it expands `related`, not from the database alone.
    for (key, got) in &mine {
        let Some(want) = biber.get(key) else {
            bad.push(format!("{key}: not in biber's .bbl at all"));
            continue;
        };
        if got != want {
            let mut lines = vec![format!("{key}:")];
            let empty = (String::new(), String::new());
            let len = want.len().max(got.len());
            for i in 0..len {
                let (nk, nv) = want.get(i).unwrap_or(&empty);
                let (gk, gv) = got.get(i).unwrap_or(&empty);
                if nk != gk || nv != gv {
                    lines.push(format!("  {nk}: biber {nv} ours {gk} {gv}"));
                }
            }
            bad.push(lines.join("\n"));
        }
    }
    assert!(
        bad.is_empty(),
        "{} of {} entries differ from biber\n{}",
        bad.len(),
        mine.len(),
        bad.join("\n")
    );
}

#[test]
fn the_truncated_hash_is_a_prefix_plus_a_marker() {
    // The measured rule behind `namehash`: with more names than the style shows, biber
    // hashes the visible ones and appends `+`. Checking it directly means a future change
    // to the template or the counter cannot quietly turn every hash into a full-list one.
    let src = std::fs::read_to_string(oracle("refs.bib")).expect("fixture");
    let ctl = ControlFile::parse(&std::fs::read_to_string(oracle("nyt.bcf")).expect("bcf"))
        .expect("parse bcf");
    let db = biblatex::Bibliography::parse(&src).expect("parse bib");
    let raw: HashMap<String, Vec<(String, String)>> = Default::default();
    let template = ctl.hash_template("global");

    let entry = db.get("aksin").expect("entry");
    let eff = effective(&db, entry, &raw);
    let people = eff
        .names
        .iter()
        .find(|(f, _)| f == "author")
        .map(|(_, p)| p)
        .expect("author");
    assert!(people.len() > 3, "aksin has seven authors");

    // biber 2.22: namehash of the cite list, fullhash of all seven.
    assert!(list_hash(people, &template, 1, false).starts_with("860705eb"));
    assert!(list_hash(people, &template, people.len(), false).starts_with("f565e365"));
    assert_eq!(
        hash_canonical(people, &template, 1, false),
        "AksınÖzge+",
        "one visible name and the truncation marker"
    );
}
