use std::collections::HashMap;
use std::path::PathBuf;
use std::process::ExitCode;

use bibsmith::{Database, normalise_all, scheme_or_default, sort_entries, write_raw};
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

fn read(path: &PathBuf) -> Result<String, String> {
    std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))
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
        Command::Check { file } => {
            let src = read(&file)?;
            let normalised = normalise_all(&src).map_err(|e| format!("{}: {e}", file.display()))?;
            let total = normalised.len();
            let mut changed = 0usize;
            for entry in &normalised {
                if !entry.dropped.is_empty() {
                    changed += 1;
                    println!("{}: {}", entry.key, entry.dropped.join(", "));
                }
            }
            println!("{changed} of {total} entries have a field biber would rename or drop");
            Ok(())
        }
    }
}
