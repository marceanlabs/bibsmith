//! CLI contract tests: the argument shapes a user actually types, and the exit
//! codes a script depends on. A mistyped flag must be an error, never silently
//! ignored, and `-o` must be safe even when it names the input.

use std::path::PathBuf;
use std::process::Command;

fn bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_bibsmith"))
}

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("oracle/refs.bib")
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("bibsmith-cli-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir.join(name)
}

#[test]
fn sorts_to_stdout() {
    let out = Command::new(bin())
        .arg("sort")
        .arg(fixture())
        .arg("--scheme")
        .arg("nyt")
        .output()
        .expect("run");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8(out.stdout).expect("utf8");
    // 92 entries plus the 8 @string definitions, all of which must survive.
    let blocks = text.matches('@').count();
    assert_eq!(blocks, 100, "block count, got {blocks}");
    assert!(
        text.contains("@string"),
        "@string definitions must be written back"
    );
    assert!(
        text.contains("@article{aksin,"),
        "first entry should be aksin"
    );
    assert_eq!(text.matches("@article").count(), 20, "article count");
}

#[test]
fn scheme_may_follow_the_file() {
    // Flags after the positional must work; people type both orders.
    let out = Command::new(bin())
        .args(["sort"])
        .arg(fixture())
        .args(["nty"])
        .output()
        .expect("run");
    assert!(
        !out.status.success(),
        "a bare second positional must be an error"
    );

    let out = Command::new(bin())
        .arg("sort")
        .arg(fixture())
        .args(["--scheme", "nty"])
        .output()
        .expect("run");
    assert!(out.status.success());
}

#[test]
fn unknown_scheme_is_refused() {
    let out = Command::new(bin())
        .arg("sort")
        .arg(fixture())
        .args(["--scheme", "nyvt"])
        .output()
        .expect("run");
    assert_eq!(out.status.code(), Some(1));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("unknown sorting scheme"), "stderr: {err}");
    assert!(
        err.contains("nty, nyt and ynt"),
        "the error must list what exists"
    );
}

#[test]
fn missing_file_exits_one() {
    let out = Command::new(bin())
        .args(["sort", "/nonexistent/refs.bib"])
        .output()
        .expect("run");
    assert_eq!(out.status.code(), Some(1));
}

#[test]
fn writes_to_output_file() {
    let dest = scratch("out.bib");
    let out = Command::new(bin())
        .arg("sort")
        .arg(fixture())
        .args(["-o"])
        .arg(&dest)
        .output()
        .expect("run");
    assert!(out.status.success());
    let text = std::fs::read_to_string(&dest).expect("read output");
    assert!(text.contains("@article{aksin,"));
}

#[test]
fn output_may_overwrite_the_input() {
    let copy = scratch("inplace.bib");
    std::fs::copy(fixture(), &copy).expect("copy fixture");
    let out = Command::new(bin())
        .arg("sort")
        .arg(&copy)
        .args(["-o"])
        .arg(&copy)
        .output()
        .expect("run");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = std::fs::read_to_string(&copy).expect("read");
    assert!(
        text.contains("@article{aksin,"),
        "the file must still be a database"
    );
}

#[test]
fn labels_prints_the_suffixes_biblatex_would() {
    let out = Command::new(bin())
        .arg("labels")
        .arg(fixture())
        .output()
        .expect("run");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8(out.stdout).expect("utf8");
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    // biber writes 21 extradates for this corpus; the first is baez/online -> a.
    assert_eq!(lines.len(), 21, "{} lines", lines.len());
    assert_eq!(lines[0], "baez/online\ta", "{}", lines[0]);
    assert!(lines.contains(&"knuth:ct:e\td"), "{lines:?}");
}

#[test]
fn check_reports_and_changes_nothing() {
    let before = std::fs::read(fixture()).expect("read fixture");
    let out = Command::new(bin())
        .arg("check")
        .arg(fixture())
        .output()
        .expect("run");
    assert!(out.status.success());
    let after = std::fs::read(fixture()).expect("read fixture");
    assert_eq!(before, after, "check must not write");

    let report = String::from_utf8_lossy(&out.stdout);
    // 12 of the 92 entries have a legacy field name that --normalise renames.
    assert!(
        report.contains("12 of 92 entries change under --normalise"),
        "{report}"
    );
    assert!(
        report.contains("year -> date"),
        "the rename must be shown: {report}"
    );
}

#[test]
fn sorting_does_not_escape_the_authors_tex() {
    // The library's own writer escapes braces and backslashes to make a safe file, which
    // turns `\emph{y}` into `\\emph\{y\}` and `100%` into `100\%`: biblatex prints those
    // literally, so a database written that way no longer says what it said.
    let path = scratch("escape.bib");
    std::fs::write(
        &path,
        "@misc{k, author = {A, B}, title = {T}, year = {2000}, note = {A \\emph{y} and 100% sure}}\n",
    )
    .unwrap();
    let out = Command::new(bin())
        .arg("sort")
        .arg(&path)
        .arg("--scheme")
        .arg("nyt")
        .output()
        .expect("run");
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("\\emph{y}"), "the command must be written as the author wrote it: {text}");
    assert!(text.contains("100% sure"), "a percent sign is not an escape: {text}");
    assert!(!text.contains("\\{y\\}"));
}
