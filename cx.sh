#!/usr/bin/env bash

# Compatibility loader. Prefer sourcing lib/cx.sh directly.
if [[ -n "${CX_CANONICAL_LOADED:-}" ]] && declare -F cxversion >/dev/null 2>&1; then
  return 0
fi
CX_CANONICAL_LOADED=1
export CX_SOURCE_LOCATION="repo:${BASH_SOURCE[0]}"

_cx_warn_deprecated_root_loader() {
  [[ "${CX_SUPPRESS_DEPRECATION_WARN:-0}" == "1" ]] && return 0
  [[ -n "${CX_ROOT_SH_WARNED:-}" ]] && return 0
  [[ -t 2 ]] || return 0
  CX_ROOT_SH_WARNED=1
  export CX_ROOT_SH_WARNED
  printf '%s\n' "cx: root loader 'cx.sh' is deprecated; prefer 'lib/cx.sh'." >&2
}

_cx_warn_deprecated_root_loader
unset -f _cx_warn_deprecated_root_loader

set +u
# shellcheck disable=SC1091
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib/cx.sh"
set +u
