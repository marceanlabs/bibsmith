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

fn name_part(entry: &Entry) -> String {
    let names = sort_names(entry);
    if names.is_empty() {
        // BibTeX's legacy `key` field is what a manual or a web page uses to say where
        // it should file: `key = {Harvard CodeBlue}` sorts under H. Only when there is
        // no key does the title decide, which is biblatex's stated rule for entries
        // without an author or editor.
        if let Some(key) = plain(entry, "key").or_else(|| plain(entry, "sortkey")) {
            if !key.trim().is_empty() {
                return key;
            }
        }
        return title_part(entry);
    }
    let mut out = String::new();
    for (i, p) in names.iter().enumerate() {
        if i > 0 {
            out.push(SEP);
        }
        out.push_str(&person_key(p));
    }
    out
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
pub type Parts = Vec<String>;

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
            presort_part(entry),
            name_part(entry),
            title_part(entry),
            year_part(entry),
            volume_part(entry),
            sortkey_part(entry),
        ],
        Scheme::Nyt => vec![
            presort_part(entry),
            name_part(entry),
            year_part(entry),
            title_part(entry),
            volume_part(entry),
            sortkey_part(entry),
        ],
        Scheme::Ynt => vec![
            presort_part(entry),
            year_part_or(entry, "9999"),
            name_part(entry),
            title_part(entry),
            sortkey_part(entry),
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
        let ord = collator.compare(pa, pb);
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
