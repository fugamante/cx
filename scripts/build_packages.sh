#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
out_dir="$repo_root/.cx/packages"
target_dir="$repo_root/.cx/package-target"
declare -a targets=()
allow_dirty=0

usage() {
  cat >&2 <<'USAGE'
usage: ./scripts/build_packages.sh [--target TRIPLE] [--out-dir PATH] [--allow-dirty]

Builds path-remapped macOS binaries and deterministic XSHELF archives.
Without --target, both aarch64-apple-darwin and x86_64-apple-darwin are required.
The command never installs a missing Rust target.
USAGE
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --target)
      targets+=("${2:?missing --target value}")
      shift 2
      ;;
    --out-dir)
      out_dir="${2:?missing --out-dir value}"
      shift 2
      ;;
    --allow-dirty)
      allow_dirty=1
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "build_packages: unknown argument: $1" >&2
      usage
      exit 2
      ;;
  esac
done

if [[ ${#targets[@]} -eq 0 ]]; then
  targets=(aarch64-apple-darwin x86_64-apple-darwin)
fi

for target in "${targets[@]}"; do
  case "$target" in
    aarch64-apple-darwin|x86_64-apple-darwin) ;;
    *)
      echo "build_packages: unsupported target: $target" >&2
      exit 2
      ;;
  esac
done

toolchain="$(python3 - "$repo_root/rust/cxrs/rust-toolchain.toml" <<'PY'
import pathlib, sys, tomllib
print(tomllib.loads(pathlib.Path(sys.argv[1]).read_text())["toolchain"]["channel"])
PY
)"
installed="$(rustup target list --toolchain "$toolchain" --installed)"
declare -a artifacts=()
cargo_bin="$(rustup which --toolchain "$toolchain" cargo)"
rustc_bin="$(rustup which --toolchain "$toolchain" rustc)"
if [[ ! -x "$cargo_bin" || ! -x "$rustc_bin" ]]; then
  echo "build_packages: pinned Cargo or rustc is not executable" >&2
  exit 2
fi
if [[ "$(dirname "$cargo_bin")" != "$(dirname "$rustc_bin")" ]]; then
  echo "build_packages: pinned Cargo and rustc resolved from different toolchains" >&2
  exit 2
fi
cargo_identity="$("$cargo_bin" --version)"
rust_identity="$("$rustc_bin" --version)"
cargo_release="${cargo_identity#cargo }"
cargo_release="${cargo_release%% *}"
rust_release="${rust_identity#rustc }"
rust_release="${rust_release%% *}"
if [[ "$cargo_release" != "$toolchain" || "$rust_release" != "$toolchain" ]]; then
  echo "build_packages: Cargo/rustc identity does not match pinned toolchain $toolchain" >&2
  echo "build_packages: cargo=$cargo_identity" >&2
  echo "build_packages: rustc=$rust_identity" >&2
  exit 2
fi
for target in "${targets[@]}"; do
  if ! grep -qx "$target" <<<"$installed"; then
    echo "build_packages: Rust target is not installed: $target" >&2
    echo "build_packages: install it explicitly, or build this target on a matching host" >&2
    exit 2
  fi
done

mkdir -p "$out_dir" "$target_dir"
home_prefix="${HOME%/}"
remap="--remap-path-prefix=$repo_root=/usr/src/xshelf --remap-path-prefix=$home_prefix=/usr/src/build-home"
if [[ -n "${RUSTFLAGS:-}" || -n "${RUSTC_WRAPPER:-}" || -n "${RUSTC_WORKSPACE_WRAPPER:-}" ]]; then
  echo "build_packages: inherited Rust flags or compiler wrappers are not allowed for release packaging" >&2
  exit 2
fi

source_before="$(python3 "$repo_root/scripts/package_release.py" source-state --repo-root "$repo_root")"

for target in "${targets[@]}"; do
  echo "build_packages: building $target" >&2
  CARGO_INCREMENTAL=0 \
  MACOSX_DEPLOYMENT_TARGET=11.0 \
  RUSTC="$rustc_bin" \
  RUSTC_WRAPPER= \
  RUSTC_WORKSPACE_WRAPPER= \
  RUSTFLAGS="$remap" \
    "$cargo_bin" build \
      --locked \
      --manifest-path "$repo_root/rust/cxrs/Cargo.toml" \
      --release \
      --target "$target" \
      --target-dir "$target_dir"

  source_after="$(python3 "$repo_root/scripts/package_release.py" source-state --repo-root "$repo_root")"
  if [[ "$source_after" != "$source_before" ]]; then
    echo "build_packages: release source changed during compilation" >&2
    exit 1
  fi

  binary="$target_dir/$target/release/cxrs"
  if strings "$binary" | grep -F "$repo_root" >/dev/null; then
    echo "build_packages: checkout path leaked into $binary" >&2
    exit 1
  fi
  if strings "$binary" | grep -F "$home_prefix" >/dev/null; then
    echo "build_packages: build-home path leaked into $binary" >&2
    exit 1
  fi

  package_args=(
    build
    --repo-root "$repo_root"
    --binary "$binary"
    --target "$target"
    --output-dir "$out_dir"
    --cargo-toolchain "$cargo_identity"
    --rust-toolchain "$rust_identity"
    --macos-min-version 11.0
  )
  if [[ "$allow_dirty" == "1" ]]; then
    package_args+=(--allow-dirty)
  fi
  python3 "$repo_root/scripts/package_release.py" "${package_args[@]}"
  source_after="$(python3 "$repo_root/scripts/package_release.py" source-state --repo-root "$repo_root")"
  if [[ "$source_after" != "$source_before" ]]; then
    echo "build_packages: release source changed during archive assembly" >&2
    exit 1
  fi
  artifacts+=("$out_dir/xshelf-$(tr -d '\n' < "$repo_root/VERSION")-$target.tar.gz")
done

python3 "$repo_root/scripts/package_release.py" summary \
  --output "$out_dir/SHA256SUMS" \
  "${artifacts[@]}"

echo "build_packages: artifacts ready in $out_dir"
