fn main() {
    let src = r"@misc{k, author = {A, B}, title = {T}, year = {2000}, note = {A \emph{y} and 100% sure}}";
    let raw = biblatex::RawBibliography::parse(src).unwrap();
    for e in &raw.entries {
        for f in &e.v.fields {
            println!("{:?} = {:?}", f.key.v, f.value.v);
        }
    }
    let n = bibsmith::normalize::normalise_all(src).unwrap();
    println!("normalised: {:?}", n[0].fields);
}
