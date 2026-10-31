use std::collections::HashMap;
use std::path::PathBuf;
use std::process::ExitCode;

use bibsmith::{
    ControlFile, Database, disambiguate, normalise_all, scheme_or_default, sort_entries,
    suffix_for, write_bbl, write_raw,
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
                text.push_str(block);
                text.push('\n');
            }
            if normalise {
                let mut by_key: HashMap<String, _> = normalise_all(&src)
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
            } else {
                for entry in ordered {
                    text.push_str(&entry.to_biblatex_string());
                    // `to_biblatex_string` does not always end with a newline, and two
                    // entries joined on one line is a file nobody can diff.
                    if !text.ends_with('\n') {
                        text.push('\n');
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

            let db = biblatex::Bibliography::parse(&src)
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
            let extras = disambiguate(&ordered, scheme);

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
            for entry in effective.iter().filter(|e| !member_keys.contains(&e.key)) {
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
            let text = write_bbl(&ctl, &refs, &extras, &labels);

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
