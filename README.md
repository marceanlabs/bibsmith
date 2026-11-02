# bibsmith

Write the `.bbl` that biblatex typesets, in Rust, without Perl or a TeX install.

```console
$ cargo install bibsmith
$ pdflatex paper.tex            # writes paper.bcf
$ bibsmith bbl paper.bcf        # writes paper.bbl, which biber would have written
$ pdflatex paper.tex && pdflatex paper.tex
```

One static binary. The `.bbl` it writes is byte-comparable with biber's, and the
bibliography it typesets is identical: 18 documents, six styles, the canonical corpus, a
real ACM bibliography and BibTeX's own torture sample.

## How it fits a build

biblatex's `backend=` option accepts `biber`, `bibtex` and `bibtex8`, so bibsmith cannot be
swapped in by changing one word. It replaces the biber *step*:

```console
$ bibsmith bbl paper.bcf        # instead of: biber paper
```

Or wire it into latexmk, which already knows how many passes a bibliography needs:

```perl
# .latexmkrc
$biber = "bibsmith bbl %S";
```

`bibsmith` writes the `.blg` log latexmk reads after the bibliography step, so a latexmk
build settles with that one line and no other changes.

## Commands

| Command | What it does |
|---|---|
| `bibsmith bbl <file.bcf>` | Write the `.bbl` biblatex reads: every entry, every field, the disambiguation suffixes, the sorting template the document declared |
| `bibsmith sort <file.bib>` | Sort a database the way biblatex sorts it (`--scheme nyt`, `-o out.bib`) |
| `--normalise` | Rename legacy fields the way `biber --tool` does (`year` to `date`, `journal` to `journaltitle`) |
| `bibsmith check <file.bib>` | Report what normalising would change, and change nothing |
| `bibsmith labels <file.bib>` | Print the `a`/`b` suffix biblatex would print, in sort order |
| `bibsmith fetch <file.bib>` | Ask Crossref about every DOI and print the fields the entries lack; `--write` adds them |

Exit codes: 0 written, 1 the file could not be read or parsed, 2 bad arguments.

`bbl` reads the sorting template out of the `.bcf` rather than recognising a scheme name,
so `nyt`, `nty`, `ynt`, `nyvt`, `ydnt`, `anyt` and templates a package invents all work.
Each `\refsection` is its own namespace, `@set` members print through `\inset`, `related`
entries are written under the hash biblatex looks them up by, and a `crossref` loop is
survived rather than followed.

## Measured

Every number below is a test in this repository, not a claim.

| Check | Result |
|---|---|
| Typeset bibliography against biber, 18 documents | identical page text, six styles |
| `biber --tool` field parity, two corpora | no field biber writes is missing |
| Sorting, 92 entries, `nyt` / `nty` / `ynt` / `nyvt` / `ydnt` | 92 of 92 positions |
| Fuzzing | 1.08M bibliography and 1.17M control-file cases, no crash |
| Name hashes, 102 entries | every `\strng` value and position matches biber |
| Speed, 1,020 entries | 0.01 s and 16 MB, against biber's 9.62 s and 253 MB |

The corpora are `biblatex-examples.bib` from CTAN (100 `@` blocks, every awkward case the
package authors could think of), the ACM `sig-alternate` sample, and BibTeX's own
`xampl.bib` (a preamble of raw TeX, bare abbreviations as values, `#` joins, dates hidden
behind `\noopsort`). Expected output comes from real biber 2.22 over the same files, so
every test compares against a different implementation, not against itself.

`scripts/check-pdf.sh` rebuilds one document twice — once with biber's `.bbl`, once with
ours — and diffs the text extracted from both PDFs.

## Why the order is not obvious

Rules that are invisible until a real file breaks:

- An entry with no author or editor sorts under its title, not at the front.
- An `@set` sorts under its first member.
- `options = {useprefix}` files `van Gennep` under V; without it, under G.
- `options = {useeditor=false}` drops the editor from the sort, so the title decides.
- A legacy `key` names where an entry files, and it is text, not markup:
  `key = {Kn{\printfirst{v}{1987}}}` files under `Knv1987`.
- `sort_direction="descending"` reverses a group, which is how `ydnt` puts the newest year
  first while leaving undated entries at the end.
- An empty field is an absent one: `organization = {}` still inherits from its `crossref`
  parent.
- Collation is Unicode-aware: `vázques` comes before `vizedom`, which byte order gets wrong.

Each was found by measuring a mismatch against biber, then confirming the rule in
`biblatex.def` or the `.bcf`.

## What it does not do

- **No ISBN or PubMed lookup.** `fetch` asks Crossref about a DOI. biber also fills entries
  from ISBNs and PubMed IDs; this does not.
- **No contact address is built in.** Crossref likes one in the `User-Agent`; set
  `BIBSMITH_CONTACT` or pass `--contact` and it goes out, otherwise the request is
  anonymous.
- **`--normalise` keeps fields that `biber --tool` deletes** — `sorttitle`, `sortname`,
  `sortyear`, `presort`, `entryset`, plus every field biblatex does not model (`acmid`,
  `numpages`, `lastaccessed`, `articleno`, `source`, `issue_date`, `mrnumber`). Those decide
  the order, and biber's own `--tool` output demonstrably reorders a bibliography when fed
  back through biblatex.
- **One known disagreement.** On `sorting=nyvt` the five-volume Knuth set comes out of biber
  in an order its own template contradicts (name, year, volume says B before C; biber prints
  C, D, B). bibsmith follows the template. Nothing else in the test set differs.

## Library

The binary is a thin caller over a public API, because the download counts that justify this
tool are library downloads: `biblatex` is at 4.5M and the largest Rust CLI in the citation
space is a TUI at 10k.

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
