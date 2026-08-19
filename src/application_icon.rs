#[cfg(target_os = "macos")]
#[allow(unexpected_cfgs)] // objc 0.2 macros still probe their historical `cargo-clippy` cfg.
pub(crate) fn install() {
    use std::ffi::c_void;

    use objc::rc::{StrongPtr, autoreleasepool};
    use objc::runtime::Object;
    use objc::{class, msg_send, sel, sel_impl};

    const ICON: &[u8] = include_bytes!("../assets/sabun.ico");

    autoreleasepool(|| {
        // SAFETY: GPUI initializes AppKit before this runs. These selectors are stable AppKit and
        // Foundation APIs. NSData copies the static bytes, and NSApplication retains the image.
        unsafe {
            let bytes = ICON.as_ptr().cast::<c_void>();
            let length = ICON.len() as u64;
            let data: *mut Object = msg_send![class!(NSData), dataWithBytes: bytes length: length];
            let image: *mut Object = msg_send![class!(NSImage), alloc];
            let image: *mut Object = msg_send![image, initWithData: data];
            if image.is_null() {
                return;
            }
            let image = StrongPtr::new(image);
            let application: *mut Object = msg_send![class!(NSApplication), sharedApplication];
            let _: () = msg_send![application, setApplicationIconImage: *image];
        }
    });
}

#[cfg(not(target_os = "macos"))]
pub(crate) const fn install() {}
