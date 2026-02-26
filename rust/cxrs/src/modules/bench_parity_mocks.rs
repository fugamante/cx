use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn write_executable(path: &Path, content: &str) -> Result<(), String> {
    fs::write(path, content).map_err(|e| format!("cxparity: write {}: {e}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(path)
            .map_err(|e| format!("cxparity: metadata {}: {e}", path.display()))?
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(path, perms)
            .map_err(|e| format!("cxparity: chmod {}: {e}", path.display()))?;
    }
    Ok(())
}

pub fn setup_parity_mocks(repo: &Path, temp_repo: &Path) -> Result<PathBuf, String> {
    let mock_dir = temp_repo.join(".codex").join("mockbin");
    fs::create_dir_all(&mock_dir)
        .map_err(|e| format!("cxparity: create {}: {e}", mock_dir.display()))?;
    write_parity_mock_bins(&mock_dir)?;
    copy_schema_registry(repo, temp_repo)?;
    Ok(mock_dir)
}

fn write_parity_mock_bins(mock_dir: &Path) -> Result<(), String> {
    let codex = r#"#!/usr/bin/env bash
set -euo pipefail
if [[ "${1:-}" != "exec" ]]; then exit 2; fi
if [[ "${2:-}" == "--json" && "${3:-}" == "-" ]]; then
  prompt="$(cat)"
  if [[ "$prompt" == *"Generate a commit object from this STAGED diff."* ]]; then
    cat <<'JSON'
{"type":"item.completed","item":{"type":"agent_message","text":"{\"subject\":\"feat: parity commit\",\"body\":[\"align rust and bash parity\"],\"breaking\":false,\"scope\":null,\"tests\":[\"cargo test -q\"]}"}}
{"type":"turn.completed","usage":{"input_tokens":64,"cached_input_tokens":8,"output_tokens":12}}
JSON
  elif [[ "$prompt" == *"Write a PR-ready summary of this diff."* ]]; then
    cat <<'JSON'
{"type":"item.completed","item":{"type":"agent_message","text":"{\"title\":\"Parity test diff summary\",\"summary\":[\"staged file prepared for parity\"],\"risk_edge_cases\":[\"none identified\"],\"suggested_tests\":[\"cargo test -q\"]}"}}
{"type":"turn.completed","usage":{"input_tokens":64,"cached_input_tokens":8,"output_tokens":12}}
JSON
  elif [[ "$prompt" == *"Based on the terminal command output below, propose the NEXT shell commands to run."* ]]; then
    cat <<'JSON'
{"type":"item.completed","item":{"type":"agent_message","text":"{\"commands\":[\"git status --short\",\"cargo test -q\"]}"}}
{"type":"turn.completed","usage":{"input_tokens":64,"cached_input_tokens":8,"output_tokens":12}}
JSON
  else
    cat <<'JSON'
{"type":"item.completed","item":{"type":"agent_message","text":"parity ok"}}
{"type":"turn.completed","usage":{"input_tokens":64,"cached_input_tokens":8,"output_tokens":12}}
JSON
  fi
  exit 0
fi
if [[ "${2:-}" == "-" ]]; then
  cat >/dev/null
  printf '%s\n' "parity plain output"
  exit 0
fi
exit 2
"#;
    let pbcopy = r#"#!/usr/bin/env bash
cat >/dev/null
exit 0
"#;
    write_executable(&mock_dir.join("codex"), codex)?;
    write_executable(&mock_dir.join("pbcopy"), pbcopy)
}

fn copy_schema_registry(repo: &Path, temp_repo: &Path) -> Result<(), String> {
    let src_schema = repo.join(".codex").join("schemas");
    let dst_schema = temp_repo.join(".codex").join("schemas");
    fs::create_dir_all(&dst_schema)
        .map_err(|e| format!("cxparity: create {}: {e}", dst_schema.display()))?;
    for ent in fs::read_dir(&src_schema)
        .map_err(|e| format!("cxparity: read {}: {e}", src_schema.display()))?
    {
        let ent = ent.map_err(|e| format!("cxparity: schema entry error: {e}"))?;
        let p = ent.path();
        if p.extension().and_then(|v| v.to_str()) != Some("json") {
            continue;
        }
        let fname = p
            .file_name()
            .ok_or_else(|| format!("cxparity: bad schema path {}", p.display()))?;
        fs::copy(&p, dst_schema.join(fname))
            .map_err(|e| format!("cxparity: copy {}: {e}", p.display()))?;
    }
    Ok(())
}

pub fn with_parity_env(cmd: &mut Command, mock_dir: &Path, temp_repo: &Path) {
    let path = std::env::var("PATH").unwrap_or_default();
    let prefixed = format!("{}:{}", mock_dir.display(), path);
    cmd.current_dir(temp_repo)
        .env("PATH", prefixed)
        .env("CX_CAPTURE_PROVIDER", "native")
        .env("CX_RTK_SYSTEM", "0")
        .env("CX_NATIVE_REDUCE", "0");
}
