# bibsmith: build notes and status

Fifth tool in the Marcean Labs family, after imgsmith, pdfsurgeon, xlsmith and sheeter.
Same shape: one narrow job, pure Rust, one static binary, refusals stated up front.

Free and open source. The goal is a body of work that is used, not revenue. Evidence and
scope in `SCOPE.md`.

## Status, 30 October 2026

**Sorting parity with biber: reached.** All three schemes biblatex ships by default
(`nyt`, `nty`, `ynt`) reproduce biber's order exactly on the canonical corpus —
92 of 92 positions, asserted as a hard test in `tests/oracle.rs`, not a percentage.

```text
cargo test --release        nyt: 92/92   nty: 92/92   ynt: 92/92
```

End to end: `bibsmith sort oracle/refs.bib --scheme nyt` written out, then fed to real
biber, produces the identical `.bbl` order with zero errors or warnings.

What it took, each found by measurement rather than reading:

| Rule | How it was found |
|---|---|
| Entries with no author or editor sort by `sorttitle`/`title` | 5 entries clustered at the front; `biblatex-examples.bib` states the rule in the `cms` annotation |
| `@set` sorts under its first member | `stdmodel` sits immediately before `glashow`, `set` before `herrmann` |
| `options = {useprefix}` files `van Gennep` under V | 3 `vangennep` entries landed 60 positions early |
| `options = {useeditor=false}` drops the editor from sorting | `britannica` sorted under P for Preece instead of E for Encyclopaedia |
| Collation, not byte order | `vázques de parga` sorted after `vizedom` without it |
| `volume` is the last key part, `0` when absent | reproduced with a synthetic pair: print article with `volume=12` files after its online twin |

The template definitions were then read straight out of `biblatex.def` and matched the
behaviour: presort, name chain, year, title, volume, `sortkey` as a final tiebreak.

- Rust toolchain installed on this Mac (`cargo` 1.99.1, `rustc` 1.99.1). It was absent
  before today: `~/.cargo` held only `credentials.toml`.
- Oracle installed: TinyTeX at `~/Library/TinyTeX`, `biber` 2.22 and `latexmk` 4.88 via
  `tlmgr install biber`. There is no Homebrew on this machine.
  The installer's last step failed (`/usr/local/bin` is not writable, so it could not make
  the symlinks) but the binaries are fine; `~/Library/TinyTeX/bin/universal-darwin` is now
  on PATH from `.zshrc`, verified in a login shell.
- `bibsmith` is free on crates.io (HTTP 404).
- Spike 001 (parse fidelity): VALIDATED, 92/92 entries, 0 round-trip failures.
- Spike 002 (names and sort): VALIDATED, 171 people, 0 empty family names, particles
  correct.
- Spike 003 (order against biber): VALIDATED at 92/92 for three schemes.
## Normalisation, 30 October 2026

`sort --normalise` now rewrites fields the way biber does: `year` to `date`, `journal` to
`journaltitle`, `origyear` to `origdate`, `address` to `location`, `annote` to `annotation`,
`archiveprefix` to `eprinttype`, `key` to `sortkey`, `pdf` to `file`, `primaryclass` to
`eprintclass`, `school` to `institution`. Values get whitespace collapsed, and page ranges
are re-punctuated (`55-65` to `55--65`, en dash to `--`, no space after a comma).

Field sets now match `biber --tool` for 92 of 92 entries. Four things were learned by
measurement, and one of them is a design difference rather than a gap:

| Finding | Consequence |
|---|---|
| `biber --tool` deletes `sorttitle`, `sortname`, `sortyear`, `presort` | Feeding biber's own `--tool` output back through biblatex reorders the bibliography. We keep those fields: a sorter must not delete the fields that decide the order. |
| `label` is kept by biber | It was in our computed/dropped list; measured, it is not. |
| A value that is one abbreviation is written bare (`JOURNALTITLE = JOMCH`), a mixed one is braced | Writing `PUBLISHER = dtv and Walter de Gruyter` bare made biber report `found "and", expected end of entry`. Found by feeding our output back to biber. |
| `entryset` is dropped by `biber --tool`, leaving a `@set` with no members | Kept; biber then warns "Set entry has no entryset field". |

Known, documented difference: LaTeX escapes are preserved as written (`{\"O}` stays) where
biber decodes them to UTF-8 (`Ö`). Decoding TeX is an encoding policy with a large surface,
and preserving what the author wrote diffs cleanly in git.

## Disambiguation, 30 October 2026

`bibsmith labels` prints the `a`/`b`/`c` suffix biblatex would print, and the numbers
match the `extradate` biber writes into the `.bbl` for all 21 entries that carry one.

Two rules had to be measured, because each obvious version failed one case:

| Case | What it proves |
|---|---|
| `baez/article` (`date = 2004`) and `baez/online` (`date = {2004-10-27}`) share a label year | extra date precision is dropped when grouping |
| `knuth:ct` (`date = {1984/1986}`, a range) does not share one with the 1984 volume `knuth:ct:a` | a range must not collapse to its first year |
| `set` and `stdmodel` (a `@set`) get no suffix | an entry with no name and no title is not grouped on an empty label |

## Files from other producers, 31 October 2026

`biblatex-examples.bib` is one producer's idea of a bibliography. Two files from elsewhere
are now fixtures: the ACM `sig-alternate` sample (265 blocks, 163 `@String`, mixed-case
entry types) and IEEEtran's `abbreviations.bib` (263 blocks). Two real bugs fell out:

| Bug | How it showed | Fix |
|---|---|---|
| Entries written one after another on a single line | 265 `@` but only 164 lines started with one | push a newline after each entry |
| A Latin-1 `.bib` was refused outright | `ieee-abrv.bib` failed with "stream did not contain valid UTF-8" | decode as Latin-1 and say so on standard error |
| `@string` blocks were dropped from `sort` output | 163 definitions in the ACM file, 0 in the output | emit them before the entries, verbatim |

What `biber --tool` does to a real legacy file, measured on the ACM sample: it deletes
every field biblatex does not model (`month` 32 times, `numpages` 20, `acmid` 19,
`lastaccessed` 8, `articleno` 6, `source` 4, `issue_date` 3), folds `month` into `date`,
synthesises `type` for report entries, and skips one entry entirely because its author
string `Werneck,, Renato` is malformed.

So the exact-parity claim is scoped honestly: field sets match biber exactly on the
biblatex corpus, and on arbitrary legacy BibTeX the guarantee is weaker and better —
no field biber keeps is ever lost, and nothing the author wrote is deleted.

Tests now: 30 (10 `.bbl` contract, 7 CLI contract, 5 normalisation, 4 order and
disambiguation, 4 real files).

## `.bcf` in, `.bbl` out (31 Oct 2026)

`bibsmith bbl doc.bcf` reads the control file biblatex writes and produces the file
LaTeX reads. Verified by typesetting, not by comparing text: `scripts/check-pdf.sh`
builds the same document with biber's `.bbl` and with ours, then diffs the extracted
PDF text.

| Document | Style | Entries | Result |
|---|---|---|---|
| `oracle/doc/doc.tex` | authoryear | 5 | identical text |
| `oracle/nyt.tex` | authoryear, nyt | 92 | identical text |
| `oracle/style-authortitle.tex` | authortitle, nty | 92 | identical text |
| `oracle/style-numeric.tex` | numeric, nty | 92 | identical text |

Three more defects came out of the second and third styles:

- `\inset{set}` has to be written on the *member*, and the member's own record does not
  carry `entryset` — only the `@set` does. Without it, `\cite{member}` prints "[]": the
  member has no number and cannot borrow the set's.
- `sortinit` is the label name's first letter, not the title's: "Weinberg" indexes
  under W even though his title starts "A Model of Leptons".
- Given names are joined with `\bibnamedelima`, not a space, and ranges with
  `\bibrangedash`: both change where lines break, so a space and a `--` look right in
  the source and reflow the page.
- The label prints the short title when there is one, so `labeltitlesource` follows the
  entry instead of always saying `title`.

Five defects only the rendered page reveals, each fixed and now covered by a test:

- `location` and `publisher` are **list** fields. Written as `\field` they do not print:
  the publisher line vanished from a real document.
- A date is never written as a date. biblatex reads parts named after the field
  (`date`→year/month/day, `urldate`→urlyear/urlmonth/urlday, `eventdate`→eventyear…).
  A range is two dates: `1984/1986` is `year` plus `endyear`, and `eventenddateera`
  is needed or both ends print in full instead of "19–25 May".
- `&` must be escaped, or "Computers & Typesetting" prints as "Computers Typesetting".
- `{\TeX}` needs its braces. The `biblatex` crate's resolved `Entry` drops the
  backslash, so values are read from the raw parse with abbreviations expanded
  instead: same expansion, braces intact.
- The `—` that replaces a repeated author comes from comparing name hashes with the
  previous entry, so identical people must produce identical hashes. They are our own
  md5, not biber's: biblatex only compares them for equality.

Known difference that does not affect output: biber writes 102 `\entry` blocks to our
92, because it also emits an entry for every set member and every `crossref`-only
parent. The typeset pages are identical, so this is not chased further.

## Files

| Path | What |
|---|---|
| `SCOPE.md` | evidence, gap numbers, honest scope, verification plan |
| `spikes/001-biblatex-parse/` | parse fidelity on real input, with verdict |
| `spikes/002-names-and-sort/` | name split and sort key, with verdict |
| `spikes/001-biblatex-parse/biblatex-examples.bib` | the canonical oracle corpus, from CTAN |

## Decisions so far

- **Depend on `biblatex` 0.12, do not write a parser.** It is Typst's reference engine,
  4.5M downloads, and it already implements the BibTeX and extended name grammars.
- **The oracle is biber, never the crate.** The crate that parses must not be the only
  thing that reads the output.
- **Say what parity means in the README.** Common styles and `.bib` input, not `.bcf`
  and remote lookups. A tool that claims biber parity and misses it is worse than a
  narrower tool that states its half.

## Decisions that will come up again

**Name rewriting in `sort` is not behind a flag.** biber writes `Given Family` back as
`Family, Given`, and we do the same, including its failure ("Van Morrison" becomes
"Morrison, Van"). A drop-in replacement's promise is that nothing changes; a flag that
makes our output differ from biber's means the parity tests stop proving equality, which
is the only evidence this project has. A user who wants different behaviour is asking for
a different tool.

## `nyvt`: what biber actually does with `volume`

Measured on fixtures in `/tmp/nv2`, each with one variable changed:

| Entries | biber's order |
|---|---|
| volumes 1, 2, 3 | 1, 2, 3 — the numbers decide |
| volumes A, B, C, D, E, all else equal | C, D, A, B, E |
| volumes Alpha, Beta, Gamma, all else equal | input order, both ways round |
| volume C + sorttitle "Aye" vs volume B + sorttitle "Zed" | Aye first — the title decided, not the volume |

Two facts, and they do not agree with each other:

1. a non-numeric `volume` does not order entries. With distinct sorttitles the title
   decides even when the volume says the opposite, so a letter volume is being dropped
   somewhere between the field and the comparison
2. with every slot tied, the order biber produces is deterministic but is neither the input
   order nor any collation of the values — `C, D, A, B, E` for entries in `A…E` order

That second fact is the tie order of a Perl hash, and no amount of reading the template
reproduces it. Our reader follows the template literally: name, year, volume, title, with
`volume` compared as text.

The real corpus disagrees in the same place for the same reason: the five Knuth volumes
come out `a, c, d, b, e` from biber against `a, b, c, d, e` from the template. `a` is 1984
and sorts first on its year; the other four share 1986 and tie on name and year, so
whatever decides them is the same unexplained tie-break.

Until that is settled, `oracle/style-nyvt.tex` stays a known difference rather than a
passing test. Nothing else depends on it: `nyt`, `nty` and `anyt` are identical to biber on
every entry.

## Name hashes, 2 November 2026

The `.bbl` carries eight name hashes per name list, and bibsmith wrote one value eight
times. biblatex compares them when it decides whether two names need initials, so the
single value was the thing standing between `uniquename` and this writer.

What biber actually does, read from its source and then measured:

- `namehash` hashes the names a citation shows, with `+` for a truncated list.
- `bibnamehash` is the same for the bibliography's name count.
- `fullhash` hashes every name, and `fullhashraw` every name without the hash template.
- The template is in the `.bcf` (`hashscope="full"` or `init` per name part), and the
  counts come from the style's `maxcitenames`/`maxbibnames`, which here are 3 and 1.
- Every name list gets a role-prefixed group (`author…`, `editor…`), not just the label
  list, and the parts are hashed in the data model's order.

The digest is md5 over the concatenated parts after `normalise_string_hash` (TeX macros
become `name:`, accent macros become `34:`, braces and spaces go) and NFC.

Measured: all 102 entries and every one of their hashes match biber byte for byte, in the
same order. `tests/name_hashes.rs` asserts that against `oracle/nyt.bbl.biber`, and the
four typeset documents still match biber's pages.

One correction fell out of it: `options = {useeditor=false}` (Britannica) means the editor
is not a label name at all, so biber writes no `namehash` for that entry. The label list is
now chosen with the same `use*` rules the sort key already used.

Not yet modelled: `uniquelist` raises the visible count, and a per-entry or per-name-list
`namehashtemplate` is not read, only `global`.

## uniquename and uniquelist, 2 November 2026

The `\name` block carries the fields biblatex reads to decide which names to expand:
`un` (the level), `uniquepart` (`base` or `given`), `givenun` (the level of the given
name) and a per-name `hash`. We wrote a level, and a hash of our own shape, and no
`givenun` at all — so `uniquename` had nothing to work with even where the levels were
right.

Measured against a document built to need all of it (`oracle/uniq/`, nine entries):

- three `Smith`s in one year: John and Jane share an initial and get level 2, John A.
  does not and gets 1, because `J. A.` is not `J.`;
- two three-name lists sharing first and last author get `ul=2`, so `uniquelist` widens
  the visible list to the second name;
- a repeated author alone is level 0, with `givenun=0` still written.

Three fixes, all verified against that fixture and still identical pages on the four
typeset documents:

- the per-name hash is biber's: one person hashed through the same template as a list;
- `givenun` is written for every name that has a given part, whatever its level;
- an entry that is not in the bibliography proper (a set member, a `related` copy) gets
  the per-name hashes and nothing else, because biber skips disambiguation for it.

Known differences left, none of which changes a page:

- a name part biber protects (`Aks{\i}n`) is written by us as UTF-8 (`Aksın`);
- `sortinithash` is md5 of an ICU sort key here and of a Perl `Unicode::Collate` key
  there, so the values differ. Both are only compared for equality within one file;
- `uniquetitle` is not written: it needs `labeltitle`, which none of the fixtures use.
- fields after `sortinithash` are written in a different order than biber's; biblatex
  reads them by name, and the pages agree.
