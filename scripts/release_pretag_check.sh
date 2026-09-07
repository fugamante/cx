#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

MAX_VERSION_AGE_DAYS="${XSHELF_RELEASE_MAX_VERSION_AGE_DAYS:-14}"

python3 rust/cxrs/tools/release_check.py \
  --repo-root "$ROOT_DIR" \
  --max-version-age-days "$MAX_VERSION_AGE_DAYS" \
  --require-current-release-notes \
  --require-current-release-source \
  --require-published-status-docs
