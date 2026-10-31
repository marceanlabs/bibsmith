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
