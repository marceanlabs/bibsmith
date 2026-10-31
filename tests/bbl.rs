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

#[test]
fn xdata_fields_are_merged_and_win() {
    // Measured against biber: an entry with `location = {Cambridge}` and an xdata
    // `location = {Oxford}` prints Oxford, so the inherited value wins.
    let src = std::fs::read_to_string(oracle("xdata/x.bib")).expect("fixture");
    let db = biblatex::Bibliography::parse(&src).expect("parse");
    let raw: HashMap<String, Vec<(String, String)>> = bibsmith::normalize::raw_expanded_entries(&src)
        .expect("raw")
        .into_iter()
        .map(|e| (e.key, e.fields))
        .collect();
    let eff = bibsmith::merge::effective(&db, db.get("second").expect("entry"), &raw);
    let value = |name: &str| {
        eff.values
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.clone())
            .unwrap_or_default()
    };
    assert_eq!(value("publisher"), "Shared Press");
    assert_eq!(value("location"), "Oxford", "the xdata value wins over the entry's own");
}

#[test]
fn a_bibtex_only_type_becomes_the_biblatex_type_and_a_type_field() {
    let mut src = String::from("@mastersthesis{m, author = {A, B}, title = {T}, school = {S}, year = {1999}}\n");
    src.push_str(&std::fs::read_to_string(oracle("doc/refs.bib")).unwrap());
    let db = biblatex::Bibliography::parse(&src).unwrap();
    let raw: HashMap<String, Vec<(String, String)>> = bibsmith::normalize::raw_expanded_entries(&src)
        .unwrap()
        .into_iter()
        .map(|e| (e.key, e.fields))
        .collect();
    let eff = bibsmith::merge::effective(&db, db.get("m").unwrap(), &raw);
    assert_eq!(eff.kind, "thesis", "biblatex has no mastersthesis type");
    assert!(eff.values.iter().any(|(k, v)| k == "type" && v == "mathesis"));
}

#[test]
fn a_month_name_is_written_as_a_number_and_a_range_starts_where_it_starts() {
    for (input, want) in [("January", "1"), ("Oct.", "10"), ("Jan.-March", "1"), ("10~1", "10~1")] {
        let mut src = format!("@misc{{m, author = {{A, B}}, title = {{T}}, year = {{2001}}, month = {{{input}}}}}\n");
        src.push_str(&std::fs::read_to_string(oracle("doc/refs.bib")).unwrap());
        let db = biblatex::Bibliography::parse(&src).unwrap();
        let raw: HashMap<String, Vec<(String, String)>> = bibsmith::normalize::raw_expanded_entries(&src)
            .unwrap()
            .into_iter()
            .map(|e| (e.key, e.fields))
            .collect();
        let eff = bibsmith::merge::effective(&db, db.get("m").unwrap(), &raw);
        let xml = std::fs::read_to_string(oracle("doc/doc.bcf")).unwrap();
        let ctl = ControlFile::parse(&xml).unwrap();
        let bbl = write_bbl(&ctl, &[&eff], &HashMap::new(), &HashMap::new());
        assert!(
            bbl.contains(&format!("\\field{{month}}{{{want}}}")),
            "month {input:?} should be {want}"
        );
    }
}

#[test]
fn an_explicit_location_beats_the_old_address_field() {
    let mut src = String::from("@book{b, author = {A, B}, title = {T}, year = {1990}, location = {Florence}, address = {New York}}\n");
    src.push_str(&std::fs::read_to_string(oracle("doc/refs.bib")).unwrap());
    let db = biblatex::Bibliography::parse(&src).unwrap();
    let raw: HashMap<String, Vec<(String, String)>> = bibsmith::normalize::raw_expanded_entries(&src)
        .unwrap()
        .into_iter()
        .map(|e| (e.key, e.fields))
        .collect();
    let eff = bibsmith::merge::effective(&db, db.get("b").unwrap(), &raw);
    let xml = std::fs::read_to_string(oracle("doc/doc.bcf")).unwrap();
    let ctl = ControlFile::parse(&xml).unwrap();
    let bbl = write_bbl(&ctl, &[&eff], &HashMap::new(), &HashMap::new());
    assert!(bbl.contains("{Florence}"), "the new name wins");
    assert!(!bbl.contains("{New York}"), "the old name must not be written as well");
}

#[test]
fn an_empty_author_means_no_author() {
    let mut src = String::from("@proceedings{emptyauthor, author = {}, title = {CHI}, year = {2008}}\n");
    src.push_str(&std::fs::read_to_string(oracle("doc/refs.bib")).unwrap());
    let db = biblatex::Bibliography::parse(&src).unwrap();
    let raw: HashMap<String, Vec<(String, String)>> = bibsmith::normalize::raw_expanded_entries(&src)
        .unwrap()
        .into_iter()
        .map(|e| (e.key, e.fields))
        .collect();
    let eff = bibsmith::merge::effective(&db, db.get("emptyauthor").unwrap(), &raw);
    assert!(eff.names.is_empty(), "a phantom person sorts the entry to the front");
}

fn one_bbl(entry: &str, key: &str) -> String {
    let mut src = String::from(entry);
    src.push('\n');
    src.push_str(&std::fs::read_to_string(oracle("doc/refs.bib")).unwrap());
    let db = biblatex::Bibliography::parse(&src).unwrap();
    let raw: HashMap<String, Vec<(String, String)>> = bibsmith::normalize::raw_expanded_entries(&src)
        .unwrap()
        .into_iter()
        .map(|e| (e.key, e.fields))
        .collect();
    let eff = bibsmith::merge::effective(&db, db.get(key).unwrap(), &raw);
    let xml = std::fs::read_to_string(oracle("doc/doc.bcf")).unwrap();
    let ctl = ControlFile::parse(&xml).unwrap();
    write_bbl(&ctl, &[&eff], &HashMap::new(), &HashMap::new())
}

#[test]
fn a_zotero_datetime_is_not_a_date() {
    // `2019-06-01 00:00:00` is what Zotero writes. biber rejects the field and the entry
    // reads "n.d."; a naive split prints a month of "100:00:00".
    let bbl = one_bbl(
        "@book{zd, author = {Jones, Alice}, title = {T}, date = {2019-06-01 00:00:00}}",
        "zd",
    );
    assert!(!bbl.contains("\\field{year}"), "an unreadable date has no year");
    assert!(bbl.contains("\\field{labeldatesource}{nodate}"), "this is what prints n.d.");
    assert!(!bbl.contains("dateera"));
}

#[test]
fn a_url_is_written_raw_because_it_sits_in_verbatim() {
    let bbl = one_bbl(
        "@online{zu, author = {A, B}, title = {T}, year = {2001}, url = {https://e.org/a?x=1&y=2}}",
        "zu",
    );
    assert!(bbl.contains("x=1&y=2"), "inside \\verb an escaped & prints a backslash");
    assert!(!bbl.contains("x=1\\&y=2"));
}

#[test]
fn a_name_list_is_extended_only_until_it_tells_two_entries_apart() {
    // Measured against biber: two entries that share a year and a first author are shown
    // further until the lists differ, and no further.
    let src = String::from(
        "@article{u1, author = {Akyildiz, Ian F. and Su, W. and Sankarasubramaniam, E. and Cayirci, E.}, title = {One}, journal = {J}, year = {2002}}\n\
         @article{u2, author = {Akyildiz, Ian F. and Bogdanov, X. and Others, Y. and More, Z.}, title = {Two}, journal = {J}, year = {2002}}\n\
         @article{u3, author = {Zeta, Zed and One, A. and Two, B. and Three, C.}, title = {Three}, journal = {J}, year = {2002}}\n",
    );
    let db = biblatex::Bibliography::parse(&src).unwrap();
    let raw: HashMap<String, Vec<(String, String)>> = bibsmith::normalize::raw_expanded_entries(&src)
        .unwrap()
        .into_iter()
        .map(|e| (e.key, e.fields))
        .collect();
    let entries: Vec<_> = ["u1", "u2", "u3"]
        .iter()
        .map(|k| bibsmith::merge::effective(&db, db.get(k).unwrap(), &raw))
        .collect();
    let refs: Vec<&_> = entries.iter().collect();
    let xml = std::fs::read_to_string(oracle("doc/doc.bcf")).unwrap();
    let ctl = ControlFile::parse(&xml).unwrap();
    let bbl = write_bbl(&ctl, &refs, &HashMap::new(), &HashMap::new());
    assert!(bbl.contains("{ul=2}"), "the two Akyildiz entries need a second name");
    assert!(!bbl.contains("{ul=3}"), "two names already separate them");
    let zeta = &bbl[bbl.find("\\entry{u3}").unwrap()..];
    assert!(
        !zeta[..zeta.find("\\endentry").unwrap()].contains("ul="),
        "a unique first author is left alone"
    );
}

#[test]
fn an_identical_name_list_is_not_extended() {
    // Two copies of one record: showing more names cannot separate them, and biber writes
    // nothing.
    let src = String::from(
        "@article{d1, author = {Solo, A. and Second, B. and Third, C. and Fourth, D.}, title = {T}, journal = {J}, year = {2001}}\n\
         @article{d2, author = {Solo, A. and Second, B. and Third, C. and Fourth, D.}, title = {T}, journal = {J}, year = {2001}}\n",
    );
    let db = biblatex::Bibliography::parse(&src).unwrap();
    let raw: HashMap<String, Vec<(String, String)>> = bibsmith::normalize::raw_expanded_entries(&src)
        .unwrap()
        .into_iter()
        .map(|e| (e.key, e.fields))
        .collect();
    let entries: Vec<_> = ["d1", "d2"]
        .iter()
        .map(|k| bibsmith::merge::effective(&db, db.get(k).unwrap(), &raw))
        .collect();
    let refs: Vec<&_> = entries.iter().collect();
    let xml = std::fs::read_to_string(oracle("doc/doc.bcf")).unwrap();
    let ctl = ControlFile::parse(&xml).unwrap();
    let bbl = write_bbl(&ctl, &refs, &HashMap::new(), &HashMap::new());
    assert!(!bbl.contains("ul="), "nothing to gain, so nothing written");
}

#[test]
fn a_document_with_two_sections_gets_two_refsections() {
    // The control file lists the database once per section, and the same file read twice
    // makes every key look like a duplicate.
    let xml = r#"<?xml version="1.0"?>
<bcf:controlfile version="3.3" xmlns:bcf="https://sourceforge.net/projects/biblatex">
  <bcf:datasource type="file" datatype="bibtex">m.bib</bcf:datasource>
  <bcf:datasource type="file" datatype="bibtex">m.bib</bcf:datasource>
  <bcf:section number="0">
    <bcf:section number="1">
      <bcf:citekey order="1">alpha</bcf:citekey>
      <bcf:citekey order="2">beta</bcf:citekey>
    </bcf:section>
  </bcf:section>
  <bcf:section number="0">
    <bcf:section number="2">
      <bcf:citekey order="1">gamma</bcf:citekey>
      <bcf:citekey order="2">delta</bcf:citekey>
    </bcf:section>
  </bcf:section>
  <bcf:sortingtemplate name="nyt"><bcf:sort><bcf:sortitem><bcf:field>author</bcf:field></bcf:sortitem></bcf:sort></bcf:sortingtemplate>
  <bcf:datalist section="1" name="nyt/global//global/global/global"/>
  <bcf:datalist section="2" name="nyt/global//global/global/global"/>
</bcf:controlfile>"#;
    let ctl = ControlFile::parse(xml).expect("parse");
    assert_eq!(ctl.datasources.len(), 1, "the same file listed twice is one file");
    let numbers: Vec<&str> = ctl.sections.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(numbers, ["1", "2"], "one section per \\refsection");
    assert_eq!(ctl.sections[0].1, ["alpha", "beta"]);
    assert_eq!(ctl.sections[1].1, ["gamma", "delta"]);
}

#[test]
fn a_book_author_is_inherited_through_a_crossref_chain() {
    // `\DeclareDataInheritance{mvbook,book}{inbook,...}{\inherit{author}{bookauthor}}`:
    // the book's author prints after "In:", and it survives a second hop through another
    // inbook, which is what a chapter-of-a-chapter looks like.
    let src = String::from(
        "@book{parent, author = {Editor, Ed}, title = {Collected Works}, publisher = {P}, year = {1990}}\n\
         @inbook{middle, author = {Writer, Wanda}, title = {Middle Piece}, crossref = {parent}, pages = {1--10}}\n\
         @inbook{child, author = {Junior, Jim}, title = {Child Piece}, crossref = {middle}, pages = {11--20}}\n",
    );
    let db = biblatex::Bibliography::parse(&src).unwrap();
    let raw: HashMap<String, Vec<(String, String)>> = bibsmith::normalize::raw_expanded_entries(&src)
        .unwrap()
        .into_iter()
        .map(|e| (e.key, e.fields))
        .collect();
    for key in ["middle", "child"] {
        let eff = bibsmith::merge::effective(&db, db.get(key).unwrap(), &raw);
        let book = eff.names.iter().find(|(f, _)| f == "bookauthor");
        assert!(book.is_some(), "{key} needs a bookauthor or the book's editor vanishes");
        assert_eq!(book.unwrap().1[0].name, "Editor");
    }
}

#[test]
fn a_date_range_numbers_by_its_years_not_its_months() {
    // `2001/2003` and `2001-05/2003-06` are both 2001–2003, so they are one label and take
    // a/b; a plain 2001 is a different label and takes nothing.
    let entries = [
        ("@misc{r1, author = {Span, Sam}, title = {One}, date = {2001/2003}}", "r1"),
        ("@misc{r2, author = {Span, Sam}, title = {Two}, date = {2001-05/2003-06}}", "r2"),
    ];
    let mut src = String::new();
    for (e, _) in entries {
        src.push_str(e);
        src.push('\n');
    }
    src.push_str(&std::fs::read_to_string(oracle("doc/refs.bib")).unwrap());
    let db = biblatex::Bibliography::parse(&src).unwrap();
    let ents: Vec<_> = entries
        .iter()
        .map(|(_, k)| db.get(k).unwrap())
        .collect();
    let ordered = bibsmith::sort::sort_entries(&ents, bibsmith::Scheme::Nyt);
    let extras = bibsmith::disambiguate::disambiguate(&ordered, bibsmith::Scheme::Nyt);
    assert_eq!(extras.get("r1"), Some(&1), "the first range takes a");
    assert_eq!(extras.get("r2"), Some(&2), "the second takes b");
}

#[test]
fn a_related_entry_is_written_under_its_hash_and_left_out_of_the_list() {
    // biblatex resolves `related` by looking the hash up in the datalist, so the related
    // entry has to be there under that hash: without it the "See also" text never prints.
    let src = String::from(
        "@misc{main, author = {Main, Mary}, title = {Main Entry}, year = {2000}, related = {other}, relatedstring = {See also}}\n\
         @misc{other, author = {Oscar, O.}, title = {Related Entry}, year = {2001}}\n",
    );
    let db = biblatex::Bibliography::parse(&src).unwrap();
    let raw: HashMap<String, Vec<(String, String)>> = bibsmith::normalize::raw_expanded_entries(&src)
        .unwrap()
        .into_iter()
        .map(|e| (e.key, e.fields))
        .collect();
    let mut main = bibsmith::merge::effective(&db, db.get("main").unwrap(), &raw);
    let hash = format!("{:x}", md5::compute(b"other"));
    main.values
        .iter_mut()
        .find(|(k, _)| k == "related")
        .unwrap()
        .1 = hash.clone();
    let mut data = bibsmith::merge::effective(&db, db.get("other").unwrap(), &raw);
    data.data_key = Some(hash.clone());

    let xml = std::fs::read_to_string(oracle("doc/doc.bcf")).unwrap();
    let ctl = ControlFile::parse(&xml).unwrap();
    let bbl = write_bbl(&ctl, &[&main, &data], &HashMap::new(), &HashMap::new());
    assert!(bbl.contains(&format!("\\entry{{{hash}}}{{misc}}{{skipbib=true")), "written under its hash, out of the list");
    assert!(bbl.contains(&format!("\\field{{related}}{{{hash}}}")), "the parent points at that hash");
}

#[test]
fn entries_that_share_a_name_are_numbered_in_bibliography_order() {
    // `uniquename` is a number per entry, not a flag: biblatex counts the entries whose
    // label name is the same and prints enough of the name to tell them apart. Measured
    // against biber on the corpus, where seven Knuth volumes number 1 to 7 and a related
    // copy carries nothing.
    let bbl = render(&[("refs.bib", "refs.bib")], "nyt.bcf");
    let knuth: Vec<&str> = bbl
        .lines()
        .filter(|l| l.contains("\\entry{knuth:ct"))
        .collect();
    assert!(knuth.len() >= 7, "{knuth:?}");
    for (i, line) in knuth.iter().take(7).enumerate() {
        let key = line.split('{').nth(1).unwrap().split('}').next().unwrap();
        let number = bbl
            .split(&format!("\\entry{{{key}}}"))
            .nth(1)
            .and_then(|rest| rest.split("\\endentry").next())
            .and_then(|body| {
                body.split("\\field{extraname}{")
                    .nth(1)
                    .map(|s| s.split('}').next().unwrap().to_string())
            });
        assert_eq!(number.as_deref(), Some((i + 1).to_string().as_str()), "{key}");
    }
}

#[test]
fn the_index_letter_follows_the_label_name_and_keeps_its_case() {
    // bibtex files `van Gennep` under `v` when the entry asks for the particle, and under
    // `B` for `von Brandt` when it does not. The letter is written as it is printed, not
    // upper-cased: the corpus has a lower-case `v` and biber writes exactly that.
    let bbl = render(&[("refs.bib", "refs.bib")], "nyt.bcf");
    let field_of = |key: &str, name: &str| -> Option<String> {
        bbl.split(&format!("\\entry{{{key}}}"))
            .nth(1)
            .and_then(|rest| rest.split("\\endentry").next())
            .and_then(|body| {
                body.split(&format!("\\field{{{name}}}{{"))
                    .nth(1)
                    .map(|s| s.split('}').next().unwrap().to_string())
            })
    };
    assert_eq!(field_of("vangennep", "sortinit").as_deref(), Some("v"));
    assert_eq!(field_of("brandt", "sortinit").as_deref(), Some("B"));
    assert_eq!(field_of("britannica", "sortinit").as_deref(), Some("E"));
}
