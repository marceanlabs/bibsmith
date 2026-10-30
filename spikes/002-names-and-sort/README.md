# Spike 002: name parsing and a sort key on real entries

**Question.** Given the 92 entries from `biblatex-examples.bib`, when names are parsed with
`entry.get_as::<Vec<Person>>(...)`, then no name yields an empty family name, and particles
(`van`, `von`, `de`) land in `prefix` rather than in the family name — because biber's sort
keys depend on that split.

**Why second.** Sorting is the visible half of biber, and a wrong key reorders a
bibliography silently. If the library's name split is wrong, that is our work to write.

## Result

```
entries:            92
with author:        81
with editor:        23
total people:       171
empty family name:  0
names with prefix:  5

  vangennep:          van|Gennep (given Arnold)
  vangennep:trans:    van|Gennep (given Arnold)
  vangennep:related:  van|Gennep (given Arnold)
  brandt:             von|Brandt (given Ahasver)
  geer:               de|Geer   (given Ingrid)
```

A deliberately simple sort (`family | year | title`) orders the file sensibly:
`aksın`, `almendro`, `angenendt`, `aristotle:rhetoric`, `aristotle:anima`, `aristotle:physics`,
`aristotle:poetics`, `augustine`, ... Entries with neither author nor editor (`set`,
`stdmodel`, `jcg`) sort on an empty key and collect at the end, which is correct for a
placeholder key but is a case the real sort must decide explicitly.

## Verdict: VALIDATED

### What worked
- `Person` implements the BibTeX name grammar and biblatex's extended `given=..., family=...`
  form. Particles split correctly on all five real cases, including `van Gennep` written
  with a lowercase particle and `von Brandt`.
- LaTeX escapes survive intact into the sort input (`\mkbibquote{...}` appears in the title
  used by the key), and `ChunksExt::format_verbatim()` gives the raw text without formatting.
- 171 people parsed with zero empty family names, so there is no fallback path needed for
  this corpus.

### What didn't
- The library gives names, not **sort keys**. There is no `sortname`, no collation, no
  `uniquename`. Sorting biblatex's way, with unicode collation and the `nyt`/`nty`
  templates, is ours to write. That is the product, not a gap in the dependency.

### Surprises
- Sorting on the raw family name puts `aksın` first (Turkish dotless ı). biber collates with
  ICU/locale, so the real implementation needs a collator (`icu_collator` or `unicode-collation`)
  rather than `Ord`. This is the first place the two implementations will visibly differ.
- `sorttitle` is not in the input at all; biber computes it. See the oracle finding in
  `../../SCOPE.md`: 42 of 92 entries differ from their input text for that reason alone.

### Recommendation for the real build
- Use `Person` and `ChunksExt` from `biblatex`; add a collator crate for ordering.
- Write the sorting templates (`nyt`, `nty`, `ynt`) as data, not code, so a missing template
  is a config error rather than a silent wrong order.
- Test order against biber on this same 92-entry file before writing any formatting code.
