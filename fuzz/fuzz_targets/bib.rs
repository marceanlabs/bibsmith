#![no_main]

//! The whole `.bib` pipeline on arbitrary bytes.
//!
//! libFuzzer watches coverage, so unlike generated text it follows the code where the code
//! actually goes: a brace that opens a branch nobody wrote a test for, a date string that
//! reaches a parse path the corpus never did. The requirement is only that no input panics
//! and no output is produced twice differently.

use libfuzzer_sys::fuzz_target;
use std::collections::HashMap;

fuzz_target!(|data: &[u8]| {
    let Ok(src) = std::str::from_utf8(data) else { return };

    // Every reader in the library, in the order the CLI calls them.
    let _ = bibsmith::normalize::raw_entries(src);
    let _ = bibsmith::normalize::normalise_all(src);
    let _ = bibsmith::normalize::raw_expanded_entries(src);
    let _ = bibsmith::normalize::macro_table(src);
    let _ = bibsmith::normalize::decode_accents(src);

    let Ok(db) = bibsmith::parse_lenient(src) else { return };
    let Ok(raw) = bibsmith::normalize::raw_expanded_entries(src) else { return };
    let raw: HashMap<String, Vec<(String, String)>> =
        raw.into_iter().map(|e| (e.key, e.fields)).collect();
    let effective: Vec<bibsmith::merge::Effective> = db
        .iter()
        .map(|entry| bibsmith::merge::effective(&db, entry, &raw))
        .collect();
    let refs: Vec<&bibsmith::merge::Effective> = effective.iter().collect();

    let ctl = bibsmith::bcf::ControlFile::default();
    let out = bibsmith::write_bbl(&ctl, &refs, &HashMap::new(), &HashMap::new());

    // The same input must give the same file: a HashMap iteration order that leaks into
    // the output would make every build a different file for the same bibliography.
    let again = bibsmith::write_bbl(&ctl, &refs, &HashMap::new(), &HashMap::new());
    assert_eq!(out, again, "rendering is not deterministic");

    // A file that half-writes is worse than one that fails: LaTeX reads the `.bbl` and
    // reports an error a hundred lines away. Every entry that opens must close, the file
    // must end, and nothing may carry a byte a `.bbl` cannot hold.
    assert!(out.ends_with("\\endinput\n"), "the file must end: {:?}", &out[out.len().saturating_sub(40)..]);
    assert_eq!(
        out.matches("\\entry{").count(),
        out.matches("\\endentry").count(),
        "an entry was opened and not closed"
    );
    assert!(!out.contains('\0'), "a NUL byte reached the output");
    // A brace that is escaped is not a group: `\{IEEE\}` is a literal brace, and counting
    // it would make every value that had to be escaped look unbalanced.
    let (mut opens, mut closes) = (0i64, 0i64);
    let mut it = out.chars().peekable();
    while let Some(c) = it.next() {
        if c == '\\' {
            // `\\`, `\{` and `\}`: the next character belongs to the command.
            it.next();
            continue;
        }
        match c {
            '{' => opens += 1,
            '}' => closes += 1,
            _ => {}
        }
    }
    assert_eq!(opens, closes, "the braces of the output do not balance");
});
