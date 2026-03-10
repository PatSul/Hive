#!/bin/bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

case "$(uname -s)" in
  Linux|Darwin)
    cd "${ROOT_DIR}"
    cargo "$@"
    ;;
  MINGW*|MSYS*|CYGWIN*)
    # Windows Git Bash/MSYS wrapper that injects the local MSVC environment.
    MSVC_BIN="C:\\Program Files\\Microsoft Visual Studio\\2022\\Enterprise\\VC\\Tools\\MSVC\\14.44.35207"
    SCOPE_SDK="C:\\Program Files\\Microsoft Visual Studio\\2022\\Enterprise\\SDK\\ScopeCppSDK\\vc15\\VC"
    SDK_ROOT="C:\\Program Files (x86)\\Windows Kits\\10"
    SDK_VER="10.0.22621.0"

    export LIB="${SCOPE_SDK}\\lib;${SDK_ROOT}\\Lib\\${SDK_VER}\\um\\x64;${SDK_ROOT}\\Lib\\${SDK_VER}\\ucrt\\x64"
    export INCLUDE="${SCOPE_SDK}\\include;${SDK_ROOT}\\Include\\${SDK_VER}\\ucrt;${SDK_ROOT}\\Include\\${SDK_VER}\\um;${SDK_ROOT}\\Include\\${SDK_VER}\\shared"
    export PATH="${MSVC_BIN}\\bin\\Hostx64\\x64:${PATH}"

    cd "${ROOT_DIR}"
    /c/Users/pat/.cargo/bin/cargo.exe "$@"
    ;;
  *)
    echo "Unsupported shell platform: $(uname -s)" >&2
    exit 1
    ;;
esac
