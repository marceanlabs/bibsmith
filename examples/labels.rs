use bibsmith::merge::Effective;
use std::collections::HashMap;

fn main() {
    let src = std::fs::read_to_string("oracle/refs.bib").unwrap();
    let db = biblatex::Bibliography::parse(&src).unwrap();
    let raw: HashMap<String, Vec<(String, String)>> =
        bibsmith::normalize::raw_expanded_entries(&src)
            .unwrap()
            .into_iter()
            .map(|e| (e.key, e.fields))
            .collect();
    let expected: HashMap<String, String> =
        std::fs::read_to_string("oracle/labelalpha-alphabetic.tsv")
            .unwrap()
            .lines()
            .filter_map(|l| l.split_once('\t'))
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
    let mut ok = 0;
    let mut bad = 0;
    for (key, want) in expected.iter() {
        let Some(entry) = db.get(key) else {
            println!("MISSING {key}");
            continue;
        };
        let eff: Effective = bibsmith::merge::effective(&db, entry, &raw);
        let member = eff
            .entryset
            .as_ref()
            .and_then(|s| s.split(',').next())
            .map(|k| k.trim().to_string())
            .and_then(|k| {
                db.get(&k)
                    .map(|e| (k.clone(), bibsmith::merge::effective(&db, e, &raw)))
            })
            .map(|(_, e)| e);
        let got = bibsmith::label::label_alpha_with(&eff, 2, member.as_ref());
        if &got == want {
            ok += 1
        } else {
            bad += 1;
            if bad <= 12 {
                println!("{key:22} want={want:8} got={got}");
            }
        }
    }
    println!("{ok} match, {bad} differ");
}
