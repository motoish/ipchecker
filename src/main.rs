#[cfg(target_os = "macos")]
mod macos {
    use std::{net::Ipv4Addr, thread, time::Duration};

    use arboard::Clipboard;
    use chrono::Local;
    use ipchecker::{
        about::{apply_application_icon, show_about},
        app::{
            EventSink, EventSinkClosed, NotificationService, NotifierReadySink, WorkerCommand,
            WorkerEvent, WorkerHandle,
        },
        config::{Config, ConfigStore, TrayDisplayField},
        daily_ip_log::{
            DailyIpLogEvent, DailyIpLogEventSink, DailyIpLogHandle, DailyIpLogWorkerClosed,
        },
        daily_ip_log_ui::{choose_daily_ip_log_directory, show_daily_ip_log_error},
        i18n,
        ip_input::prompt_expected_ip,
        ip_source::ReqwestIpSource,
        monitor::{Monitor, MonitorOutcome, MonitorState},
        net_metrics::{NetworkMetricsHandle, NetworkMetricsSampling, NetworkMetricsSink},
        net_speed::NetworkSpeedLabels,
        notification::{ActionSink, MacNotifier, NotificationAction},
        session::Session,
        ui::{FeedbackRestoreGuard, TrayUi, UiCommand, UiModel, install_app_edit_menu},
        update_coordinator::{UpdateCoordinator, UpdateCoordinatorEvent, UpdateEventSink},
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
        DailyIpLog(DailyIpLogEvent),
        Update(UpdateCoordinatorEvent),
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

    #[derive(Clone)]
    struct DailyIpLogEventProxy {
        proxy: EventLoopProxy<UserEvent>,
    }

    impl DailyIpLogEventSink for DailyIpLogEventProxy {
        fn send(&self, event: DailyIpLogEvent) -> Result<(), DailyIpLogWorkerClosed> {
            self.proxy
                .send_event(UserEvent::DailyIpLog(event))
                .map_err(|_| DailyIpLogWorkerClosed)
        }
    }

    #[derive(Clone)]
    struct UpdateEventProxy {
        proxy: EventLoopProxy<UserEvent>,
    }

    impl UpdateEventSink for UpdateEventProxy {
        fn send(&self, event: UpdateCoordinatorEvent) -> Result<(), EventSinkClosed> {
            self.proxy
                .send_event(UserEvent::Update(event))
                .map_err(|_| EventSinkClosed)
        }
    }

    struct NotifierReadyProxy {
        proxy: EventLoopProxy<UserEvent>,
    }

    impl NotifierReadySink for NotifierReadyProxy {
        fn send(&self, result: Result<MacNotifier, String>) {
            if self
                .proxy
                .send_event(UserEvent::NotifierReady(result))
                .is_err()
            {
                log::debug!("notifier bootstrap finished after the event loop closed");
            }
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
        notifications: NotificationService,
        updates: UpdateCoordinator,
        worker: Option<WorkerHandle>,
        network_metrics: Option<NetworkMetricsHandle>,
        daily_ip_log: Option<DailyIpLogHandle>,
        daily_ip_log_error_shown: bool,
        feedback_restore: FeedbackRestoreGuard,
        /// Retained so Edit key equivalents stay registered for dialogs.
        app_edit_menu: Option<Menu>,
        speed_labels: NetworkSpeedLabels,
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
                notifications: NotificationService::default(),
                updates: UpdateCoordinator::default(),
                worker: None,
                network_metrics: None,
                daily_ip_log: None,
                daily_ip_log_error_shown: false,
                feedback_restore: FeedbackRestoreGuard::default(),
                app_edit_menu: None,
                speed_labels: NetworkSpeedLabels::unknown(),
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
            self.daily_ip_log = Some(DailyIpLogHandle::start(DailyIpLogEventProxy {
                proxy: self.proxy.clone(),
            }));
            self.sync_update_menu();

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
            self.notifications.bootstrap(NotifierReadyProxy {
                proxy: self.proxy.clone(),
            });
        }

        fn handle_user_event(&mut self, event: UserEvent, control_flow: &mut ControlFlow) {
            match event {
                UserEvent::Worker(WorkerEvent::FetchCompleted(result)) => {
                    let successful_ip = result.as_ref().ok().copied();
                    let outcome = self.monitor.apply(
                        result,
                        self.config.expected_ip,
                        self.session.is_muted(),
                    );
                    self.notifications.observe(
                        outcome.notification.clone(),
                        self.session.is_muted(),
                        self.config.is_show_status_icon,
                    );
                    self.outcome = outcome;
                    self.apply_ui();
                    self.deliver_pending_notification();
                    if let Some(ip) = successful_ip {
                        self.record_daily_ip(ip);
                    }
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
                UserEvent::DailyIpLog(DailyIpLogEvent::Failed(error)) => {
                    log::warn!("failed to record daily public IP: {error}");
                    if !self.daily_ip_log_error_shown {
                        self.daily_ip_log_error_shown = true;
                        show_daily_ip_log_error();
                    }
                }
                UserEvent::Update(UpdateCoordinatorEvent::CheckCompleted(result)) => {
                    if let Some(release) = self.updates.handle_check_completed(result) {
                        let _ = self.updates.start_download(
                            release,
                            UpdateEventProxy {
                                proxy: self.proxy.clone(),
                            },
                        );
                    }
                    self.sync_update_menu();
                }
                UserEvent::Update(UpdateCoordinatorEvent::DownloadCompleted {
                    version,
                    result,
                }) => {
                    self.updates.handle_download_completed(&version, result);
                    self.sync_update_menu();
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
                    self.set_tray_display(TrayDisplayField::NetworkSpeed, is_show_network_speed);
                }
                UiCommand::SetShowNetworkLatency(is_show_network_latency) => {
                    self.set_tray_display(
                        TrayDisplayField::NetworkLatency,
                        is_show_network_latency,
                    );
                }
                UiCommand::SetShowStatusIcon(is_show_status_icon) => {
                    self.set_tray_display(TrayDisplayField::StatusIcon, is_show_status_icon);
                }
                UiCommand::SetDailyIpLogEnabled(enabled) => {
                    self.set_daily_ip_log_enabled(enabled);
                }
                UiCommand::ChangeDailyIpLogDirectory => {
                    self.change_daily_ip_log_directory();
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
                self.notifications
                    .observe(None, true, self.config.is_show_status_icon);
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

        fn set_tray_display(&mut self, field: TrayDisplayField, enabled: bool) {
            let Some(candidate) = self.config.clone().with_tray_display(field, enabled) else {
                self.apply_ui();
                return;
            };
            if !self.save_candidate(candidate) {
                self.apply_ui();
                return;
            }
            if field == TrayDisplayField::StatusIcon && !enabled {
                self.notifications.clear_on_status_icon_hidden();
            }
            if matches!(
                field,
                TrayDisplayField::NetworkSpeed | TrayDisplayField::NetworkLatency
            ) {
                self.sync_network_metrics_sampling();
            }
            self.apply_ui();
        }

        fn set_daily_ip_log_enabled(&mut self, enabled: bool) {
            if enabled == self.config.is_daily_ip_log_enabled {
                self.apply_ui();
                return;
            }

            let mut candidate = self.config.clone();
            if enabled && candidate.daily_ip_log_directory.is_none() {
                let Some(directory) = choose_daily_ip_log_directory() else {
                    self.apply_ui();
                    return;
                };
                candidate.daily_ip_log_directory = Some(directory);
            }
            candidate.is_daily_ip_log_enabled = enabled;
            if self.save_candidate(candidate) && enabled {
                self.send_worker_command(WorkerCommand::CheckNow);
            }
            self.apply_ui();
        }

        fn change_daily_ip_log_directory(&mut self) {
            let Some(directory) = choose_daily_ip_log_directory() else {
                return;
            };
            let mut candidate = self.config.clone();
            candidate.daily_ip_log_directory = Some(directory);
            let should_check = candidate.is_daily_ip_log_enabled;
            if self.save_candidate(candidate) && should_check {
                self.send_worker_command(WorkerCommand::CheckNow);
            }
            self.apply_ui();
        }

        fn record_daily_ip(&self, ip: Ipv4Addr) {
            if !self.config.is_daily_ip_log_enabled {
                return;
            }
            let Some(directory) = self.config.daily_ip_log_directory.clone() else {
                log::warn!("daily public IP log is enabled without an output directory");
                return;
            };
            let Some(worker) = &self.daily_ip_log else {
                log::warn!("daily public IP log worker is unavailable");
                return;
            };
            if worker
                .record(directory, Local::now().date_naive(), ip)
                .is_err()
            {
                log::warn!("daily public IP log worker is closed");
            }
        }

        fn start_update_check(&mut self) {
            let _ = self.updates.start_check(UpdateEventProxy {
                proxy: self.proxy.clone(),
            });
            self.sync_update_menu();
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
            self.notifications
                .observe(None, false, self.config.is_show_status_icon);
            self.outcome = self.outcome.recompare_expected(self.config.expected_ip);
            self.apply_ui();
            self.send_worker_command(WorkerCommand::CheckNow);
        }

        fn finish_notifier_bootstrap(&mut self, result: Result<MacNotifier, String>) {
            self.notifications
                .finish_bootstrap(result, self.config.is_show_status_icon);
            self.deliver_pending_notification();
        }

        fn deliver_pending_notification(&mut self) {
            self.notifications.deliver_pending(NotificationActionProxy {
                proxy: self.proxy.clone(),
            });
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

        fn sync_update_menu(&self) {
            let Some(tray_ui) = &self.tray_ui else {
                return;
            };
            tray_ui.set_check_for_updates_enabled(!self.updates.in_progress());
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
