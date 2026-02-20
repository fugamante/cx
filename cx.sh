#!/usr/bin/env bash

# Canonical cx entrypoint for sourcing from shell profiles.
if [[ -n "${CX_CANONICAL_LOADED:-}" ]]; then
  return 0
fi
export CX_CANONICAL_LOADED=1
export CX_SOURCE_LOCATION="repo:${BASH_SOURCE[0]}"

set +u
# shellcheck disable=SC1091
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib/cx.sh"
set +u
