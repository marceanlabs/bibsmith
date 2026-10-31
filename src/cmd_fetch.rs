// Filling a database's gaps from the DOIs it already names.

use super::*;

/// `bibsmith fetch refs.bib [--write]` — ask Crossref about every entry that names a DOI
/// and lacks fields its type needs.
///
/// The network is only reached on this command. `--write` replaces the file; without it the
/// additions are printed so they can be read before they land.
pub(crate) fn run(file: &Path, write: bool, contact: Option<String>) -> Result<(), String> {
    let src = std::fs::read_to_string(file).map_err(|e| format!("{}: {e}", file.display()))?;
    let entries =
        bibsmith::normalize::raw_entries(&src).map_err(|e| format!("{}: {e}", file.display()))?;
    let contact = contact.or_else(|| std::env::var("BIBSMITH_CONTACT").ok());
    let mut additions: Vec<(String, Vec<(String, String)>)> = Vec::new();
    let mut failed = 0usize;
    for entry in &entries {
        let Some((_, doi)) = entry.fields.iter().find(|(k, _)| k == "doi") else {
            continue;
        };
        let doi = doi.trim();
        if doi.is_empty() {
            continue;
        }
        let have: std::collections::HashSet<String> = entry
            .fields
            .iter()
            .map(|(k, _)| k.to_ascii_lowercase())
            .collect();
        // One unreachable DOI must not stop the rest: a database with ten of them would
        // come back empty because the seventh was wrong.
        let json = match bibsmith::fetch::lookup(doi, contact.as_deref()) {
            Ok(json) => json,
            Err(e) => {
                eprintln!("bibsmith: {e}");
                failed += 1;
                continue;
            }
        };
        let Some(work) = bibsmith::fetch::parse_crossref(&json) else {
            eprintln!("bibsmith: {doi}: not a Crossref record");
            continue;
        };
        let missing = bibsmith::fetch::missing_fields(&work, &have, &entry.kind);
        if !missing.is_empty() {
            additions.push((entry.key.clone(), missing));
        }
    }
    if additions.is_empty() {
        println!("nothing to add");
        return match failed {
            0 => Ok(()),
            n => Err(format!("{n} DOI(s) could not be read")),
        };
    }
    for (key, fields) in &additions {
        println!("{key}:");
        for (name, value) in fields {
            println!("  {name} = {{{value}}}");
        }
    }
    if !write {
        return Ok(());
    }
    let mut out = src.clone();
    for (key, fields) in &additions {
        out = bibsmith::fetch::add_fields(&out, key, fields);
    }
    std::fs::write(file, out).map_err(|e| format!("{}: {e}", file.display()))?;
    println!("wrote {}", file.display());
    match failed {
        0 => Ok(()),
        n => Err(format!("{n} DOI(s) could not be read; the rest were added")),
    }
}
