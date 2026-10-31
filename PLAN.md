# What is left before this replaces biber

Written 31 October 2026, after five styles typeset identically to biber on the canonical
corpus. Ordered by what a real LaTeX user hits first, not by what is easy.

Sizes are working days at the pace so far: roughly one measured defect per hundred lines
written, each verified by `scripts/check-pdf.sh`.

## Done

| Piece | Evidence |
|---|---|
| `.bib` parse, normalise, sort, `extradate` | 92/92 field parity against `biber --tool` |
| `.bcf` in, `.bbl` out | five documents typeset identically to biber |
| Styles: authoryear, authortitle, numeric, alphabetic | identical PDF text |
| `labelalpha` and `extraalpha` | 86/86 labels match biber |
| `verbose`, footnotes, `\footcites`, repeats | identical PDF text |

## Steps, in order

1. ~~**`verbose` and its relatives.**~~ Done 31 Oct 2026: `verbose` and a document with
   `\footcite`, a repeated citation and `\footcites` with pre- and postnotes both
   typeset identically. The style's citation machinery lives in biblatex and the `.aux`;
   the `.bbl` it needs is the same one the other styles need. That two-week estimate was
   wrong, and finding out cost one document.
   Still untested: `verbose-note`, `verbose-inote`, `verbose-trad1/2/3`, which differ in
   *when* the short form is used, so they are a document each rather than a rewrite.
2. **`uniquename`, `uniquelist`, `uniquetitle`.** biber decides these across the whole
   bibliography and writes `\field{extraname}`, `\field{uniquelist}`, `\field{uniquetitle}`.
   One to two weeks, and it forces the refinement below.
3. **The rest of the `.bbl` fields** biber writes and we skip: `\true{nocite}`,
   `\field{range}`, `\field{labelnumber}` for set members, `\field{origfields}`,
   `\field{ids}`. Two to three days; each one is a line in the writer once measured.
4. **Data-driven sorting.** The sort groups are already parsed out of the `.bcf`; the
   writer still maps four template *names* to hand-written schemes. Following the groups
   directly removes the "template not implemented" error class and covers `nyvt`,
   `debug` and any user-defined template. Two to three days.
5. **`--tool` parity**: `--output-format`, `--validate-datamodel`, `--sortlocale`,
   exit codes and message wording. A week.
6. **Remote metadata** (DOI, ISBN, PubMed) and `--input-encoding`. One to two weeks, and
   the only part that needs the network, so it is deliberately last.
7. **Multi-section documents**: several `\refsection`/`\refcontext`, `\nocite` variants.
   Three to five days.
8. **Robustness**: invalid UTF-8, very large files, a fuzz target for the parser.
   Three days.

## What needs refining, not just extending

The seven `\strng{}` hashes written per entry are one md5, not seven. That is enough for
the `—` dash, which only compares for equality, and it is why the five styles match. It
is not enough for `uniquename`: biber distinguishes `namehash` from `fullhash` from
`bibnamehash` because disambiguation compares *parts* of a name. Step 2 has to split them.

`sortinit` is derived from the label name and matches on this corpus; a name that opens
with a TeX command (`{\O}stergaard`) has not been tested.

## Found by the verbose document

A key cited three times is listed three times in the control file, and writing one entry
per citation put it in the bibliography three times: a full entry followed by a dash for
each repeat. The reader now de-duplicates, and a test covers it. This affects any real
document, not just `verbose`.

## Removed as dead or duplicated (31 Oct 2026)

- `raw_values` — superseded by `raw_entries`, nothing called it.
- `COMPUTED` — an empty list guarding a branch that never ran.
- a second `RANGES` list in the writer, now shared from `normalize`.
- an unused `label_alpha` wrapper around `label_alpha_with`.
- `examples/chunks.rs`, which existed to prove the parser drops backslashes.
- 26 generated files untracked (PDFs, `.bbl.biber`, build logs): 84 tracked files to 58.
  `scripts/check-pdf.sh` rebuilds them.
