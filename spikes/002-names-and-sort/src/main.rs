// Spike 002: does the library's name parsing hold up on real entries, and is
// there enough there to build a biber-style sort on top?
//
// Failure mode being hunted: a name that parses to an empty family name, or a
// particle ("de", "van") landing in the wrong slot. Biber's sort keys depend on
// getting those right, and a wrong key silently reorders a bibliography.

use biblatex::{Bibliography, ChunksExt, Entry, Person};

fn names(entry: &Entry, field: &str) -> Vec<Person> {
    entry.get_as::<Vec<Person>>(field).unwrap_or_default()
}

fn sort_name(people: &[Person]) -> String {
    match people.first() {
        Some(p) => p.name.to_lowercase(),
        None => String::new(),
    }
}

fn main() {
    let path = std::env::args().nth(1).unwrap_or_else(|| "biblatex-examples.bib".into());
    let src = std::fs::read_to_string(&path).expect("read file");
    let bib = Bibliography::parse(&src).expect("parse");

    let mut with_authors = 0usize;
    let mut with_editors = 0usize;
    let mut empty_family: Vec<(String, String)> = Vec::new();
    let mut particles: Vec<(String, String, String)> = Vec::new();
    let mut total_people = 0usize;
    let mut rows: Vec<(String, String, String, String)> = Vec::new(); // sortkey, key, type, title

    for entry in bib.iter() {
        let authors = names(entry, "author");
        let editors = names(entry, "editor");
        if !authors.is_empty() {
            with_authors += 1;
        }
        if !editors.is_empty() {
            with_editors += 1;
        }
        total_people += authors.len() + editors.len();

        for p in authors.iter().chain(editors.iter()) {
            if p.name.trim().is_empty() {
                empty_family.push((entry.key.to_string(), p.given_name.clone()));
            }
            if !p.prefix.trim().is_empty() {
                particles.push((
                    entry.key.to_string(),
                    format!("{}|{}", p.prefix, p.name),
                    p.given_name.clone(),
                ));
            }
        }

        // A deliberately simple scheme: first author family name, then year,
        // then title. Biber's real scheme is richer; this only proves the
        // ingredients are reachable.
        let primary = if !authors.is_empty() { &authors } else { &editors };
        let year = entry
            .get("date")
            .map(|c| c.format_verbatim())
            .or_else(|| entry.get("year").map(|c| c.format_verbatim()))
            .unwrap_or_default();
        let title = entry
            .get("title")
            .map(|c| c.format_verbatim())
            .unwrap_or_default();
        let key = format!(
            "{}|{}|{}",
            sort_name(primary),
            year.chars().take(4).collect::<String>(),
            title.to_lowercase()
        );
        rows.push((key, entry.key.to_string(), format!("{:?}", entry.entry_type).to_lowercase(), title));
    }

    println!("entries:            {}", bib.len());
    println!("with author:        {with_authors}");
    println!("with editor:        {with_editors}");
    println!("total people:       {total_people}");
    println!("empty family name:  {}", empty_family.len());
    for (k, g) in empty_family.iter().take(10) {
        println!("   {k}: given={g:?}");
    }
    println!("names with prefix:  {}", particles.len());
    for (k, p, g) in particles.iter().take(10) {
        println!("   {k}: {p} (given {g})");
    }

    rows.sort();
    println!("\nfirst 12 in sort order (scheme: family | year | title):");
    for (k, key, ty, _t) in rows.iter().take(12) {
        println!("   {k:40} {key:22} {ty}");
    }
    println!("\nlast 3:");
    for (k, key, _ty, _t) in rows.iter().rev().take(3) {
        println!("   {k:40} {key}");
    }
}
