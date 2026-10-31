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
}

impl ControlFile {
    pub fn parse(xml: &str) -> Result<Self, String> {
        let mut reader = Reader::from_str(xml);
        reader.config_mut().trim_text(true);

        let mut ctl = ControlFile::default();
        let mut stack: Vec<String> = Vec::new();
        // Where we are inside a sorting template.
        let mut in_template = false;
        let mut template_name = String::new();
        let mut in_sort = false;
        let mut in_fields = false;
        let mut last_field_dtype: Option<String> = None;
        let mut last_field_kind: Option<String> = None;
        let mut current_sort: Option<SortGroup> = None;

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
                        "fields" => in_fields = true,
                        "field" if in_fields => {
                            last_field_dtype = attr("datatype");
                            last_field_kind = attr("fieldtype");
                        }
                        "sortingtemplate" => {
                            in_template = true;
                            template_name = attr("name").unwrap_or_default();
                        }
                        "sort" if in_template => {
                            in_sort = true;
                            current_sort = Some(SortGroup {
                                finalsort: attr("final").is_some_and(|v| v == "1"),
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
                        "fields" => in_fields = false,
                        "sortingtemplate" => {
                            in_template = false;
                            if template_name == ctl.sorting_template
                                || ctl.sorting_template.is_empty()
                            {
                                ctl.sorting_template = template_name.clone();
                            }
                        }
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
                        "datasource" => ctl.datasources.push(text),
                        "citekey" => ctl.citekeys.push(text),
                        "presort" => ctl.presort = text,
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
        // The datalist names the template the document sorts with; that template's
        // groups are the ones a writer must honour.
        if let Some(groups) = ctl.templates.get(&ctl.sorting_template) {
            ctl.sorts = groups.clone();
        }
        Ok(ctl)
    }
}
