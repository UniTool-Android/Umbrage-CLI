use std::ffi::c_void;
use std::os::raw::{c_int, c_uchar, c_ushort};

#[repr(C)]
struct LibusbDeviceDescriptor {
    length: c_uchar,
    descriptor_type: c_uchar,
    usb_version: c_ushort,
    device_class: c_uchar,
    device_sub_class: c_uchar,
    device_protocol: c_uchar,
    max_packet_size0: c_uchar,
    vendor_id: c_ushort,
    product_id: c_ushort,
    device_version: c_ushort,
    manufacturer_index: c_uchar,
    product_index: c_uchar,
    serial_index: c_uchar,
    configuration_count: c_uchar,
}

#[link(name = "usb")]
extern "C" {
    fn libusb_init(context: *mut *mut c_void) -> c_int;
    fn libusb_get_device_list(context: *mut c_void, list: *mut *mut *mut c_void) -> isize;
    fn libusb_get_device_descriptor(
        device: *mut c_void,
        descriptor: *mut LibusbDeviceDescriptor,
    ) -> c_int;
    fn libusb_get_bus_number(device: *mut c_void) -> c_uchar;
    fn libusb_get_device_address(device: *mut c_void) -> c_uchar;
    fn libusb_free_device_list(list: *mut *mut c_void, unref_devices: c_int);
    fn libusb_exit(context: *mut c_void);
}

fn main() {
    let mut context = std::ptr::null_mut();
    let result = unsafe { libusb_init(&mut context) };
    if result != 0 {
        eprintln!("Umbrage USB: libusb_init failed ({result})");
        std::process::exit(1);
    }

    let mut list: *mut *mut c_void = std::ptr::null_mut();
    let count = unsafe { libusb_get_device_list(context, &mut list) };
    if count < 0 {
        eprintln!("Umbrage USB: enumeration failed ({count})");
        unsafe { libusb_exit(context) };
        std::process::exit(1);
    }

    println!("USB devices: {count}");
    for index in 0..count as usize {
        let device = unsafe { *list.add(index) };
        if device.is_null() {
            continue;
        }

        let mut descriptor = std::mem::MaybeUninit::<LibusbDeviceDescriptor>::uninit();
        let result = unsafe { libusb_get_device_descriptor(device, descriptor.as_mut_ptr()) };
        if result != 0 {
            eprintln!("USB device {index}: descriptor unavailable ({result})");
            continue;
        }

        let descriptor = unsafe { descriptor.assume_init() };
        let bus = unsafe { libusb_get_bus_number(device) };
        let address = unsafe { libusb_get_device_address(device) };
        println!("{bus:03}:{address:03}  {:04x}:{:04x}",
            descriptor.vendor_id, descriptor.product_id);
    }

    unsafe {
        libusb_free_device_list(list, 1);
        libusb_exit(context);
    }
}
