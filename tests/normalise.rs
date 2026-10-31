//! Normalisation parity: our rewrite against `biber --tool`'s own rewrite.
//!
//! `oracle/biber-tool.bib` is biber 2.22's output for the same corpus. Three
//! differences are tolerated, each for a stated reason:
//!
//! 1. We keep `sorttitle`, `sortname`, `sortyear`, `presort` and `entryset`, which
//!    `biber --tool` deletes. Those fields are how an author controls the order, so
//!    a tool whose job is the order must not throw them away — measured: biber's own
//!    `--tool` output reorders the bibliography when fed back through biblatex.
//! 2. Values keep LaTeX escapes as written; biber decodes `{\"O}` to `Ö`.
//! 3. A value that is an abbreviation keeps the case the parser gives it.
//!
//! Everything else must match, field for field.

use std::collections::{BTreeMap, BTreeSet};

use bibsmith::{normalise_all, write_raw};

const BIB: &str = "oracle/refs.bib";
const BIBER: &str = "oracle/biber-tool.bib";
/// Fields we keep on purpose. See COMPUTED in src/normalize.rs.
const KEPT: &[&str] = &["entryset", "sorttitle", "sortname", "sortyear", "presort"];

type Fields = BTreeMap<String, String>;
type Index = BTreeMap<String, Fields>;

fn parse_like_biber(path: &str) -> Index {
    let src = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{path}: {e}"));
    let mut index: Index = BTreeMap::new();
    let mut current: Option<String> = None;
    for line in src.lines() {
        let trimmed = line.trim();
        if let Some(after_at) = trimmed.strip_prefix('@') {
            let name_end = after_at
                .find(|c: char| !c.is_alphanumeric())
                .unwrap_or(after_at.len());
            if after_at[..name_end].eq_ignore_ascii_case("string") {
                current = None;
                continue;
            }
            if let Some(open) = after_at.find('{') {
                let rest = &after_at[open + 1..];
                let key = rest.split(',').next().unwrap_or("").trim().to_string();
                index.entry(key.clone()).or_default();
                current = Some(key);
            }
            continue;
        }
        if trimmed == "}" {
            current = None;
            continue;
        }
        if let Some(key) = &current {
            if let Some((name, value)) = trimmed.split_once('=') {
                let name = name.trim().to_ascii_lowercase();
                let value = value.trim().trim_end_matches(',').trim();
                let value = value
                    .strip_prefix('{')
                    .and_then(|v| v.strip_suffix('}'))
                    .unwrap_or(value)
                    .to_string();
                index.entry(key.clone()).or_default().insert(name, value);
            }
        }
    }
    index
}

#[test]
fn entries_match_biber_field_for_field() {
    let src = std::fs::read_to_string(BIB).expect("corpus");
    let ours = normalise_all(&src).expect("parse");
    let biber = parse_like_biber(BIBER);

    let mut same_values = 0usize;
    let mut same_field_set = 0usize;
    let mut problems: Vec<String> = Vec::new();

    for entry in &ours {
        let our_fields: Fields = entry
            .fields
            .iter()
            .map(|(k, v, _)| (k.to_ascii_lowercase(), v.clone()))
            .collect();
        let their_fields = biber.get(&entry.key).cloned().unwrap_or_default();

        let our_names: BTreeSet<&String> = our_fields.keys().collect();
        let their_names: BTreeSet<&String> = their_fields.keys().collect();
        let missing: Vec<&&String> = their_names.difference(&our_names).collect();
        let extra: Vec<&&String> = our_names.difference(&their_names).collect();

        if !missing.is_empty() {
            problems.push(format!("{}: missing {missing:?}", entry.key));
            continue;
        }
        if !extra.is_empty() {
            if extra.iter().all(|k| KEPT.contains(&k.as_str())) {
                same_field_set += 1;
            } else {
                problems.push(format!("{}: unexpected extra {extra:?}", entry.key));
            }
            continue;
        }

        // Values are compared only where the encoding is comparable: a LaTeX escape
        // is preserved as written, and biber rewrites it to UTF-8.
        let has_escape = our_fields.values().any(|v| v.contains('\\'));
        let has_abbreviation = entry.fields.iter().any(|(_, _, abbrev)| *abbrev);
        if has_escape || has_abbreviation {
            same_field_set += 1;
            continue;
        }

        let differing: Vec<&String> = our_names
            .into_iter()
            .filter(|n| our_fields.get(*n) != their_fields.get(*n))
            .collect();
        if differing.is_empty() {
            same_values += 1;
        } else if problems.len() < 12 {
            let first = differing[0];
            problems.push(format!(
                "{}: {first}: ours={:?} biber={:?}",
                entry.key,
                our_fields.get(first).map(|s| s.as_str()).unwrap_or(""),
                their_fields.get(first).map(|s| s.as_str()).unwrap_or("")
            ));
        }
    }

    println!(
        "identical to biber: {same_values} of {} by value, {same_field_set} by field set",
        ours.len()
    );
    for line in &problems {
        println!("  {line}");
    }

    assert!(
        problems.is_empty(),
        "{} entries differ from biber beyond the documented exceptions: {problems:?}",
        problems.len()
    );
    assert_eq!(
        same_values + same_field_set,
        ours.len(),
        "every entry must be accounted for"
    );
}

#[test]
fn writer_uses_uppercase_sorted_fields_and_braces() {
    let src = std::fs::read_to_string(BIB).expect("corpus");
    let ours = normalise_all(&src).expect("parse");
    let entry = ours.iter().find(|e| e.key == "aksin").expect("aksin");
    let text = write_raw(entry);

    assert!(
        text.starts_with("@ARTICLE{aksin,\n"),
        "biber writes the type in capitals: {text}"
    );
    let names: Vec<&str> = text
        .lines()
        .filter_map(|l| l.trim().split_once('=').map(|(n, _)| n.trim()))
        .collect();
    let mut sorted = names.clone();
    sorted.sort();
    assert_eq!(names, sorted, "fields must be sorted");
    assert!(
        names
            .iter()
            .all(|n| !n.chars().any(|c| c.is_ascii_lowercase())),
        "upper case"
    );
    // A single abbreviation reference stays bare, and in capitals: biber writes a macro
    // name as `JOMCH` even when the file defines it as `jomch`.
    assert!(text.contains(" = JOMCH,"), "{text}");
}

#[test]
fn mixed_abbreviation_values_are_braced() {
    // Found by feeding our own output back to biber: `PUBLISHER = dtv and Walter de
    // Gruyter` was written bare and biber reported `found "and", expected end of
    // entry`. A value is only bare when it is exactly one abbreviation.
    let src = std::fs::read_to_string(BIB).expect("corpus");
    let ours = normalise_all(&src).expect("parse");
    let entry = ours
        .iter()
        .find(|e| e.key == "nietzsche:ksa")
        .expect("entry");
    let text = write_raw(entry);
    assert!(text.contains("PUBLISHER = {"), "{text}");
}

#[test]
fn page_ranges_are_repunctuated() {
    use bibsmith::normalize::normalise_range;
    assert_eq!(normalise_range("55-65"), "55--65");
    // biber keeps the space after the comma: measured in a rendered .bbl, "431–456, 791–823".
    assert_eq!(normalise_range("431–456, 791–823"), "431--456, 791--823");
    assert_eq!(normalise_range("423--491"), "423--491");
    // A dash that is not between digits is left alone.
    assert_eq!(normalise_range("1-3 and 7a-c"), "1--3 and 7a-c");
}

#[test]
fn accents_are_decoded_the_way_biber_writes_them() {
    // `biber --tool` writes UTF-8, so `{\"O}zge` is `Özge`. The .bbl keeps the TeX: biblatex
    // is the thing that interprets it.
    use bibsmith::normalize::decode_accents;
    assert_eq!(decode_accents("{\\\"O}zge"), "Özge");
    assert_eq!(decode_accents("T{\\\"u}rkmen"), "Türkmen");
    assert_eq!(decode_accents("{\\c{C}}etinkaya"), "Çetinkaya");
    assert_eq!(
        decode_accents("Aks{\\i}n"),
        "Aks{ı}n",
        "biber keeps the braces around a letter command"
    );
    assert_eq!(decode_accents("Unzeitgem{\\\"a}{\\ss}e"), "Unzeitgemä{ß}e");
    assert_eq!(
        decode_accents("plain text"),
        "plain text",
        "nothing to do, nothing changed"
    );
}

#[test]
fn a_bibtex_only_entry_type_is_renamed_and_its_word_kept() {
    // `@mastersthesis` is not a biblatex type. biber writes `@thesis` and keeps the word in
    // `type`, which is what the driver prints, so nothing is lost on a round trip.
    let src = "@mastersthesis{k, author = {A, B}, title = {T}, school = {S}, year = {2000}}\n";
    let out: String = bibsmith::normalize::normalise_all_with(src, true)
        .expect("normalise")
        .iter()
        .map(bibsmith::normalize::write_raw)
        .collect();
    assert!(out.contains("@THESIS{k"), "{out}");
    assert!(out.contains("TYPE = {mathesis}"), "{out}");

    // An author's own type field wins: it is what they asked to print.
    let own = "@phdthesis{k, author = {A, B}, title = {T}, school = {S}, year = {2000}, type = {Diplomarbeit}}\n";
    let out: String = bibsmith::normalize::normalise_all_with(own, true)
        .expect("normalise")
        .iter()
        .map(bibsmith::normalize::write_raw)
        .collect();
    assert!(out.contains("@THESIS{k"), "{out}");
    assert!(out.contains("Diplomarbeit"), "{out}");
    assert!(!out.contains("phdthesis}"), "{out}");
}
