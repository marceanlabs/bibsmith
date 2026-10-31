// Reading a Crossref record and deciding what an entry is missing: no network involved.

use std::collections::HashSet;

const RECORD: &str = r#"{
  "status": "ok",
  "message": {
    "DOI": "10.1103/PhysRev.47.777",
    "title": ["Can Quantum-Mechanical Description of Physical Reality Be Considered Complete?"],
    "author": [
      {"given": "A.", "family": "Einstein"},
      {"given": "B.", "family": "Podolsky"},
      {"given": "N.", "family": "Rosen"}
    ],
    "container-title": ["Physical Review"],
    "volume": "47",
    "issue": "10",
    "page": "777-780",
    "publisher": "American Physical Society",
    "issued": {"date-parts": [[1935, 5, 15]]}
  }
}"#;

#[test]
fn a_crossref_record_is_read_into_the_fields_biblatex_names() {
    let work = bibsmith::fetch::parse_crossref(RECORD).expect("a record");
    assert_eq!(work.year.as_deref(), Some("1935"));
    assert_eq!(work.volume.as_deref(), Some("47"));
    assert_eq!(work.issue.as_deref(), Some("10"));
    assert_eq!(work.pages.as_deref(), Some("777-780"));
    assert_eq!(work.container.as_deref(), Some("Physical Review"));
    assert_eq!(work.doi.as_deref(), Some("10.1103/PhysRev.47.777"));
    assert_eq!(work.authors.len(), 3);
    assert_eq!(work.authors[0], ("Einstein".into(), "A.".into()));
}

#[test]
fn nothing_the_author_wrote_is_replaced() {
    let work = bibsmith::fetch::parse_crossref(RECORD).expect("a record");
    let have: HashSet<String> = ["author", "title", "year"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let added = bibsmith::fetch::missing_fields(&work, &have, "article");
    let names: Vec<&str> = added.iter().map(|(n, _)| n.as_str()).collect();
    assert!(!names.contains(&"author"), "{names:?}");
    assert!(!names.contains(&"title"), "{names:?}");
    assert!(!names.contains(&"year"), "{names:?}");
    assert!(names.contains(&"journaltitle"), "{names:?}");
    assert!(names.contains(&"pages"), "{names:?}");
    // A publisher belongs on a book, not on a journal article.
    assert!(!names.contains(&"publisher"), "{names:?}");
}

#[test]
fn a_record_with_no_author_leaves_the_author_alone() {
    let json = r#"{"message": {"title": ["Untitled"], "issued": {"date-parts": [[2001]]}}}"#;
    let work = bibsmith::fetch::parse_crossref(json).expect("a record");
    let added = bibsmith::fetch::missing_fields(&work, &HashSet::new(), "misc");
    assert!(
        !added.iter().any(|(n, _)| n == "author"),
        "an empty author list must not write an empty author field"
    );
}

#[test]
fn a_field_is_added_inside_the_entry_and_nowhere_else() {
    let src = "@misc{a,\n  title = {T},\n}\n\n@misc{b,\n  title = {U},\n}\n";
    let out = bibsmith::fetch::add_fields(src, "a", &[("year".to_string(), "1935".to_string())]);
    assert!(out.contains("  year = {1935},\n}"), "{out}");
    // The second entry is untouched.
    let b = &out[out.find("@misc{b").unwrap()..];
    assert!(!b.contains("year"), "{out}");
}
