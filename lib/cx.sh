#!/usr/bin/env bash

# Entry point for cx utilities (sourceable, idempotent).
if [[ -n "${CX_LIB_ENTRY_LOADED:-}" ]] && declare -F _cx_log_file >/dev/null 2>&1 && declare -F cx >/dev/null 2>&1; then
  return 0
fi
CX_LIB_ENTRY_LOADED=1

_CX_LIB_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# shellcheck disable=SC1091
source "$_CX_LIB_DIR/cx/core.sh"
# shellcheck disable=SC1091
source "$_CX_LIB_DIR/cx/commands.sh"
