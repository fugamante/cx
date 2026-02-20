#!/usr/bin/env bash

# Entry point for cx utilities.

_CX_LIB_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# shellcheck disable=SC1091
source "$_CX_LIB_DIR/cx/core.sh"
# shellcheck disable=SC1091
source "$_CX_LIB_DIR/cx/commands.sh"
