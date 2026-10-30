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
- Not done yet: `sort` output is not normalised (fields are copied as written), there are
  no CLI contract tests, no README, no CI, nothing committed to git.

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
