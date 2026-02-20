#!/usr/bin/env bash

# Canonical cx entrypoint for sourcing from shell profiles.
# shellcheck disable=SC1091
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib/cx.sh"
