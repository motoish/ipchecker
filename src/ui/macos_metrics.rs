#[cfg(target_os = "macos")]
use std::cell::RefCell;

use crate::net_latency::LatencyLevel;
use crate::net_speed::{NetworkSpeedLabels, TRAY_LATENCY_WIDTH_TEMPLATE};
use tray_icon::TrayIcon;

#[cfg(target_os = "macos")]
use objc2::rc::Retained;
#[cfg(target_os = "macos")]
use objc2::runtime::AnyObject;
#[cfg(target_os = "macos")]
use objc2::{AnyThread, DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send};
#[cfg(target_os = "macos")]
use objc2_app_kit::{
    NSAttributedStringNSStringDrawing, NSBezierPath, NSCellImagePosition, NSColor,
    NSCompositingOperation, NSFont, NSFontAttributeName, NSFontWeightRegular,
    NSForegroundColorAttributeName, NSImage, NSMutableParagraphStyle,
    NSParagraphStyleAttributeName, NSRectFill, NSTextAlignment, NSTextTab, NSTextTabOptionKey,
    NSView,
};
#[cfg(target_os = "macos")]
use objc2_foundation::{
    NSArray, NSAttributedString, NSAttributedStringKey, NSDictionary, NSPoint, NSRect, NSSize,
    NSString,
};

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
pub(crate) struct SpeedTitleIvars {
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
    pub(crate) struct SpeedTitleView;

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
                    let icon_rect = pixel_aligned_rect(
                        NSRect::new(NSPoint::new(x, icon_y), icon_size),
                        self.backing_scale_factor(),
                    );
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

    fn take_icon(&self) -> Option<Retained<NSImage>> {
        self.ivars().icon.borrow_mut().take()
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
pub(crate) fn install_speed_title_view(tray: &TrayIcon) -> Retained<SpeedTitleView> {
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
pub(crate) fn set_tray_speed_title(
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
        // Custom view owned the drawn icon while metrics were visible; hand it
        // back to the native button so icon-only mode keeps a menu-bar entry.
        view.setHidden(true);
        if is_show_status_icon {
            if button.image().is_none() {
                if let Some(image) = view.take_icon() {
                    button.setImage(Some(&image));
                }
            } else {
                view.clear_icon();
            }
            button.setImagePosition(NSCellImagePosition::ImageOnly);
        } else {
            view.clear_icon();
            button.setImage(None);
            button.setImagePosition(NSCellImagePosition::NoImage);
        }
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
    // Template icons don't auto-tint in custom NSViews. Keep the dynamic
    // light/dark label color, but match native status icons with full opacity.
    NSColor::labelColor().colorWithAlphaComponent(1.0).set();
    NSRectFill(icon_rect);
    image.drawInRect_fromRect_operation_fraction(
        icon_rect,
        NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(0.0, 0.0)),
        NSCompositingOperation::DestinationIn,
        1.0,
    );
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
    let font = NSFont::monospacedDigitSystemFontOfSize_weight(SPEED_FONT_SIZE, unsafe {
        NSFontWeightRegular
    });
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
    let unit_width = ["KB/s", "MB/s", "GB/s"]
        .into_iter()
        .map(|unit| font_width(unit, font))
        .fold(0.0, f64::max);
    let unit_tab_x = number_tab_x + gap + unit_width;

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
            NSTextAlignment::Right,
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

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    #[test]
    fn speed_units_have_equal_rendered_width() {
        let color = NSColor::labelColor();
        let kilobytes = speed_attributed_title("↑\t1.0\tKB/s", &color);
        let megabytes = speed_attributed_title("↑\t1.0\tMB/s", &color);
        let gigabytes = speed_attributed_title("↑\t1.0\tGB/s", &color);

        let widths = [
            kilobytes.size().width,
            megabytes.size().width,
            gigabytes.size().width,
        ];
        let width_range = widths.iter().copied().fold(f64::MIN, f64::max)
            - widths.iter().copied().fold(f64::MAX, f64::min);
        assert!(width_range < 0.01, "speed units differ by {width_range}pt");
    }

    #[test]
    fn speed_font_keeps_system_letter_proportions() {
        let color = NSColor::labelColor();
        let narrow = speed_attributed_title("K", &color).size().width;
        let wide = speed_attributed_title("M", &color).size().width;

        assert!(
            wide - narrow > 0.5,
            "speed letters still look fully monospaced: K={narrow}pt, M={wide}pt"
        );
    }
}
