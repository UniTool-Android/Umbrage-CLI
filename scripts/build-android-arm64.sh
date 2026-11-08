#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TARGET="aarch64-linux-android"

: "${ANDROID_NDK_HOME:?`ANDROID_NDK_HOME` must point to an installed Android NDK}"
: "${UNITOOL_LIBUSB_DIR:?`UNITOOL_LIBUSB_DIR` must contain libusb.so and libusb_bridge.so}"
: "${UNITOOL_LIBUSB_INCLUDE_DIR:?`UNITOOL_LIBUSB_INCLUDE_DIR` must contain libusb.h}"

LIBUSB_SO="${UNITOOL_LIBUSB_DIR}/libusb.so"
BRIDGE_SO="${UNITOOL_LIBUSB_DIR}/libusb_bridge.so"
HEADER="${UNITOOL_LIBUSB_INCLUDE_DIR}/libusb.h"

for file in "$LIBUSB_SO" "$BRIDGE_SO" "$HEADER"; do
    if [[ ! -f "$file" ]]; then
        echo "error: required UniTool USB runtime file not found: $file" >&2
        exit 1
    fi
done

HOST_TAG="linux-x86_64"
TOOLCHAIN="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/$HOST_TAG/bin"
if [[ ! -x "$TOOLCHAIN/aarch64-linux-android21-clang" ]]; then
    echo "error: Android NDK ARM64 clang not found under $TOOLCHAIN" >&2
    exit 1
fi

rustup target add "$TARGET"
TMP_PC="$(mktemp -d)"
trap 'rm -rf "$TMP_PC"' EXIT
mkdir -p "$TMP_PC/pkgconfig"

cat > "$TMP_PC/pkgconfig/libusb-1.0.pc" <<EOF
prefix=/
exec_prefix=\${prefix}
libdir=$UNITOOL_LIBUSB_DIR
includedir=$UNITOOL_LIBUSB_INCLUDE_DIR

Name: libusb-1.0
Description: UniTool Android broker-backed libusb
Version: 1.0.29
Libs: -L\${libdir} -lusb
Cflags: -I\${includedir}
EOF

export PKG_CONFIG_PATH="$TMP_PC/pkgconfig"
export PKG_CONFIG_ALLOW_CROSS=1
export PKG_CONFIG_LIBDIR="$TMP_PC/pkgconfig"
export CC_aarch64_linux_android="$TOOLCHAIN/aarch64-linux-android21-clang"
export AR_aarch64_linux_android="$TOOLCHAIN/llvm-ar"
export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$TOOLCHAIN/aarch64-linux-android21-clang"

cd "$ROOT"
cargo build --locked --release --target "$TARGET" --no-default-features --features android --lib --bins

BIN="$ROOT/target/$TARGET/release/umbrage-cli"
READELF="$TOOLCHAIN/llvm-readelf"
"$READELF" -d "$BIN" | grep -q 'Shared library: \[libusb.so\]' || {
    echo "error: CLI is not dynamically linked to UniTool's libusb.so" >&2
    exit 1
}
LIB="$ROOT/target/$TARGET/release/libumbrage_cli.so"
if [[ ! -s "$LIB" ]]; then
    echo "error: Android shared library was not produced: $LIB" >&2
    exit 1
fi
"$READELF" -d "$LIB" | grep -q 'Shared library: [libusb.so]' || {
    echo "error: JNI shared library is not dynamically linked to UniTool's libusb.so" >&2
    exit 1
}
"$READELF" -d "$LIBUSB_SO" | grep -q 'Shared library: \[libusb_bridge.so\]' || {
    echo "error: UniTool libusb.so does not declare libusb_bridge.so dependency" >&2
    exit 1
}

echo "Built: $BIN"
echo "Runtime dependencies:"
"$READELF" -d "$BIN" | grep NEEDED
echo "Install umbrage-cli, libusb.so and libusb_bridge.so in the app runtime's executable/native-library search paths."
