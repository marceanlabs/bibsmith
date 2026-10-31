use biblatex::{Bibliography, ChunksExt};

fn main() {
    let src = r#"@book{a, title = {{\TeX}: The Program}, note = {The {\TeX}book}}"#;
    let bib = Bibliography::parse(src).unwrap();
    let e = bib.get("a").unwrap();
    for (k, v) in e.fields.iter() {
        println!(
            "{k}: {:?}",
            v.iter().map(|c| format!("{:?}", c.v)).collect::<Vec<_>>()
        );
        println!("  format_verbatim = {:?}", v.format_verbatim());
        println!(
            "  render_value    = {:?}",
            bibsmith::normalize::render_value(v)
        );
    }
}
