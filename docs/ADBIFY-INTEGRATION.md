# Adbify runtime integration

## Runtime ownership

Adbify is the Android runtime owner. Umbrage must not package another
libusb implementation for Android and must not silently fall back to an
unrelated system libusb.

Adbify currently installs these exact files into its private prefix:

- `$PREFIX/lib/libusb.so`
- `$PREFIX/lib/libusb_bridge.so`

The Adbify terminal process exports:

- `PREFIX=<app-private files/usr>`
- `LD_LIBRARY_PATH=$PREFIX/lib:<ApplicationInfo.nativeLibraryDir>`

The Android executable must verify both files exist before USB initialization.
Keep any dynamic loader handles alive for the complete lifetime of libusb use.
Load the bridge before the libusb entry points and log canonical resolved paths.
Do not copy or overwrite the Adbify-managed libraries.

## Permission and transport

A terminal subprocess runs under the Adbify application UID, but that fact alone
does not confer a USB Host permission grant. The permission is granted by
Android's `UsbManager` to the app for a selected device. A process that merely
inherits `LD_LIBRARY_PATH` cannot independently request that grant.

The bridge must therefore consume a session which Adbify has already opened
after its permission flow, or call a documented Adbify in-process API. It must
not run a second independent USB backend in parallel with Adbify.

The current Umbrage CLI JNI module is only a JNI port prototype. Its session
API is not yet connected to the existing C ABI command functions, so it must
not be described as a working CLI USB transport until that connection and
end-to-end device tests are complete.

## Native deliverables

The workflow builds only `arm64-v8a` and publishes:

- `libumbrage_cli.so`
- `umbrage-cli`

The artifact is intended for integration into Adbify's private terminal runtime.
It does not include or replace `libusb.so` or `libusb_bridge.so`.
