#!/usr/bin/env bash
set -euo pipefail

N="${1:-50}"
if [[ ! "$N" =~ ^[0-9]+$ ]] || (( N <= 0 )); then
  echo "Usage: $(basename "$0") [positive_run_count]" >&2
  exit 2
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CXRS_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
REPO_ROOT="$(cd "$CXRS_DIR/../.." && pwd)"

require_bin() {
  command -v "$1" >/dev/null 2>&1 || {
    echo "Missing required binary: $1" >&2
    exit 1
  }
}

require_bin bash
require_bin jq
require_bin cargo
require_bin diff

tmpdir="$(mktemp -d)"
trap 'rm -rf "$tmpdir"' EXIT

run_bash_cx() {
  local cmd="$1"
  bash -lc "source \"$REPO_ROOT/cx.sh\" >/dev/null 2>&1; $cmd"
}

extract_profile_json() {
  local src="$1"
  awk '
    BEGIN {
      runs=""; avg_dur=""; avg_eff=""; cache=""; ratio="";
      slow=""; heavy="";
    }
    /^Runs: / { runs=$2 }
    /^Avg duration: / {
      gsub(/^Avg duration: /, "", $0); gsub(/ms$/, "", $0); avg_dur=$0
    }
    /^Avg effective tokens: / { avg_eff=$4 }
    /^Cache hit rate: / { cache=$4 }
    /^Output\/input ratio: / { ratio=$3 }
    /^Slowest run: / {
      gsub(/^Slowest run: /, "", $0); slow=$0
    }
    /^Heaviest context: / {
      gsub(/^Heaviest context: /, "", $0); heavy=$0
    }
    END {
      printf("{\"runs\":%s,\"avg_duration_ms\":%s,\"avg_effective_tokens\":%s,\"cache_hit_rate\":\"%s\",\"output_input_ratio\":\"%s\",\"slowest\":\"%s\",\"heaviest\":\"%s\"}\n",
        runs, avg_dur, avg_eff, cache, ratio, slow, heavy)
    }
  ' "$src" | jq -S .
}

echo "[compat] comparing metrics (N=$N)"
run_bash_cx "cxmetrics $N" \
  | jq -S '
      def ns:
        if . == null then null else
          (tonumber as $n | if ($n|floor) == $n then ($n|floor|tostring) else ($n|tostring) end)
        end;
      .runs |= ns
      | .avg_duration_ms |= ns
      | .avg_input_tokens |= ns
      | .avg_cached_input_tokens |= ns
      | .avg_effective_input_tokens |= ns
      | .avg_output_tokens |= ns
      | .by_tool |= (map({
        tool: (.tool // "unknown"),
        runs: (.runs | ns),
        avg_duration_ms: (.avg_duration_ms | ns),
        avg_effective_input_tokens: (.avg_effective_input_tokens | ns),
        avg_output_tokens: (.avg_output_tokens | ns)
      }) | sort_by(.tool))
    ' > "$tmpdir/bash_metrics.json"

cargo run --quiet --manifest-path "$CXRS_DIR/Cargo.toml" -- metrics "$N" \
  | jq -S '
      def ns:
        if . == null then null else
          (tonumber as $n | if ($n|floor) == $n then ($n|floor|tostring) else ($n|tostring) end)
        end;
      .runs |= ns
      | .avg_duration_ms |= ns
      | .avg_input_tokens |= ns
      | .avg_cached_input_tokens |= ns
      | .avg_effective_input_tokens |= ns
      | .avg_output_tokens |= ns
      | .by_tool |= (map({
        tool: (.tool // "unknown"),
        runs: (.runs | ns),
        avg_duration_ms: (.avg_duration_ms | ns),
        avg_effective_input_tokens: (.avg_effective_input_tokens | ns),
        avg_output_tokens: (.avg_output_tokens | ns)
      }) | sort_by(.tool))
    ' > "$tmpdir/rust_metrics.json"

if ! diff -u "$tmpdir/bash_metrics.json" "$tmpdir/rust_metrics.json"; then
  echo "[compat] metrics mismatch" >&2
  exit 1
fi

echo "[compat] comparing profile (N=$N)"
run_bash_cx "cxprofile $N" > "$tmpdir/bash_profile.txt"
cargo run --quiet --manifest-path "$CXRS_DIR/Cargo.toml" -- profile "$N" \
  | sed '/^== cxrs profile/d;/^log_file:/d;/^$/d' > "$tmpdir/rust_profile.txt"

extract_profile_json "$tmpdir/bash_profile.txt" > "$tmpdir/bash_profile.json"
extract_profile_json "$tmpdir/rust_profile.txt" > "$tmpdir/rust_profile.json"

if ! diff -u "$tmpdir/bash_profile.json" "$tmpdir/rust_profile.json"; then
  echo "[compat] profile mismatch" >&2
  exit 1
fi

echo "[compat] PASS: metrics/profile match for N=$N"
