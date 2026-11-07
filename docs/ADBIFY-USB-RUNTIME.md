# Adbify USB runtime integration

Target runtime paths supplied by Adbify:

- `$PREFIX/lib/libusb.so`
- `$PREFIX/lib/libusb-1.0.so` (same packaged library alias)
- `$PREFIX/lib/libusb_bridge.so`
- `UNITOOL_LIBUSB_PATH` and `UNITOOL_LIBUSB_BRIDGE_PATH`
- `LD_LIBRARY_PATH` includes the Adbify prefix library directory

## Current implementation status

This CLI currently enables the `rusb` vendored feature, which builds/links a separate libusb implementation. It is **not yet using Adbify's packaged libusb**.

Removing vendoring alone is insufficient: the Android build must link against the exact Adbify library and arrange bridge loading before libusb's dynamic dependency is resolved. More importantly, Android USB permission is owned by the Adbify app UID; a standalone child process cannot acquire that permission from environment variables.

The Adbify repository currently has no published bridge source, ABI/header, IPC protocol, or Kotlin permission/session service. Do not replace the existing backend with guessed symbols or claim working USB I/O until those interfaces are available.

## Required integration contract

Once the bridge ABI is supplied, implement a single Android backend used by both the CLI binary and JNI library:

1. Load the Adbify bridge and libusb in the documented order.
2. Acquire/release an app-authorized USB session using the documented API.
3. Route enumeration, interface claim/release, control transfers, bulk transfers, and disconnect events through that session.
4. Keep the current non-Android libusb backend unchanged.
5. Add ARM64 CI checks for dynamic dependencies and runtime loading.
6. Validate on-device with permission grant, enumeration, claim, transfers, and reconnect.

Do not ship a statically vendored libusb alongside the Adbify runtime.
