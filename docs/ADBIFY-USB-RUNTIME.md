# Adbify USB integration reference

See [antumbra-android](https://github.com/UniTool-Android/antumbra-android), specifically:

- `android/app/src/main/java/com/penumbra/antumbra/UsbHelper.kt`
- `crates/android-usb/src/lib.rs`
- `crates/jni-bridge/src/lib.rs`

It shows Android's USB Host permission and connection lifecycle and provides a Rust `MtkPort` implementation over JNI. This is a Java USB Host backend, not Adbify's libusb broker. It should not be represented as using `libusb_bridge.so`.

The reference's DA/flash/unlock/relock JNI entrypoints are unfinished placeholders and its workflow uses `continue-on-error: true` on build steps. Treat it as an architectural reference, not a tested complete implementation.

The required Adbify libusb bridge integration still needs the bridge's source or an authoritative ABI/IPC contract.
