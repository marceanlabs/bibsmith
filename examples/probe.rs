fn main() {
    let src = "@proceedings{emptyauthor, author = {}, title = {CHI}, year = {2008}}";
    let db = biblatex::Bibliography::parse(src).unwrap();
    let e = db.get("emptyauthor").unwrap();
    let parsed: Result<Vec<biblatex::Person>, _> = e.get_as("author");
    println!("parse ok: {}, count: {}", parsed.is_ok(), parsed.as_ref().map(|v| v.len()).unwrap_or(0));
    if let Ok(v) = &parsed { for p in v { println!("  family={:?} given={:?}", p.name, p.given_name); } }
    println!("repaired: {}", bibsmith::names::names(e, "author").len());
}
