#[cfg(target_os = "macos")]
use std::path::PathBuf;

#[cfg(target_os = "macos")]
use objc2::MainThreadMarker;
#[cfg(target_os = "macos")]
use objc2_app_kit::{NSAlert, NSApplication, NSModalResponseOK, NSOpenPanel};
#[cfg(target_os = "macos")]
use objc2_foundation::NSString;

#[cfg(target_os = "macos")]
pub fn choose_daily_ip_log_directory() -> Option<PathBuf> {
    let mtm = MainThreadMarker::new().expect("folder picker must run on the main thread");
    let app = NSApplication::sharedApplication(mtm);
    #[allow(deprecated)]
    app.activateIgnoringOtherApps(true);

    let panel = NSOpenPanel::openPanel(mtm);
    panel.setCanChooseDirectories(true);
    panel.setCanChooseFiles(false);
    panel.setAllowsMultipleSelection(false);
    if panel.runModal() != NSModalResponseOK {
        return None;
    }

    let path = panel.URL()?.path()?;
    Some(PathBuf::from(path.to_string()))
}

#[cfg(target_os = "macos")]
pub fn show_daily_ip_log_error() {
    let mtm = MainThreadMarker::new().expect("daily IP log alert must run on the main thread");
    let alert = NSAlert::new(mtm);
    alert.setMessageText(&NSString::from_str(
        t!("dialog.daily_ip_log_error_title").as_ref(),
    ));
    alert.setInformativeText(&NSString::from_str(
        t!("dialog.daily_ip_log_error_body").as_ref(),
    ));
    alert.addButtonWithTitle(&NSString::from_str(t!("dialog.update_ok").as_ref()));
    let app = NSApplication::sharedApplication(mtm);
    #[allow(deprecated)]
    app.activateIgnoringOtherApps(true);
    alert.runModal();
}
