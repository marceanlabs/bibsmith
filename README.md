# bibsmith

Sort a `.bib` file the way biblatex sorts it, without Perl and without a TeX install.

```console
$ cargo install bibsmith
$ bibsmith sort refs.bib --scheme nyt > sorted.bib
```

One static binary. No network, no TeX Live, no biber, no runtime.

## What it does

| Command | What it does |
|---|---|
| `bibsmith sort <file>` | Sort the database and write it out (`--scheme nyt`, `-o out.bib`) |
| `bibsmith check <file>` | Report the fields biber would rename or drop, change nothing |
| `--scheme` | `nyt`, `nty` or `ynt`, exactly as biblatex names them |

`sort` reads `@string` definitions and every entry type biblatex knows, and writes the same
entries in biblatex's order. Exit codes: 0 written, 1 the file could not be read or parsed,
2 bad arguments.

## Measured

Every number below is a test in this repository, not a claim.

| Check | Result |
|---|---|
| Order matches biber, `nyt` | 92 of 92 positions |
| Order matches biber, `nty` | 92 of 92 positions |
| Order matches biber, `ynt` | 92 of 92 positions |
| Parse round-trip on the canonical corpus | 92 of 92 entries, 0 lost fields |

The corpus is `biblatex-examples.bib` from CTAN, the file the biblatex manual is built
from: 100 `@` blocks, 92 entries, 8 `@string`, every awkward case the package authors
could think of. The expected order comes from a `.bbl` written by real biber 2.22 over
that file, so the test compares against a different implementation, not against itself.

Regenerate the fixtures with `scripts/make_oracle.sh` (needs `pdflatex` and `biber`).

## Why the order is not obvious

Six rules decide the order, and five of them are invisible until a real file breaks:

- An entry with no author or editor sorts under its title, not at the front.
- An `@set` sorts under its first member.
- `options = {useprefix}` files `van Gennep` under V; without it, under G.
- `options = {useeditor=false}` drops the editor from the sort, so the title decides.
- Collation is Unicode-aware: `vázques` comes before `vizedom`, which byte order gets wrong.
- `volume` is the last key part and an absent one counts as `0`, so a print article with
  `volume = 12` files after its own online-only twin.

Each was found by measuring a mismatch against biber, then confirming the rule in
`biblatex.def`.

## What it does not do

- **No `.bcf` input.** biber runs from a `.bcf` written by biblatex. This reads `.bib`.
- **No disambiguation.** `uniquename`, `uniquelist` and the `a`/`b` suffixes biblatex adds
  when two works share an author-year are not implemented.
- **No remote lookups.** No DOI, ISBN or PubMed queries.
- **No field normalisation in `sort` yet.** Fields are written as they were read; `check`
  only reports what biber would change.
- **Three schemes.** `nyt`, `nty` and `ynt` are the ones with oracle tests. `nyvt`, `ydnt`
  and user-declared templates are not implemented, and asking for one is an error rather
  than a silent fallback to a different order.

So: not a biber replacement. A sorter that agrees with biber, which is the half that
matters when a bibliography has to be reproducible in CI.

## Library

The binary is a thin caller over a public API, because the download counts that justify
this tool are library downloads: `biblatex` is at 4.5M and the largest Rust CLI in the
citation space is a TUI at 10k.

```rust
use bibsmith::{Database, Scheme, sort_entries};

let db = Database::parse(&std::fs::read_to_string("refs.bib")?)?;
let entries: Vec<_> = db.iter().collect();
for entry in sort_entries(&entries, Scheme::Nyt) {
    println!("{}", entry.key);
}
```

## Licence

Apache-2.0. Free, no warranty, no support promise.
