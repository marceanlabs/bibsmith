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
});
