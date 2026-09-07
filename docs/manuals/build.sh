#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/../.." && pwd)"

manual_src="$repo_root/docs/manuals/03_src/latex/CX_MANUAL_MASTER.tex"
build_dir="$repo_root/docs/manuals/99_build/latexmk"
pdf_out="$repo_root/docs/manuals/01_pdf/CX_MANUAL_MASTER.pdf"
build_log="$build_dir/CX_MANUAL_MASTER.log"

if [[ -x /Library/TeX/texbin/latexmk ]]; then
  latexmk_bin="/Library/TeX/texbin/latexmk"
elif command -v latexmk >/dev/null 2>&1; then
  latexmk_bin="$(command -v latexmk)"
else
  echo "manual build: latexmk not found" >&2
  exit 127
fi

mkdir -p "$build_dir" "$(dirname "$pdf_out")"

"$latexmk_bin" \
  -xelatex \
  -interaction=nonstopmode \
  -halt-on-error \
  -file-line-error \
  -output-directory="$build_dir" \
  "$manual_src"

cp -f "$build_dir/CX_MANUAL_MASTER.pdf" "$pdf_out"

if rg -n "Warning|Overfull|Underfull|Unable" "$build_log" >&2; then
  echo "manual build: warnings found in $build_log" >&2
  exit 1
fi

echo "manual build: refreshed $pdf_out" >&2
echo "manual build: HTML mirror is tracked separately and was not regenerated" >&2
