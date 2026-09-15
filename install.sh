#!/usr/bin/env bash

set -Eeuo pipefail

readonly APP_NAME="viewlume"
readonly SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
readonly MANIFEST_PATH="${SCRIPT_DIR}/Cargo.toml"

usage() {
    cat <<'EOF'
Build and install Viewlume for the current user.

Usage:
  ./install.sh [--install-dir DIRECTORY]
  ./install.sh --help

Options:
  --install-dir DIRECTORY  Destination for the executable.
                           Default: $VIEWLUME_INSTALL_DIR or ~/.local/bin
  -h, --help               Show this help message.

Environment:
  VIEWLUME_INSTALL_DIR     Default installation directory.
  CARGO_TARGET_DIR         Cargo build directory.
EOF
}

fail() {
    printf 'error: %s\n' "$*" >&2
    exit 1
}

if [[ -n "${VIEWLUME_INSTALL_DIR:-}" ]]; then
    install_dir="${VIEWLUME_INSTALL_DIR}"
elif [[ -n "${HOME:-}" ]]; then
    install_dir="${HOME}/.local/bin"
else
    fail 'HOME is not set; use --install-dir or VIEWLUME_INSTALL_DIR'
fi

while (($# > 0)); do
    case "$1" in
        --install-dir)
            (($# >= 2)) || fail '--install-dir requires a directory'
            install_dir="$2"
            shift 2
            ;;
        -h | --help)
            usage
            exit 0
            ;;
        *)
            fail "unknown argument: $1"
            ;;
    esac
done

[[ -n "${install_dir}" ]] || fail 'installation directory cannot be empty'
[[ -f "${MANIFEST_PATH}" ]] || fail "Cargo.toml not found at ${MANIFEST_PATH}"
command -v cargo >/dev/null 2>&1 || fail 'cargo was not found in PATH'
command -v install >/dev/null 2>&1 || fail 'the install utility was not found in PATH'

build_dir="${CARGO_TARGET_DIR:-${SCRIPT_DIR}/target}"
binary_path="${build_dir}/release/${APP_NAME}"
destination="${install_dir}/${APP_NAME}"

printf 'Building %s in release mode...\n' "${APP_NAME}"
cargo build --manifest-path "${MANIFEST_PATH}" --target-dir "${build_dir}" --release --locked

[[ -f "${binary_path}" ]] || fail "build succeeded but ${binary_path} was not created"

install -d -- "${install_dir}"
install -m 0755 -- "${binary_path}" "${destination}"

printf 'Installed %s to %s\n' "${APP_NAME}" "${destination}"

case ":${PATH:-}:" in
    *":${install_dir}:"*) ;;
    *)
        printf 'Note: %s is not in PATH. Add it to your shell profile.\n' "${install_dir}"
        ;;
esac
