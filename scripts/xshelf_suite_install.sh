#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
install_bin="${XSHELF_INSTALL_BIN:-$HOME/.local/bin}"
cx_ops_repo="${CX_OPS_REPO:-}"
launcher_path="${XSHELF_LAUNCHER_PATH:-$HOME/Desktop/XSHELF.command}"
install_launcher=1
install_shell=0

usage() {
  cat >&2 <<'USAGE'
Usage: xshelf-suite-install [--cx-ops-repo PATH] [--bin-dir PATH] [--launcher PATH]
                            [--no-launcher] [--shell]

Installs the local XSHELF runtime command surface plus the cxops UI/server
companion from a sibling cx-eval-lab checkout.
USAGE
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --cx-ops-repo)
      cx_ops_repo="${2:?missing --cx-ops-repo value}"
      shift 2
      ;;
    --bin-dir)
      install_bin="${2:?missing --bin-dir value}"
      shift 2
      ;;
    --launcher)
      launcher_path="${2:?missing --launcher value}"
      shift 2
      ;;
    --no-launcher)
      install_launcher=0
      shift
      ;;
    --shell)
      install_shell=1
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "xshelf-suite-install: unknown argument: $1" >&2
      usage
      exit 2
      ;;
  esac
done

display_path() {
  case "$1" in
    "$HOME"/*) printf '~/%s' "${1#"$HOME"/}" ;;
    "$HOME") printf '~' ;;
    *) printf '%s' "$1" ;;
  esac
}

resolve_cx_ops_repo() {
  if [[ -n "$cx_ops_repo" ]]; then
    [[ -d "$cx_ops_repo" ]] || {
      echo "xshelf-suite-install: cx-ops repo not found: $cx_ops_repo" >&2
      exit 1
    }
    return 0
  fi

  local parent
  parent="$(cd "$repo_root/.." && pwd)"
  for candidate in "$parent/cx-eval-lab" "$parent/cx-ops"; do
    if [[ -x "$candidate/scripts/install_cxops.sh" ]]; then
      cx_ops_repo="$candidate"
      return 0
    fi
  done

  echo "xshelf-suite-install: could not find cx-eval-lab/cx-ops next to $(display_path "$repo_root")" >&2
  echo "xshelf-suite-install: pass --cx-ops-repo PATH or set CX_OPS_REPO" >&2
  exit 1
}

install_xshelf_links() {
  mkdir -p "$install_bin"
  ln -sf "$repo_root/bin/xshelf" "$install_bin/xshelf"
  ln -sf "$repo_root/bin/xs" "$install_bin/xs"
  ln -sf "$repo_root/bin/cx" "$install_bin/cx"
  echo "xshelf-suite-install: installed XSHELF links into $(display_path "$install_bin")"
}

install_optional_shell() {
  if [[ "$install_shell" == "1" ]]; then
    "$repo_root/bin/xshelf-install"
  else
    echo "xshelf-suite-install: skipped shell profile edits; pass --shell to install shell functions"
  fi
}

install_cxops() {
  resolve_cx_ops_repo
  echo "xshelf-suite-install: installing cxops from $(display_path "$cx_ops_repo")"
  (cd "$cx_ops_repo" && ./scripts/install_cxops.sh)
}

create_launcher() {
  [[ "$install_launcher" == "1" ]] || {
    echo "xshelf-suite-install: skipped launcher creation"
    return 0
  }
  mkdir -p "$(dirname "$launcher_path")"
  cat > "$launcher_path" <<EOF
#!/usr/bin/env bash
set -euo pipefail
export PATH="$install_bin:\$HOME/.cargo/bin:\$PATH"
exec "$install_bin/xshelf" launch
EOF
  chmod +x "$launcher_path"
  echo "xshelf-suite-install: launcher ready at $(display_path "$launcher_path")"
}

verify_install() {
  "$install_bin/xshelf" version >/dev/null
  if command -v cxops >/dev/null 2>&1; then
    cxops version >/dev/null
  elif [[ -x "$HOME/.cargo/bin/cxops" ]]; then
    "$HOME/.cargo/bin/cxops" version >/dev/null
  else
    echo "xshelf-suite-install: warning: cxops installed but not found in PATH" >&2
    echo "xshelf-suite-install: add \$HOME/.cargo/bin to PATH before using xshelf launch" >&2
  fi
}

install_xshelf_links
install_optional_shell
install_cxops
create_launcher
verify_install

cat <<EOF
xshelf-suite-install: done
  launch: $install_bin/xshelf launch
EOF
if [[ "$install_launcher" == "1" ]]; then
  echo "  button: $(display_path "$launcher_path")"
fi
