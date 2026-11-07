# Adbify USB runtime integration contract

This project must use the exact Adbify-packaged ARM64 native libraries:
- `$PREFIX/lib/libusb_bridge.so`
- `$PREFIX/lib/libusb.so`

The terminal runtime exports:
- `PREFIX=<app-private files/usr>`
- `LD_LIBRARY_PATH=$PREFIX/lib:<ApplicationInfo.nativeLibraryDir>`
- `UNITOOL_LIBUSB_PATH=$PREFIX/lib/libusb.so`
- `UNITOOL_LIBUSB_BRIDGE_PATH=$PREFIX/lib/libusb_bridge.so`

The paths above are canonicalized by the app before starting a child process. Clients must never silently fall back to system libusb on Android. Missing bridge/library files must produce a clear startup error. Keep CDLL handles alive for the lifetime of PyUSB/libusb use, preload bridge with RTLD_GLOBAL before libusb, and verify the loaded file paths.

USB permissions:
- USB host permission is granted by Android for the app UID/device via UsbManager.
- A child CLI process cannot independently obtain Android UsbManager permission. Any bridge relying on app-side permission brokerage needs a documented IPC/JNI contract and an open app-side service/session.
- Do not claim that the terminal environment alone grants USB permission.
- Keep non-Android PyUSB backend behavior unchanged.

Architecture goal:
Adbify is the only Android USB backend/runtime owner. MTKClient and Umbrage consume it; do not statically or dynamically load a second libusb build, do not use raw Android bulkTransfer in parallel, and do not ship duplicate USB drivers.
