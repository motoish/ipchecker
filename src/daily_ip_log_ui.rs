#[cfg(target_os = "macos")]
use std::path::PathBuf;

#[cfg(target_os = "macos")]
use objc2::MainThreadMarker;
#[cfg(target_os = "macos")]
use objc2_app_kit::{NSAlert, NSApplication, NSModalResponseOK, NSOpenPanel};
#[cfg(target_os = "macos")]
use objc2_foundation::NSString;

#[cfg(target_os = "macos")]
trait DirectoryPanel {
    fn set_can_choose_directories(&self, enabled: bool);
    fn set_can_choose_files(&self, enabled: bool);
    fn set_allows_multiple_selection(&self, enabled: bool);
    fn set_can_create_directories(&self, enabled: bool);
}

#[cfg(target_os = "macos")]
impl DirectoryPanel for NSOpenPanel {
    fn set_can_choose_directories(&self, enabled: bool) {
        self.setCanChooseDirectories(enabled);
    }

    fn set_can_choose_files(&self, enabled: bool) {
        self.setCanChooseFiles(enabled);
    }

    fn set_allows_multiple_selection(&self, enabled: bool) {
        self.setAllowsMultipleSelection(enabled);
    }

    fn set_can_create_directories(&self, enabled: bool) {
        self.setCanCreateDirectories(enabled);
    }
}

#[cfg(target_os = "macos")]
fn configure_daily_ip_log_panel(panel: &impl DirectoryPanel) {
    panel.set_can_choose_directories(true);
    panel.set_can_choose_files(false);
    panel.set_allows_multiple_selection(false);
    panel.set_can_create_directories(true);
}

#[cfg(target_os = "macos")]
pub fn choose_daily_ip_log_directory() -> Option<PathBuf> {
    let mtm = MainThreadMarker::new().expect("folder picker must run on the main thread");
    let app = NSApplication::sharedApplication(mtm);
    #[allow(deprecated)]
    app.activateIgnoringOtherApps(true);

    let panel = NSOpenPanel::openPanel(mtm);
    configure_daily_ip_log_panel(&*panel);
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

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use std::cell::Cell;

    use super::{DirectoryPanel, configure_daily_ip_log_panel};

    #[derive(Default)]
    struct TestPanel {
        can_choose_directories: Cell<bool>,
        can_choose_files: Cell<bool>,
        allows_multiple_selection: Cell<bool>,
        can_create_directories: Cell<bool>,
    }

    impl DirectoryPanel for TestPanel {
        fn set_can_choose_directories(&self, enabled: bool) {
            self.can_choose_directories.set(enabled);
        }

        fn set_can_choose_files(&self, enabled: bool) {
            self.can_choose_files.set(enabled);
        }

        fn set_allows_multiple_selection(&self, enabled: bool) {
            self.allows_multiple_selection.set(enabled);
        }

        fn set_can_create_directories(&self, enabled: bool) {
            self.can_create_directories.set(enabled);
        }
    }

    #[test]
    fn folder_picker_explicitly_allows_creating_directories() {
        let panel = TestPanel::default();
        panel.can_create_directories.set(false);

        configure_daily_ip_log_panel(&panel);

        assert!(panel.can_choose_directories.get());
        assert!(!panel.can_choose_files.get());
        assert!(!panel.allows_multiple_selection.get());
        assert!(panel.can_create_directories.get());
    }
}
