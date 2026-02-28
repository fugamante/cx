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

ensure_seed_runlog() {
  local runlog="$REPO_ROOT/.codex/cxlogs/runs.jsonl"
  if [[ -s "$runlog" ]]; then
    return 0
  fi
  mkdir -p "$(dirname "$runlog")"
  cat > "$runlog" <<'JSONL'
{"execution_id":"seed_1","timestamp":"2026-01-01T00:00:00Z","ts":"2026-01-01T00:00:00Z","command":"cxo","tool":"cxo","cwd":".","duration_ms":1200,"input_tokens":1000,"cached_input_tokens":200,"effective_input_tokens":800,"output_tokens":120,"scope":"repo","repo_root":".","backend_used":"codex","capture_provider":"native","execution_mode":"lean","schema_enforced":false,"schema_valid":true}
{"execution_id":"seed_2","timestamp":"2026-01-01T00:01:00Z","ts":"2026-01-01T00:01:00Z","command":"cxcommitjson","tool":"cxcommitjson","cwd":".","duration_ms":1800,"input_tokens":1300,"cached_input_tokens":400,"effective_input_tokens":900,"output_tokens":160,"scope":"repo","repo_root":".","backend_used":"codex","capture_provider":"native","execution_mode":"deterministic","schema_enforced":true,"schema_valid":true}
{"execution_id":"seed_3","timestamp":"2026-01-01T00:02:00Z","ts":"2026-01-01T00:02:00Z","command":"cxdiffsum_staged","tool":"cxdiffsum_staged","cwd":".","duration_ms":2200,"input_tokens":1600,"cached_input_tokens":600,"effective_input_tokens":1000,"output_tokens":200,"scope":"repo","repo_root":".","backend_used":"codex","capture_provider":"native","execution_mode":"deterministic","schema_enforced":true,"schema_valid":true}
JSONL
}

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

extract_trace_json() {
  local src="$1"
  awk '
    BEGIN {
      ts="n/a"; tool="n/a"; cwd="n/a"; dur="n/a";
      in_tok="n/a"; cached="n/a"; eff="n/a"; out="n/a";
      scope="n/a"; repo="n/a"; sha="n/a";
    }
    /^ts: / { sub(/^ts: /, "", $0); ts=$0 }
    /^tool: / { sub(/^tool: /, "", $0); tool=$0 }
    /^cwd: / { sub(/^cwd: /, "", $0); cwd=$0 }
    /^duration_ms: / { sub(/^duration_ms: /, "", $0); gsub(/ms$/, "", $0); dur=$0 }
    /^input_tokens: / { sub(/^input_tokens: /, "", $0); in_tok=$0 }
    /^cached_input_tokens: / { sub(/^cached_input_tokens: /, "", $0); cached=$0 }
    /^effective_input_tokens: / { sub(/^effective_input_tokens: /, "", $0); eff=$0 }
    /^output_tokens: / { sub(/^output_tokens: /, "", $0); out=$0 }
    /^scope: / { sub(/^scope: /, "", $0); scope=$0 }
    /^repo_root: / { sub(/^repo_root: /, "", $0); repo=$0 }
    /^prompt_sha256: / { sub(/^prompt_sha256: /, "", $0); sha=$0 }
    END {
      printf("{\"ts\":\"%s\",\"tool\":\"%s\",\"cwd\":\"%s\",\"duration_ms\":\"%s\",\"input_tokens\":\"%s\",\"cached_input_tokens\":\"%s\",\"effective_input_tokens\":\"%s\",\"output_tokens\":\"%s\",\"scope\":\"%s\",\"repo_root\":\"%s\",\"prompt_sha256\":\"%s\"}\n",
        ts, tool, cwd, dur, in_tok, cached, eff, out, scope, repo, sha)
    }
  ' "$src" | jq -S .
}

extract_alert_json() {
  local src="$1"
  awk '
    function trim(s){ sub(/^[[:space:]]+/, "", s); sub(/[[:space:]]+$/, "", s); return s }
    BEGIN { section=""; runs="0"; slow="0"; eff="0"; cache="n/a" }
    /^Runs analyzed: / { sub(/^Runs analyzed: /, "", $0); runs=trim($0) }
    /^Runs: / { sub(/^Runs: /, "", $0); runs=trim($0) }
    /^Slow threshold violations/ { sub(/^.*: /, "", $0); slow=trim($0) }
    /^Effective token violations/ { sub(/^.*: /, "", $0); eff=trim($0) }
    /^Token threshold violations/ { sub(/^.*: /, "", $0); eff=trim($0) }
    /^Average cache hit rate: / { sub(/^Average cache hit rate: /, "", $0); cache=trim($0) }
    /^Avg cache hit rate: / { sub(/^Avg cache hit rate: /, "", $0); cache=trim($0) }
    /^Top 5 slowest/ { section="slow"; next }
    /^Top 5 heaviest/ { section="heavy"; next }
    /^Top tools by effective token cost/ { section="other"; next }
    /^log_file: / { section=""; next }
    /^- / {
      raw=substr($0, 3)
      n=split(raw, a, /[[:space:]]\|[[:space:]]/)
      if (section == "slow" && n >= 3) {
        if (a[1] ~ /ms$/) {
          dur=a[1]; sub(/ms$/, "", dur); tool=a[2]; ts=a[3]
        } else {
          tool=a[1]; dur=a[2]; sub(/ms$/, "", dur); ts=a[3]
        }
        print "SLOW\t" tool "\t" dur "\t" ts
      } else if (section == "heavy" && n >= 3) {
        if (a[1] ~ /^[0-9]+/) {
          split(a[1], p, /[[:space:]]+/); effv=p[1]; tool=a[2]; ts=a[3]
        } else {
          tool=a[1]; split(a[2], p, /[[:space:]]+/); effv=p[1]; ts=a[3]
        }
        print "HEAVY\t" tool "\t" effv "\t" ts
      }
    }
    END {
      print "META\t" runs "\t" slow "\t" eff "\t" cache
    }
  ' "$src" > "$tmpdir/alert_parse.tsv"

  local runs slow eff cache
  runs="$(awk -F '\t' '$1=="META"{print $2}' "$tmpdir/alert_parse.tsv" | tail -n1)"
  slow="$(awk -F '\t' '$1=="META"{print $3}' "$tmpdir/alert_parse.tsv" | tail -n1)"
  eff="$(awk -F '\t' '$1=="META"{print $4}' "$tmpdir/alert_parse.tsv" | tail -n1)"
  cache="$(awk -F '\t' '$1=="META"{print $5}' "$tmpdir/alert_parse.tsv" | tail -n1)"
  awk -F '\t' '$1=="SLOW"{print $3}' "$tmpdir/alert_parse.tsv" \
    | sort -nr \
    | jq -Rsc 'split("\n") | map(select(length>0))' > "$tmpdir/alert_slow.json"
  awk -F '\t' '$1=="HEAVY"{print $3}' "$tmpdir/alert_parse.tsv" \
    | sort -nr \
    | jq -Rsc 'split("\n") | map(select(length>0))' > "$tmpdir/alert_heavy.json"
  jq -n -S \
    --arg runs "${runs:-0}" \
    --arg slow_violations "${slow:-0}" \
    --arg eff_violations "${eff:-0}" \
    --arg cache_hit_rate "${cache:-n/a}" \
    --slurpfile slow_values "$tmpdir/alert_slow.json" \
    --slurpfile heavy_values "$tmpdir/alert_heavy.json" \
    '{runs:$runs,slow_violations:$slow_violations,eff_violations:$eff_violations,cache_hit_rate:$cache_hit_rate,slowest_duration_values:($slow_values[0] // []),heaviest_effective_values:($heavy_values[0] // [])}'
}

extract_worklog_json() {
  local src="$1"
  awk '
    BEGIN { by=0; tl=0; runs=""; }
    /^- Runs analyzed: / { sub(/^- Runs analyzed: /, "", $0); runs=$0 }
    /^##+[[:space:]]+By Tool/ { by=1; tl=0; next }
    /^##+[[:space:]]+Chronological Runs/ { by=0; tl=1; next }
    /^\| Tool \| Runs \| Avg Duration \(ms\) \| Avg Effective Tokens \|/ { next }
    /^\|---\|---:\|---:\|---:\|/ { next }
    /^\| / && by==1 {
      line=$0
      gsub(/^\| /, "", line)
      gsub(/ \|$/, "", line)
      n=split(line, a, / \| /)
      if (n == 4) print "TOOL\t" a[1] "\t" a[2] "\t" a[3] "\t" a[4]
      next
    }
    /^- / && tl==1 {
      line=substr($0, 3)
      if (line ~ /^`/) {
        gsub(/`/, "", line)
        split(line, a, /[[:space:]]+/)
        ts=a[1]; tool=a[2]
        dur="0"; eff="0"
        for (i=3; i<=NF; i++) {
          if (a[i] ~ /^duration=/) { dur=a[i]; sub(/^duration=/, "", dur); gsub(/ms$/, "", dur) }
          if (a[i] ~ /^eff=/) { eff=a[i]; sub(/^eff=/, "", eff) }
        }
        if (dur == "n/a") dur="0"
        if (eff == "n/a") eff="0"
        print "TIME\t" ts "\t" tool "\t" dur "\t" eff
      } else {
        n=split(line, a, /[[:space:]]\|[[:space:]]/)
        if (n >= 4) {
          ts=a[1]; tool=a[2]; dur=a[3]; gsub(/ms$/, "", dur)
          eff=a[4]; sub(/[[:space:]].*$/, "", eff)
          if (dur == "n/a") dur="0"
          if (eff == "n/a") eff="0"
          print "TIME\t" ts "\t" tool "\t" dur "\t" eff
        }
      }
    }
  ' "$src" > "$tmpdir/worklog_parse.tsv"

  local runs
  runs="$(awk -F '\t' '$1=="TIME"{c++} END{print c+0}' "$tmpdir/worklog_parse.tsv")"
  awk -F '\t' '$1=="TOOL"{print "{\"tool\":\""$2"\",\"runs\":\""$3"\",\"avg_duration_ms\":\""$4"\",\"avg_effective_tokens\":\""$5"\"}"}' "$tmpdir/worklog_parse.tsv" \
    | jq -s 'sort_by(.tool)' > "$tmpdir/worklog_tools.json"
  awk -F '\t' '$1=="TIME"{print "{\"ts\":\""$2"\",\"tool\":\""$3"\",\"duration_ms\":\""$4"\",\"effective_input_tokens\":\""$5"\"}"}' "$tmpdir/worklog_parse.tsv" \
    | jq -s 'sort_by(.ts, .tool, (.duration_ms|tostring))' > "$tmpdir/worklog_timeline.json"
  jq -n -S \
    --arg runs "${runs:-0}" \
    --slurpfile by_tool "$tmpdir/worklog_tools.json" \
    --slurpfile timeline "$tmpdir/worklog_timeline.json" \
    '{runs:$runs,by_tool:($by_tool[0] // []),timeline:($timeline[0] // [])}'
}

norm_metrics() {
  jq -S '
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
  '
}

export CXALERT_MAX_MS="${CXALERT_MAX_MS:-8000}"
export CXALERT_MAX_EFF_IN="${CXALERT_MAX_EFF_IN:-5000}"

ensure_seed_runlog

echo "[compat] comparing metrics (N=$N)"
run_bash_cx "cxmetrics $N" | norm_metrics > "$tmpdir/bash_metrics.json"
cargo run --quiet --manifest-path "$CXRS_DIR/Cargo.toml" -- metrics "$N" | norm_metrics > "$tmpdir/rust_metrics.json"

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

echo "[compat] comparing trace (N=1)"
run_bash_cx "cxtrace 1" > "$tmpdir/bash_trace.txt"
cargo run --quiet --manifest-path "$CXRS_DIR/Cargo.toml" -- trace 1 > "$tmpdir/rust_trace.txt"
extract_trace_json "$tmpdir/bash_trace.txt" > "$tmpdir/bash_trace.json"
extract_trace_json "$tmpdir/rust_trace.txt" > "$tmpdir/rust_trace.json"
if ! diff -u "$tmpdir/bash_trace.json" "$tmpdir/rust_trace.json"; then
  echo "[compat] trace mismatch" >&2
  exit 1
fi

echo "[compat] comparing alert (N=$N)"
run_bash_cx "cxalert $N" > "$tmpdir/bash_alert.txt"
cargo run --quiet --manifest-path "$CXRS_DIR/Cargo.toml" -- alert "$N" > "$tmpdir/rust_alert.txt"
extract_alert_json "$tmpdir/bash_alert.txt" > "$tmpdir/bash_alert.json"
extract_alert_json "$tmpdir/rust_alert.txt" > "$tmpdir/rust_alert.json"
if ! diff -u "$tmpdir/bash_alert.json" "$tmpdir/rust_alert.json"; then
  echo "[compat] alert mismatch" >&2
  exit 1
fi

echo "[compat] comparing worklog (N=$N)"
run_bash_cx "cxworklog $N" > "$tmpdir/bash_worklog.txt"
cargo run --quiet --manifest-path "$CXRS_DIR/Cargo.toml" -- worklog "$N" > "$tmpdir/rust_worklog.txt"
extract_worklog_json "$tmpdir/bash_worklog.txt" > "$tmpdir/bash_worklog.json"
extract_worklog_json "$tmpdir/rust_worklog.txt" > "$tmpdir/rust_worklog.json"
if ! diff -u "$tmpdir/bash_worklog.json" "$tmpdir/rust_worklog.json"; then
  echo "[compat] worklog mismatch" >&2
  exit 1
fi

echo "[compat] PASS: metrics/profile/trace/alert/worklog match for N=$N"
