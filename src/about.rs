#[cfg(target_os = "macos")]
use objc2::rc::Retained;
#[cfg(target_os = "macos")]
use objc2::runtime::ProtocolObject;
#[cfg(target_os = "macos")]
use objc2::{AllocAnyThread, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
#[cfg(target_os = "macos")]
use objc2_app_kit::{
    NSApplication, NSBackingStoreType, NSBezelStyle, NSButton, NSButtonType, NSColor, NSFont,
    NSImage, NSImageView, NSLineBreakMode, NSPanel, NSTextAlignment, NSTextField, NSView,
    NSWindowDelegate, NSWindowStyleMask, NSWindowTitleVisibility,
};
#[cfg(target_os = "macos")]
use objc2_foundation::{
    NSData, NSNotification, NSObject, NSObjectProtocol, NSPoint, NSRect, NSSize, NSString,
};

/// Compact app mark embedded so About works even outside a .app bundle.
#[cfg(target_os = "macos")]
const ABOUT_ICON_PNG: &[u8] = include_bytes!("../resources/about-icon.png");

const PACKAGE_VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn about_message_title() -> &'static str {
    "ipchecker"
}

pub fn marketing_version() -> &'static str {
    PACKAGE_VERSION
        .split_once('-')
        .map(|(head, _)| head)
        .unwrap_or(PACKAGE_VERSION)
}

pub fn about_informative_text() -> String {
    t!("dialog.about_version", version = marketing_version()).to_string()
}

pub fn about_description() -> String {
    t!("dialog.about_body").to_string()
}

pub fn about_ok_label() -> String {
    t!("dialog.about_ok").to_string()
}

#[cfg(target_os = "macos")]
pub fn about_build_time_line() -> String {
    use objc2::rc::autoreleasepool;
    use objc2_foundation::{
        NSDate, NSDateFormatter, NSDateFormatterStyle, NSRelativeDateTimeFormatter,
        NSRelativeDateTimeFormatterStyle, NSRelativeDateTimeFormatterUnitsStyle,
    };

    let secs: f64 = env!("IPCHECKER_BUILD_UNIX_SECS")
        .parse()
        .expect("IPCHECKER_BUILD_UNIX_SECS must be a unix timestamp");
    let built_at = NSDate::dateWithTimeIntervalSince1970(secs);
    let absolute = NSDateFormatter::localizedStringFromDate_dateStyle_timeStyle(
        &built_at,
        NSDateFormatterStyle::MediumStyle,
        NSDateFormatterStyle::ShortStyle,
    );
    let relative_formatter = NSRelativeDateTimeFormatter::new();
    relative_formatter.setDateTimeStyle(NSRelativeDateTimeFormatterStyle::Numeric);
    relative_formatter.setUnitsStyle(NSRelativeDateTimeFormatterUnitsStyle::Full);
    let relative =
        relative_formatter.localizedStringForDate_relativeToDate(&built_at, &NSDate::date());

    autoreleasepool(|pool| {
        let absolute = unsafe { absolute.to_str(pool) };
        let relative = unsafe { relative.to_str(pool) };
        format!("{absolute} ({relative})")
    })
}

#[cfg(target_os = "macos")]
fn about_icon_image() -> Option<Retained<NSImage>> {
    // Always use the transparent glyph asset. The Finder AppIcon has an opaque
    // plate that shows up as a white box in the About window.
    let data = NSData::with_bytes(ABOUT_ICON_PNG);
    NSImage::initWithData(NSImage::alloc(), &data)
}

/// Sets `NSApplication.applicationIconImage` so system dialogs (About) show the mark.
#[cfg(target_os = "macos")]
pub fn apply_application_icon() {
    let Some(mtm) = MainThreadMarker::new() else {
        log::warn!("skipped application icon; not on the main thread");
        return;
    };
    let Some(icon) = about_icon_image() else {
        log::warn!("failed to load application icon for About dialog");
        return;
    };
    let app = NSApplication::sharedApplication(mtm);
    // SAFETY: icon is a valid NSImage retained for the call.
    unsafe { app.setApplicationIconImage(Some(&icon)) };
}

#[cfg(target_os = "macos")]
define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[ivars = ()]
    #[name = "IpcheckerAboutWindowDelegate"]
    struct AboutWindowDelegate;

    unsafe impl NSObjectProtocol for AboutWindowDelegate {}

    unsafe impl NSWindowDelegate for AboutWindowDelegate {
        #[unsafe(method(windowWillClose:))]
        fn window_will_close(&self, _notification: &NSNotification) {
            let mtm = MainThreadMarker::from(self);
            NSApplication::sharedApplication(mtm).stopModal();
        }
    }

    impl AboutWindowDelegate {
        #[unsafe(method(closeAbout:))]
        fn close_about(&self, sender: &NSView) {
            if let Some(window) = sender.window() {
                window.close();
            }
        }
    }
);

#[cfg(target_os = "macos")]
impl AboutWindowDelegate {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = mtm.alloc().set_ivars(());
        unsafe { msg_send![super(this), init] }
    }
}

#[cfg(target_os = "macos")]
fn about_label(
    mtm: MainThreadMarker,
    text: &str,
    font: &NSFont,
    color: &NSColor,
    max_width: f64,
) -> Retained<NSTextField> {
    let field = NSTextField::labelWithString(&NSString::from_str(text), mtm);
    field.setFont(Some(font));
    field.setTextColor(Some(color));
    field.setAlignment(NSTextAlignment::Center);
    field.setLineBreakMode(NSLineBreakMode::ByWordWrapping);
    field.setPreferredMaxLayoutWidth(max_width);
    field.sizeToFit();
    let mut frame = field.frame();
    frame.size.width = max_width;
    field.setFrame(frame);
    field
}

#[cfg(target_os = "macos")]
fn place_from_top(view: &NSView, top: f64, x: f64, content_height: f64) {
    let height = view.frame().size.height;
    view.setFrameOrigin(NSPoint::new(x, content_height - top - height));
}

#[cfg(target_os = "macos")]
pub fn show_about() {
    const WIDTH: f64 = 360.0;
    const PAD: f64 = 28.0;
    const ICON: f64 = 64.0;
    const TITLE_GAP: f64 = 8.0;
    const TEXT_GAP: f64 = 4.0;
    const BUTTON_GAP: f64 = 16.0;

    let mtm = MainThreadMarker::new().expect("about dialog must run on the main thread");
    let title = about_label(
        mtm,
        about_message_title(),
        &NSFont::boldSystemFontOfSize(20.0),
        &NSColor::labelColor(),
        WIDTH,
    );
    let version = about_label(
        mtm,
        &about_informative_text(),
        &NSFont::systemFontOfSize(13.0),
        &NSColor::labelColor(),
        WIDTH,
    );
    let date = about_label(
        mtm,
        &about_build_time_line(),
        &NSFont::systemFontOfSize(11.0),
        &NSColor::secondaryLabelColor(),
        WIDTH,
    );
    let description = about_label(
        mtm,
        &about_description(),
        &NSFont::systemFontOfSize(12.0),
        &NSColor::labelColor(),
        WIDTH,
    );

    let button = NSButton::new(mtm);
    button.setTitle(&NSString::from_str(&about_ok_label()));
    button.setBezelStyle(NSBezelStyle::Push);
    button.setButtonType(NSButtonType::MomentaryPushIn);
    button.setKeyEquivalent(&NSString::from_str("\r"));
    button.sizeToFit();
    let button_size = button.frame().size;
    let button_width = button_size.width.max(72.0);
    let button_height = button_size.height.max(32.0);
    button.setFrameSize(NSSize::new(button_width, button_height));

    let icon = about_icon_image().map(|image| {
        image.setSize(NSSize::new(ICON, ICON));
        let view = NSImageView::imageViewWithImage(&image, mtm);
        view.setFrame(NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(ICON, ICON)));
        view
    });

    let mut height = PAD;
    if icon.is_some() {
        height += ICON + 12.0;
    }
    height += title.frame().size.height + TITLE_GAP;
    height += version.frame().size.height + TEXT_GAP;
    height += date.frame().size.height + TEXT_GAP;
    height += description.frame().size.height + BUTTON_GAP;
    height += button_height + PAD;

    let panel = NSPanel::initWithContentRect_styleMask_backing_defer(
        mtm.alloc(),
        NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(WIDTH, height)),
        NSWindowStyleMask::Titled | NSWindowStyleMask::Closable,
        NSBackingStoreType::Buffered,
        false,
    );
    panel.setTitle(&NSString::from_str(about_message_title()));
    panel.setTitleVisibility(NSWindowTitleVisibility::Hidden);
    // SAFETY: the panel is owned by this `Retained` for the modal loop.
    unsafe { panel.setReleasedWhenClosed(false) };

    let Some(content) = panel.contentView() else {
        log::error!("About panel is missing a content view");
        return;
    };

    let mut top = PAD;
    if let Some(icon) = &icon {
        place_from_top(icon, top, ((WIDTH - ICON) / 2.0).round(), height);
        content.addSubview(icon);
        top += ICON + 12.0;
    }
    place_from_top(&title, top, 0.0, height);
    content.addSubview(&title);
    top += title.frame().size.height + TITLE_GAP;
    place_from_top(&version, top, 0.0, height);
    content.addSubview(&version);
    top += version.frame().size.height + TEXT_GAP;
    place_from_top(&date, top, 0.0, height);
    content.addSubview(&date);
    top += date.frame().size.height + TEXT_GAP;
    place_from_top(&description, top, 0.0, height);
    content.addSubview(&description);
    top += description.frame().size.height + BUTTON_GAP;
    place_from_top(&button, top, ((WIDTH - button_width) / 2.0).round(), height);
    content.addSubview(&button);

    let delegate = AboutWindowDelegate::new(mtm);
    panel.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
    // SAFETY: `delegate` outlives the modal session and implements `closeAbout:`.
    unsafe {
        button.setTarget(Some(delegate.as_ref()));
        button.setAction(Some(sel!(closeAbout:)));
    }

    let app = NSApplication::sharedApplication(mtm);
    #[allow(deprecated)]
    app.activateIgnoringOtherApps(true);
    panel.center();
    panel.makeKeyAndOrderFront(None);
    let _ = app.runModalForWindow(&panel);
}
