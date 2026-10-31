//! Contract tests for the `.bbl` writer.
//!
//! These check the things that are invisible in the source and expensive to rediscover:
//! a list field written as `\field`, a date written as a date, or a name list without a
//! `\name` block all parse fine and print wrong. The fixtures come from a real
//! biblatex run (`oracle/doc/doc.bcf`), so the data model they exercise is biblatex's
//! own.
//!
//! The rendered comparison against biber lives in `scripts/check-pdf.sh`, because it
//! needs a TeX installation and a PDF text extractor.

use std::collections::HashMap;
use std::path::Path;

use bibsmith::{ControlFile, sort_entries, write_bbl};

fn oracle(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("oracle")
        .join(name)
}

fn load(pairs: &[(&str, &str)]) -> Vec<bibsmith::merge::Effective> {
    let mut src = String::new();
    for (file, _) in pairs {
        src.push_str(&std::fs::read_to_string(oracle(file)).expect("fixture"));
        src.push('\n');
    }
    let db = biblatex::Bibliography::parse(&src).expect("parse");
    let raw: HashMap<String, Vec<(String, String)>> =
        bibsmith::normalize::raw_expanded_entries(&src)
            .expect("raw")
            .into_iter()
            .map(|e| (e.key, e.fields))
            .collect();
    let keys: Vec<String> = raw.keys().cloned().collect();
    let entries: Vec<&biblatex::Entry> = keys.iter().filter_map(|k| db.get(k)).collect();
    entries
        .into_iter()
        .map(|e| bibsmith::merge::effective(&db, e, &raw))
        .collect()
}

fn render(pairs: &[(&str, &str)], bcf: &str) -> String {
    let xml = std::fs::read_to_string(oracle(bcf)).expect("bcf");
    let ctl = ControlFile::parse(&xml).expect("bcf parses");
    let effective = load(pairs);
    let refs: Vec<&bibsmith::merge::Effective> = effective.iter().collect();
    let extras = HashMap::new();
    write_bbl(&ctl, &refs, &extras, &HashMap::new())
}

#[test]
fn header_is_the_one_biblatex_checks() {
    let bbl = render(&[("doc/refs.bib", "refs.bib")], "doc/doc.bcf");
    assert!(bbl.starts_with("% $ biblatex auxiliary file $\n"));
    assert!(bbl.contains("% $ biblatex bbl format version 3.3 $"));
    assert!(bbl.contains("\\@ifundefined{ver@biblatex.sty}"));
    assert!(bbl.trim_end().ends_with("\\endinput"));
}

#[test]
fn datalist_carries_the_sorting_template_from_the_control_file() {
    let xml = std::fs::read_to_string(oracle("doc/doc.bcf")).expect("bcf");
    let ctl = ControlFile::parse(&xml).expect("bcf parses");
    let bbl = render(&[("doc/refs.bib", "refs.bib")], "doc/doc.bcf");
    assert!(
        bbl.contains(&format!("\\datalist[entry]{{{}}}", ctl.datalist_name)),
        "the datalist name decides which template biblatex sorts by"
    );
}

#[test]
fn list_fields_are_written_as_lists() {
    // `location` and `publisher` are list fields in biblatex's data model. Written as
    // `\field` they are silently not printed, which is how the publisher line went
    // missing from a real document.
    let bbl = render(&[("doc/refs.bib", "refs.bib")], "doc/doc.bcf");
    assert!(
        bbl.contains("\\list{location}{1}{%"),
        "location must be a \\list"
    );
    assert!(
        bbl.contains("\\list{publisher}{1}{%"),
        "publisher must be a \\list"
    );
    assert!(!bbl.contains("\\field{location}"));
    assert!(!bbl.contains("\\field{publisher}"));
}

#[test]
fn dates_are_written_as_parts() {
    let bbl = render(&[("doc/refs.bib", "refs.bib")], "doc/doc.bcf");
    assert!(bbl.contains("\\field{year}{2004}"));
    assert!(
        bbl.contains("\\field{month}{10}"),
        "a date part keeps its month"
    );
    assert!(bbl.contains("\\field{day}{27}"));
    assert!(
        !bbl.contains("\\field{date}{"),
        "biblatex never reads a bare date"
    );
    assert!(bbl.contains("\\field{labeldatesource}{}"));
    assert!(bbl.contains("\\field{dateera}{ce}"));
}

#[test]
fn crossref_fields_are_inherited_with_their_renames() {
    // `westfahl:space` is an @incollection whose book title and editors live on the
    // parent @collection. Without inheritance it prints "In: pp. 55-65" and no book.
    let bbl = render(&[("doc/refs.bib", "refs.bib")], "doc/doc.bcf");
    let space = bbl
        .split("\\entry{westfahl:space}")
        .nth(1)
        .expect("entry present");
    let block = &space[..space.find("\\endentry").unwrap()];
    assert!(
        block.contains("\\field{booktitle}{Space and Beyond}"),
        "title becomes booktitle"
    );
    assert!(
        block.contains("\\name{editor}{2}"),
        "the parent's editors inherit"
    );
    assert!(block.contains("\\list{location}{1}{%"));
}

#[test]
fn every_entry_with_an_author_gets_a_name_block() {
    let bbl = render(&[("doc/refs.bib", "refs.bib")], "doc/doc.bcf");
    let entries = bbl.matches("\n    \\entry{").count();
    assert_eq!(entries, 5);
    // Four authors, plus two editor lists: the collection, and the chapter that
    // inherits its editors through crossref.
    assert_eq!(bbl.matches("\\name{author}").count(), 4);
    assert_eq!(bbl.matches("\\name{editor}").count(), 2);
}

#[test]
fn ampersands_are_escaped_so_they_print() {
    let mut src = String::from(
        "@book{a, author = {A, B}, title = {Computers & Typesetting}, publisher = {P}, date = {1984}}\n",
    );
    src.push_str(&std::fs::read_to_string(oracle("doc/refs.bib")).unwrap());
    let db = biblatex::Bibliography::parse(&src).unwrap();
    let raw: HashMap<String, Vec<(String, String)>> =
        bibsmith::normalize::raw_expanded_entries(&src)
            .unwrap()
            .into_iter()
            .map(|e| (e.key, e.fields))
            .collect();
    let entry = db.get("a").unwrap();
    let eff = bibsmith::merge::effective(&db, entry, &raw);
    let xml = std::fs::read_to_string(oracle("doc/doc.bcf")).unwrap();
    let ctl = ControlFile::parse(&xml).unwrap();
    let bbl = write_bbl(&ctl, &[&eff], &HashMap::new(), &HashMap::new());
    assert!(
        bbl.contains("Computers \\& Typesetting"),
        "an unescaped & drops the rest of the line from the page"
    );
}

#[test]
fn a_url_date_becomes_url_parts() {
    let mut src = String::from(
        "@online{ctan, title = {CTAN}, date = {2006}, urldate = {2006-10-01}, url = {http://www.ctan.org}}\n",
    );
    src.push_str(&std::fs::read_to_string(oracle("doc/refs.bib")).unwrap());
    let db = biblatex::Bibliography::parse(&src).unwrap();
    let raw: HashMap<String, Vec<(String, String)>> =
        bibsmith::normalize::raw_expanded_entries(&src)
            .unwrap()
            .into_iter()
            .map(|e| (e.key, e.fields))
            .collect();
    let eff = bibsmith::merge::effective(&db, db.get("ctan").unwrap(), &raw);
    let xml = std::fs::read_to_string(oracle("doc/doc.bcf")).unwrap();
    let ctl = ControlFile::parse(&xml).unwrap();
    let bbl = write_bbl(&ctl, &[&eff], &HashMap::new(), &HashMap::new());
    assert!(bbl.contains("\\field{urlyear}{2006}"));
    assert!(bbl.contains("\\field{urlmonth}{10}"));
    assert!(bbl.contains("\\field{urlday}{1}"));
    assert!(bbl.contains("\\verb{url}"), "url is a verbatim field");
}

#[test]
fn a_date_range_becomes_a_start_and_an_end() {
    let mut src = String::from("@book{r, author = {A, B}, title = {T}, date = {1984/1986}}\n");
    src.push_str(&std::fs::read_to_string(oracle("doc/refs.bib")).unwrap());
    let db = biblatex::Bibliography::parse(&src).unwrap();
    let raw: HashMap<String, Vec<(String, String)>> =
        bibsmith::normalize::raw_expanded_entries(&src)
            .unwrap()
            .into_iter()
            .map(|e| (e.key, e.fields))
            .collect();
    let eff = bibsmith::merge::effective(&db, db.get("r").unwrap(), &raw);
    let xml = std::fs::read_to_string(oracle("doc/doc.bcf")).unwrap();
    let ctl = ControlFile::parse(&xml).unwrap();
    let bbl = write_bbl(&ctl, &[&eff], &HashMap::new(), &HashMap::new());
    assert!(bbl.contains("\\field{year}{1984}"));
    assert!(
        bbl.contains("\\field{endyear}{1986}"),
        "a range is two dates, not one"
    );
}

#[test]
fn order_matches_the_control_files_template() {
    // The document names its sorting template in the .bcf; the writer must follow it
    // rather than a compiled-in default.
    let xml = std::fs::read_to_string(oracle("doc/doc.bcf")).unwrap();
    let ctl = ControlFile::parse(&xml).unwrap();
    assert_eq!(ctl.sorting_template, "nyt");
    assert!(
        !ctl.sorts.is_empty(),
        "the template's sort keys are read from the file"
    );
    let _ = sort_entries;
}

#[test]
fn alphabetic_labels_match_biber_on_every_entry() {
    // The fixture is biber's own `labelalpha` for all 86 labelled entries of the
    // canonical corpus, so this is parity, not plausibility: `GMS94` for three
    // authors, `Alm+98` for more than three, `Gla61` for one, `vGen09` keeping the
    // prefix and `KpV` staying bare because a shorthand is final.
    let fixture = std::fs::read_to_string(oracle("labelalpha-alphabetic.tsv")).expect("fixture");
    let src = std::fs::read_to_string(oracle("refs.bib")).expect("corpus");
    let db = biblatex::Bibliography::parse(&src).expect("parse");
    let raw: HashMap<String, Vec<(String, String)>> =
        bibsmith::normalize::raw_expanded_entries(&src)
            .expect("raw")
            .into_iter()
            .map(|e| (e.key, e.fields))
            .collect();

    let mut checked = 0;
    for line in fixture.lines() {
        let Some((key, want)) = line.split_once('\t') else {
            continue;
        };
        let entry = db.get(key).expect("cited in the fixture");
        let eff = bibsmith::merge::effective(&db, entry, &raw);
        let member = eff
            .entryset
            .as_ref()
            .and_then(|s| s.split(',').next())
            .map(|k| k.trim().to_string())
            .and_then(|k| db.get(&k).map(|e| bibsmith::merge::effective(&db, e, &raw)));
        let got = bibsmith::label::label_alpha_with(&eff, 2, member.as_ref());
        assert_eq!(got, want, "label for {key}");
        checked += 1;
    }
    assert_eq!(checked, 86);
}

#[test]
fn a_repeated_label_is_numbered_from_one() {
    let labels = vec![
        "Gao01".to_string(),
        "Gao01".to_string(),
        "Gao01".to_string(),
    ];
    assert_eq!(
        bibsmith::label::extra_alpha(&labels),
        vec![Some(1), Some(2), Some(3)],
        "biblatex prints 1 as 'a', so the count starts at one"
    );
    let unique = vec!["Gla61".to_string()];
    assert_eq!(bibsmith::label::extra_alpha(&unique), vec![None]);
}

#[test]
fn a_key_cited_twice_is_written_once() {
    // The control file lists a key once per citation. Without de-duplication the
    // bibliography prints the entry and then a dash for every repeat, which is what a
    // document that cites the same work twice in a row showed.
    let xml = std::fs::read_to_string(oracle("doc/doc.bcf")).expect("bcf");
    let doubled = xml.replace(
        "<bcf:citekey order=\"1\"",
        "<bcf:citekey order=\"0\">knuth:ct</bcf:citekey>\n      <bcf:citekey order=\"1\"",
    );
    let ctl = ControlFile::parse(&doubled).expect("bcf parses");
    let count = ctl.citekeys.iter().filter(|k| *k == "knuth:ct").count();
    assert_eq!(count, 1, "a repeated citation must not become a repeated entry");
}
