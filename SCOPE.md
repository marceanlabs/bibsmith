# bibsmith: scope and evidence

30 October 2026. Written before any product code, per the shape used for imgsmith,
pdfsurgeon and xlsmith.

## What it is

A single static binary that does biber's job on a `.bib` file: read it, resolve what it
can, sort it the way biblatex sorts, and write it back. No Perl, no TeX install, no
network.

Name check: `bibsmith` returns HTTP 404 from the crates.io API (free as of 30 Oct 2026).
`biber-rs` and `bibsmith-cli` are also free.

## Why this one, in numbers

Read on 30 October 2026 from the crates.io API and the GitHub API.

| Evidence | Number |
|---|---|
| `biblatex` crate downloads (library layer, maintained by Typst) | 4,534,156 |
| `hayagriva` (the other reference engine in Rust) | 4,067,973 |
| `citationberg` (CSL parser) | 3,986,516 |
| Rust CLI doing the same job as biber | none found |
| Closest neighbour: `bibiman`, a TUI over BibLaTeX databases | 10,818 downloads |

The library layer is solved and heavily used; the command line layer is empty. That is the
same gap shape that justified xlsmith (`calamine` 5.97M downloads, best Rust CLI 14k).

The incumbent is biber: Perl, shipped inside TeX Live, one maintainer, and every biblatex
user depends on it. It is genuinely popular software and genuinely unloved.

## The gap is not "read a .bib"

Reading is the easy half and the library already does it. What a replacement has to get
right, in order of how likely it is to be wrong:

1. **Normalisation biber performs silently.** Measured with `biber --tool` on
   `biblatex-examples.bib`: it rewrites `year` to `date`, `journal` to `journaltitle`, and
   drops `sorttitle` because it recomputes it. Of the 92 entries, 42 differ from the input
   for exactly these reasons. A replacement that copies fields through verbatim produces a
   file biber would not.
2. **Sorting.** biblatex's schemes (`nyt`, `nty`, `ynt`, and `sortname`/`sortkey`
   overrides) with unicode collation, not `str::sort`. Every case that breaks is a name
   particle or a corporate author.
3. **Uniquifying.** `uniquename`, `uniquelist` and the `a`/`b`/`c` disambiguation suffix
   that appears when two works share an author-year. This is where biber earns its keep.
4. **`.bcf` input.** Real biber runs from a `.bcf` written by biblatex, not from a bare
   `.bib`. A drop-in replacement has to read that XML too, or it is a separate tool.

## Honest scope

Sort, format and uniquify for the common styles, driven by a `.bib` and by `--tool`-style
options: **8 to 12 weeks** at his pace. Full biber parity, including `.bcf` input, remote
lookups (DOI, ISBN, PubMed), and every `\DeclareSortingTemplate`, is a year-plus and not a
solo job. The README will say which half it does.

Selling point is not features, it is the shape: one binary, no Perl, no TeX, works in a
container and in CI where installing TeX Live is 5 GB nobody wants.

## Verification plan

The oracle is biber itself, not the library. Circular checks are the failure mode this
project cannot afford: the crate that parses the file must not be the only thing that
reads the output.

| Layer | Oracle | Available |
|---|---|---|
| Parse fidelity | `biber --tool` normalised output, compared entry by entry | biber 2.22, installed |
| Sort order | biber's `--tool` sorting templates on the same input | biber 2.22 |
| Uniquifying | a real document built with latexmk + biber, `.bbl` compared | TinyTeX universal-darwin |
| Real input | `biblatex-examples.bib` (100 blocks, 92 entries, 8 `@string`) plus files from at least three producers | fetched from CTAN |

## Where the oracle lives

- TinyTeX: `~/Library/TinyTeX/bin/universal-darwin/` (`tlmgr`, `latexmk`, `biber` 2.22).
- No Homebrew on this machine; `tlmgr` is the package manager that matters here.

## Spike results so far

See `spikes/001-biblatex-parse/README.md` and `spikes/002-names-and-sort/README.md`.
Both parsed the canonical 100-block file with no losses: 92/92 entries, 0 round-trip
failures, 171 people parsed, 0 empty family names, particles (`van`, `von`, `de`) in the
right slot.
