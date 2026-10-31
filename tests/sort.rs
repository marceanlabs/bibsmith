
#[test]
fn the_bcf_template_is_the_rule_not_a_name() {
    // `nyvt` sorts name, year, volume, title. It was never implemented as a scheme, and
    // reading the template out of the control file is what makes it work at all: the CLI
    // used to refuse any template whose name it did not recognise.
    let src = std::fs::read_to_string("oracle/refs.bib").unwrap();
    let db = bibsmith::parse_lenient(&src).unwrap();
    let entries: Vec<&biblatex::Entry> = db.iter().collect();
    let xml = std::fs::read_to_string("oracle/style-nyvt.bcf").expect("nyvt bcf");
    let ctl = bibsmith::bcf::ControlFile::parse(&xml).unwrap();
    let groups = ctl.templates.get("nyvt").expect("template parsed");
    let extra = std::collections::HashMap::new();
    let ordered = bibsmith::sort::sort_entries_by_template(&entries, groups, &extra);
    let keys: Vec<&str> = ordered.iter().map(|e| e.key.as_str()).collect();
    assert!(keys.len() > 50, "{keys:?}");
    // Name, then year, then the volume letter: the four 1986 Knuth volumes come out A to E
    // except the one from 1984, which sorts before them all.
    let knuth: Vec<&str> = keys.iter().copied().filter(|k| k.starts_with("knuth:ct")).collect();
    assert_eq!(
        knuth,
        vec![
            "knuth:ct", "knuth:ct:related", "knuth:ct:a", "knuth:ct:b", "knuth:ct:c",
            "knuth:ct:d", "knuth:ct:e"
        ],
        "{knuth:?}"
    );
}

#[test]
fn a_legacy_key_is_sorted_as_text_not_as_markup() {
    // `key = {Kn{\printfirst{v}{1987}}}` is how BibTeX asks to file under "Knv1987". Collating
    // the braces instead files it before `Knuth`, because this collator orders a brace after
    // letters only when it is not treated as text.
    let src = "@misc{a1, key = {Kn{aa}}, title = {T1}}\n\
               @misc{a2, author = {Knuth, Donald}, title = {T2}}\n\
               @misc{a3, key = {Kn{\\printfirst{v}{1987}}}, title = {T3}}\n\
               @misc{a4, key = {Knz}, title = {T4}}\n";
    let db = bibsmith::parse_lenient(src).expect("parse");
    let entries: Vec<&biblatex::Entry> = db.iter().collect();
    let by_key: std::collections::HashMap<&str, &biblatex::Entry> =
        entries.iter().map(|e| (e.key.as_str(), *e)).collect();
    let groups = vec![bibsmith::bcf::SortGroup {
        finalsort: false,
        items: vec![
            bibsmith::bcf::SortItem::Field("author".into()),
            bibsmith::bcf::SortItem::Field("title".into()),
        ],
    }];
    let extra = std::collections::HashMap::new();
    let order = bibsmith::sort::sort_entries_by_template(&entries, &groups, &extra);
    let keys: Vec<&str> = order.iter().map(|e| e.key.as_str()).collect();
    // biber's own order for these keys: Knaa, Knuth, Knv1987, Knz.
    assert_eq!(keys, ["a1", "a2", "a3", "a4"], "{keys:?}");
    let _ = by_key;
}
