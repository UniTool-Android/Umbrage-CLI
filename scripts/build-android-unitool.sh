#!/usr/bin/env bash
set -euo pipefail

# Build Umbrage against UniTool's broker-backed libusb shared library.
if [[ $# -lt 1 || $# -gt 2 ]]; then
  echo "Usage: $0 <unitool-jniLibs/arm64-v8a> [output-directory]" >&2
  exit 2
fi

native_dir="$(cd "$1" && pwd)"
out_dir="${2:-dist/android}"
bridge_source="${UNITOOL_USB_BRIDGE_SOURCE:-}"
target="aarch64-linux-android"

[[ -s "$native_dir/libusb.so" ]] || { echo "Missing UniTool libusb.so in $native_dir" >&2; exit 1; }
readelf -d "$native_dir/libusb.so" | grep -F 'Shared library: [libusb_bridge.so]' >/dev/null || {
  echo "The supplied libusb.so is not linked to UniTool's libusb_bridge.so" >&2
  exit 1
}
if [[ ! -s "$native_dir/libusb_bridge.so" ]]; then
  [[ -n "$bridge_source" && -s "$bridge_source" ]] || {
    echo "Missing libusb_bridge.so; set UNITOOL_USB_BRIDGE_SOURCE to UniTool-core/app/src/main/cpp/usb_bridge.cpp" >&2
    exit 1
  }
  : "${CXX:?Set CXX to the Android NDK aarch64 clang++ driver}"
  "$CXX" --target=aarch64-linux-android24 -shared -fPIC -std=c++17 \
    "$bridge_source" -llog -o "$native_dir/libusb_bridge.so"
fi

pkg_dir="$(mktemp -d)"
trap 'rm -rf "$pkg_dir"' EXIT
cat > "$pkg_dir/libusb-1.0.pc" <<PC
prefix=$native_dir
libdir=\${prefix}
includedir=\${prefix}/include

Name: libusb-1.0
Description: UniTool broker-backed Android libusb
Version: 1.0.29
Libs: -L\${libdir} -lusb
Cflags: -I\${includedir}
PC

export PKG_CONFIG_PATH="$pkg_dir${PKG_CONFIG_PATH:+:$PKG_CONFIG_PATH}"
export PKG_CONFIG_ALLOW_CROSS=1
export PKG_CONFIG_LIBDIR="$pkg_dir"
if cargo -V | grep -q nightly; then
  cargo build --locked --release --target "$target" --features android --bin umbrage-cli
else
  cargo +nightly build --locked --release --target "$target" --features android --bin umbrage-cli
fi

mkdir -p "$out_dir/jniLibs/arm64-v8a" "$out_dir/bin/arm64-v8a"
cp "target/$target/release/umbrage-cli" "$out_dir/bin/arm64-v8a/"
chmod 0755 "$out_dir/bin/arm64-v8a/umbrage-cli"
cp "$out_dir/bin/arm64-v8a/umbrage-cli" "$native_dir/libumbrage_cli_exec.so"
chmod 0755 "$native_dir/libumbrage_cli_exec.so"
cp "$native_dir/libumbrage_cli_exec.so" "$out_dir/jniLibs/arm64-v8a/"

readelf -d "$out_dir/bin/arm64-v8a/umbrage-cli" | grep -F 'Shared library: [libusb.so]' >/dev/null || {
  echo "Umbrage did not dynamically link to UniTool's libusb.so" >&2
  exit 1
}
echo "Built Umbrage for Android against $native_dir/libusb.so and $native_dir/libusb_bridge.so"
