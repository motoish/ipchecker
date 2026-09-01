use crate::{config::ALLOWED_INTERVAL_MINUTES, net_speed::NetworkSpeedLabels};
use tray_icon::{
    TrayIcon, TrayIconBuilder,
    menu::{CheckMenuItem, Menu, MenuId, MenuItem, PredefinedMenuItem, Submenu},
};

use super::UiError;
use super::icon::IconSet;
use super::model::{MenuAction, UiModel};

#[cfg(target_os = "macos")]
use super::macos_metrics::{SpeedTitleView, install_speed_title_view, set_tray_speed_title};
#[cfg(target_os = "macos")]
use objc2::rc::Retained;

struct IntervalMenuItem {
    minutes: u64,
    item: CheckMenuItem,
}

pub struct TrayUi {
    tray: TrayIcon,
    current_item: MenuItem,
    expected_item: MenuItem,
    set_expected_from_input_item: MenuItem,
    use_current_ip_item: MenuItem,
    interval_items: [IntervalMenuItem; 5],
    check_now_item: MenuItem,
    show_network_speed_item: CheckMenuItem,
    show_network_latency_item: CheckMenuItem,
    show_status_icon_item: CheckMenuItem,
    daily_ip_log_item: CheckMenuItem,
    change_daily_ip_log_directory_item: MenuItem,
    mute_item: CheckMenuItem,
    check_for_updates_item: MenuItem,
    about_item: MenuItem,
    quit_item: MenuItem,
    icons: IconSet,
    #[cfg(target_os = "macos")]
    speed_title_view: Retained<SpeedTitleView>,
}

impl TrayUi {
    pub fn new(model: &UiModel) -> Result<Self, UiError> {
        let menu = Menu::new();
        let current_item = MenuItem::new(&model.current_title, model.can_copy_current_ip, None);
        let expected_item = MenuItem::new(&model.expected_title, false, None);
        let set_expected_from_input_item =
            MenuItem::new(t!("menu.set_expected").as_ref(), true, None);
        let use_current_ip_item = MenuItem::new(
            t!("menu.use_current").as_ref(),
            model.can_use_current_ip,
            None,
        );
        let interval_items = ALLOWED_INTERVAL_MINUTES.map(|minutes| IntervalMenuItem {
            minutes,
            item: CheckMenuItem::new(
                &t!("menu.minutes", minutes = minutes),
                true,
                model.interval_minutes == minutes,
                None,
            ),
        });
        let interval_menu = Submenu::new(t!("menu.interval").as_ref(), true);
        for interval in &interval_items {
            interval_menu.append(&interval.item)?;
        }
        let check_now_item = MenuItem::new(t!("menu.check_now").as_ref(), true, None);
        let show_network_speed_item = CheckMenuItem::new(
            t!("menu.show_network_speed").as_ref(),
            true,
            model.is_show_network_speed,
            None,
        );
        let show_network_latency_item = CheckMenuItem::new(
            t!("menu.show_network_latency").as_ref(),
            true,
            model.is_show_network_latency,
            None,
        );
        let show_status_icon_item = CheckMenuItem::new(
            t!("menu.show_status_icon").as_ref(),
            true,
            model.is_show_status_icon,
            None,
        );
        let daily_ip_log_item = CheckMenuItem::new(
            t!("menu.daily_ip_log").as_ref(),
            true,
            model.is_daily_ip_log_enabled,
            None,
        );
        let change_daily_ip_log_directory_item = MenuItem::new(
            t!("menu.change_daily_ip_log_directory").as_ref(),
            true,
            None,
        );
        let mute_item =
            CheckMenuItem::new(t!("menu.mute_session").as_ref(), true, model.muted, None);
        let check_for_updates_item =
            MenuItem::new(t!("menu.check_for_updates").as_ref(), true, None);
        let about_item = MenuItem::new(t!("menu.about").as_ref(), true, None);
        let quit_item = MenuItem::new(t!("menu.quit").as_ref(), true, None);
        let separators = [
            PredefinedMenuItem::separator(),
            PredefinedMenuItem::separator(),
            PredefinedMenuItem::separator(),
            PredefinedMenuItem::separator(),
            PredefinedMenuItem::separator(),
        ];

        menu.append_items(&[
            &current_item,
            &expected_item,
            &separators[0],
            &set_expected_from_input_item,
            &use_current_ip_item,
            &separators[1],
            &interval_menu,
            &check_now_item,
            &separators[2],
            &show_network_speed_item,
            &show_network_latency_item,
            &show_status_icon_item,
            &daily_ip_log_item,
            &change_daily_ip_log_directory_item,
            &mute_item,
            &separators[3],
            &check_for_updates_item,
            &about_item,
            &separators[4],
            &quit_item,
        ])?;

        let icons = IconSet::new()?;
        let tray = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip(&model.tooltip)
            .with_icon(icons.for_state(model.icon_state).clone())
            .with_icon_as_template(true)
            .build()?;
        #[cfg(target_os = "macos")]
        let speed_title_view = install_speed_title_view(&tray);

        let ui = Self {
            tray,
            current_item,
            expected_item,
            set_expected_from_input_item,
            use_current_ip_item,
            interval_items,
            check_now_item,
            show_network_speed_item,
            show_network_latency_item,
            show_status_icon_item,
            daily_ip_log_item,
            change_daily_ip_log_directory_item,
            mute_item,
            check_for_updates_item,
            about_item,
            quit_item,
            icons,
            #[cfg(target_os = "macos")]
            speed_title_view,
        };
        ui.set_network_speed(
            &NetworkSpeedLabels::unknown(),
            model.is_show_network_speed,
            model.is_show_network_latency,
            model.is_show_status_icon,
        );
        Ok(ui)
    }

    pub fn apply(&self, model: &UiModel) -> Result<(), UiError> {
        self.current_item.set_text(&model.current_title);
        self.current_item.set_enabled(model.can_copy_current_ip);
        self.expected_item.set_text(&model.expected_title);
        self.use_current_ip_item
            .set_enabled(model.can_use_current_ip);
        for interval in &self.interval_items {
            interval
                .item
                .set_checked(interval.minutes == model.interval_minutes);
        }
        self.show_network_speed_item
            .set_checked(model.is_show_network_speed);
        self.show_network_speed_item
            .set_enabled(model.can_toggle_show_network_speed);
        self.show_network_latency_item
            .set_checked(model.is_show_network_latency);
        self.show_network_latency_item
            .set_enabled(model.can_toggle_show_network_latency);
        self.show_status_icon_item
            .set_checked(model.is_show_status_icon);
        self.show_status_icon_item
            .set_enabled(model.can_toggle_show_status_icon);
        self.daily_ip_log_item
            .set_checked(model.is_daily_ip_log_enabled);
        self.mute_item.set_checked(model.muted);
        self.tray.set_tooltip(Some(&model.tooltip))?;
        self.tray.set_icon_with_as_template(
            Some(self.icons.for_state(model.icon_state).clone()),
            true,
        )?;
        Ok(())
    }

    pub fn set_current_title(&self, title: &str) {
        self.current_item.set_text(title);
    }

    pub fn set_network_speed(
        &self,
        labels: &NetworkSpeedLabels,
        is_show_network_speed: bool,
        is_show_network_latency: bool,
        is_show_status_icon: bool,
    ) {
        #[cfg(target_os = "macos")]
        set_tray_speed_title(
            &self.tray,
            &self.speed_title_view,
            labels,
            is_show_network_speed,
            is_show_network_latency,
            is_show_status_icon,
        );
        #[cfg(not(target_os = "macos"))]
        {
            let _ = labels;
            let _ = is_show_network_speed;
            let _ = is_show_network_latency;
            let _ = is_show_status_icon;
        }
    }

    pub fn set_check_for_updates_enabled(&self, enabled: bool) {
        self.check_for_updates_item.set_enabled(enabled);
    }

    pub fn menu_action(&self, id: &MenuId) -> Option<MenuAction> {
        if id == self.current_item.id() {
            return Some(MenuAction::CopyCurrentIp);
        }
        if id == self.set_expected_from_input_item.id() {
            return Some(MenuAction::SetExpectedFromInput);
        }
        if id == self.use_current_ip_item.id() {
            return Some(MenuAction::UseCurrentIp);
        }
        if id == self.check_now_item.id() {
            return Some(MenuAction::CheckNow);
        }
        if id == self.show_network_speed_item.id() {
            return Some(MenuAction::ToggleShowNetworkSpeed);
        }
        if id == self.show_network_latency_item.id() {
            return Some(MenuAction::ToggleShowNetworkLatency);
        }
        if id == self.show_status_icon_item.id() {
            return Some(MenuAction::ToggleShowStatusIcon);
        }
        if id == self.daily_ip_log_item.id() {
            return Some(MenuAction::SetDailyIpLogEnabled(
                self.daily_ip_log_item.is_checked(),
            ));
        }
        if id == self.change_daily_ip_log_directory_item.id() {
            return Some(MenuAction::ChangeDailyIpLogDirectory);
        }
        if id == self.mute_item.id() {
            return Some(MenuAction::ToggleMuted);
        }
        if id == self.check_for_updates_item.id() {
            return Some(MenuAction::CheckForUpdates);
        }
        if id == self.about_item.id() {
            return Some(MenuAction::About);
        }
        if id == self.quit_item.id() {
            return Some(MenuAction::Quit);
        }

        self.interval_items
            .iter()
            .find(|interval| id == interval.item.id())
            .map(|interval| MenuAction::SetInterval(interval.minutes))
    }
}
