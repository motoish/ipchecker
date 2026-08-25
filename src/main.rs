#[cfg(target_os = "macos")]
mod macos {
    use std::{net::Ipv4Addr, thread, time::Duration};

    use arboard::Clipboard;
    use ipchecker::{
        about::{apply_application_icon, show_about},
        app::{
            EventSink, EventSinkClosed, NotificationCoordinator, WorkerCommand, WorkerEvent,
            WorkerHandle,
        },
        config::{Config, ConfigStore},
        i18n,
        ip_input::prompt_expected_ip,
        ip_source::ReqwestIpSource,
        monitor::{Monitor, MonitorOutcome, MonitorState},
        net_metrics::{NetworkMetricsHandle, NetworkMetricsSampling, NetworkMetricsSink},
        net_speed::NetworkSpeedLabels,
        notification::{ActionSink, MacNotifier, NotificationAction, Notifier},
        session::Session,
        ui::{FeedbackRestoreGuard, TrayUi, UiCommand, UiModel, install_app_edit_menu},
        update::{
            UpdateError, UpdateRelease, UpdateStatus, check_for_updates, download_and_extract,
            open_releases_page, reveal_in_finder,
        },
        update_dialog::{
            confirm_update_download, show_current_version, show_update_error, show_update_ready,
        },
    };
    use tao::{
        event::{Event, StartCause},
        event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy},
        platform::macos::{ActivationPolicy, EventLoopExtMacOS},
    };
    use tray_icon::menu::{Menu, MenuEvent, MenuId};

    const FEEDBACK_RESTORE_DELAY: Duration = Duration::from_secs(2);

    enum UserEvent {
        Worker(WorkerEvent),
        Menu(MenuId),
        Notification(NotificationAction),
        NotifierReady(Result<MacNotifier, String>),
        RestoreExpectedTitle(u64),
        NetworkSpeed(NetworkSpeedLabels),
        UpdateCheckCompleted(Result<UpdateStatus, UpdateError>),
        UpdateDownloadCompleted {
            version: String,
            result: Result<std::path::PathBuf, UpdateError>,
        },
    }

    #[derive(Clone)]
    struct WorkerEventProxy {
        proxy: EventLoopProxy<UserEvent>,
    }

    impl EventSink for WorkerEventProxy {
        fn send(&self, event: WorkerEvent) -> Result<(), EventSinkClosed> {
            self.proxy
                .send_event(UserEvent::Worker(event))
                .map_err(|_| EventSinkClosed)
        }
    }

    struct NotificationActionProxy {
        proxy: EventLoopProxy<UserEvent>,
    }

    impl ActionSink for NotificationActionProxy {
        fn send(&self, action: NotificationAction) {
            if self
                .proxy
                .send_event(UserEvent::Notification(action))
                .is_err()
            {
                log::warn!("notification action arrived after the event loop closed");
            }
        }
    }

    #[derive(Clone)]
    struct NetworkMetricsEventProxy {
        proxy: EventLoopProxy<UserEvent>,
    }

    impl NetworkMetricsSink for NetworkMetricsEventProxy {
        fn send_labels(&self, labels: NetworkSpeedLabels) -> Result<(), EventSinkClosed> {
            self.proxy
                .send_event(UserEvent::NetworkSpeed(labels))
                .map_err(|_| EventSinkClosed)
        }
    }

    struct Runtime {
        proxy: EventLoopProxy<UserEvent>,
        store: Option<ConfigStore>,
        config: Config,
        session: Session,
        monitor: Monitor,
        outcome: MonitorOutcome,
        tray_ui: Option<TrayUi>,
        notifier: Option<MacNotifier>,
        notification_coordinator: NotificationCoordinator,
        worker: Option<WorkerHandle>,
        network_metrics: Option<NetworkMetricsHandle>,
        feedback_restore: FeedbackRestoreGuard,
        /// Retained so Edit key equivalents stay registered for dialogs.
        app_edit_menu: Option<Menu>,
        speed_labels: NetworkSpeedLabels,
        update_in_progress: bool,
        initialized: bool,
    }

    impl Runtime {
        fn new(proxy: EventLoopProxy<UserEvent>) -> Self {
            let (store, config) = load_config();
            Self {
                proxy,
                store,
                config,
                session: Session::new(),
                monitor: Monitor::default(),
                outcome: MonitorOutcome {
                    state: MonitorState::Unknown,
                    current_ip: None,
                    last_success_ip: None,
                    notification: None,
                },
                tray_ui: None,
                notifier: None,
                notification_coordinator: NotificationCoordinator::default(),
                worker: None,
                network_metrics: None,
                feedback_restore: FeedbackRestoreGuard::default(),
                app_edit_menu: None,
                speed_labels: NetworkSpeedLabels::unknown(),
                update_in_progress: false,
                initialized: false,
            }
        }

        fn initialize_after_loop_start(&mut self) {
            if self.initialized {
                return;
            }
            self.initialized = true;

            apply_application_icon();

            match install_app_edit_menu() {
                Ok(menu) => self.app_edit_menu = Some(menu),
                Err(error) => log::error!("failed to install app Edit menu: {error}"),
            }

            let model = self.ui_model();
            match TrayUi::new(&model) {
                Ok(tray_ui) => self.tray_ui = Some(tray_ui),
                Err(error) => log::error!("failed to create tray UI: {error}"),
            }
            self.start_network_metrics_sampler();

            match ReqwestIpSource::new() {
                Ok(source) => {
                    self.worker = Some(WorkerHandle::start(
                        source,
                        Duration::from_secs(self.config.interval_minutes * 60),
                        WorkerEventProxy {
                            proxy: self.proxy.clone(),
                        },
                    ));
                }
                Err(error) => log::error!("failed to create public IP source: {error}"),
            }
            self.start_notifier_bootstrap();
        }

        fn handle_user_event(&mut self, event: UserEvent, control_flow: &mut ControlFlow) {
            match event {
                UserEvent::Worker(WorkerEvent::FetchCompleted(result)) => {
                    let outcome = self.monitor.apply(
                        result,
                        self.config.expected_ip,
                        self.session.is_muted(),
                    );
                    self.notification_coordinator
                        .observe(outcome.notification.clone(), self.session.is_muted());
                    self.outcome = outcome;
                    self.apply_ui();
                    self.deliver_pending_notification();
                }
                UserEvent::Menu(id) => {
                    let Some(action) = self
                        .tray_ui
                        .as_ref()
                        .and_then(|tray_ui| tray_ui.menu_action(&id))
                    else {
                        return;
                    };
                    let command = UiCommand::from_menu_action(
                        action,
                        self.session.is_muted(),
                        self.config.is_show_network_speed,
                        self.config.is_show_network_latency,
                        self.config.is_show_status_icon,
                    );
                    self.handle_command(command, control_flow);
                }
                UserEvent::Notification(NotificationAction::Continue) => {}
                UserEvent::Notification(NotificationAction::MuteSession) => self.set_muted(true),
                UserEvent::NotifierReady(result) => self.finish_notifier_bootstrap(result),
                UserEvent::RestoreExpectedTitle(token) => {
                    if self.feedback_restore.claim(token) {
                        self.render_ui();
                    }
                }
                UserEvent::NetworkSpeed(labels) => {
                    if self.speed_labels == labels {
                        return;
                    }
                    self.speed_labels = labels;
                    self.apply_network_speed();
                }
                UserEvent::UpdateCheckCompleted(result) => {
                    self.handle_update_check_completed(result);
                }
                UserEvent::UpdateDownloadCompleted { version, result } => {
                    self.handle_update_download_completed(version, result);
                }
            }
        }

        fn handle_command(&mut self, command: UiCommand, control_flow: &mut ControlFlow) {
            match command {
                UiCommand::CopyCurrentIp => self.copy_current_ip(),
                UiCommand::SetExpectedFromInput => self.set_expected_from_input(),
                UiCommand::UseCurrentIp => self.use_current_ip(),
                UiCommand::SetInterval(minutes) => self.set_interval(minutes),
                UiCommand::CheckNow => self.send_worker_command(WorkerCommand::CheckNow),
                UiCommand::SetMuted(muted) => self.set_muted(muted),
                UiCommand::SetShowNetworkSpeed(is_show_network_speed) => {
                    self.set_show_network_speed(is_show_network_speed);
                }
                UiCommand::SetShowNetworkLatency(is_show_network_latency) => {
                    self.set_show_network_latency(is_show_network_latency);
                }
                UiCommand::SetShowStatusIcon(is_show_status_icon) => {
                    self.set_show_status_icon(is_show_status_icon);
                }
                UiCommand::CheckForUpdates => self.start_update_check(),
                UiCommand::About => show_about(),
                UiCommand::Quit => {
                    self.send_worker_command(WorkerCommand::Shutdown);
                    *control_flow = ControlFlow::Exit;
                }
            }
        }

        fn copy_current_ip(&mut self) {
            let Some(ip) = self.outcome.current_ip.or(self.outcome.last_success_ip) else {
                log::warn!("ignored copy-current-IP command without a known address");
                return;
            };

            if let Err(error) = set_clipboard_text(&ip.to_string()) {
                log::warn!("failed to copy current IP to clipboard: {error}");
                return;
            }

            let Some(tray_ui) = &self.tray_ui else {
                return;
            };
            tray_ui.set_current_title(&i18n::current_ip_copied_title());
            let token = self.feedback_restore.issue();
            self.schedule_feedback_restore(token);
        }

        fn set_expected_from_input(&mut self) {
            let Some(expected_ip) = prompt_expected_ip(self.config.expected_ip) else {
                return;
            };
            self.set_expected_ip(expected_ip);
        }

        fn use_current_ip(&mut self) {
            let Some(current_ip) = self.outcome.current_ip else {
                log::warn!("ignored use-current-IP command without a current successful check");
                return;
            };

            self.set_expected_ip(current_ip);
        }

        fn set_expected_ip(&mut self, expected_ip: Ipv4Addr) {
            let mut candidate = self.config.clone();
            candidate.expected_ip = Some(expected_ip);
            if self.save_candidate(candidate) {
                self.recompare_expected_and_check();
            }
        }

        fn set_muted(&mut self, muted: bool) {
            self.session.set_muted(muted);
            if muted {
                self.notification_coordinator.observe(None, true);
            }
            self.apply_ui();
        }

        fn set_interval(&mut self, minutes: u64) {
            let mut candidate = self.config.clone();
            if let Err(error) = candidate.set_interval(minutes) {
                log::warn!("ignored invalid interval command: {error}");
                return;
            }
            if !self.save_candidate(candidate) {
                self.apply_ui();
                return;
            }

            self.apply_ui();
            self.send_worker_command(WorkerCommand::SetInterval(Duration::from_secs(
                minutes * 60,
            )));
        }

        fn set_show_network_speed(&mut self, is_show_network_speed: bool) {
            let mut candidate = self.config.clone();
            candidate.is_show_network_speed = is_show_network_speed;
            if !candidate.has_visible_tray_item() {
                self.apply_ui();
                return;
            }
            if !self.save_candidate(candidate) {
                self.apply_ui();
                return;
            }
            self.sync_network_metrics_sampling();
            self.apply_ui();
        }

        fn set_show_network_latency(&mut self, is_show_network_latency: bool) {
            let mut candidate = self.config.clone();
            candidate.is_show_network_latency = is_show_network_latency;
            if !candidate.has_visible_tray_item() {
                self.apply_ui();
                return;
            }
            if !self.save_candidate(candidate) {
                self.apply_ui();
                return;
            }
            self.sync_network_metrics_sampling();
            self.apply_ui();
        }

        fn set_show_status_icon(&mut self, is_show_status_icon: bool) {
            let mut candidate = self.config.clone();
            candidate.is_show_status_icon = is_show_status_icon;
            if !candidate.has_visible_tray_item() {
                self.apply_ui();
                return;
            }
            if !self.save_candidate(candidate) {
                self.apply_ui();
                return;
            }
            self.apply_ui();
        }

        fn start_update_check(&mut self) {
            if self.update_in_progress {
                return;
            }
            self.set_update_in_progress(true);

            let proxy = self.proxy.clone();
            if let Err(error) = thread::Builder::new()
                .name("ipchecker-update-check".to_owned())
                .spawn(move || {
                    if proxy
                        .send_event(UserEvent::UpdateCheckCompleted(check_for_updates()))
                        .is_err()
                    {
                        log::debug!("update check finished after the event loop closed");
                    }
                })
            {
                log::warn!("failed to start update check: {error}");
                self.set_update_in_progress(false);
                self.present_update_error();
            }
        }

        fn handle_update_check_completed(&mut self, result: Result<UpdateStatus, UpdateError>) {
            match result {
                Ok(UpdateStatus::Current) => {
                    self.set_update_in_progress(false);
                    show_current_version(env!("CARGO_PKG_VERSION"));
                }
                Ok(UpdateStatus::Available(release)) => {
                    if confirm_update_download(&release.version) {
                        self.start_update_download(release);
                    } else {
                        self.set_update_in_progress(false);
                    }
                }
                Err(error) => {
                    log::warn!("update check failed: {error}");
                    self.set_update_in_progress(false);
                    self.present_update_error();
                }
            }
        }

        fn start_update_download(&mut self, release: UpdateRelease) {
            let version = release.version.clone();
            let proxy = self.proxy.clone();
            if let Err(error) = thread::Builder::new()
                .name("ipchecker-update-download".to_owned())
                .spawn(move || {
                    let result = download_and_extract(&release);
                    if proxy
                        .send_event(UserEvent::UpdateDownloadCompleted { version, result })
                        .is_err()
                    {
                        log::debug!("update download finished after the event loop closed");
                    }
                })
            {
                log::warn!("failed to start update download: {error}");
                self.set_update_in_progress(false);
                self.present_update_error();
            }
        }

        fn handle_update_download_completed(
            &mut self,
            version: String,
            result: Result<std::path::PathBuf, UpdateError>,
        ) {
            self.set_update_in_progress(false);
            match result {
                Ok(app) => {
                    show_update_ready(&version, &app);
                    if let Err(error) = reveal_in_finder(&app) {
                        log::warn!("failed to reveal downloaded update: {error}");
                        self.present_update_error();
                    }
                }
                Err(error) => {
                    log::warn!("update download failed: {error}");
                    self.present_update_error();
                }
            }
        }

        fn set_update_in_progress(&mut self, in_progress: bool) {
            self.update_in_progress = in_progress;
            if let Some(tray_ui) = &self.tray_ui {
                tray_ui.set_check_for_updates_enabled(!in_progress);
            }
        }

        fn present_update_error(&self) {
            if show_update_error()
                && let Err(error) = open_releases_page()
            {
                log::warn!("failed to open Releases page: {error}");
            }
        }

        fn save_candidate(&mut self, candidate: Config) -> bool {
            let Some(store) = &self.store else {
                log::error!("cannot save configuration because its path is unavailable");
                return false;
            };
            if let Err(error) = store.save(&candidate) {
                log::error!("failed to save configuration; keeping live state unchanged: {error}");
                return false;
            }

            self.config = candidate;
            true
        }

        fn schedule_feedback_restore(&mut self, token: u64) {
            let proxy = self.proxy.clone();
            let spawn = thread::Builder::new()
                .name("ipchecker-feedback-restore".to_owned())
                .spawn(move || {
                    thread::sleep(FEEDBACK_RESTORE_DELAY);
                    if proxy
                        .send_event(UserEvent::RestoreExpectedTitle(token))
                        .is_err()
                    {
                        log::debug!("feedback restore expired after the event loop closed");
                    }
                });
            if let Err(error) = spawn {
                log::warn!("failed to schedule feedback restoration: {error}");
                if self.feedback_restore.claim(token) {
                    self.render_ui();
                }
            }
        }

        fn recompare_expected_and_check(&mut self) {
            self.notification_coordinator.observe(None, false);
            self.outcome = self.outcome.recompare_expected(self.config.expected_ip);
            self.apply_ui();
            self.send_worker_command(WorkerCommand::CheckNow);
        }

        fn start_notifier_bootstrap(&self) {
            let proxy = self.proxy.clone();
            if let Err(error) = thread::Builder::new()
                .name("ipchecker-notifier-bootstrap".to_owned())
                .spawn(move || {
                    let mut notifier = MacNotifier::new();
                    let result = notifier
                        .authorize()
                        .map(|()| notifier)
                        .map_err(|error| error.to_string());
                    if proxy.send_event(UserEvent::NotifierReady(result)).is_err() {
                        log::debug!("notifier bootstrap finished after the event loop closed");
                    }
                })
            {
                log::warn!("failed to start notification authorization: {error}");
            }
        }

        fn finish_notifier_bootstrap(&mut self, result: Result<MacNotifier, String>) {
            match result {
                Ok(notifier) => {
                    self.notifier = Some(notifier);
                    self.deliver_pending_notification();
                }
                Err(error) => {
                    self.notification_coordinator.observe(None, false);
                    log::warn!("notification authorization unavailable: {error}");
                }
            }
        }

        fn deliver_pending_notification(&mut self) {
            let Some(decision) = self.notification_coordinator.pending() else {
                return;
            };
            let Some(notifier) = &mut self.notifier else {
                return;
            };
            match notifier.send(
                decision.clone(),
                Box::new(NotificationActionProxy {
                    proxy: self.proxy.clone(),
                }),
            ) {
                Ok(()) => self.notification_coordinator.mark_delivered(&decision),
                Err(error) => log::warn!("failed to send notification: {error}"),
            }
        }

        fn send_worker_command(&self, command: WorkerCommand) {
            let Some(worker) = &self.worker else {
                log::warn!("ignored worker command because the worker is unavailable");
                return;
            };
            if worker.command(command).is_err() {
                log::warn!("worker command channel is closed");
            }
        }

        fn ui_model(&self) -> UiModel {
            UiModel::from_state(&self.config, &self.session, &self.outcome)
        }

        fn apply_ui(&mut self) {
            self.feedback_restore.cancel();
            self.render_ui();
        }

        fn render_ui(&self) {
            let Some(tray_ui) = &self.tray_ui else {
                return;
            };
            if let Err(error) = tray_ui.apply(&self.ui_model()) {
                log::warn!("failed to update tray UI: {error}");
            }
            tray_ui.set_network_speed(
                &self.speed_labels,
                self.config.is_show_network_speed,
                self.config.is_show_network_latency,
                self.config.is_show_status_icon,
            );
        }

        fn apply_network_speed(&self) {
            if !self.config.is_show_network_speed && !self.config.is_show_network_latency {
                return;
            }
            let Some(tray_ui) = &self.tray_ui else {
                return;
            };
            tray_ui.set_network_speed(
                &self.speed_labels,
                self.config.is_show_network_speed,
                self.config.is_show_network_latency,
                self.config.is_show_status_icon,
            );
        }

        fn sync_network_metrics_sampling(&self) {
            let Some(network_metrics) = &self.network_metrics else {
                return;
            };
            network_metrics.set_sampling(NetworkMetricsSampling {
                is_show_network_speed: self.config.is_show_network_speed,
                is_show_network_latency: self.config.is_show_network_latency,
            });
        }

        fn start_network_metrics_sampler(&mut self) {
            self.network_metrics = Some(NetworkMetricsHandle::start(
                NetworkMetricsEventProxy {
                    proxy: self.proxy.clone(),
                },
                NetworkMetricsSampling {
                    is_show_network_speed: self.config.is_show_network_speed,
                    is_show_network_latency: self.config.is_show_network_latency,
                },
            ));
        }
    }

    fn load_config() -> (Option<ConfigStore>, Config) {
        let path = match ConfigStore::default_path() {
            Ok(path) => path,
            Err(error) => {
                log::error!("failed to resolve configuration path: {error}");
                return (None, Config::default());
            }
        };
        let store = ConfigStore::new(path);
        let config = match store.load_or_create() {
            Ok(config) => config,
            Err(error) => {
                log::error!("failed to load configuration; using in-memory defaults: {error}");
                Config::default()
            }
        };
        (Some(store), config)
    }

    fn set_clipboard_text(text: &str) -> Result<(), arboard::Error> {
        let mut clipboard = Clipboard::new()?;
        clipboard.set_text(text)
    }

    pub fn run() -> ! {
        env_logger::init();
        i18n::init_from_system();

        let mut event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();
        event_loop.set_activation_policy(ActivationPolicy::Accessory);
        let proxy = event_loop.create_proxy();

        let menu_proxy = proxy.clone();
        MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
            if menu_proxy.send_event(UserEvent::Menu(event.id)).is_err() {
                log::warn!("menu event arrived after the event loop closed");
            }
        }));

        let mut runtime = Runtime::new(proxy);
        event_loop.run(move |event, _, control_flow| {
            *control_flow = ControlFlow::Wait;
            match event {
                Event::NewEvents(StartCause::Init) => runtime.initialize_after_loop_start(),
                Event::UserEvent(event) => runtime.handle_user_event(event, control_flow),
                _ => {}
            }
        });
    }
}

#[cfg(target_os = "macos")]
fn main() {
    macos::run();
}

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("ipchecker is supported only on macOS");
}
