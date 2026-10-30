// Spike 001: can `biblatex` 0.12 parse the canonical 100-entry
// biblatex-examples.bib and round-trip it without losing entries or fields?
//
// Question: does the library layer actually cover real-world .bib input,
// including @string, @preamble, @comment, crossref and LaTeX escapes?
//
// Output is deliberately ugly: counts, then anything that failed to parse.

use std::collections::BTreeMap;

fn main() {
    let path = std::env::args().nth(1).unwrap_or_else(|| "biblatex-examples.bib".into());
    let src = std::fs::read_to_string(&path).expect("read file");

    // Raw count from the text, independent of the parser.
    let raw_at = src.lines().filter(|l| l.trim_start().starts_with('@')).count();
    let mut raw_types: BTreeMap<String, usize> = BTreeMap::new();
    for line in src.lines() {
        let t = line.trim_start();
        if let Some(rest) = t.strip_prefix('@') {
            let name: String = rest
                .chars()
                .take_while(|c| c.is_ascii_alphabetic())
                .collect::<String>()
                .to_ascii_lowercase();
            if !name.is_empty() {
                *raw_types.entry(name).or_default() += 1;
            }
        }
    }

    let bib = match biblatex::Bibliography::parse(&src) {
        Ok(b) => b,
        Err(e) => {
            println!("PARSE FAILED: {e}");
            std::process::exit(1);
        }
    };

    let mut parsed_types: BTreeMap<String, usize> = BTreeMap::new();
    let mut fields_total = 0usize;
    let mut per_entry: Vec<(String, String, usize)> = Vec::new();
    for entry in bib.iter() {
        let ty = format!("{:?}", entry.entry_type).to_ascii_lowercase();
        *parsed_types.entry(ty.clone()).or_default() += 1;
        fields_total += entry.fields.len();
        per_entry.push((entry.key.to_string(), ty, entry.fields.len()));
    }

    println!("file: {path}");
    println!("raw @-blocks:      {raw_at}");
    println!("parsed entries:    {}", per_entry.len());
    println!("total fields:      {fields_total}");
    println!("raw types:         {raw_types:?}");
    println!("parsed types:      {parsed_types:?}");

    let missing = raw_at.saturating_sub(per_entry.len());
    println!("unaccounted:       {missing}");

    // Round-trip: re-serialise each entry and re-parse. A field the writer
    // drops silently is the failure mode that matters.
    let mut rt_failures = 0;
    for (key, _, n) in per_entry.iter().take(usize::MAX) {
        let entry = bib.get(key.as_str());
        if let Some(e) = entry {
            let text = e.to_biblatex_string();
            match biblatex::Bibliography::parse(&text) {
                Ok(re) => match re.get(key.as_str()) {
                    Some(e2) if e2.fields.len() == *n => {}
                    Some(e2) => {
                        rt_failures += 1;
                        if rt_failures <= 10 {
                            println!("  ROUND-TRIP field loss {key}: {n} -> {}", e2.fields.len());
                        }
                    }
                    None => {
                        rt_failures += 1;
                        if rt_failures <= 10 {
                            println!("  ROUND-TRIP key lost: {key}");
                        }
                    }
                },
                Err(err) => {
                    rt_failures += 1;
                    if rt_failures <= 10 {
                        println!("  ROUND-TRIP unparseable {key}: {err}");
                    }
                }
            }
        }
    }
    println!("round-trip failures: {rt_failures}/{}", per_entry.len());

    // Show where the parser disagrees with the raw text on entry count.
    if missing > 0 {
        println!("\nfirst five keys parsed:");
        for (k, ty, n) in per_entry.iter().take(5) {
            println!("  {k} ({ty}, {n} fields)");
        }
    }
}
