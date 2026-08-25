#[cfg(target_os = "macos")]
use std::cell::RefCell;

#[cfg(target_os = "macos")]
use crate::net_latency::LatencyLevel;
use crate::{
    config::{ALLOWED_INTERVAL_MINUTES, Config},
    monitor::{MonitorOutcome, MonitorState},
    net_speed::{NetworkSpeedLabels, TRAY_LATENCY_WIDTH_TEMPLATE},
    session::Session,
};
use tray_icon::{
    BadIcon, Icon, TrayIcon, TrayIconBuilder,
    menu::{CheckMenuItem, Menu, MenuId, MenuItem, PredefinedMenuItem, Submenu},
};

#[cfg(target_os = "macos")]
use objc2::rc::Retained;
#[cfg(target_os = "macos")]
use objc2::runtime::AnyObject;
#[cfg(target_os = "macos")]
use objc2::{AnyThread, DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send};
#[cfg(target_os = "macos")]
use objc2_app_kit::{
    NSAttributedStringNSStringDrawing, NSBezierPath, NSCellImagePosition, NSColor,
    NSCompositingOperation, NSFont, NSFontAttributeName, NSForegroundColorAttributeName,
    NSGraphicsContext, NSImage, NSMutableParagraphStyle, NSParagraphStyleAttributeName, NSRectFill,
    NSTextAlignment, NSTextTab, NSTextTabOptionKey, NSView,
};
#[cfg(target_os = "macos")]
use objc2_foundation::{
    NSArray, NSAttributedString, NSAttributedStringKey, NSDictionary, NSPoint, NSRect, NSSize,
    NSString,
};

/// Installs an app `Edit` menu so ⌘X/C/V/A work in dialogs.
///
/// Agent apps (`LSUIElement` / `ActivationPolicy::Accessory`) have no default
/// Edit menu; without these key equivalents, `NSTextField` accepts typing but
/// not paste/copy/cut shortcuts (context-menu Paste still works).
pub fn install_app_edit_menu() -> Result<Menu, UiError> {
    let menu = Menu::new();
    let edit = Submenu::new("Edit", true);
    edit.append(&PredefinedMenuItem::cut(None))?;
    edit.append(&PredefinedMenuItem::copy(None))?;
    edit.append(&PredefinedMenuItem::paste(None))?;
    edit.append(&PredefinedMenuItem::select_all(None))?;
    menu.append(&edit)?;
    menu.init_for_nsapp();
    Ok(menu)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IconState {
    Normal,
    Alert,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UiModel {
    pub current_title: String,
    pub expected_title: String,
    pub can_use_current_ip: bool,
    pub can_copy_current_ip: bool,
    pub interval_minutes: u64,
    pub muted: bool,
    pub is_show_network_speed: bool,
    pub is_show_network_latency: bool,
    pub is_show_status_icon: bool,
    pub can_toggle_show_network_speed: bool,
    pub can_toggle_show_network_latency: bool,
    pub can_toggle_show_status_icon: bool,
    pub icon_state: IconState,
    pub tooltip: String,
}

impl UiModel {
    pub fn from_state(config: &Config, session: &Session, outcome: &MonitorOutcome) -> Self {
        let display_ip = outcome.current_ip.or(outcome.last_success_ip);
        let expected_value = config
            .expected_ip
            .map(|ip| ip.to_string())
            .unwrap_or_else(|| t!("status.unset").to_string());
        let (current_title, tooltip) = match (outcome.current_ip, outcome.last_success_ip) {
            (Some(current), _) => (
                t!("status.current_ip", ip = current).to_string(),
                t!(
                    "status.tooltip",
                    current = current,
                    expected = expected_value
                )
                .to_string(),
            ),
            (None, Some(last_success)) => (
                t!("status.last_success_ip", ip = last_success).to_string(),
                t!(
                    "status.tooltip_last",
                    current = last_success,
                    expected = expected_value
                )
                .to_string(),
            ),
            (None, None) => (
                t!("status.current_ip", ip = t!("status.unknown")).to_string(),
                t!("status.tooltip", current = "—", expected = expected_value).to_string(),
            ),
        };
        let icon_state = match outcome.state {
            MonitorState::Matched | MonitorState::Unconfigured => IconState::Normal,
            MonitorState::Mismatched => IconState::Alert,
            MonitorState::Unknown => IconState::Unknown,
        };

        Self {
            current_title,
            expected_title: t!("status.expected_ip", ip = expected_value).to_string(),
            can_use_current_ip: outcome.current_ip.is_some(),
            can_copy_current_ip: display_ip.is_some(),
            interval_minutes: config.interval_minutes,
            muted: session.is_muted(),
            is_show_network_speed: config.is_show_network_speed,
            is_show_network_latency: config.is_show_network_latency,
            is_show_status_icon: config.is_show_status_icon,
            can_toggle_show_network_speed: !config.is_show_network_speed
                || config.is_show_network_latency
                || config.is_show_status_icon,
            can_toggle_show_network_latency: !config.is_show_network_latency
                || config.is_show_network_speed
                || config.is_show_status_icon,
            can_toggle_show_status_icon: !config.is_show_status_icon
                || config.is_show_network_speed
                || config.is_show_network_latency,
            icon_state,
            tooltip,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuAction {
    CopyCurrentIp,
    SetExpectedFromInput,
    UseCurrentIp,
    SetInterval(u64),
    CheckNow,
    ToggleMuted,
    ToggleShowNetworkSpeed,
    ToggleShowNetworkLatency,
    ToggleShowStatusIcon,
    CheckForUpdates,
    About,
    Quit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiCommand {
    CopyCurrentIp,
    SetExpectedFromInput,
    UseCurrentIp,
    SetInterval(u64),
    CheckNow,
    SetMuted(bool),
    SetShowNetworkSpeed(bool),
    SetShowNetworkLatency(bool),
    SetShowStatusIcon(bool),
    CheckForUpdates,
    About,
    Quit,
}

impl UiCommand {
    pub fn from_menu_action(
        action: MenuAction,
        is_muted: bool,
        is_show_network_speed: bool,
        is_show_network_latency: bool,
        is_show_status_icon: bool,
    ) -> Self {
        match action {
            MenuAction::CopyCurrentIp => Self::CopyCurrentIp,
            MenuAction::SetExpectedFromInput => Self::SetExpectedFromInput,
            MenuAction::UseCurrentIp => Self::UseCurrentIp,
            MenuAction::SetInterval(minutes) => Self::SetInterval(minutes),
            MenuAction::CheckNow => Self::CheckNow,
            MenuAction::ToggleMuted => Self::SetMuted(!is_muted),
            MenuAction::ToggleShowNetworkSpeed => Self::SetShowNetworkSpeed(!is_show_network_speed),
            MenuAction::ToggleShowNetworkLatency => {
                Self::SetShowNetworkLatency(!is_show_network_latency)
            }
            MenuAction::ToggleShowStatusIcon => Self::SetShowStatusIcon(!is_show_status_icon),
            MenuAction::CheckForUpdates => Self::CheckForUpdates,
            MenuAction::About => Self::About,
            MenuAction::Quit => Self::Quit,
        }
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct FeedbackRestoreGuard {
    generation: u64,
    active: Option<u64>,
}

impl FeedbackRestoreGuard {
    pub fn issue(&mut self) -> u64 {
        self.generation = self
            .generation
            .checked_add(1)
            .expect("feedback restore generation exhausted");
        self.active = Some(self.generation);
        self.generation
    }

    pub fn claim(&mut self, token: u64) -> bool {
        if self.active == Some(token) {
            self.active = None;
            true
        } else {
            false
        }
    }

    pub fn cancel(&mut self) {
        self.active = None;
    }
}

#[derive(Debug, thiserror::Error)]
pub enum UiError {
    #[error(transparent)]
    Menu(#[from] tray_icon::menu::Error),
    #[error(transparent)]
    Icon(#[from] BadIcon),
    #[error(transparent)]
    Tray(#[from] tray_icon::Error),
}

struct IntervalMenuItem {
    minutes: u64,
    item: CheckMenuItem,
}

struct IconSet {
    normal: Icon,
    alert: Icon,
    unknown: Icon,
}

impl IconSet {
    fn new() -> Result<Self, BadIcon> {
        Ok(Self {
            normal: Icon::from_rgba(icon_rgba_for_state(IconState::Normal), ICON_SIZE, ICON_SIZE)?,
            alert: Icon::from_rgba(icon_rgba_for_state(IconState::Alert), ICON_SIZE, ICON_SIZE)?,
            unknown: Icon::from_rgba(
                icon_rgba_for_state(IconState::Unknown),
                ICON_SIZE,
                ICON_SIZE,
            )?,
        })
    }

    fn for_state(&self, state: IconState) -> &Icon {
        match state {
            IconState::Normal => &self.normal,
            IconState::Alert => &self.alert,
            IconState::Unknown => &self.unknown,
        }
    }
}

const ICON_SIZE: u32 = 36;
const ICON_CENTER: f32 = (ICON_SIZE as f32 - 1.0) / 2.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IconGlyph {
    Check,
    Cross,
    Question,
}

/// Builds a 36×36 template RGBA buffer: filled disc with glyph knocked out.
pub fn icon_rgba_for_state(state: IconState) -> Vec<u8> {
    let glyph = match state {
        IconState::Normal => IconGlyph::Check,
        IconState::Alert => IconGlyph::Cross,
        IconState::Unknown => IconGlyph::Question,
    };
    template_icon_rgba(glyph)
}

fn template_icon_rgba(glyph: IconGlyph) -> Vec<u8> {
    let mut rgba = vec![0; (ICON_SIZE * ICON_SIZE * 4) as usize];
    draw_filled_disc(&mut rgba);
    match glyph {
        IconGlyph::Check => carve_check(&mut rgba),
        IconGlyph::Cross => carve_cross(&mut rgba),
        IconGlyph::Question => carve_question(&mut rgba),
    }
    rgba
}

fn blend_ink(rgba: &mut [u8], x: i32, y: i32, coverage: f32) {
    if coverage <= 0.0 || x < 0 || y < 0 || x >= ICON_SIZE as i32 || y >= ICON_SIZE as i32 {
        return;
    }
    let alpha = (coverage.clamp(0.0, 1.0) * 255.0).round() as u8;
    if alpha == 0 {
        return;
    }
    let index = ((y as u32 * ICON_SIZE + x as u32) * 4) as usize;
    if alpha <= rgba[index + 3] {
        return;
    }
    rgba[index] = 0;
    rgba[index + 1] = 0;
    rgba[index + 2] = 0;
    rgba[index + 3] = alpha;
}

fn carve_ink(rgba: &mut [u8], x: i32, y: i32, coverage: f32) {
    if coverage <= 0.0 || x < 0 || y < 0 || x >= ICON_SIZE as i32 || y >= ICON_SIZE as i32 {
        return;
    }
    let index = ((y as u32 * ICON_SIZE + x as u32) * 4) as usize;
    let keep = 1.0 - coverage.clamp(0.0, 1.0);
    let alpha = (rgba[index + 3] as f32 * keep).round() as u8;
    rgba[index + 3] = alpha;
    if alpha == 0 {
        rgba[index] = 0;
        rgba[index + 1] = 0;
        rgba[index + 2] = 0;
    }
}

fn coverage_from_distance(distance: f32, half_width: f32) -> f32 {
    let aa = 0.65_f32;
    let solid = (half_width - aa).max(0.0);
    let edge = half_width + aa;
    if distance <= solid {
        1.0
    } else if distance >= edge {
        0.0
    } else {
        1.0 - (distance - solid) / (edge - solid)
    }
}

fn disc_coverage(distance: f32, radius: f32) -> f32 {
    let aa = 0.65_f32;
    if distance <= radius - aa {
        1.0
    } else if distance >= radius + aa {
        0.0
    } else {
        1.0 - (distance - (radius - aa)) / (2.0 * aa)
    }
}

fn draw_filled_disc(rgba: &mut [u8]) {
    const RADIUS: f32 = 14.6;

    for y in 0..ICON_SIZE {
        for x in 0..ICON_SIZE {
            let dx = x as f32 - ICON_CENTER;
            let dy = y as f32 - ICON_CENTER;
            let coverage = disc_coverage((dx * dx + dy * dy).sqrt(), RADIUS);
            blend_ink(rgba, x as i32, y as i32, coverage);
        }
    }
}

fn carve_thick_segment(rgba: &mut [u8], x0: f32, y0: f32, x1: f32, y1: f32, thickness: f32) {
    let steps = (((x1 - x0).abs().max((y1 - y0).abs()) * 3.0).ceil() as i32).max(1);
    let half = thickness / 2.0;
    for step in 0..=steps {
        let t = step as f32 / steps as f32;
        let cx = x0 + (x1 - x0) * t;
        let cy = y0 + (y1 - y0) * t;
        let min_x = (cx - half - 1.0).floor() as i32;
        let max_x = (cx + half + 1.0).ceil() as i32;
        let min_y = (cy - half - 1.0).floor() as i32;
        let max_y = (cy + half + 1.0).ceil() as i32;
        for y in min_y..=max_y {
            for x in min_x..=max_x {
                let dx = x as f32 - cx;
                let dy = y as f32 - cy;
                let coverage = coverage_from_distance((dx * dx + dy * dy).sqrt(), half);
                carve_ink(rgba, x, y, coverage);
            }
        }
    }
}

fn carve_check(rgba: &mut [u8]) {
    carve_thick_segment(rgba, 10.0, 18.0, 16.0, 24.0, 2.6);
    carve_thick_segment(rgba, 16.0, 24.0, 26.0, 12.0, 2.6);
}

fn carve_cross(rgba: &mut [u8]) {
    carve_thick_segment(rgba, 12.0, 12.0, 24.0, 24.0, 2.6);
    carve_thick_segment(rgba, 24.0, 12.0, 12.0, 24.0, 2.6);
}

fn carve_question(rgba: &mut [u8]) {
    carve_thick_segment(rgba, 14.0, 12.0, 18.0, 10.0, 2.4);
    carve_thick_segment(rgba, 18.0, 10.0, 22.0, 12.0, 2.4);
    carve_thick_segment(rgba, 22.0, 12.0, 22.0, 16.0, 2.4);
    carve_thick_segment(rgba, 22.0, 16.0, 18.0, 19.0, 2.4);
    carve_thick_segment(rgba, 18.0, 19.0, 18.0, 22.0, 2.4);
    carve_thick_segment(rgba, 17.0, 25.0, 19.0, 25.0, 2.4);
    carve_thick_segment(rgba, 18.0, 24.0, 18.0, 26.0, 2.4);
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

#[cfg(target_os = "macos")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TrayMetricsVisibility {
    is_show_network_latency: bool,
    is_show_network_speed: bool,
    is_show_status_icon: bool,
}

#[cfg(target_os = "macos")]
const TRAY_ICON_POINTS: f64 = 18.0;
#[cfg(target_os = "macos")]
const TRAY_ICON_TEXT_GAP: f64 = 2.0;
/// Extra points after measured content so multi-display scale rounding
/// cannot clip the trailing unit character (e.g. the "s" in "KB/s").
#[cfg(target_os = "macos")]
const TRAY_TRAILING_PAD: f64 = 2.0;
#[cfg(target_os = "macos")]
const SPEED_FONT_SIZE: f64 = 9.0;
#[cfg(target_os = "macos")]
const SPEED_LINE_HEIGHT: f64 = 10.0;
#[cfg(target_os = "macos")]
const LATENCY_DOT_DIAMETER: f64 = 6.0;

#[cfg(target_os = "macos")]
#[derive(Debug)]
struct SpeedTitleIvars {
    speed: RefCell<Retained<NSAttributedString>>,
    latency: RefCell<Retained<NSAttributedString>>,
    latency_level: RefCell<LatencyLevel>,
    latency_column_width: RefCell<f64>,
    is_show_network_latency: RefCell<bool>,
    is_show_network_speed: RefCell<bool>,
    is_show_status_icon: RefCell<bool>,
    icon: RefCell<Option<Retained<NSImage>>>,
    last_latency_text: RefCell<String>,
    last_speed_text: RefCell<String>,
}

#[cfg(target_os = "macos")]
define_class!(
    #[unsafe(super(NSView))]
    #[thread_kind = MainThreadOnly]
    #[name = "IpcheckerSpeedTitleView"]
    #[ivars = SpeedTitleIvars]
    struct SpeedTitleView;

    impl SpeedTitleView {
        #[unsafe(method(drawRect:))]
        fn draw_rect(&self, _dirty_rect: NSRect) {
            let bounds = self.bounds();
            let ivars = self.ivars();
            let icon = ivars.icon.borrow();
            let speed = ivars.speed.borrow();
            let latency = ivars.latency.borrow();

            let latency_column_width = *ivars.latency_column_width.borrow();
            let latency_level = *ivars.latency_level.borrow();
            let is_show_network_latency = *ivars.is_show_network_latency.borrow();
            let is_show_network_speed = *ivars.is_show_network_speed.borrow();
            let is_show_status_icon = *ivars.is_show_status_icon.borrow();
            let latency_size = latency.size();
            let icon_size = if is_show_status_icon {
                icon
                    .as_ref()
                    .map(|image| image.size())
                    .unwrap_or(NSSize::new(TRAY_ICON_POINTS, TRAY_ICON_POINTS))
            } else {
                NSSize::new(0.0, TRAY_ICON_POINTS)
            };
            let icon_y = ((bounds.size.height - icon_size.height) / 2.0).round();
            let mut x = 0.0;
            let mut latency_dot_rect = None;
            let mut latency_text_point = None;
            if is_show_network_latency && latency_column_width > 0.0 && latency_size.width > 0.0 {
                let stack_height = SPEED_LINE_HEIGHT * 2.0;
                let stack_y = ((bounds.size.height - stack_height) / 2.0).round().max(0.0);
                let text_x = ((latency_column_width - latency_size.width) / 2.0).max(0.0);
                let circle_x = ((latency_column_width - LATENCY_DOT_DIAMETER) / 2.0).max(0.0);
                // Non-flipped NSView: higher y is toward the menu-bar top.
                let circle_y = stack_y
                    + SPEED_LINE_HEIGHT
                    + (SPEED_LINE_HEIGHT - LATENCY_DOT_DIAMETER) / 2.0;
                latency_dot_rect = Some(pixel_aligned_rect(
                    NSRect::new(
                        NSPoint::new(circle_x, circle_y),
                        NSSize::new(LATENCY_DOT_DIAMETER, LATENCY_DOT_DIAMETER),
                    ),
                    self.backing_scale_factor(),
                ));
                let text_y =
                    stack_y + ((SPEED_LINE_HEIGHT - latency_size.height) / 2.0).round();
                latency_text_point = Some(NSPoint::new(text_x, text_y));
                x = latency_column_width + TRAY_ICON_TEXT_GAP;
            }

            if is_show_status_icon {
                if let Some(image) = icon.as_ref() {
                    let icon_rect = NSRect::new(NSPoint::new(x, icon_y), icon_size);
                    draw_tray_template_icon(image, icon_rect);
                }
                x += icon_size.width + TRAY_ICON_TEXT_GAP;
            }

            if is_show_network_speed {
                let speed_size = speed.size();
                let speed_y = (icon_y + (icon_size.height - speed_size.height) / 2.0).round();
                speed.drawAtPoint(NSPoint::new(x, speed_y));
            }

            if let Some(text_point) = latency_text_point {
                latency.drawAtPoint(text_point);
            }
            if let Some(dot_rect) = latency_dot_rect {
                draw_latency_dot(dot_rect, latency_level);
            }
        }

        #[unsafe(method(isFlipped))]
        fn is_flipped(&self) -> bool {
            false
        }

        #[unsafe(method(allowsVibrancy))]
        fn allows_vibrancy(&self) -> bool {
            // Saturated latency dots flicker when composited through vibrancy on light menu bars.
            false
        }

        #[unsafe(method_id(hitTest:))]
        fn hit_test(&self, _point: NSPoint) -> Option<Retained<NSView>> {
            None
        }
    }
);

#[cfg(target_os = "macos")]
impl SpeedTitleView {
    fn new(mtm: MainThreadMarker, speed: Retained<NSAttributedString>) -> Retained<Self> {
        let view = mtm.alloc().set_ivars(SpeedTitleIvars {
            speed: RefCell::new(speed),
            latency: RefCell::new(speed_attributed_title("", &NSColor::labelColor())),
            latency_level: RefCell::new(LatencyLevel::High),
            latency_column_width: RefCell::new(0.0),
            is_show_network_latency: RefCell::new(true),
            is_show_network_speed: RefCell::new(true),
            is_show_status_icon: RefCell::new(true),
            icon: RefCell::new(None),
            last_latency_text: RefCell::new(String::new()),
            last_speed_text: RefCell::new(String::new()),
        });
        unsafe { msg_send![super(view), init] }
    }

    fn set_speed_labels(
        &self,
        speed: Retained<NSAttributedString>,
        latency: Retained<NSAttributedString>,
        latency_level: LatencyLevel,
        latency_column_width: f64,
        visibility: TrayMetricsVisibility,
    ) {
        let latency_text = latency.string().to_string();
        let speed_text = speed.string().to_string();
        let ivars = self.ivars();
        if *ivars.is_show_network_latency.borrow() == visibility.is_show_network_latency
            && *ivars.is_show_network_speed.borrow() == visibility.is_show_network_speed
            && *ivars.is_show_status_icon.borrow() == visibility.is_show_status_icon
            && *ivars.latency_level.borrow() == latency_level
            && (*ivars.latency_column_width.borrow() - latency_column_width).abs() < f64::EPSILON
            && *ivars.last_latency_text.borrow() == latency_text
            && *ivars.last_speed_text.borrow() == speed_text
        {
            return;
        }

        *ivars.speed.borrow_mut() = speed;
        *ivars.latency.borrow_mut() = latency;
        *ivars.latency_level.borrow_mut() = latency_level;
        *ivars.latency_column_width.borrow_mut() = latency_column_width;
        *ivars.is_show_network_latency.borrow_mut() = visibility.is_show_network_latency;
        *ivars.is_show_network_speed.borrow_mut() = visibility.is_show_network_speed;
        *ivars.is_show_status_icon.borrow_mut() = visibility.is_show_status_icon;
        *ivars.last_latency_text.borrow_mut() = latency_text;
        *ivars.last_speed_text.borrow_mut() = speed_text;
        self.setNeedsDisplay(true);
    }

    fn set_icon(&self, icon: Retained<NSImage>) {
        *self.ivars().icon.borrow_mut() = Some(icon);
        self.setNeedsDisplay(true);
    }

    fn clear_icon(&self) {
        *self.ivars().icon.borrow_mut() = None;
        self.setNeedsDisplay(true);
    }

    fn icon_size(&self) -> NSSize {
        let ivars = self.ivars();
        if !*ivars.is_show_status_icon.borrow() {
            return NSSize::new(0.0, TRAY_ICON_POINTS);
        }
        ivars
            .icon
            .borrow()
            .as_ref()
            .map(|image| image.size())
            .unwrap_or(NSSize::new(TRAY_ICON_POINTS, TRAY_ICON_POINTS))
    }

    fn backing_scale_factor(&self) -> f64 {
        self.window()
            .map(|window| window.backingScaleFactor())
            .unwrap_or(2.0)
    }
}

#[cfg(target_os = "macos")]
fn install_speed_title_view(tray: &TrayIcon) -> Retained<SpeedTitleView> {
    let mtm = MainThreadMarker::new().expect("tray UI is created on the main thread");
    let view = SpeedTitleView::new(mtm, speed_attributed_title("", &NSColor::labelColor()));
    if let Some(status_item) = tray.ns_status_item()
        && let Some(button) = status_item.button(mtm)
    {
        button.setTitle(&NSString::from_str(""));
        button.setImagePosition(NSCellImagePosition::NoImage);
        button.addSubview(&view);
    }
    view
}

#[cfg(target_os = "macos")]
fn set_tray_speed_title(
    tray: &TrayIcon,
    view: &SpeedTitleView,
    labels: &NetworkSpeedLabels,
    is_show_network_speed: bool,
    is_show_network_latency: bool,
    is_show_status_icon: bool,
) {
    let Some(mtm) = MainThreadMarker::new() else {
        log::warn!("skipped tray speed title; not on the main thread");
        return;
    };
    let Some(status_item) = tray.ns_status_item() else {
        return;
    };
    let Some(button) = status_item.button(mtm) else {
        return;
    };

    let is_metrics_visible = is_show_network_speed || is_show_network_latency;
    if !is_metrics_visible {
        view.setHidden(true);
        status_item.setLength(-1.0);
        return;
    }

    view.setHidden(false);
    button.setTitle(&NSString::from_str(""));
    button.setImagePosition(NSCellImagePosition::NoImage);
    if is_show_status_icon {
        if let Some(image) = button.image() {
            view.set_icon(image);
            button.setImage(None);
        }
    } else {
        view.clear_icon();
        button.setImage(None);
    }
    if unsafe { view.superview() }.is_none() {
        button.addSubview(view);
    }

    let color = NSColor::labelColor();
    let speed_attributed = speed_attributed_title(&labels.speed_tray_title(), &color);
    let latency_attributed = speed_attributed_title(labels.latency_tray_title(), &color);
    let latency_template = speed_attributed_title(TRAY_LATENCY_WIDTH_TEMPLATE, &color);
    let speed_width = if is_show_network_speed {
        speed_attributed.size().width
    } else {
        0.0
    };
    let latency_column_width = if is_show_network_latency {
        latency_template
            .size()
            .width
            .max(latency_attributed.size().width)
    } else {
        0.0
    };
    let icon_size = view.icon_size();
    let mut content_width = 0.0;
    if is_show_network_latency {
        content_width += latency_column_width;
    }
    if is_show_status_icon {
        if is_show_network_latency {
            content_width += TRAY_ICON_TEXT_GAP;
        }
        content_width += icon_size.width;
    }
    if is_show_network_speed {
        if is_show_network_latency || is_show_status_icon {
            content_width += TRAY_ICON_TEXT_GAP;
        }
        content_width += speed_width;
    }
    // Do not shrink below measured content: an earlier trailing trim clipped
    // "KB/s" on external displays with a different backing scale.
    let width = (content_width + TRAY_TRAILING_PAD).max(if is_show_status_icon {
        icon_size.width
    } else {
        0.0
    });
    status_item.setLength(width);
    view.set_speed_labels(
        speed_attributed,
        latency_attributed,
        labels.latency.level,
        latency_column_width,
        TrayMetricsVisibility {
            is_show_network_latency,
            is_show_network_speed,
            is_show_status_icon,
        },
    );
    let button_height = button.bounds().size.height;
    let frame_width = button.bounds().size.width.max(width);
    view.setFrame(NSRect::new(
        NSPoint::new(0.0, 0.0),
        NSSize::new(frame_width, button_height),
    ));
}

#[cfg(target_os = "macos")]
fn draw_tray_template_icon(image: &NSImage, icon_rect: NSRect) {
    let clip_rect = icon_disc_rect(icon_rect);
    if let Some(context) = NSGraphicsContext::currentContext() {
        context.saveGraphicsState();
    }
    NSBezierPath::bezierPathWithOvalInRect(clip_rect).addClip();
    // Template icons don't auto-tint in custom NSViews; mask labelColor with image alpha.
    NSColor::labelColor().set();
    NSRectFill(icon_rect);
    image.drawInRect_fromRect_operation_fraction(
        icon_rect,
        NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(0.0, 0.0)),
        NSCompositingOperation::DestinationIn,
        1.0,
    );
    if let Some(context) = NSGraphicsContext::currentContext() {
        context.restoreGraphicsState();
    }
}

#[cfg(target_os = "macos")]
fn icon_disc_rect(icon_rect: NSRect) -> NSRect {
    // Match the carved 36pt disc scaled into the 18pt tray icon rect.
    const DISC_INSET: f64 = 0.9;
    NSRect::new(
        NSPoint::new(
            icon_rect.origin.x + DISC_INSET,
            icon_rect.origin.y + DISC_INSET,
        ),
        NSSize::new(
            (icon_rect.size.width - DISC_INSET * 2.0).max(0.0),
            (icon_rect.size.height - DISC_INSET * 2.0).max(0.0),
        ),
    )
}

#[cfg(target_os = "macos")]
fn pixel_aligned_rect(rect: NSRect, scale: f64) -> NSRect {
    if scale <= 0.0 {
        return rect;
    }
    let align = |value: f64| (value * scale).round() / scale;
    NSRect::new(
        NSPoint::new(align(rect.origin.x), align(rect.origin.y)),
        NSSize::new(
            align(rect.size.width).max(1.0 / scale),
            align(rect.size.height).max(1.0 / scale),
        ),
    )
}

#[cfg(target_os = "macos")]
fn draw_latency_dot(rect: NSRect, level: LatencyLevel) {
    let color = match level {
        LatencyLevel::Low => NSColor::systemGreenColor(),
        LatencyLevel::Medium => NSColor::systemYellowColor(),
        LatencyLevel::High => NSColor::systemRedColor(),
    };
    color.setFill();
    NSBezierPath::bezierPathWithOvalInRect(rect).fill();
}

#[cfg(target_os = "macos")]
fn speed_attributed_title(title: &str, color: &NSColor) -> Retained<NSAttributedString> {
    let font = NSFont::systemFontOfSize(SPEED_FONT_SIZE);
    let paragraph = speed_paragraph_style(&font, title);
    let font_obj: &AnyObject = font.as_ref();
    let paragraph_obj: &AnyObject = paragraph.as_ref();
    let color_obj: &AnyObject = color.as_ref();
    let attrs = NSDictionary::<NSAttributedStringKey, AnyObject>::from_slices(
        &[
            unsafe { NSFontAttributeName },
            unsafe { NSParagraphStyleAttributeName },
            unsafe { NSForegroundColorAttributeName },
        ],
        &[font_obj, paragraph_obj, color_obj],
    );
    unsafe {
        NSAttributedString::initWithString_attributes(
            NSAttributedString::alloc(),
            &NSString::from_str(title),
            Some(&attrs),
        )
    }
}

#[cfg(target_os = "macos")]
fn speed_paragraph_style(font: &NSFont, title: &str) -> Retained<NSMutableParagraphStyle> {
    let arrow_width = font_width("↑", font).max(font_width("↓", font));
    let number_width = title_number_width(title, font);
    let gap = font_width(" ", font);
    let number_tab_x = arrow_width + number_width;
    let unit_tab_x = number_tab_x + gap;

    let options = NSDictionary::<NSTextTabOptionKey, AnyObject>::from_slices(
        &[] as &[&NSString],
        &[] as &[&AnyObject],
    );
    let number_tab = unsafe {
        NSTextTab::initWithTextAlignment_location_options(
            NSTextTab::alloc(),
            NSTextAlignment::Right,
            number_tab_x,
            &options,
        )
    };
    let unit_tab = unsafe {
        NSTextTab::initWithTextAlignment_location_options(
            NSTextTab::alloc(),
            NSTextAlignment::Left,
            unit_tab_x,
            &options,
        )
    };

    let paragraph = NSMutableParagraphStyle::new();
    paragraph.setAlignment(NSTextAlignment::Left);
    paragraph.setLineSpacing(0.0);
    paragraph.setParagraphSpacing(0.0);
    paragraph.setMinimumLineHeight(SPEED_LINE_HEIGHT);
    paragraph.setMaximumLineHeight(SPEED_LINE_HEIGHT);
    paragraph.setDefaultTabInterval(0.0);
    paragraph.setTabStops(Some(&NSArray::from_retained_slice(&[number_tab, unit_tab])));
    paragraph
}

#[cfg(target_os = "macos")]
fn title_number_width(title: &str, font: &NSFont) -> f64 {
    title
        .lines()
        .filter_map(|line| line.split('\t').nth(1))
        .map(|number| font_width(number, font))
        .fold(font_width("99.9", font), f64::max)
}

#[cfg(target_os = "macos")]
fn font_width(text: &str, font: &NSFont) -> f64 {
    let font_obj: &AnyObject = font.as_ref();
    let attrs = NSDictionary::<NSAttributedStringKey, AnyObject>::from_slices(
        &[unsafe { NSFontAttributeName }],
        &[font_obj],
    );
    let attributed = unsafe {
        NSAttributedString::initWithString_attributes(
            NSAttributedString::alloc(),
            &NSString::from_str(text),
            Some(&attrs),
        )
    };
    attributed.size().width
}
