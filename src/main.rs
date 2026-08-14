#[cfg(target_os = "macos")]
mod macos {
    use std::{thread, time::Duration};

    use arboard::Clipboard;
    use ipchecker::{
        about::{apply_application_icon, show_about},
        app::{
            EventSink, EventSinkClosed, PendingNotificationDecision, WorkerCommand, WorkerEvent,
            WorkerHandle,
        },
        config::{Config, ConfigStore},
        i18n,
        ip_input::prompt_expected_ip,
        ip_source::ReqwestIpSource,
        monitor::{Monitor, MonitorOutcome, MonitorState},
        notification::{ActionSink, MacNotifier, NotificationAction, Notifier},
        session::Session,
        ui::{FeedbackRestoreGuard, TrayUi, UiCommand, UiModel, install_app_edit_menu},
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

    struct Runtime {
        proxy: EventLoopProxy<UserEvent>,
        store: Option<ConfigStore>,
        config: Config,
        session: Session,
        monitor: Monitor,
        outcome: MonitorOutcome,
        tray_ui: Option<TrayUi>,
        notifier: Option<MacNotifier>,
        notifier_bootstrapping: bool,
        pending_notification: PendingNotificationDecision,
        worker: Option<WorkerHandle>,
        feedback_restore: FeedbackRestoreGuard,
        /// Retained so Edit key equivalents stay registered for dialogs.
        app_edit_menu: Option<Menu>,
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
                notifier_bootstrapping: false,
                pending_notification: PendingNotificationDecision::default(),
                worker: None,
                feedback_restore: FeedbackRestoreGuard::default(),
                app_edit_menu: None,
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
            self.notifier_bootstrapping = self.start_notifier_bootstrap();
        }

        fn handle_user_event(&mut self, event: UserEvent, control_flow: &mut ControlFlow) {
            match event {
                UserEvent::Worker(WorkerEvent::FetchCompleted(result)) => {
                    let outcome = self.monitor.apply(
                        result,
                        self.config.expected_ip,
                        self.session.is_muted(),
                    );
                    let notification = outcome.notification.clone();
                    self.outcome = outcome;
                    self.apply_ui();

                    if self.notifier.is_some() {
                        if let Some(decision) = notification {
                            self.send_notification(decision);
                        }
                    } else if self.notifier_bootstrapping {
                        self.pending_notification.replace(notification);
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
                    let command = UiCommand::from_menu_action(action, self.session.is_muted());
                    self.handle_command(command, control_flow);
                }
                UserEvent::Notification(NotificationAction::Continue) => {}
                UserEvent::Notification(NotificationAction::MuteSession) => {
                    self.session.set_muted(true);
                    self.pending_notification.replace(None);
                    self.apply_ui();
                }
                UserEvent::NotifierReady(result) => self.finish_notifier_bootstrap(result),
                UserEvent::RestoreExpectedTitle(token) => {
                    if self.feedback_restore.claim(token) {
                        self.render_ui();
                    }
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
                UiCommand::SetMuted(muted) => {
                    self.session.set_muted(muted);
                    if muted {
                        self.pending_notification.replace(None);
                    }
                    self.apply_ui();
                }
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

            let mut candidate = self.config.clone();
            candidate.expected_ip = Some(expected_ip);
            if self.save_candidate(candidate) {
                self.recompare_expected_and_check();
            }
        }

        fn use_current_ip(&mut self) {
            let Some(current_ip) = self.outcome.last_success_ip else {
                log::warn!("ignored use-current-IP command before a successful check");
                return;
            };

            let mut candidate = self.config.clone();
            candidate.expected_ip = Some(current_ip);
            if self.save_candidate(candidate) {
                self.recompare_expected_and_check();
            }
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
            self.pending_notification.replace(None);
            self.outcome = self.outcome.recompare_expected(self.config.expected_ip);
            self.apply_ui();
            self.send_worker_command(WorkerCommand::CheckNow);
        }

        fn start_notifier_bootstrap(&self) -> bool {
            let proxy = self.proxy.clone();
            match thread::Builder::new()
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
                }) {
                Ok(_) => true,
                Err(error) => {
                    log::warn!("failed to start notification authorization: {error}");
                    false
                }
            }
        }

        fn finish_notifier_bootstrap(&mut self, result: Result<MacNotifier, String>) {
            self.notifier_bootstrapping = false;
            match result {
                Ok(notifier) => {
                    self.notifier = Some(notifier);
                    if let Some(decision) = self.pending_notification.take() {
                        self.send_notification(decision);
                    }
                }
                Err(error) => {
                    self.pending_notification.replace(None);
                    log::warn!("notification authorization unavailable: {error}");
                }
            }
        }

        fn send_notification(&mut self, decision: ipchecker::monitor::NotificationDecision) {
            let Some(notifier) = &mut self.notifier else {
                return;
            };
            if let Err(error) = notifier.send(
                decision,
                Box::new(NotificationActionProxy {
                    proxy: self.proxy.clone(),
                }),
            ) {
                log::warn!("failed to send notification: {error}");
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
