use std::env;

fn main() {
    let lib_dir = env::var("UNITOOL_LIBUSB_DIR")
        .expect("UNITOOL_LIBUSB_DIR must point to UniTool's broker-patched libusb output");
    println!("cargo:rustc-link-search=native={lib_dir}");
    println!("cargo:rustc-link-lib=dylib=usb");
    println!("cargo:rerun-if-env-changed=UNITOOL_LIBUSB_DIR");
}
