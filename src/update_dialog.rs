#[cfg(target_os = "macos")]
use std::path::Path;

#[cfg(target_os = "macos")]
use objc2::MainThreadMarker;
#[cfg(target_os = "macos")]
use objc2_app_kit::{NSAlert, NSAlertFirstButtonReturn, NSApplication};
#[cfg(target_os = "macos")]
use objc2_foundation::NSString;

#[cfg(target_os = "macos")]
pub fn show_current_version(version: &str) {
    let alert = alert(
        t!("dialog.update_current_title").as_ref(),
        &t!("dialog.update_current_body", version = version),
    );
    alert.addButtonWithTitle(&NSString::from_str(t!("dialog.update_ok").as_ref()));
    alert.runModal();
}

#[cfg(target_os = "macos")]
pub fn confirm_update_download(version: &str) -> bool {
    let alert = alert(
        t!("dialog.update_available_title").as_ref(),
        &t!("dialog.update_available_body", version = version),
    );
    alert.addButtonWithTitle(&NSString::from_str(t!("dialog.update_download").as_ref()));
    alert.addButtonWithTitle(&NSString::from_str(t!("dialog.update_cancel").as_ref()));
    alert.runModal() == NSAlertFirstButtonReturn
}

#[cfg(target_os = "macos")]
pub fn show_update_ready(version: &str, app: &Path) {
    let alert = alert(
        t!("dialog.update_ready_title").as_ref(),
        &t!(
            "dialog.update_ready_body",
            version = version,
            path = app.display()
        ),
    );
    alert.addButtonWithTitle(&NSString::from_str(
        t!("dialog.update_show_finder").as_ref(),
    ));
    alert.runModal();
}

#[cfg(target_os = "macos")]
pub fn show_update_error() -> bool {
    let alert = alert(
        t!("dialog.update_error_title").as_ref(),
        t!("dialog.update_error_body").as_ref(),
    );
    alert.addButtonWithTitle(&NSString::from_str(
        t!("dialog.update_open_releases").as_ref(),
    ));
    alert.addButtonWithTitle(&NSString::from_str(t!("dialog.update_ok").as_ref()));
    alert.runModal() == NSAlertFirstButtonReturn
}

#[cfg(target_os = "macos")]
fn alert(message: &str, informative: &str) -> objc2::rc::Retained<NSAlert> {
    let mtm = MainThreadMarker::new().expect("update dialogs must run on the main thread");
    let alert = NSAlert::new(mtm);
    alert.setMessageText(&NSString::from_str(message));
    alert.setInformativeText(&NSString::from_str(informative));
    let app = NSApplication::sharedApplication(mtm);
    #[allow(deprecated)]
    app.activateIgnoringOtherApps(true);
    alert
}
