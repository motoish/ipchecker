#[cfg(target_os = "macos")]
use objc2::{AllocAnyThread, MainThreadMarker, rc::Retained};
#[cfg(target_os = "macos")]
use objc2_app_kit::{NSAlert, NSApplication, NSImage};
#[cfg(target_os = "macos")]
use objc2_foundation::{NSData, NSSize, NSString};

/// Compact app mark embedded so About works even outside a .app bundle.
#[cfg(target_os = "macos")]
const ABOUT_ICON_PNG: &[u8] = include_bytes!("../resources/about-icon.png");

pub fn about_message_title() -> &'static str {
    "ipchecker"
}

pub fn about_informative_text() -> String {
    t!("dialog.about_body", version = env!("CARGO_PKG_VERSION")).to_string()
}

pub fn about_ok_label() -> String {
    t!("dialog.about_ok").to_string()
}

#[cfg(target_os = "macos")]
fn about_icon_image() -> Option<Retained<NSImage>> {
    // Always use the transparent glyph asset. The Finder AppIcon has an opaque
    // plate that shows up as a white box inside NSAlert.
    let data = NSData::with_bytes(ABOUT_ICON_PNG);
    NSImage::initWithData(NSImage::alloc(), &data)
}

/// Sets `NSApplication.applicationIconImage` so system dialogs (About) show the mark.
#[cfg(target_os = "macos")]
pub fn apply_application_icon() {
    let Some(mtm) = MainThreadMarker::new() else {
        log::warn!("skipped application icon; not on the main thread");
        return;
    };
    let Some(icon) = about_icon_image() else {
        log::warn!("failed to load application icon for About dialog");
        return;
    };
    let app = NSApplication::sharedApplication(mtm);
    // SAFETY: icon is a valid NSImage retained for the call.
    unsafe { app.setApplicationIconImage(Some(&icon)) };
}

#[cfg(target_os = "macos")]
pub fn show_about() {
    let mtm = MainThreadMarker::new().expect("about dialog must run on the main thread");
    let alert = NSAlert::new(mtm);
    alert.setMessageText(&NSString::from_str(about_message_title()));
    let informative = NSString::from_str(&about_informative_text());
    alert.setInformativeText(&informative);
    alert.addButtonWithTitle(&NSString::from_str(&about_ok_label()));

    if let Some(icon) = about_icon_image() {
        // NSAlert icon slot is ~64pt; keep 1x logical size so the glyph fills it.
        icon.setSize(NSSize::new(64.0, 64.0));
        // SAFETY: icon is a valid NSImage retained for the call.
        unsafe { alert.setIcon(Some(&icon)) };
    }

    let app = NSApplication::sharedApplication(mtm);
    #[allow(deprecated)]
    app.activateIgnoringOtherApps(true);

    let _ = alert.runModal();
}
