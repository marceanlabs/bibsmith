use std::collections::HashMap;
use std::path::PathBuf;
use std::process::ExitCode;

use bibsmith::{
    Database, disambiguate, normalise_all, scheme_or_default, sort_entries, suffix_for, write_raw,
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
