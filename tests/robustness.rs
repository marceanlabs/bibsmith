//! Files that are not the ones in the corpus.
//!
//! A tool meets its users through their files, and those files are exports from other
//! programs: a BOM, a Windows line ending, a byte that is not UTF-8, a brace left open by
//! a hand edit. None of those may produce a panic — an error message and a non-zero exit
//! is the contract, because a panic tells the user nothing about their file.

use std::path::PathBuf;
use std::process::Command;

fn bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_bibsmith"))
}

fn run(name: &str, bytes: &[u8]) -> (bool, String, String) {
    let dir = std::env::temp_dir().join(format!("bibsmith-robust-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("scratch");
    let path = dir.join(name);
    std::fs::write(&path, bytes).expect("write");
    let out = Command::new(bin())
        .arg("sort")
        .arg(&path)
        .arg("--scheme")
        .arg("nyt")
        .output()
        .expect("run");
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).to_string(),
        String::from_utf8_lossy(&out.stderr).to_string(),
    )
}

fn no_panic(name: &str, bytes: &[u8]) {
    let (ok, out, err) = run(name, bytes);
    assert!(
        !err.contains("panicked"),
        "{name}: the tool panicked instead of reporting: {err}"
    );
    assert!(
        !out.contains("panicked"),
        "{name}: a panic reached stdout: {out}"
    );
    // Either it read the file or it said why not; both are answers.
    assert!(ok || !err.trim().is_empty(), "{name}: failed silently");
}

#[test]
fn a_file_that_is_not_utf8_is_reported() {
    no_panic("latin1.bib", b"@misc{k, author = {M\xfcller, J}, title = {T}}\n");
}

#[test]
fn a_byte_order_mark_is_not_part_of_the_first_field() {
    let mut bytes = vec![0xEF, 0xBB, 0xBF];
    bytes.extend_from_slice(b"@misc{k, author = {A, B}, title = {T}, year = {2000}}\n");
    no_panic("bom.bib", &bytes);
}

#[test]
fn windows_line_endings_parse() {
    no_panic(
        "crlf.bib",
        b"@misc{k,\r\n  author = {A, B},\r\n  title = {T},\r\n  year = {2000},\r\n}\r\n",
    );
}

#[test]
fn an_unbalanced_brace_is_reported() {
    no_panic("open.bib", b"@misc{k, author = {A, B}, title = {T\n");
    no_panic("extra.bib", b"@misc{k, author = {A, B}}}\n");
}

#[test]
fn an_empty_file_is_an_empty_bibliography() {
    no_panic("empty.bib", b"");
    no_panic("space.bib", b"   \n\n\t\n");
}

#[test]
fn a_huge_line_does_not_take_the_process_down() {
    let mut bytes = Vec::from(&b"@misc{k, title = {"[..]);
    bytes.extend(std::iter::repeat_n(b'x', 2_000_000));
    bytes.extend_from_slice(b"}, year = {2000}}\n");
    no_panic("huge.bib", &bytes);
}

#[test]
fn deep_nesting_does_not_take_the_process_down() {
    let mut bytes = Vec::from(&b"@misc{k, title = "[..]);
    bytes.extend(std::iter::repeat_n(b'{', 5_000));
    bytes.extend(std::iter::repeat_n(b'}', 5_000));
    bytes.extend_from_slice(b"}\n");
    no_panic("deep.bib", &bytes);
}

#[test]
fn a_file_of_at_signs_does_not_take_the_process_down() {
    no_panic("ats.bib", &vec![b'@'; 100_000]);
}

#[test]
fn a_crossref_loop_is_read_as_written_not_followed_forever() {
    // Two entries pointing at each other make a recursive resolver run until the stack
    // gives out, which aborts the process. biber reads the file, keeps both crossref
    // fields and exits zero, so that is the contract.
    let (ok, out, err) = run(
        "loop.bib",
        b"@book{a, crossref={b}, title={A}}\n@book{b, crossref={a}, title={B}}\n",
    );
    assert!(ok, "stderr: {err}");
    assert!(out.contains("@BOOK{a,"), "a must survive: {out}");
    assert!(out.contains("CROSSREF = {b}"), "the field must survive: {out}");
    assert!(out.contains("CROSSREF = {a}"), "{out}");
}

#[test]
fn an_entry_that_crossrefs_itself_is_read_as_written() {
    let (ok, out, err) = run("self.bib", b"@book{a, crossref={a}, title={Self}}\n");
    assert!(ok, "stderr: {err}");
    assert!(out.contains("CROSSREF = {a}"), "{out}");
}

#[test]
fn a_legitimate_crossref_is_still_resolved() {
    // The inheritance is the reason the resolver exists: a chapter inherits its book's
    // title and editor.
    let (ok, out, err) = run(
        "chain.bib",
        b"@book{parent, title={The Book}, publisher={P}, year={2000}}\n\
          @inbook{child, crossref={parent}, title={A Chapter}, pages={1--10}}\n",
    );
    assert!(ok, "stderr: {err}");
    assert!(out.contains("PARENT") || out.contains("@INBOOK{child,"), "{out}");
}

#[test]
fn a_crossref_inside_a_damaged_entry_is_still_not_followed() {
    // The guard has to work on a file that is already broken: an entry nested inside
    // another entry's value is exactly where a loop hides from a brace-counting scanner,
    // and a stack overflow is not a parse error.
    let (ok, out, err) = run(
        "nested.bib",
        b"@collection{a,\n title = {A},\n note = {@book{b, title={B}, crossref={a}}}\n}\n@book{c, crossref={a}, title={C}}\n",
    );
    assert!(ok, "stderr: {err}");
    assert!(out.contains("@BOOK{c,"), "later entries must survive: {out}");
}

#[test]
fn a_value_ending_in_a_backslash_does_not_swallow_its_delimiter() {
    // `given = {Knuth\}` escapes the closing brace and the rest of the entry spills onto the
    // page. A stray backslash at the end of a value is made even.
    let (ok, out, err) = run("slash.bib", b"@misc{k, author = {Knuth\\, Donald E.}, title = {T}, year = {2000}}\n");
    assert!(ok, "stderr: {err}");
    assert!(out.contains("KNUTH") || out.contains("Knuth"), "{out}");
}
