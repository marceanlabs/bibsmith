#!/bin/zsh
# Compare the typeset bibliography against biber's, not the source against biber's.
#
# The `.bbl` is a file LaTeX reads, so the only honest test is the printed page. This
# script builds the same document twice — once with bibsmith's `.bbl`, once with
# biber's — and diffs the text extracted from both PDFs.
#
# Requirements: a TeX installation with biblatex and biber, and Python with PyMuPDF
# (`python3 -m venv /tmp/pdfenv && /tmp/pdfenv/bin/pip install pymupdf`).
#
# Usage: scripts/check-pdf.sh [document.tex]   (default: oracle/nyt.tex)

set -e
cd "$(dirname "$0")/.."

BINSMITH=$(pwd)/target/release/bibsmith
DOC=${1:-oracle/nyt.tex}
DIR=$(dirname "$DOC")
BASE=$(basename "$DOC" .tex)
TEXBIN=${TEXBIN:-$HOME/Library/TinyTeX/bin/universal-darwin}
PY=${PY:-/tmp/pdfenv/bin/python}

for tool in "$TEXBIN/pdflatex" "$TEXBIN/biber" "$PY"; do
  if [[ ! -e $tool ]]; then
    print -u2 "missing: $tool"
    exit 1
  fi
done

cd "$DIR"
"$TEXBIN/pdflatex" -interaction=nonstopmode "$BASE.tex" >/dev/null 2>&1 || true

# What biber produces. It exits non-zero on this corpus because it warns about an entry
# it refuses, and it still writes the .bbl: the file, not the status, is the oracle.
"$TEXBIN/biber" --quiet "$BASE" >/dev/null 2>&1 || true
"$TEXBIN/pdflatex" -interaction=nonstopmode "$BASE.tex" >/dev/null 2>&1 || true
"$TEXBIN/pdflatex" -interaction=nonstopmode "$BASE.tex" >/dev/null 2>&1 || true
"$TEXBIN/pdflatex" -interaction=nonstopmode "$BASE.tex" >/dev/null 2>&1 || true
cp "$BASE.pdf" "$BASE.pdf.biber"

# What bibsmith produces.
"$BINSMITH" bbl "$BASE.bcf" -o "$BASE.bbl"
# Two passes: biblatex's ibidem tracker and cross-references settle on the second,
# and comparing after one pass reports differences that are only missing runs.
"$TEXBIN/pdflatex" -interaction=nonstopmode "$BASE.tex" >/tmp/bibsmith-pdflatex.log 2>&1 || true
"$TEXBIN/pdflatex" -interaction=nonstopmode "$BASE.tex" >>/tmp/bibsmith-pdflatex.log 2>&1 || true
# A name biber has already turned into Unicode can carry a combining accent, which this
# font encoding cannot set: biber's own `.bbl` raises exactly the same error and still
# produces the page, so that one is not a difference between the two files.
if grep -E '^! ' /tmp/bibsmith-pdflatex.log | grep -qv 'Unicode character'; then
  print -u2 "LaTeX errors with bibsmith's .bbl:"
  grep -E '^! ' -A3 /tmp/bibsmith-pdflatex.log | grep -v 'Unicode character'
  exit 1
fi
cp "$BASE.pdf" "$BASE.pdf.bibsmith"

"$PY" - "$BASE" <<'EOF'
import sys, difflib, fitz

base = sys.argv[1]
def text(path):
    return "\n".join(page.get_text() for page in fitz.open(path))

ours, biber = text(f"{base}.pdf.bibsmith"), text(f"{base}.pdf.biber")
diff = list(difflib.unified_diff(biber.splitlines(), ours.splitlines(),
                                 "biber", "bibsmith", lineterm="", n=0))
if not diff:
    print("identical: the typeset bibliography matches biber exactly")
    raise SystemExit(0)
print(f"{len(diff)} differing lines (of {len(ours.splitlines())} in the document)")
print("\n".join(diff[:60]))
raise SystemExit(1)
EOF
