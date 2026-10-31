use std::collections::HashMap;
use std::path::PathBuf;
use std::process::ExitCode;

use bibsmith::{
    ControlFile, Database, disambiguate, normalise_all, scheme_or_default, sort_entries,
    normalise_all_with, suffix_for, write_bbl_scoped, write_raw,
};
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "bibsmith",
    version,
    about = "Sort and normalise .bib files the way biblatex does, without Perl or TeX"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Sort a database and write it back out
    Sort {
        file: PathBuf,
        /// Sorting scheme, as biblatex names it: nty, nyt or ynt
        #[arg(long, default_value = "nyt")]
        scheme: String,
        /// Rename legacy fields the way biber does, and drop the ones it recomputes
        #[arg(long)]
        normalise: bool,
        /// Write to this file instead of standard output
        #[arg(short, long)]
        out: Option<PathBuf>,
    },
    /// Report what biber would rename or drop, without writing anything
    Check { file: PathBuf },
    /// Build the .bbl for a document from the .bcf biblatex wrote
    Bbl {
        /// The .bcf file; the .bbl is written next to it
        bcf: PathBuf,
        /// Write to this file instead of alongside the .bcf
        #[arg(short, long)]
        out: Option<PathBuf>,
    },
    /// Print the label suffix biblatex would print for each entry that needs one
    Labels {
        file: PathBuf,
        /// Sorting scheme, as biblatex names it: nty, nyt or ynt
        #[arg(long, default_value = "nyt")]
        scheme: String,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("bibsmith: {e}");
            ExitCode::FAILURE
        }
    }
}

/// Read a bibliography, accepting the Latin-1 files real tools still produce.
///
/// `ieee-abrv.bib` from CTAN is not valid UTF-8, and refusing it is not a defensible
/// answer for a tool that reads other people's databases. UTF-8 is tried first; a
/// file that fails is decoded as Latin-1 and a note goes to standard error, so the
/// substitution is never silent.
fn read(path: &PathBuf) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    match String::from_utf8(bytes) {
        Ok(text) => Ok(text),
        Err(err) => {
            eprintln!(
                "bibsmith: {}: not valid UTF-8, decoding as Latin-1 ({})",
                path.display(),
                err.utf8_error()
            );
            Ok(err.into_bytes().iter().map(|b| *b as char).collect())
        }
    }
}

fn run(cli: Cli) -> Result<(), String> {
    match cli.command {
        Command::Sort {
            file,
            scheme,
            normalise,
            out,
        } => {
            let src = read(&file)?;
            let db = Database::parse(&src).map_err(|e| format!("{}: {e}", file.display()))?;
            let scheme = scheme_or_default(&scheme)?;
            let entries: Vec<&biblatex::Entry> = db.iter().collect();
            let ordered = sort_entries(&entries, scheme);

            let mut text = String::new();
            // `@string` definitions are needed in both modes: entries refer to the
            // macros by name, and a file without them resolves to literal text.
            for block in db.string_blocks() {
                text.push_str(&upper_string_block(block));
                text.push('\n');
            }
            {
                let mut by_key: HashMap<String, _> = normalise_all_with(&src, normalise)
                    .map_err(|e| format!("{}: {e}", file.display()))?
                    .into_iter()
                    .map(|n| (n.key.clone(), n))
                    .collect();
                for entry in ordered {
                    match by_key.remove(entry.key.as_str()) {
                        Some(n) => text.push_str(&write_raw(&n)),
                        // The two parses disagreeing means a bug here, not in the
                        // file; refusing beats writing a partial database.
                        None => {
                            return Err(format!(
                                "{}: no normalised form for {}",
                                file.display(),
                                entry.key
                            ));
                        }
                    }
                }
            }

            match out {
                Some(path) => {
                    std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))?
                }
                None => print!("{text}"),
            }
            Ok(())
        }
        Command::Bbl { bcf, out } => {
            let xml = read(&bcf)?;
            let ctl = ControlFile::parse(&xml)?;

            // Every datasource in the control file is read; a document may add several.
            let mut src = String::new();
            let dir = bcf.parent().unwrap_or_else(|| std::path::Path::new("."));
            for ds in &ctl.datasources {
                let path = dir.join(ds);
                src.push_str(&read(&path)?);
                src.push('\n');
            }

            let db = bibsmith::parse_lenient(&src)
                .map_err(|e| format!("{}: {e}", bcf.display()))?;

            // `\nocite{*}` reaches the control file as a literal `*`, and means every
            // entry in the database rather than a key called star.
            let mut citekeys = ctl.citekeys.clone();
            if citekeys.iter().any(|k| k == "*") {
                citekeys.retain(|k| k != "*");
                let all = bibsmith::normalize::raw_entries(&src)
                    .map_err(|e| format!("{}: {e}", bcf.display()))?;
                for key in all.into_iter().map(|e| e.key) {
                    if !citekeys.contains(&key) {
                        citekeys.push(key);
                    }
                }
            }
            let missing: Vec<&String> = citekeys.iter().filter(|k| db.get(k).is_none()).collect();
            if !missing.is_empty() {
                return Err(format!("cited but not in the database: {missing:?}"));
            }

            // Order follows the template named in the .bcf: the document says how it
            // wants its bibliography sorted, so there is nothing to guess.
            // `anyt` is the alphabetic template: the same name-year-title order, with
            // the label sorted first (handled below).
            let scheme = match ctl.sorting_template.as_str() {
                "nyt" | "anyt" => bibsmith::Scheme::Nyt,
                "nty" => bibsmith::Scheme::Nty,
                "ynt" => bibsmith::Scheme::Ynt,
                other => {
                    return Err(format!(
                        "sorting template {other:?} is not implemented; this build knows nyt, nty, ynt and anyt"
                    ));
                }
            };
            let cited: Vec<&biblatex::Entry> = citekeys.iter().filter_map(|k| db.get(k)).collect();
            let ordered = sort_entries(&cited, scheme);

            // `crossref` last: a chapter's book title and editor live on the parent, and
            // without them the entry prints with no book at all.
            let raw: HashMap<String, Vec<(String, String)>> =
                bibsmith::normalize::raw_expanded_entries(&src)
                    .map_err(|e| format!("{}: {e}", bcf.display()))?
                    .into_iter()
                    .map(|e| (e.key, e.fields))
                    .collect();
            let effective: Vec<bibsmith::merge::Effective> = ordered
                .iter()
                .map(|entry| bibsmith::merge::effective(&db, entry, &raw))
                .collect();
            let mut effective = effective;
            // biber skips an entry whose name list cannot be parsed and carries on: one
            // bad author in a thousand-entry file must not shift every label after it.
            // "Werneck,, Renato" is the case that turned up, and it is the difference
            // between our numbering and biber's on a real ACM file.
            effective.retain(|entry| match &entry.malformed_name {
                Some(name) => {
                    eprintln!(
                        "WARN - Name {:?} is malformed (consecutive commas): skipping entry {:?}",
                        name, entry.key
                    );
                    false
                }
                None => true,
            });

            // `related = {key}` is written as the *hash* of the related entry, which
            // biblatex then looks up in the datalist. So every related entry has to be
            // written too, under that hash and with the options that keep it out of the
            // list; without it the "See also" text simply does not appear.
            let mut data_entries: Vec<bibsmith::merge::Effective> = Vec::new();
            let mut seen_data: std::collections::HashSet<String> = Default::default();
            for entry in effective.iter_mut() {
                let Some(slot) = entry.values.iter_mut().find(|(k, _)| k == "related") else {
                    continue;
                };
                let keys: Vec<String> = slot
                    .1
                    .split(',')
                    .map(|k| k.trim().to_string())
                    .filter(|k| !k.is_empty())
                    .collect();
                let mut hashed = Vec::new();
                for key in keys {
                    let hash = format!("{:x}", md5::compute(key.as_bytes()));
                    if seen_data.insert(key.clone()) {
                        if let Some(related) = db.get(&key) {
                            let mut data =
                                bibsmith::merge::effective(&db, related, &raw);
                            data.data_key = Some(hash.clone());
                            data_entries.push(data);
                        }
                    }
                    hashed.push(hash);
                }
                slot.1 = hashed.join(",");
            }

            let by_key: HashMap<&str, &bibsmith::merge::Effective> =
                effective.iter().map(|e| (e.key.as_str(), e)).collect();

            // The alphabetic label, and the label order the `anyt` template asks for:
            // it sorts by the label first, which is why `jcg` sits before `Almendro`.
            // Set members are not labelled: they carry the set's label, and counting
            // them makes the set look like a duplicate of itself (`Her+06a`).
            let member_keys: std::collections::HashSet<String> = effective
                .iter()
                .filter_map(|e| e.entryset.as_ref())
                .flat_map(|set| set.split(',').map(|k| k.trim().to_string()))
                .collect();

            let mut labels: HashMap<String, (String, Option<u8>)> = HashMap::new();
            let mut labelled: Vec<(String, &bibsmith::merge::Effective)> = Vec::new();
            for entry in effective
                .iter()
                .filter(|e| e.data_key.is_none() && !member_keys.contains(&e.key))
            {
                let member = entry
                    .entryset
                    .as_ref()
                    .and_then(|s| s.split(',').next())
                    .map(|k| k.trim())
                    .and_then(|k| by_key.get(k).copied());
                let label = bibsmith::label::label_alpha_with(entry, 2, member);
                labelled.push((label.clone(), entry));
                labels.insert(entry.key.clone(), (label, None));
            }
            let sequence: Vec<String> = labelled.iter().map(|(l, _)| l.clone()).collect();
            // Numbers follow bibliography order, so they are assigned from the same
            // sequence they were counted from, never from a map's iteration order.
            let extras_alpha = bibsmith::label::extra_alpha(&sequence);
            for ((_, entry), extra) in labelled.iter().zip(extras_alpha) {
                if let Some((_, slot)) = labels.get_mut(&entry.key) {
                    *slot = extra;
                }
            }

            let sort_by_label =
                ctl.sorts.iter().flat_map(|g| g.items.iter()).any(
                    |item| matches!(item, bibsmith::bcf::SortItem::Field(f) if f == "labelalpha"),
                );

            let refs: Vec<&bibsmith::merge::Effective> = if sort_by_label {
                // `collated_sort` is stable, so the name-year-title order stays as the
                // tiebreak for two entries with the same label.
                let labels_for: HashMap<&str, &str> = labelled
                    .iter()
                    .map(|(l, e)| (e.key.as_str(), l.as_str()))
                    .collect();
                let keyed: Vec<(String, &biblatex::Entry)> = ordered
                    .iter()
                    .map(|e| {
                        (
                            labels_for
                                .get(e.key.as_str())
                                .copied()
                                .unwrap_or("")
                                .to_string(),
                            *e,
                        )
                    })
                    .collect();
                bibsmith::sort::collated_sort(keyed)
                    .into_iter()
                    .filter_map(|e| by_key.get(e.key.as_str()).copied())
                    .collect()
            } else {
                effective.iter().collect()
            };
            // A document with several `\refsection` blocks gets a `\refsection` per
            // section; each section's citations are the keys listed under its number.
            let numbered: Vec<(String, Vec<&_>)> = if ctl.sections.len() <= 1 {
                vec![("0".to_string(), refs.clone())]
            } else {
                ctl.sections
                    .iter()
                    .map(|(num, keys)| {
                        (
                            num.clone(),
                            refs.iter()
                                .copied()
                                .filter(|e| keys.contains(&e.key))
                                .collect::<Vec<_>>(),
                        )
                    })
                    .filter(|(_, list): &(String, Vec<&bibsmith::merge::Effective>)| !list.is_empty())
                    .collect()
            };
            // The `related` entries go last, out of the sorted flow: they exist only to
            // be looked up by hash.
            let mut numbered = numbered;
            if let Some((_, list)) = numbered.first_mut() {
                list.extend(data_entries.iter());
            }
            let sections: Vec<(&str, &[&bibsmith::merge::Effective])> = numbered
                .iter()
                .map(|(n, v)| (n.as_str(), v.as_slice()))
                .collect();
            let extras_by_section: Vec<HashMap<String, u8>> = numbered
                .iter()
                .map(|(_, list)| {
                    let entries: Vec<&biblatex::Entry> = list
                        .iter()
                        .filter(|e| e.data_key.is_none())
                        .filter_map(|e| db.get(&e.key))
                        .collect();
                    disambiguate(&entries, scheme)
                })
                .collect();
            let text = write_bbl_scoped(&ctl, &sections, &extras_by_section, &labels);

            let dest = out.unwrap_or_else(|| bcf.with_extension("bbl"));
            std::fs::write(&dest, text).map_err(|e| format!("{}: {e}", dest.display()))?;
            eprintln!("bibsmith: wrote {}", dest.display());
            Ok(())
        }
        Command::Labels { file, scheme } => {
            let src = read(&file)?;
            let db = Database::parse(&src).map_err(|e| format!("{}: {e}", file.display()))?;
            let scheme = scheme_or_default(&scheme)?;
            let entries: Vec<&biblatex::Entry> = db.iter().collect();
            let ordered = sort_entries(&entries, scheme);
            let extras = disambiguate(&ordered, scheme);
            // Printed in sort order, because that is the order the suffixes count in.
            for entry in &ordered {
                if let Some(n) = extras.get(&entry.key) {
                    println!("{}\t{}", entry.key, suffix_for(*n));
                }
            }
            if extras.is_empty() {
                println!("no entry needs a suffix: every label is already unique");
            }
            Ok(())
        }
        Command::Check { file } => {
            let src = read(&file)?;
            let normalised = normalise_all(&src).map_err(|e| format!("{}: {e}", file.display()))?;
            let total = normalised.len();
            let mut changed = 0usize;
            let mut renames = 0usize;
            for entry in &normalised {
                if entry.renamed.is_empty() && entry.dropped.is_empty() {
                    continue;
                }
                changed += 1;
                renames += entry.renamed.len();
                let mut parts: Vec<String> = Vec::new();
                for (from, to) in &entry.renamed {
                    parts.push(format!("{from} -> {to}"));
                }
                for drop in &entry.dropped {
                    parts.push(format!("{drop} (not written)"));
                }
                println!("{}: {}", entry.key, parts.join(", "));
            }
            println!(
                "{changed} of {total} entries change under --normalise ({renames} field renames)"
            );
            Ok(())
        }
    }
}

/// `@string{dtv = ...}` is written `@STRING{DTV = ...}`, as biber writes it.
fn upper_string_block(block: &str) -> String {
    let Some(open) = block.find('{') else { return block.to_string() };
    let rest = &block[open + 1..];
    let Some(eq) = rest.find('=') else { return block.to_string() };
    let name = rest[..eq].trim();
    format!(
        "@STRING{{{} = {}",
        name.to_uppercase(),
        rest[eq + 1..].trim_start()
    )
    .trim_end()
    .to_string()
}
