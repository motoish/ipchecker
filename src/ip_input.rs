use std::net::{AddrParseError, Ipv4Addr};

#[cfg(target_os = "macos")]
use objc2::{MainThreadMarker, rc::autoreleasepool};
#[cfg(target_os = "macos")]
use objc2_app_kit::{
    NSAlert, NSAlertFirstButtonReturn, NSAlertSecondButtonReturn, NSApplication, NSTextField,
};
#[cfg(target_os = "macos")]
use objc2_foundation::{NSPoint, NSRect, NSSize, NSString};

pub fn parse_expected_ip(input: &str) -> Result<Ipv4Addr, AddrParseError> {
    input.trim().parse()
}

#[cfg(target_os = "macos")]
pub fn prompt_expected_ip(initial: Option<Ipv4Addr>) -> Option<Ipv4Addr> {
    let mtm = MainThreadMarker::new().expect("expected-IP prompt must run on the main thread");
    let alert = NSAlert::new(mtm);
    alert.setMessageText(&NSString::from_str(
        t!("dialog.set_expected_title").as_ref(),
    ));

    let text_field = NSTextField::new(mtm);
    text_field.setFrame(NSRect::new(
        NSPoint::new(0.0, 0.0),
        NSSize::new(300.0, 24.0),
    ));
    let initial = initial.map(|ip| ip.to_string()).unwrap_or_default();
    text_field.setStringValue(&NSString::from_str(&initial));
    alert.setAccessoryView(Some(&text_field));

    alert.addButtonWithTitle(&NSString::from_str(t!("dialog.save").as_ref()));
    alert.addButtonWithTitle(&NSString::from_str(t!("dialog.cancel").as_ref()));

    let app = NSApplication::sharedApplication(mtm);
    #[allow(deprecated)]
    app.activateIgnoringOtherApps(true);

    let window = alert.window();
    window.setInitialFirstResponder(Some(&text_field));
    window.makeFirstResponder(Some(&text_field));

    loop {
        let response = alert.runModal();
        if response == NSAlertSecondButtonReturn {
            return None;
        }

        debug_assert_eq!(response, NSAlertFirstButtonReturn);
        let input = autoreleasepool(|pool| {
            let value = text_field.stringValue();
            // SAFETY: `pool` is the innermost autorelease pool, and the borrowed
            // UTF-8 view is copied into an owned `String` before the pool exits.
            unsafe { value.to_str(pool) }.to_owned()
        });
        if let Ok(expected_ip) = parse_expected_ip(&input) {
            return Some(expected_ip);
        }

        alert.setInformativeText(&NSString::from_str(t!("dialog.invalid_ipv4").as_ref()));
        window.makeFirstResponder(Some(&text_field));
    }
}
