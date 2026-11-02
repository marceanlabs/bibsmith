//! Reading the `.bcf` file biblatex writes.
//!
//! biblatex writes a control file that tells the backend exactly what to do: which
//! keys were cited, which sorting template to use, and the template's sort keys in
//! order. That means the order is not guessed from a scheme name — the document
//! hands us the recipe, and this module reads it.
//!
//! Only the parts a bibliography builder needs are parsed. The option tables, label
//! templates and data model are read far enough to know what they say, and the rest
//! is left in the file for a later slice.

use quick_xml::Reader;
use quick_xml::events::Event;

/// One `<bcf:sort>` group: alternatives in order, the first non-empty wins.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SortGroup {
    pub finalsort: bool,
    /// `sort_direction="descending"` — the group is compared backwards. A year sorted
    /// newest first carries a `9999` literal so undated entries stay at the end.
    pub descending: bool,
    pub items: Vec<SortItem>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SortItem {
    /// A field name such as `author` or `sortyear`.
    Field(String),
    /// A literal, used as a fallback such as `0` for a missing volume.
    Literal(String),
}

#[derive(Debug, Clone, Default)]
pub struct ControlFile {
    /// `<bcf:datasource>` values, in document order.
    pub datasources: Vec<String>,
    /// Citekeys in citation order.
    pub citekeys: Vec<String>,
    /// The datalist name, e.g. `nyt/global//global/global/global`.
    pub datalist_name: String,
    /// Citekeys per numbered section, for a document with several `\refsection`s.
    ///
    /// The order is the order the sections appear, and section `0` is the global one that
    /// wraps each bibliography; it is kept as written.
    pub sections: Vec<(String, Vec<String>)>,
    /// Effective biblatex options, as the style left them.
    ///
    /// `uniquelist` lives here: the numeric style turns it off, and biber then writes no
    /// `ul` at all, so the value has to be read rather than assumed.
    pub options: std::collections::HashMap<String, String>,
    /// The sorting template name, e.g. `nyt`.
    pub sorting_template: String,
    /// The template's sort groups, in order.
    pub sorts: Vec<SortGroup>,
    /// Every sorting template in the file, by name.
    ///
    /// The datalist element comes after the templates, so the template cannot be
    /// chosen while reading them; they are all collected and the named one is picked
    /// at the end.
    pub templates: std::collections::HashMap<String, Vec<SortGroup>>,
    /// The `<bcf:presort>` setting, `mm` or `mp` etc.
    pub presort: String,
    /// The data model: field name to datatype (`literal`, `key`, `name`, `verbatim`,
    /// `uri`, `datepart`, `integer`, ...). The `.bbl` writer needs this because the
    /// three are written three different ways, and a field written the wrong way is
    /// silently not printed.
    pub fieldtypes: std::collections::HashMap<String, String>,
    /// The data model's *fieldtype* per field: `field` or `list`.
    ///
    /// This is the one that decides how the `.bbl` writes a value, and it is not the
    /// same as the datatype: `location` is `fieldtype="list" datatype="literal"`.
    pub fieldkinds: std::collections::HashMap<String, String>,
    /// `<bcf:namehashtemplate>` blocks, by name: the name parts hashed and in what form.
    ///
    /// biber hashes a name list by concatenating the parts this template names, so the
    /// value of `\strng{namehash}` depends on it. The style ships one called `global`,
    /// and a document can pick another per entry or per name list.
    pub hash_templates: std::collections::HashMap<String, Vec<HashPart>>,
}

/// One `<bcf:namepart>` inside a name-hash template.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HashPart {
    /// `family`, `given`, `prefix` or `suffix`.
    pub part: String,
    /// `full` hashes the whole part, `init` only its initials.
    pub scope: String,
}

impl ControlFile {
    /// One effective option, e.g. `uniquelist`.
    pub fn option(&self, name: &str) -> Option<&str> {
        self.options.get(name).map(|s| s.as_str())
    }

    /// The name-hash template called `name`, or `global`, or biber's own default.
    ///
    /// A document can rename the template; when the file carries none at all the default
    /// is what biblatex ships: every part, whole.
    pub fn hash_template(&self, name: &str) -> Vec<HashPart> {
        if let Some(t) = self.hash_templates.get(name) {
            if !t.is_empty() {
                return t.clone();
            }
        }
        if let Some(t) = self.hash_templates.get("global") {
            if !t.is_empty() {
                return t.clone();
            }
        }
        ["family", "given", "prefix", "suffix"]
            .iter()
            .map(|p| HashPart {
                part: (*p).to_string(),
                scope: "full".to_string(),
            })
            .collect()
    }

    pub fn parse(xml: &str) -> Result<Self, String> {
        let mut ctl_options = options_from_xml(xml);
        let mut reader = Reader::from_str(xml);
        reader.config_mut().trim_text(true);

        let mut ctl = ControlFile {
            options: std::mem::take(&mut ctl_options),
            ..ControlFile::default()
        };
        let mut stack: Vec<String> = Vec::new();
        // The number of each open <bcf:section>, innermost last.
        let mut section_stack: Vec<String> = Vec::new();
        // Where we are inside a sorting template.
        let mut in_template = false;
        let mut template_name = String::new();
        let mut in_sort = false;
        let mut in_fields = false;
        let mut last_field_dtype: Option<String> = None;
        let mut last_field_kind: Option<String> = None;
        let mut current_sort: Option<SortGroup> = None;
        // Where we are inside a <bcf:namehashtemplate>.
        let mut hash_name: Option<String> = None;
        let mut hash_part: Option<HashPart> = None;

        loop {
            match reader.read_event() {
                Err(e) => return Err(format!("bcf: {e}")),
                Ok(Event::Eof) => break,
                Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                    let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                    let local = name.rsplit(':').next().unwrap_or(&name).to_string();
                    let attrs: Vec<(String, String)> = e
                        .attributes()
                        .flatten()
                        .map(|a| {
                            (
                                String::from_utf8_lossy(a.key.as_ref()).to_string(),
                                String::from_utf8_lossy(&a.value).to_string(),
                            )
                        })
                        .collect();
                    let attr = |k: &str| {
                        attrs
                            .iter()
                            .find(|(name, _)| name == k)
                            .map(|(_, v)| v.clone())
                    };
                    match local.as_str() {
                        "section" => {
                            if let Some(n) = attr("number") {
                                section_stack.push(n);
                            }
                        }
                        "fields" => in_fields = true,
                        "field" if in_fields => {
                            last_field_dtype = attr("datatype");
                            last_field_kind = attr("fieldtype");
                        }
                        "sortingtemplate" => {
                            in_template = true;
                            template_name = attr("name").unwrap_or_default();
                        }
                        "namehashtemplate" => {
                            let name = attr("name").unwrap_or_default();
                            ctl.hash_templates.entry(name.clone()).or_default();
                            hash_name = Some(name);
                        }
                        "namepart" if hash_name.is_some() => {
                            hash_part = Some(HashPart {
                                part: String::new(),
                                scope: attr("hashscope").unwrap_or_else(|| "full".to_string()),
                            });
                        }
                        "sort" if in_template => {
                            in_sort = true;
                            current_sort = Some(SortGroup {
                                finalsort: attr("final").is_some_and(|v| v == "1"),
                                descending: attr("sort_direction")
                                    .is_some_and(|v| v == "descending"),
                                items: Vec::new(),
                            });
                        }
                        "datalist" => {
                            if let Some(n) = attr("name") {
                                ctl.datalist_name = n;
                            }
                            if let Some(t) = attr("sortingtemplatename") {
                                ctl.sorting_template = t;
                            }
                        }
                        "sortitem" if in_sort => {
                            let is_literal = attr("literal").is_some_and(|v| v == "1");
                            if let Some(group) = current_sort.as_mut() {
                                // The value arrives as text, handled below.
                                group.items.push(if is_literal {
                                    SortItem::Literal(String::new())
                                } else {
                                    SortItem::Field(String::new())
                                });
                            }
                        }
                        _ => {}
                    }
                    stack.push(local);
                }
                Ok(Event::End(e)) => {
                    let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                    let local = name.rsplit(':').next().unwrap_or(&name).to_string();
                    match local.as_str() {
                        "section" => {
                            section_stack.pop();
                        }
                        "fields" => in_fields = false,
                        "sortingtemplate" => {
                            in_template = false;
                            if template_name == ctl.sorting_template
                                || ctl.sorting_template.is_empty()
                            {
                                ctl.sorting_template = template_name.clone();
                            }
                        }
                        "namepart" if hash_name.is_some() => {
                            if let (Some(name), Some(part)) = (hash_name.as_ref(), hash_part.take())
                            {
                                if !part.part.is_empty() {
                                    ctl.hash_templates
                                        .entry(name.clone())
                                        .or_default()
                                        .push(part);
                                }
                            }
                        }
                        "namehashtemplate" => hash_name = None,
                        "sort" if in_sort => {
                            in_sort = false;
                            if let Some(group) = current_sort.take() {
                                ctl.templates
                                    .entry(template_name.clone())
                                    .or_default()
                                    .push(group);
                            }
                        }
                        _ => {}
                    }
                    stack.pop();
                }
                Ok(Event::Text(t)) => {
                    let text = t.decode().unwrap_or_default().trim().to_string();
                    let current = stack.last().map(|s| s.as_str()).unwrap_or("");
                    match current {
                        "field" if in_fields => {
                            if let Some(dtype) = last_field_dtype.take() {
                                ctl.fieldtypes.insert(text.clone(), dtype);
                            }
                            if let Some(kind) = last_field_kind.take() {
                                ctl.fieldkinds.insert(text, kind);
                            }
                        }
                        // The same file is listed once per section, and a document with
                        // two \refsection blocks would otherwise have its database read
                        // twice and every key in it look like a duplicate.
                        "datasource" => {
                            if !ctl.datasources.contains(&text) {
                                ctl.datasources.push(text);
                            }
                        }
                        "citekey" => {
                            let section = section_stack
                                .last()
                                .cloned()
                                .unwrap_or_else(|| "0".to_string());
                            ctl.citekeys.push(text.clone());
                            match ctl.sections.iter_mut().find(|(n, _)| *n == section) {
                                Some((_, keys)) => keys.push(text),
                                None => ctl.sections.push((section, vec![text])),
                            }
                        }
                        "presort" => ctl.presort = text,
                        "namepart" if hash_part.is_some() => {
                            if let Some(part) = hash_part.as_mut() {
                                part.part = text;
                            }
                        }
                        "sortitem" if in_sort => {
                            if let Some(group) = current_sort.as_mut() {
                                match group.items.last_mut() {
                                    Some(SortItem::Field(f)) if f.is_empty() => *f = text,
                                    Some(SortItem::Literal(l)) if l.is_empty() => *l = text,
                                    _ => {}
                                }
                            }
                        }
                        _ => {}
                    }
                }
                Ok(_) => {}
            }
        }

        if ctl.citekeys.is_empty() {
            return Err("bcf: no citekeys; is this the right file?".to_string());
        }
        // A key cited three times is listed three times in the control file. Writing one
        // entry per citation puts it in the bibliography three times: a full entry and a
        // dash for each repeat.
        let mut seen = std::collections::HashSet::new();
        ctl.citekeys.retain(|key| seen.insert(key.clone()));
        // The datalist names the template the document sorts with; that template's
        // groups are the ones a writer must honour.
        if let Some(groups) = ctl.templates.get(&ctl.sorting_template) {
            ctl.sorts = groups.clone();
        }
        Ok(ctl)
    }
}

/// The `<bcf:key>name</bcf:key><bcf:value>v</bcf:value>` pairs in the options blocks.
///
/// Every component repeats the same keys, so the last value wins; for `uniquelist` the
/// components agree, and the style's own answer is the one that matters.
fn options_from_xml(xml: &str) -> std::collections::HashMap<String, String> {
    let mut out = std::collections::HashMap::new();
    let mut key: Option<String> = None;
    for line in xml.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("<bcf:key>") {
            key = rest.strip_suffix("</bcf:key>").map(|k| k.to_string());
        } else if let Some(rest) = line.strip_prefix("<bcf:value>") {
            if let (Some(k), Some(v)) = (key.take(), rest.strip_suffix("</bcf:value>")) {
                out.insert(k, v.to_string());
            }
        }
    }
    out
}
