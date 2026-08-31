use std::{path::PathBuf, thread};

use crate::{
    app::EventSinkClosed,
    update::{
        UpdateError, UpdateRelease, UpdateStatus, check_for_updates, download_and_extract,
        open_releases_page, reveal_in_finder,
    },
    update_dialog::{
        confirm_update_download, show_current_version, show_update_error, show_update_ready,
    },
};

#[derive(Debug)]
pub enum UpdateCoordinatorEvent {
    CheckCompleted(Result<UpdateStatus, UpdateError>),
    DownloadCompleted {
        version: String,
        result: Result<PathBuf, UpdateError>,
    },
}

pub trait UpdateEventSink: Send + Clone + 'static {
    fn send(&self, event: UpdateCoordinatorEvent) -> Result<(), EventSinkClosed>;
}

#[derive(Debug, Default)]
pub struct UpdateCoordinator {
    in_progress: bool,
}

impl UpdateCoordinator {
    pub fn in_progress(&self) -> bool {
        self.in_progress
    }

    pub fn start_check<S: UpdateEventSink>(&mut self, sink: S) -> bool {
        if self.in_progress {
            return false;
        }
        self.in_progress = true;
        if thread::Builder::new()
            .name("ipchecker-update-check".to_owned())
            .spawn(move || {
                if sink
                    .send(UpdateCoordinatorEvent::CheckCompleted(check_for_updates()))
                    .is_err()
                {
                    log::debug!("update check finished after the event loop closed");
                }
            })
            .is_err()
        {
            log::warn!("failed to start update check");
            self.abort_in_progress();
            return false;
        }
        true
    }

    pub(crate) fn apply_check_resolution(
        &mut self,
        resolution: CheckResolution,
    ) -> Option<UpdateRelease> {
        match resolution {
            CheckResolution::ProceedToDownload(release) => Some(release),
            CheckResolution::UpToDate
            | CheckResolution::UserDeclinedDownload
            | CheckResolution::Failed => {
                self.reset_in_progress();
                None
            }
        }
    }

    pub fn handle_check_completed(
        &mut self,
        result: Result<UpdateStatus, UpdateError>,
    ) -> Option<UpdateRelease> {
        let resolution = match result {
            Ok(UpdateStatus::Current) => {
                show_current_version(env!("CARGO_PKG_VERSION"));
                CheckResolution::UpToDate
            }
            Ok(UpdateStatus::Available(release)) => {
                if confirm_update_download(&release.version) {
                    CheckResolution::ProceedToDownload(release)
                } else {
                    CheckResolution::UserDeclinedDownload
                }
            }
            Err(error) => {
                log::warn!("update check failed: {error}");
                Self::present_error();
                CheckResolution::Failed
            }
        };
        self.apply_check_resolution(resolution)
    }

    pub(crate) fn complete_download(&mut self) {
        self.reset_in_progress();
    }

    fn abort_in_progress(&mut self) {
        self.reset_in_progress();
        Self::present_error();
    }

    pub(crate) fn reset_in_progress(&mut self) {
        self.in_progress = false;
    }

    pub fn start_download<S: UpdateEventSink>(&mut self, release: UpdateRelease, sink: S) -> bool {
        let version = release.version.clone();
        if thread::Builder::new()
            .name("ipchecker-update-download".to_owned())
            .spawn(move || {
                let result = download_and_extract(&release);
                if sink
                    .send(UpdateCoordinatorEvent::DownloadCompleted { version, result })
                    .is_err()
                {
                    log::debug!("update download finished after the event loop closed");
                }
            })
            .is_err()
        {
            log::warn!("failed to start update download");
            self.abort_in_progress();
            return false;
        }
        true
    }

    pub fn handle_download_completed(
        &mut self,
        version: &str,
        result: Result<PathBuf, UpdateError>,
    ) {
        self.complete_download();
        match result {
            Ok(app) => {
                show_update_ready(version, &app);
                if let Err(error) = reveal_in_finder(&app) {
                    log::warn!("failed to reveal downloaded update: {error}");
                    Self::present_error();
                }
            }
            Err(error) => {
                log::warn!("update download failed: {error}");
                Self::present_error();
            }
        }
    }

    fn present_error() {
        if show_update_error()
            && let Err(error) = open_releases_page()
        {
            log::warn!("failed to open Releases page: {error}");
        }
    }
}

/// Outcome of an update check after any user-facing prompts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CheckResolution {
    UpToDate,
    UserDeclinedDownload,
    Failed,
    ProceedToDownload(UpdateRelease),
}

#[cfg(test)]
mod tests {
    use super::{CheckResolution, UpdateCoordinator};
    use crate::update::UpdateRelease;

    fn sample_release() -> UpdateRelease {
        UpdateRelease {
            version: "9.9.9".to_owned(),
            build: 999,
            url: "https://example.com/ipchecker.zip".to_owned(),
            size: 0,
            sha256: String::new(),
        }
    }

    fn coordinator_in_check() -> UpdateCoordinator {
        UpdateCoordinator { in_progress: true }
    }

    #[test]
    fn apply_check_resolution_clears_in_progress_when_up_to_date() {
        let mut coordinator = coordinator_in_check();

        assert!(
            coordinator
                .apply_check_resolution(CheckResolution::UpToDate)
                .is_none()
        );
        assert!(!coordinator.in_progress());
    }

    #[test]
    fn apply_check_resolution_clears_in_progress_when_user_declines_download() {
        let mut coordinator = coordinator_in_check();

        assert!(
            coordinator
                .apply_check_resolution(CheckResolution::UserDeclinedDownload)
                .is_none()
        );
        assert!(!coordinator.in_progress());
    }

    #[test]
    fn apply_check_resolution_clears_in_progress_when_check_failed() {
        let mut coordinator = coordinator_in_check();

        assert!(
            coordinator
                .apply_check_resolution(CheckResolution::Failed)
                .is_none()
        );
        assert!(!coordinator.in_progress());
    }

    #[test]
    fn apply_check_resolution_keeps_in_progress_when_download_accepted() {
        let mut coordinator = coordinator_in_check();
        let release = sample_release();

        assert_eq!(
            coordinator.apply_check_resolution(CheckResolution::ProceedToDownload(release.clone())),
            Some(release)
        );
        assert!(coordinator.in_progress());
    }

    #[test]
    fn reset_in_progress_clears_in_progress() {
        let mut coordinator = coordinator_in_check();

        coordinator.reset_in_progress();

        assert!(!coordinator.in_progress());
    }

    #[test]
    fn start_check_rejects_when_already_in_progress() {
        let mut coordinator = coordinator_in_check();
        let sink = TestSink;

        assert!(!coordinator.start_check(sink));
        assert!(coordinator.in_progress());
    }

    #[derive(Clone)]
    struct TestSink;

    impl super::UpdateEventSink for TestSink {
        fn send(
            &self,
            _event: super::UpdateCoordinatorEvent,
        ) -> Result<(), crate::app::EventSinkClosed> {
            Ok(())
        }
    }
}
