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
