//! Filling in what a database leaves out, from the DOI it does give.
//!
//! biber reaches out to Crossref when an entry names a DOI and lacks the fields the entry
//! type needs. This does the same, but only when asked: nothing here runs during `sort`,
//! `check`, `labels` or `bbl`, so a build never touches the network by accident.
//!
//! Nothing about the author is compiled in. Crossref likes a contact address in the
//! `User-Agent`; `BIBSMITH_CONTACT` supplies it if the user wants to, and the request goes
//! out without one otherwise.

use std::collections::HashSet;

/// The parts of a Crossref record this program has any use for.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Work {
    pub title: Option<String>,
    pub authors: Vec<(String, String)>,
    pub year: Option<String>,
    pub container: Option<String>,
    pub volume: Option<String>,
    pub issue: Option<String>,
    pub pages: Option<String>,
    pub publisher: Option<String>,
    pub doi: Option<String>,
}

/// Read the `message` object of a Crossref response.
pub fn parse_crossref(json: &str) -> Option<Work> {
    let value: serde_json::Value = serde_json::from_str(json).ok()?;
    let message = value.get("message")?;
    let text = |key: &str| {
        message
            .get(key)
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    };
    let list = |key: &str| {
        message
            .get(key)
            .and_then(|v| v.as_array())
            .and_then(|a| a.first())
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    };
    let authors = message
        .get("author")
        .and_then(|v| v.as_array())
        .map(|people| {
            people
                .iter()
                .filter_map(|p| {
                    let family = p.get("family").and_then(|v| v.as_str())?.trim();
                    let given = p.get("given").and_then(|v| v.as_str()).unwrap_or("").trim();
                    (!family.is_empty()).then(|| (family.to_string(), given.to_string()))
                })
                .collect()
        })
        .unwrap_or_default();
    // `issued` carries `date-parts: [[1986, 5, 12]]`.
    let year = message
        .get("issued")
        .and_then(|v| v.get("date-parts"))
        .and_then(|v| v.as_array())
        .and_then(|a| a.first())
        .and_then(|v| v.as_array())
        .and_then(|a| a.first())
        .and_then(|v| v.as_i64())
        .map(|y| y.to_string());
    Some(Work {
        title: list("title"),
        authors,
        year,
        container: list("container-title"),
        volume: text("volume"),
        issue: text("issue"),
        pages: text("page"),
        publisher: text("publisher"),
        doi: text("DOI"),
    })
}

/// The fields to add to an entry, given what it already has. Nothing already written is
/// replaced: the author's own values win, which is the rule `--tool` uses as well.
pub fn missing_fields(
    work: &Work,
    have: &HashSet<String>,
    entry_type: &str,
) -> Vec<(String, String)> {
    fn push(
        out: &mut Vec<(String, String)>,
        have: &HashSet<String>,
        name: &str,
        value: Option<&String>,
    ) {
        if have.contains(name) {
            return;
        }
        if let Some(value) = value.filter(|v| !v.trim().is_empty()) {
            out.push((name.to_string(), value.clone()));
        }
    }
    let mut out = Vec::new();
    push(&mut out, have, "title", work.title.as_ref());
    if !work.authors.is_empty() && !have.contains("author") {
        let people: Vec<String> = work
            .authors
            .iter()
            .map(|(family, given)| match given.is_empty() {
                true => family.clone(),
                false => format!("{family}, {given}"),
            })
            .collect();
        out.push(("author".into(), people.join(" and ")));
    }
    push(&mut out, have, "year", work.year.as_ref());
    push(&mut out, have, "journaltitle", work.container.as_ref());
    push(&mut out, have, "volume", work.volume.as_ref());
    push(&mut out, have, "number", work.issue.as_ref());
    push(&mut out, have, "pages", work.pages.as_ref());
    if matches!(
        entry_type,
        "book" | "inbook" | "incollection" | "proceedings"
    ) {
        push(&mut out, have, "publisher", work.publisher.as_ref());
    }
    push(&mut out, have, "doi", work.doi.as_ref());
    out
}

/// Ask Crossref about one DOI.
pub fn lookup(doi: &str, contact: Option<&str>) -> Result<String, String> {
    let url = format!("https://api.crossref.org/works/{}", urlencoded(doi));
    let agent = match contact {
        Some(address) => format!("bibsmith/{} (mailto:{address})", env!("CARGO_PKG_VERSION")),
        None => format!("bibsmith/{}", env!("CARGO_PKG_VERSION")),
    };
    let mut response = ureq::get(&url)
        .header("User-Agent", &agent)
        .call()
        .map_err(|e| format!("{doi}: {e}"))?;
    response
        .body_mut()
        .read_to_string()
        .map_err(|e| format!("{doi}: {e}"))
}

/// Percent-encode everything a DOI may contain that a URL may not: `/` is legal in a path
/// but a DOI's own slashes are part of the path, so only the characters that would end it
/// are escaped.
fn urlencoded(doi: &str) -> String {
    let mut out = String::with_capacity(doi.len() + 8);
    for c in doi.chars() {
        match c {
            ' ' => out.push_str("%20"),
            '<' => out.push_str("%3C"),
            '>' => out.push_str("%3E"),
            '"' => out.push_str("%22"),
            '#' => out.push_str("%23"),
            '?' => out.push_str("%3F"),
            c => out.push(c),
        }
    }
    out
}

/// Insert fields just before the closing brace of one entry, leaving the rest of the file
/// byte for byte as it was: a database is the user's file, not this program's.
pub fn add_fields(src: &str, key: &str, fields: &[(String, String)]) -> String {
    let needle = format!("{{{key},");
    let Some(at) = src.find(&needle) else {
        return src.to_string();
    };
    let mut depth = 0usize;
    let mut end = src.len();
    for (i, c) in src[at..].char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    end = at + i;
                    break;
                }
            }
            _ => {}
        }
    }
    let mut block = String::new();
    if !src[..end].ends_with(",\n") && !src[..end].trim_end().ends_with(',') {
        block.push(',');
        block.push('\n');
    }
    for (name, value) in fields {
        block.push_str(&format!("  {name} = {{{value}}},\n"));
    }
    format!("{}{block}{}", &src[..end], &src[end..])
}
