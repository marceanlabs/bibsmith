#!/bin/sh
# Regenerate the oracle fixtures from real biber.
#
# Everything here is a different implementation from the crate under test: TeX
# and biber write the expected order, bibsmith is compared against it. Run this
# when the corpus or the biblatex version changes, not on every build.
set -eu

BIN="${TEXBIN:-$HOME/Library/TinyTeX/bin/universal-darwin}"
HERE=$(cd "$(dirname "$0")/.." && pwd)
cd "$HERE/oracle"

for scheme in nyt nty ynt; do
    sed "s/sorting=nyt/sorting=$scheme/" nyt.tex > "$scheme.tex"
    "$BIN/pdflatex" -interaction=nonstopmode -halt-on-error "$scheme.tex" >/dev/null
    "$BIN/biber" "$scheme" >/dev/null
    "$BIN/pdflatex" -interaction=nonstopmode -halt-on-error "$scheme.tex" >/dev/null
    grep -o '^ *\\entry{[^}]*}' "$scheme.bbl" \
        | sed 's/^ *\\entry{//;s/}$//' > "order-$scheme.txt"
    printf '%s: %s entries\n' "$scheme" "$(wc -l < "order-$scheme.txt" | tr -d ' ')"
done

printf 'biber %s, biblatex %s\n' \
    "$("$BIN/biber" --version | sed -n 's/biber version: //p')" \
    "$("$BIN/tlmgr" info biblatex 2>/dev/null | sed -n 's/^installed:.*//;s/^cat-version: *//p' | head -1)"
