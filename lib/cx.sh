#!/usr/bin/env bash

# Rust-only shell shim (sourceable, idempotent).
if [[ -n "${CX_LIB_ENTRY_LOADED:-}" ]] && declare -F cx >/dev/null 2>&1; then
  return 0
fi
CX_LIB_ENTRY_LOADED=1

_CX_LIB_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
_CX_REPO_ROOT="$(cd "$_CX_LIB_DIR/.." && pwd)"
_CX_BIN="$_CX_REPO_ROOT/bin/cx"

cx() {
  "$_CX_BIN" "$@"
}

xshelf() {
  "$_CX_BIN" "$@"
}

cxreload() {
  # shellcheck disable=SC1090
  source "$HOME/.bashrc"
}

cxversion() {
  "$_CX_BIN" version
}
