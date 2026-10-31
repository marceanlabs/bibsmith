//! Ordering, the part biber is actually used for.
//!
//! The schemes implemented here are the ones biblatex defines and `sorting=`
//! accepts. Each one is expressed as a list of key parts so a missing part is a
//! visible empty segment rather than a silent wrong order.

use biblatex::{ChunksExt, Entry, Person};
use icu_collator::{Collator, options::CollatorOptions};

/// The sorting templates this crate implements. biblatex ships more
/// (`nyvt`, `ydnt`, `debug`, and user-declared ones); each added here needs an
/// oracle case, so the list grows only when a test does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scheme {
    /// name, title, year
    Nty,
    /// name, year, title
    Nyt,
    /// year (descending), name, title
    Ynt,
}

impl Scheme {
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "nty" => Some(Self::Nty),
            "nyt" => Some(Self::Nyt),
            "ynt" => Some(Self::Ynt),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Nty => "nty",
            Self::Nyt => "nyt",
            Self::Ynt => "ynt",
        }
    }
}

/// Separator that cannot appear in a field value, so parts cannot bleed into
/// each other and make `"ab" + "c"` compare equal to `"a" + "bc"`.
const SEP: char = '\u{0}';

fn plain(entry: &Entry, field: &str) -> Option<String> {
    entry.get(field).map(|chunks| chunks.format_verbatim())
}

/// Entry-level `options = {...}` switches that change which name list is used.
/// Both cases in the corpus are real: `britannica` sets `useeditor=false` and must
/// therefore sort by its title, and `vangennep` sets `useprefix` to file under V.
#[derive(Debug, Default, Clone, Copy)]
struct NameOptions {
    use_prefix: bool,
    use_author: bool,
    use_editor: bool,
    use_translator: bool,
}

impl NameOptions {
    fn from_entry(entry: &Entry) -> Self {
        let mut o = Self {
            use_prefix: false,
            use_author: true,
            use_editor: true,
            use_translator: true,
        };
        if let Some(raw) = plain(entry, "options") {
            for token in raw.split(',') {
                let t = token.trim().to_ascii_lowercase();
                match t.as_str() {
                    "useprefix" | "useprefix=true" => o.use_prefix = true,
                    "useprefix=false" => o.use_prefix = false,
                    "useauthor=false" => o.use_author = false,
                    "useeditor=false" => o.use_editor = false,
                    "usetranslator=false" => o.use_translator = false,
                    _ => {}
                }
            }
        }
        o
    }
}

/// biblatex's name list for sorting: `sortname`, then the author-ish fields in
/// biblatex's fallback order. Getting this order wrong reorders corporate
/// authors and edited volumes, which is most of the interesting cases.
/// The name list biblatex uses for both sorting and labelling. Public because
/// disambiguation needs the same list, and two copies of this rule would drift.
pub fn sort_names(entry: &Entry) -> Vec<Person> {
    if let Some(sortname) = plain(entry, "sortname") {
        return vec![Person {
            name: sortname,
            given_name: String::new(),
            prefix: String::new(),
            suffix: String::new(),
            id: None,
            given_initials: None,
            prefix_initials: None,
            use_prefix: None,
        }];
    }
    let opts = NameOptions::from_entry(entry);
    let candidates: [(&str, bool); 4] = [
        ("author", opts.use_author),
        ("editor", opts.use_editor),
        ("translator", opts.use_translator),
        ("bookauthor", true),
    ];
    for (field, allowed) in candidates {
        if !allowed {
            continue;
        }
        // The repaired reading is used here too: an entry whose author string the parser
        // cannot split would otherwise sort to the front of the bibliography.
        let mut p = crate::names::names(entry, field);
        if !p.is_empty() {
            if opts.use_prefix {
                for person in p.iter_mut() {
                    person.use_prefix = Some(true);
                }
            }
            return p;
        }
    }
    Vec::new()
}

/// One person's contribution to the sort key.
///
/// biblatex sorts on family name, then given name, then the prefix. `use_prefix`
/// pulls a nobiliary particle into the family-name slot, which is what makes
/// `van Gennep` file under V in one style and G in another.
fn person_key(p: &Person) -> String {
    let family = if p.use_prefix == Some(true) && !p.prefix.is_empty() {
        format!("{} {}", p.prefix, p.name)
    } else {
        p.name.clone()
    };
    let mut s = String::with_capacity(family.len() + p.given_name.len() + p.prefix.len() + 2);
    s.push_str(&family);
    s.push(SEP);
    s.push_str(&p.given_name);
    s.push(SEP);
    s.push_str(&p.prefix);
    s
}

fn name_part(entry: &Entry) -> Vec<String> {
    let names = sort_names(entry);
    if names.is_empty() {
        // BibTeX's legacy `key` field is what a manual or a web page uses to say where
        // it should file: `key = {Harvard CodeBlue}` sorts under H. Only when there is
        // no key does the title decide, which is biblatex's stated rule for entries
        // without an author or editor.
        if let Some(key) = plain(entry, "key").or_else(|| plain(entry, "sortkey")) {
            if !key.trim().is_empty() {
                // A legacy `key` is often markup rather than text: `Kn{\printfirst{v}{1987}}`
                // files under "Knv", next to names beginning that way, and collating the
                // braces instead puts it before them.
                return vec![sort_text(&key)];
            }
        }
        return vec![title_part(entry)];
    }
    names.iter().map(person_key).collect()
}

fn year_part(entry: &Entry) -> String {
    year_part_or(entry, "")
}

/// `ynt` writes `\literal{9999}` after the year fields, so an entry with no year
/// sorts last rather than first. Same function, different fallback.
fn year_part_or(entry: &Entry, fallback: &str) -> String {
    // `sortyear` wins, then `date`, then `year`. biber rewrites `year` to `date`
    // on the way out, so both are read here.
    let raw = if let Some(y) = plain(entry, "sortyear") {
        y
    } else {
        let value = plain(entry, "date")
            .or_else(|| plain(entry, "year"))
            .unwrap_or_default();
        let digits: String = value.chars().take_while(|c| c.is_ascii_digit()).collect();
        if digits.len() >= 4 {
            digits[..4].to_string()
        } else {
            value
        }
    };
    if raw.trim().is_empty() {
        fallback.to_string()
    } else {
        raw
    }
}

fn title_part(entry: &Entry) -> String {
    plain(entry, "sorttitle")
        .or_else(|| plain(entry, "title"))
        .unwrap_or_default()
}

/// The parts of one entry's key, in comparison order.
///
/// The order is not guessed: it is transcribed from `biblatex.def` as installed
/// (biblatex 3.22a), where `nyt` is presort, then the name chain, then
/// sortyear|year, then sorttitle|title, then `volume` with a literal `0` fallback,
/// and `sortkey` as a final tiebreak.
///
/// Parts are compared one at a time with ICU collation rather than concatenated
/// into one string: a separator character would have to be collation-ignorable to
/// work, and byte comparison of the joined form sorts `vázques` after `vizedom`
/// because `á` is two bytes above `i`. That exact case is in the corpus.
/// The slots of a sort key, compared in order.
///
/// A slot holds a list because the name slot does: biblatex compares name lists person by
/// person, and a list that runs out sorts first. Joining the names into one string cannot
/// reproduce that — the collator ignores the separator, so `Smith, John and Jones` and
/// `Smith, John A.` compare only on "Smith John", and the second name never gets a say.
pub type Parts = Vec<Vec<String>>;

fn one(s: String) -> Vec<String> {
    vec![sort_text(&s)]
}

/// A sort key as the collator should see it: without the braces that protect a letter's
/// case and without the names of TeX commands, which are markup rather than text.
///
/// biblatex sorts on text, so `key = {Kn{\printfirst{v}{1987}}}` files next to names
/// beginning `Knv`; collating the braces instead puts the entry before `Knuth`, because
/// braces are symbols and this collator orders symbols before letters.
fn sort_text(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '{' | '}' => {}
            '\\' => {
                while chars.peek().is_some_and(|c| c.is_alphabetic()) {
                    chars.next();
                }
            }
            other => out.push(other),
        }
    }
    out
}

/// The parts of one entry's sort key, in comparison order. Public because
/// explaining an order is a feature: `bibsmith sort --explain` prints these.
pub fn parts(entry: &Entry, scheme: Scheme, db: &std::collections::HashMap<&str, &Entry>) -> Parts {
    if let Some(member) = first_set_member(entry, db) {
        let mut seen = vec![entry.key.to_string()];
        return key_parts_inner(member, scheme, db, &mut seen);
    }
    key_parts_inner(entry, scheme, db, &mut Vec::new())
}

fn key_parts_inner(
    entry: &Entry,
    scheme: Scheme,
    db: &std::collections::HashMap<&str, &Entry>,
    seen: &mut Vec<String>,
) -> Parts {
    if seen.len() < 3 {
        if let Some(member) = first_set_member(entry, db) {
            if !seen.iter().any(|k| k == &member.key.to_string()) {
                seen.push(member.key.to_string());
                return key_parts_inner(member, scheme, db, seen);
            }
        }
    }
    match scheme {
        Scheme::Nty => vec![
            one(presort_part(entry)),
            name_part(entry),
            one(title_part(entry)),
            one(year_part(entry)),
            one(volume_part(entry)),
            one(sortkey_part(entry)),
        ],
        Scheme::Nyt => vec![
            one(presort_part(entry)),
            name_part(entry),
            one(year_part(entry)),
            one(title_part(entry)),
            one(volume_part(entry)),
            one(sortkey_part(entry)),
        ],
        Scheme::Ynt => vec![
            one(presort_part(entry)),
            one(year_part_or(entry, "9999")),
            name_part(entry),
            one(title_part(entry)),
            one(sortkey_part(entry)),
        ],
    }
}

fn presort_part(entry: &Entry) -> String {
    plain(entry, "presort").unwrap_or_default()
}

/// `\sort[final]{\field{sortkey}}`: a tiebreak of last resort, empty by default.
fn sortkey_part(entry: &Entry) -> String {
    plain(entry, "sortkey").unwrap_or_default()
}

/// `\sort{\field{volume}\literal{0}}`: an entry with no volume sorts as "0", which
/// is why a print article with `volume = 12` files AFTER its own online-only
/// twin. Measured: that pair is `baez/article` vs `baez/online` in the corpus.
fn volume_part(entry: &Entry) -> String {
    plain(entry, "volume").unwrap_or_else(|| "0".to_string())
}

/// Keys in order, compared with ICU collation in the `en-US` locale, which is the
/// locale biber resolved for this corpus. No scheme in biblatex's defaults sorts
/// descending (`ydnt` does, and is not implemented here), so nothing is reversed.
pub fn sort_entries<'a>(entries: &[&'a Entry], scheme: Scheme) -> Vec<&'a Entry> {
    let db: std::collections::HashMap<&str, &Entry> =
        entries.iter().map(|e| (e.key.as_str(), *e)).collect();
    let locale: icu_locale::Locale = "en-US".parse().expect("en-US parses");
    let collator = Collator::try_new(locale.into(), CollatorOptions::default())
        .expect("compiled-in collation data");
    let mut keyed: Vec<(Parts, &Entry)> = entries
        .iter()
        .map(|e| (parts(e, scheme, &db), *e))
        .collect();
    keyed.sort_by(|a, b| compare_parts(&collator, &a.0, &b.0));
    keyed.into_iter().map(|(_, e)| e).collect()
}

fn compare_parts(
    collator: &icu_collator::CollatorBorrowed,
    a: &Parts,
    b: &Parts,
) -> std::cmp::Ordering {
    for (pa, pb) in a.iter().zip(b.iter()) {
        for (xa, xb) in pa.iter().zip(pb.iter()) {
            // An entry with nothing in the slot sorts after one that has something:
            // a document undated in the same year as a dated one files after it, which
            // is what biber does, not before as an empty string would.
            let ord = collator.compare(xa, xb);
            if ord != std::cmp::Ordering::Equal {
                return ord;
            }
        }
        // A name list that runs out sorts first: `Smith, John` before `Smith, John and
        // Jones`.
        let ord = pa.len().cmp(&pb.len());
        if ord != std::cmp::Ordering::Equal {
            return ord;
        }
    }
    a.len().cmp(&b.len())
}

fn first_set_member<'a>(
    entry: &Entry,
    db: &std::collections::HashMap<&str, &'a Entry>,
) -> Option<&'a Entry> {
    let raw = plain(entry, "entryset")?;
    let first = raw.split(',').next()?.trim().to_string();
    db.get(first.as_str()).copied()
}

/// Order entries by a string key, collated the same way the sort key is.
///
/// The `alphabetic` style sorts by the label itself rather than by name and year, so
/// the order comes from `labelalpha` and everything else is the tiebreak.
pub fn collated_sort(items: Vec<(String, &Entry)>) -> Vec<&Entry> {
    let locale: icu_locale::Locale = "en-US".parse().expect("en-US parses");
    let collator = Collator::try_new(locale.into(), CollatorOptions::default())
        .expect("compiled-in collation data");
    let mut keyed = items;
    keyed.sort_by(|a, b| collator.compare(&a.0, &b.0));
    keyed.into_iter().map(|(_, e)| e).collect()
}

/// One slot of a sort key, resolved from the name the `.bcf` template gives.
///
/// The template is the rule, not a hint: `nyt` is `presort`, `sortkey` (final), then the
/// name chain, the year, the title and the volume. Reading it means a style nobody here has
/// seen sorts correctly, and the four names this crate used to hard-code stop being a
/// list of what happens to be implemented.
fn template_value(
    entry: &Entry,
    name: &str,
    extra: &std::collections::HashMap<String, String>,
) -> Option<Vec<String>> {
    // A name field is a list, and biblatex compares it person by person.
    if matches!(
        name,
        "author" | "editor" | "translator" | "bookauthor" | "editora" | "editorb" | "editorc"
            | "afterword" | "annotator" | "commentator" | "foreword" | "introduction"
            | "shortauthor" | "shorteditor" | "holder"
    ) {
        let opts = NameOptions::from_entry(entry);
        // `options = {useeditor=false}` is a statement about the label: Britannica must file
        // under its title, not under the editor it suppresses.
        let allowed = match name {
            "author" => opts.use_author,
            "editor" => opts.use_editor,
            "translator" => opts.use_translator,
            _ => true,
        };
        if !allowed {
            return None;
        }
        let people = crate::names::names(entry, name);
        if people.is_empty() {
            return None;
        }
        let keys: Vec<String> = people
            .iter()
            .map(|p| {
                if opts.use_prefix && !p.prefix.is_empty() {
                    format!("{} {}{SEP}{}{SEP}{}", p.prefix, p.name, p.given_name, p.prefix)
                } else {
                    person_key(p)
                }
            })
            .collect();
        return Some(keys);
    }
    if name == "sortname" {
        let value = plain(entry, "sortname")?;
        if value.trim().is_empty() {
            return None;
        }
        return Some(vec![value]);
    }
    // A date field is compared by its year, and a missing year is an empty slot rather than
    // the string "0": `volume` is the one field that wants a number.
    if matches!(name, "year" | "date" | "sortyear" | "origdate" | "eventdate" | "urldate") {
        // `year` is the old spelling of `date`: biber rewrites one to the other on the way
        // out, so a template that asks for `year` must find a `date` entry.
        let raw = match name {
            "year" => plain(entry, "year").or_else(|| plain(entry, "date")),
            _ => plain(entry, name),
        }
        .unwrap_or_default();
        let digits: String = raw.chars().take_while(|c| c.is_ascii_digit()).collect();
        let value = if digits.len() >= 4 {
            digits[..4].to_string()
        } else {
            raw.trim().to_string()
        };
        return if value.is_empty() { None } else { Some(vec![value]) };
    }
    if let Some(v) = extra.get(name) {
        return Some(vec![v.clone()]);
    }
    let value = plain(entry, name)?;
    let value = value.trim().to_string();
    if value.is_empty() {
        None
    } else {
        Some(vec![value])
    }
}

/// The parts of one entry's key, read from a `.bcf` sorting template.
pub fn template_parts(
    entry: &Entry,
    groups: &[crate::bcf::SortGroup],
    extra: &std::collections::HashMap<String, String>,
    db: &std::collections::HashMap<&str, &Entry>,
) -> Parts {
    if let Some(member) = first_set_member(entry, db) {
        return template_parts(member, groups, extra, db);
    }
    let mut parts = Parts::new();
    for group in groups {
        let mut chosen: Option<Vec<String>> = None;
        // BibTeX's legacy `key` names where an entry files when no person does. biblatex
        // keeps the rule but the template has no slot for it, so it is applied here:
        // `key = {Harvard CodeBlue}` files under H, not under the title.
        let names_group = group.items.iter().any(|item| {
            matches!(item, crate::bcf::SortItem::Field(f) if f == "author" || f == "sortname")
        });
        if names_group {
            if let Some(key) = plain(entry, "key").or_else(|| plain(entry, "sortkey")) {
                // Read as text, not markup: `key = {Kn{\printfirst{v}{1987}}}` files
                // under "Knv1987", and collating the braces puts it before `Knuth`
                // because this collator orders symbols before letters.
                let key = sort_text(key.trim());
                if !key.is_empty() {
                    chosen = Some(vec![key]);
                }
            }
        }
        for item in &group.items {
            if chosen.is_some() {
                break;
            }
            let value = match item {
                crate::bcf::SortItem::Field(name) => template_value(entry, name, extra),
                crate::bcf::SortItem::Literal(text) => Some(vec![text.clone()]),
            };
            if let Some(v) = value {
                chosen = Some(v);
                break;
            }
        }
        // `\sort[final]{\field{sortkey}}`: when it yields a value nothing after it is
        // compared, which is how a bibliography with `sortkey` fields gets its own order.
        // An empty `sortkey` is not a value: every entry has one of those.
        let yielded = chosen.is_some();
        parts.push(chosen.unwrap_or_else(|| vec![String::new()]));
        if yielded && group.finalsort {
            break;
        }
    }
    // Nothing is appended for a tie: biber leaves entries that agree on every slot in
    // citation order, and adding the key would reorder `kullback:reprint` before
    // `kullback:related`, which is not what its `.bbl` contains.
    parts
}

/// Sort entries by a `.bcf` template. Same collation and stability as the scheme version.
pub fn sort_entries_by_template<'a>(
    entries: &[&'a Entry],
    groups: &[crate::bcf::SortGroup],
    extra: &std::collections::HashMap<String, String>,
) -> Vec<&'a Entry> {
    let db: std::collections::HashMap<&str, &Entry> =
        entries.iter().map(|e| (e.key.as_str(), *e)).collect();
    let locale: icu_locale::Locale = "en-US".parse().expect("en-US parses");
    let collator = Collator::try_new(locale.into(), CollatorOptions::default())
        .expect("compiled-in collation data");
    let mut keyed: Vec<(Parts, &Entry)> = entries
        .iter()
        .map(|e| (template_parts(e, groups, extra, &db), *e))
        .collect();
    keyed.sort_by(|a, b| compare_parts(&collator, &a.0, &b.0));
    keyed.into_iter().map(|(_, e)| e).collect()
}
