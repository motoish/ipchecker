use crate::{
    config::{ALLOWED_INTERVAL_MINUTES, Config},
    monitor::{MonitorOutcome, MonitorState},
    session::Session,
};
use tray_icon::{
    BadIcon, Icon, TrayIcon, TrayIconBuilder,
    menu::{CheckMenuItem, Menu, MenuId, MenuItem, PredefinedMenuItem, Submenu},
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
    pub icon_state: IconState,
    pub tooltip: String,
}

impl UiModel {
    pub fn from_state(config: &Config, session: &Session, outcome: &MonitorOutcome) -> Self {
        let display_ip = outcome.current_ip.or(outcome.last_success_ip);
        let current_value = display_ip
            .map(|ip| ip.to_string())
            .unwrap_or_else(|| t!("status.unknown").to_string());
        let expected_value = config
            .expected_ip
            .map(|ip| ip.to_string())
            .unwrap_or_else(|| t!("status.unset").to_string());
        let tooltip_current = display_ip
            .map(|ip| ip.to_string())
            .unwrap_or_else(|| "—".to_owned());
        let icon_state = match outcome.state {
            MonitorState::Matched | MonitorState::Unconfigured => IconState::Normal,
            MonitorState::Mismatched => IconState::Alert,
            MonitorState::Unknown => IconState::Unknown,
        };

        Self {
            current_title: t!("status.current_ip", ip = current_value).to_string(),
            expected_title: t!("status.expected_ip", ip = expected_value).to_string(),
            can_use_current_ip: outcome.last_success_ip.is_some(),
            can_copy_current_ip: display_ip.is_some(),
            interval_minutes: config.interval_minutes,
            muted: session.is_muted(),
            icon_state,
            tooltip: t!(
                "status.tooltip",
                current = tooltip_current,
                expected = expected_value
            )
            .to_string(),
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
    About,
    Quit,
}

impl UiCommand {
    pub fn from_menu_action(action: MenuAction, currently_muted: bool) -> Self {
        match action {
            MenuAction::CopyCurrentIp => Self::CopyCurrentIp,
            MenuAction::SetExpectedFromInput => Self::SetExpectedFromInput,
            MenuAction::UseCurrentIp => Self::UseCurrentIp,
            MenuAction::SetInterval(minutes) => Self::SetInterval(minutes),
            MenuAction::CheckNow => Self::CheckNow,
            MenuAction::ToggleMuted => Self::SetMuted(!currently_muted),
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
    mute_item: CheckMenuItem,
    about_item: MenuItem,
    quit_item: MenuItem,
    icons: IconSet,
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
        let mute_item =
            CheckMenuItem::new(t!("menu.mute_session").as_ref(), true, model.muted, None);
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
            &mute_item,
            &separators[3],
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

        Ok(Self {
            tray,
            current_item,
            expected_item,
            set_expected_from_input_item,
            use_current_ip_item,
            interval_items,
            check_now_item,
            mute_item,
            about_item,
            quit_item,
            icons,
        })
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
        if id == self.mute_item.id() {
            return Some(MenuAction::ToggleMuted);
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
