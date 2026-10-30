# Spike 001: does `biblatex` 0.12 parse the canonical real-world .bib?

**Question.** Given `biblatex-examples.bib` from CTAN (the file the biblatex manual is
built from, 100 `@` blocks, 1,661 lines, `@string`, `@preamble`, `crossref`, LaTeX
escapes), when parsed with `biblatex` 0.12, then every entry survives and every entry
round-trips back to text without losing a field.

**Why this first.** If the library mangles real input, nothing downstream matters.

## Result

```
file: biblatex-examples.bib
raw @-blocks:      100
parsed entries:    92
total fields:      1036
raw types:         {article 20, book 35, collection 3, inbook 3, incollection 5,
                    inproceedings 2, manual 1, mvbook 5, mvcollection 2, online 5,
                    patent 4, periodical 1, report 2, set 2, string 8, thesis 2}
parsed types:      identical, minus the 8 @string definitions
unaccounted:       8        (= the @string blocks, not lost entries)
round-trip failures: 0/92
```

## Verdict: VALIDATED

### What worked
- Every one of the 92 entries parsed, with entry types matching the raw text exactly.
- `to_biblatex_string()` then re-parse is lossless for all 92: same key, same field count.
- `@string` definitions are handled as a separate namespace (8 of them), not as entries,
  which is the correct behaviour and the reason the raw count is 100 and the entry count 92.

### What didn't
- Nothing failed at this layer.

### Surprises
- The 8-block difference is the whole of `@string` usage; there is no `@preamble` or
  `@comment` in this file, so those two remain untested.
- Field-count equality is not field-value equality. A value that is normalised (case,
  brace stripping) would pass this test and still differ from biber's output. Spike 003
  has to compare values against the biber oracle.

### Recommendation for the real build
- Depend on `biblatex` 0.12 for parsing and writing; do not hand-roll a parser.
- Keep `to_biblatex_string()` in the test suite as a regression net: reparse and compare
  field count per entry, which catches a dependency upgrade that drops a field silently.
