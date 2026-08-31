mod icon;
#[cfg(target_os = "macos")]
mod macos_metrics;
mod model;
mod tray;

pub use icon::icon_rgba_for_state;
pub use model::{FeedbackRestoreGuard, IconState, MenuAction, UiCommand, UiModel};
pub use tray::TrayUi;

use tray_icon::menu::{Menu, PredefinedMenuItem, Submenu};

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

#[derive(Debug, thiserror::Error)]
pub enum UiError {
    #[error(transparent)]
    Menu(#[from] tray_icon::menu::Error),
    #[error(transparent)]
    Icon(#[from] tray_icon::BadIcon),
    #[error(transparent)]
    Tray(#[from] tray_icon::Error),
}
