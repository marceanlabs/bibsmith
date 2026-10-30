use std::path::PathBuf;
use std::process::ExitCode;

use bibsmith::{Database, scheme_or_default};
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

fn run(cli: Cli) -> Result<(), String> {
    match cli.command {
        Command::Sort { file, scheme, out } => {
            let src =
                std::fs::read_to_string(&file).map_err(|e| format!("{}: {e}", file.display()))?;
            let db = Database::parse(&src).map_err(|e| format!("{}: {e}", file.display()))?;
            let scheme = scheme_or_default(&scheme)?;
            let entries: Vec<&biblatex::Entry> = db.iter().collect();
            let ordered = bibsmith::sort_entries(&entries, scheme);
            let text: String = ordered.iter().map(|e| e.to_biblatex_string()).collect();
            match out {
                Some(path) => {
                    std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))?
                }
                None => print!("{text}"),
            }
            Ok(())
        }
        Command::Check { file } => {
            let src =
                std::fs::read_to_string(&file).map_err(|e| format!("{}: {e}", file.display()))?;
            let db = Database::parse(&src).map_err(|e| format!("{}: {e}", file.display()))?;
            let mut changed = 0usize;
            for entry in db.iter() {
                let n = bibsmith::normalise_entry(entry);
                if !n.dropped.is_empty() {
                    changed += 1;
                    println!("{}: {}", entry.key, n.dropped.join(", "));
                }
            }
            println!(
                "{changed} of {} entries have a field biber would rename or drop",
                db.len()
            );
            Ok(())
        }
    }
}
