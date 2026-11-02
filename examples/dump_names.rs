//! Dump the parsed name lists, one person per line, for measuring hashes.
//!
//!     cargo run --quiet --example dump_names -- oracle/refs.bib

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .ok_or("usage: dump_names <file.bib>")?;
    let src = std::fs::read_to_string(&path)?;
    let db = bibsmith::Database::parse(&src)?;
    for entry in db.iter() {
        for field in bibsmith::merge::NAME_FIELDS {
            let people = bibsmith::names::names_with(entry, field, None);
            if people.is_empty() {
                continue;
            }
            for (i, p) in people.iter().enumerate() {
                // TSV: key, field, position, family, given, prefix, suffix
                println!(
                    "{}\t{}\t{}\t{}\t{}\t{}\t{}",
                    entry.key, field, i, p.name, p.given_name, p.prefix, p.suffix
                );
            }
        }
    }
    Ok(())
}
