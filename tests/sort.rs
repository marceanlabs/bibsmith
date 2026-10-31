
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
