//! Print the sort key bibsmith computes for named entries.
//! `cargo run --example dump_keys -- oracle/refs.bib nyt vizedom:related vazques-de-parga`
use bibsmith::{Database, Scheme};
use std::collections::HashMap;

fn main() {
    let mut args = std::env::args().skip(1);
    let file = args
        .next()
        .expect("usage: dump_keys <bib> <scheme> <key>...");
    let scheme = Scheme::parse(&args.next().expect("scheme")).expect("scheme");
    let wanted: Vec<String> = args.collect();
    let src = std::fs::read_to_string(&file).expect("read");
    let db = Database::parse(&src).expect("parse");
    let entries: Vec<&biblatex::Entry> = db.iter().collect();
    let map: HashMap<&str, &biblatex::Entry> =
        entries.iter().map(|e| (e.key.as_str(), *e)).collect();
    for w in &wanted {
        match db.get(w) {
            Some(e) => {
                let k = bibsmith::sort::template_parts(
                    e,
                    &bibsmith::sort::groups_for(scheme),
                    &HashMap::new(),
                    &map,
                );
                println!("{w}\n   parts: {k:?}");
            }
            None => println!("{w}: not in database"),
        }
    }
}
