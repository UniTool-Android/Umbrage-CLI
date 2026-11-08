# UniTool Android USB backend

Umbrage-CLI can use UniTool-core's existing Android USB permission and FD
broker without shipping a second libusb implementation.

## Runtime design

- Android `UsbManager` remains the authority for device discovery and
  permission prompts.
- UniTool-core owns the app-local abstract Unix socket
  `unitool_usb_fd_broker` and transfers only authorized USB file descriptors.
- UniTool-core's patched `libusb.so` obtains device snapshots and FDs through
  `libusb_bridge.so`; its Linux usbfs backend performs the existing libusb
  lifecycle on those broker-provided descriptors.
- Umbrage's Rust `rusb` backend dynamically links to that same `libusb.so`.
  The Android process resolves `libusb_bridge.so` as libusb's dependency.

Do not enable `rusb/vendored` for Android. That would embed a second,
unpatched upstream libusb and bypass the UniTool broker.

## Build ARM64 Android CLI

First build UniTool-core's Android native targets so the following files are
available for `arm64-v8a`:

- `libusb.so`
- `libusb_bridge.so`

Then build the CLI:

```sh
export ANDROID_NDK_HOME=/path/to/android-ndk
export UNITOOL_LIBUSB_DIR=/path/to/UniTool-core/app/src/main/jniLibs/arm64-v8a
export UNITOOL_LIBUSB_INCLUDE_DIR=/path/to/libusb-1.0.29/libusb

./scripts/build-android-arm64.sh
```

The include directory must contain the `libusb.h` matching the pinned
UniTool-core libusb build. The build script creates a temporary pkg-config
file that points `libusb1-sys` at UniTool's shared library and verifies the
resulting ELF dependency chain.

## Runtime requirements

Place the CLI executable in the app-private executable path and keep both
native libraries in Android's native library directory:

- `umbrage-cli`
- `libusb.so`
- `libusb_bridge.so`

The app runtime must include its `nativeLibraryDir` in the child process
`LD_LIBRARY_PATH`. The broker socket is app-local and is available only
while UniTool's Android USB service is running. Android USB permission must
be granted through the app before the CLI can see the device.

Use `--backend unitool` to explicitly select the broker-backed libusb path.
On Android this maps to the existing `PortBackend::Libusb` implementation;
the patched shared libusb supplies the UniTool transport. `auto` and
`libusb` remain available for desktop builds.

## Validation boundary

A successful cross-build proves only that the binary links against the
expected shared library. It does not prove that a phone has granted USB
permission, that the broker is alive, or that a physical USB session works.
Validate discovery and descriptor access using UniTool-core's existing
read-only USB probes before testing higher-level protocol operations.
