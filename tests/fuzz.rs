//! Random input, not hand-picked input.
//!
//! Every corpus so far was chosen by hand, which means every bug found so far was found
//! by someone thinking of it. A real file nobody thought of is what breaks a tool, so the
//! parser is fed generated text and the only requirement is that nothing panics: a parse
//! error is a fine answer, a panic is not.

use proptest::prelude::*;

/// The characters that mean something to a `.bib` parser: braces that must balance,
/// quotes, commas that separate fields, the `@` that starts an entry, backslashes that
/// escape, and the `and` that splits a name list.
fn bib_text() -> impl Strategy<Value = String> {
    proptest::collection::vec(
        prop_oneof![
            Just("@article{".to_string()),
            Just("@".to_string()),
            Just("{".to_string()),
            Just("}".to_string()),
            Just(",".to_string()),
            Just("\"".to_string()),
            Just("\\".to_string()),
            Just(" and ".to_string()),
            Just("=".to_string()),
            Just("#".to_string()),
            Just("%".to_string()),
            Just("\n".to_string()),
            Just("author".to_string()),
            Just("year".to_string()),
            Just("date".to_string()),
            Just("pages".to_string()),
            Just("--".to_string()),
            Just("-".to_string()),
            Just("/".to_string()),
            Just("k1".to_string()),
            Just("é".to_string()),
            Just("{\\\"o}".to_string()),
            "[a-zA-Z0-9 ]{0,8}".prop_map(|s| s),
        ],
        0..40,
    )
    .prop_map(|parts| parts.concat())
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 4000, ..ProptestConfig::default() })]

    #[test]
    fn parsing_arbitrary_text_never_panics(src in bib_text()) {
        // Whatever the input, the answer is an entry list or an error.
        let _ = bibsmith::parse_lenient(&src);
        let _ = bibsmith::normalize::raw_entries(&src);
        let _ = bibsmith::normalize::normalise_all(&src);
    }

    #[test]
    fn a_bibliography_that_parses_renders_without_panicking(src in bib_text()) {
        use std::collections::HashMap;
        let Ok(db) = bibsmith::parse_lenient(&src) else { return Ok(()) };
        let Ok(raw) = bibsmith::normalize::raw_expanded_entries(&src) else { return Ok(()) };
        let raw: HashMap<String, Vec<(String, String)>> =
            raw.into_iter().map(|e| (e.key, e.fields)).collect();
        let effective: Vec<bibsmith::merge::Effective> = db
            .iter()
            .map(|entry| bibsmith::merge::effective(&db, entry, &raw))
            .collect();
        let refs: Vec<&bibsmith::merge::Effective> = effective.iter().collect();
        let ctl = bibsmith::bcf::ControlFile::default();
        let _ = bibsmith::write_bbl(&ctl, &refs, &HashMap::new(), &HashMap::new());
    }
}

/// A well-formed entry with a random field set, so the renderer is exercised rather than
/// only the parser's error path.
fn structured_bib() -> impl Strategy<Value = String> {
    let value = prop_oneof![
        "[a-zA-Z0-9 ]{0,20}".prop_map(|s| s),
        Just("2019-06-01 00:00:00".to_string()),
        Just("Jan.-March".to_string()),
        Just("2019/2020".to_string()),
        Just("55-65, 100--200".to_string()),
        Just("Smith, John and Doe, Jane".to_string()),
        Just("Werneck,, Renato".to_string()),
        Just("\\emph{x} and 100% sure".to_string()),
        Just("https://example.com/?a=1&b=2".to_string()),
        Just("".to_string()),
        Just("{}".to_string()),
        Just("useprefix=false".to_string()),
        Just("dtv".to_string()),
    ];
    let field = (
        prop_oneof![
            Just("author"),
            Just("editor"),
            Just("translator"),
            Just("title"),
            Just("year"),
            Just("date"),
            Just("pages"),
            Just("note"),
            Just("url"),
            Just("options"),
            Just("entryset"),
            Just("crossref"),
            Just("related"),
            Just("shorthand"),
            Just("sorttitle"),
        ],
        value,
    );
    (
        prop_oneof![
            Just("article"),
            Just("book"),
            Just("set"),
            Just("misc"),
            Just("thesis")
        ],
        "[a-z0-9:]{1,8}",
        proptest::collection::vec(field, 0..8),
    )
        .prop_map(|(kind, key, fields)| {
            let body: Vec<String> = fields
                .into_iter()
                .map(|(name, value)| format!("  {name} = {{{value}}},\n"))
                .collect();
            format!("@{kind}{{{key},\n{}}}", body.concat())
        })
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 4000, ..ProptestConfig::default() })]

    #[test]
    fn a_well_formed_entry_always_renders(src in structured_bib()) {
        use std::collections::HashMap;
        let db = bibsmith::parse_lenient(&src).expect("a well-formed entry must parse");
        let raw = bibsmith::normalize::raw_expanded_entries(&src).expect("raw");
        let raw: HashMap<String, Vec<(String, String)>> =
            raw.into_iter().map(|e| (e.key, e.fields)).collect();
        let effective: Vec<bibsmith::merge::Effective> = db
            .iter()
            .map(|entry| bibsmith::merge::effective(&db, entry, &raw))
            .collect();
        let refs: Vec<&bibsmith::merge::Effective> = effective.iter().collect();
        let ctl = bibsmith::bcf::ControlFile::default();
        let out = bibsmith::write_bbl(&ctl, &refs, &HashMap::new(), &HashMap::new());
        prop_assert!(out.contains("\\endinput"), "a render must be complete: {out}");
    }
}

/// The corpus with a handful of characters inserted, deleted or swapped.
///
/// Random text mostly fails to parse, which only exercises the error path. Mutating a file
/// that does parse keeps the input close to real, and that is where a hand edit or a bad
/// export lands: one brace moved, one quote lost.
fn mutated_corpus() -> impl Strategy<Value = String> {
    let source = std::fs::read_to_string("oracle/refs.bib").expect("corpus");
    let op = prop_oneof![
        (0usize..3000, Just('{')),
        (0usize..3000, Just('}')),
        (0usize..3000, Just('@')),
        (0usize..3000, Just(',')),
        (0usize..3000, Just('\\')),
        (0usize..3000, Just('"')),
        (0usize..3000, Just('=')),
        (0usize..3000, Just('\n')),
        (0usize..3000, Just('\u{7f}')),
    ];
    proptest::collection::vec(op, 1..10).prop_map(move |ops| {
        let mut chars: Vec<char> = source.chars().collect();
        for (at, ch) in ops {
            let i = at % chars.len();
            match ch {
                '\u{7f}' => {
                    chars.remove(i);
                }
                _ => chars.insert(i, ch),
            }
        }
        chars.into_iter().collect()
    })
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 600, ..ProptestConfig::default() })]

    #[test]
    fn a_corrupted_corpus_never_panics(src in mutated_corpus()) {
        use std::collections::HashMap;
        let _ = bibsmith::normalize::raw_entries(&src);
        let _ = bibsmith::normalize::normalise_all(&src);
        let Ok(db) = bibsmith::parse_lenient(&src) else { return Ok(()) };
        let Ok(raw) = bibsmith::normalize::raw_expanded_entries(&src) else { return Ok(()) };
        let raw: HashMap<String, Vec<(String, String)>> =
            raw.into_iter().map(|e| (e.key, e.fields)).collect();
        let effective: Vec<bibsmith::merge::Effective> = db
            .iter()
            .map(|entry| bibsmith::merge::effective(&db, entry, &raw))
            .collect();
        let refs: Vec<&bibsmith::merge::Effective> = effective.iter().collect();
        let ctl = bibsmith::bcf::ControlFile::default();
        let _ = bibsmith::write_bbl(&ctl, &refs, &HashMap::new(), &HashMap::new());
    }
}

/// The control file is XML written by LaTeX, so it is machine-generated and normally
/// well-formed — but it is also the file a user can hand-edit, and a truncated `.bcf` from
/// an interrupted run must be an error, not a crash.
fn mutated_bcf() -> impl Strategy<Value = String> {
    let source = std::fs::read_to_string("oracle/nyt.bcf").expect("bcf");
    let op = prop_oneof![
        (0usize..4000, Just('<')),
        (0usize..4000, Just('>')),
        (0usize..4000, Just('/')),
        (0usize..4000, Just('"')),
        (0usize..4000, Just('=')),
        (0usize..4000, Just('\u{7f}')),
    ];
    proptest::collection::vec(op, 1..8).prop_map(move |ops| {
        let mut chars: Vec<char> = source.chars().collect();
        for (at, ch) in ops {
            let i = at % chars.len();
            match ch {
                '\u{7f}' => {
                    chars.remove(i);
                }
                _ => chars.insert(i, ch),
            }
        }
        chars.into_iter().collect()
    })
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 400, ..ProptestConfig::default() })]

    #[test]
    fn a_corrupted_control_file_never_panics(src in mutated_bcf()) {
        let _ = bibsmith::bcf::ControlFile::parse(&src);
    }
}
