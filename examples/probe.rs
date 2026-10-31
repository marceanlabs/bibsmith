use std::collections::HashMap;
fn main() {
    let bytes = std::fs::read("oracle/corpus/acm-sample-base.bib").unwrap();
    let src = String::from_utf8_lossy(&bytes).into_owned();
    let db = bibsmith::parse_lenient(&src).unwrap();
    let raw: HashMap<String, Vec<(String, String)>> = bibsmith::normalize::raw_expanded_entries(&src)
        .unwrap().into_iter().map(|e| (e.key, e.fields)).collect();
    let mut effs = Vec::new();
    for e in db.iter() {
        effs.push(bibsmith::merge::effective(&db, e, &raw));
    }
    let target: Vec<_> = effs.iter().filter(|e| e.key == "1555162").collect();
    let mine = &target[0];
    let my_year: String = mine.values.iter().find(|(k,_)| k=="date").map(|(_,v)| v.chars().take(4).collect())
        .or_else(|| mine.values.iter().find(|(k,_)| k=="year").map(|(_,v)| v.clone())).unwrap_or_default();
    let first = &mine.names[0].1[0];
    println!("year={my_year} first={} {}", first.name, first.given_name);
    for other in &effs {
        if other.key == mine.key { continue }
        let y: String = other.values.iter().find(|(k,_)| k=="date").map(|(_,v)| v.chars().take(4).collect())
            .or_else(|| other.values.iter().find(|(k,_)| k=="year").map(|(_,v)| v.clone())).unwrap_or_default();
        if y != my_year { continue }
        if let Some((_, people)) = other.names.iter().find(|(f,_)| f == "author") {
            if let Some(p) = people.first() {
                if p.name.eq_ignore_ascii_case(&first.name) {
                    println!("CLASH {} -> {}, {}", other.key, p.name, p.given_name);
                }
            }
        }
    }
}
